//! Background worker that talks to the buds over the Maestro protocol.
//!
//! The GTK main loop and the Bluetooth/RPC stack live on different threads:
//! the worker runs its own Tokio runtime and exchanges [`Command`]s and
//! [`Event`]s with the UI through async channels.

use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use bluer::rfcomm::{Profile, ProfileHandle, ReqError, Role, Stream};
use bluer::{Adapter, Address, Device, Session};
use futures::StreamExt;

use maestro::protocol::codec::Codec;
use maestro::protocol::types::{RuntimeInfo, SettingsRsp, settings_rsp};
use maestro::protocol::utils;
use maestro::pwrpc::client::Client;
use maestro::service::MaestroService;
use maestro::service::settings::{self, SettingValue};

pub use maestro::service::settings::AncState;

/// How long to wait before looking for the buds again after losing them.
/// Doubles on each quick failure, up to [`MAX_RETRY_DELAY`].
const RETRY_DELAY: Duration = Duration::from_secs(5);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(60);

/// A session that lasted this long resets the backoff.
const STABLE_SESSION: Duration = Duration::from_secs(60);

/// Requests from the UI to the worker.
#[derive(Debug, Clone)]
pub enum Command {
    SetAnc(AncState),
    /// Skip the retry delay and look for the buds right now.
    Reconnect,
}

/// Updates from the worker to the UI.
#[derive(Debug, Clone)]
pub enum Event {
    Searching,
    Connected { name: String },
    Disconnected { reason: DisconnectReason },
    Battery(BatteryInfo),
    Anc(AncState),
    /// The buds rejected an ANC mode, usually because the model lacks it
    /// (e.g. Adaptive on the Pixel Buds 2a). The connection stays up.
    AncRejected(AncState),
}

/// Why there is no session. The UI turns it into translated text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisconnectReason {
    NoBuds,
    /// The buds dropped the link, usually because another app took it.
    Reset,
    Closed,
    Other(String),
}

#[derive(Debug)]
struct NoBudsError;

impl std::fmt::Display for NoBudsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("no compatible buds connected")
    }
}

impl std::error::Error for NoBudsError {}

#[derive(Debug, Clone, Copy, Default)]
pub struct Battery {
    pub level: u8,
    pub charging: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BatteryInfo {
    pub left: Option<Battery>,
    pub right: Option<Battery>,
    pub case: Option<Battery>,
    pub left_in_case: bool,
    pub right_in_case: bool,
}

/// Starts the worker thread and returns the channel ends the UI uses.
pub fn spawn() -> (async_channel::Sender<Command>, async_channel::Receiver<Event>) {
    let (cmd_tx, cmd_rx) = async_channel::unbounded();
    let (evt_tx, evt_rx) = async_channel::unbounded();

    std::thread::Builder::new()
        .name("budlink-device".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build Tokio runtime");

            rt.block_on(worker(cmd_rx, evt_tx));
        })
        .expect("failed to spawn device thread");

    (cmd_tx, evt_rx)
}

async fn worker(cmd_rx: async_channel::Receiver<Command>, evt_tx: async_channel::Sender<Event>) {
    let mut failures: u32 = 0;

    loop {
        let _ = evt_tx.send(Event::Searching).await;

        let started = Instant::now();
        let reason = match run_session(&cmd_rx, &evt_tx).await {
            Ok(()) => DisconnectReason::Closed,
            Err(err) => {
                tracing::warn!(error = ?err, "device session ended");
                describe(&err)
            }
        };
        let _ = evt_tx.send(Event::Disconnected { reason }).await;

        // Back off on repeated failures. The buds accept a single control
        // client, so retrying fast would keep stealing the link from the
        // phone app and vice versa.
        failures = if started.elapsed() >= STABLE_SESSION { 0 } else { failures + 1 };
        let delay = (RETRY_DELAY * 2u32.pow(failures.saturating_sub(1).min(4))).min(MAX_RETRY_DELAY);
        tracing::debug!(?delay, failures, "retrying later");

        // Wait out the retry delay, unless the user asks to reconnect sooner.
        // Commands other than Reconnect make no sense without a device.
        let wait = async {
            loop {
                match cmd_rx.recv().await {
                    Ok(Command::Reconnect) => break,
                    Ok(_) => continue,
                    // The UI is gone: stop the worker.
                    Err(_) => std::future::pending::<()>().await,
                }
            }
        };
        tokio::select! {
            _ = wait => {},
            _ = tokio::time::sleep(delay) => {},
        }

        if cmd_rx.is_closed() {
            return;
        }
    }
}

/// Classifies a session error for the UI.
fn describe(err: &anyhow::Error) -> DisconnectReason {
    if err.chain().any(|e| e.is::<NoBudsError>()) {
        return DisconnectReason::NoBuds;
    }

    let reset = err.chain().any(|e| {
        e.downcast_ref::<std::io::Error>()
            .is_some_and(|e| e.kind() == std::io::ErrorKind::ConnectionReset)
    });
    if reset {
        DisconnectReason::Reset
    } else {
        DisconnectReason::Other(format!("{err:#}"))
    }
}

async fn run_session(
    cmd_rx: &async_channel::Receiver<Command>,
    evt_tx: &async_channel::Sender<Event>,
) -> Result<()> {
    let session = Session::new().await.context("Cannot reach BlueZ")?;
    let adapter = session.default_adapter().await.context("No Bluetooth adapter")?;

    let dev = find_connected_buds(&adapter).await?;
    let name = dev.alias().await.unwrap_or_else(|_| dev.address().to_string());
    tracing::info!(address = %dev.address(), %name, "found buds");

    let stream = connect_maestro_rfcomm(&session, &dev).await?;
    let mut client = Client::new(Codec::new().wrap(stream));
    let channel = utils::resolve_channel(&mut client).await?;

    let _ = evt_tx.send(Event::Connected { name }).await;

    let service = MaestroService::new(client.handle(), channel);

    tokio::select! {
        res = client.run() => {
            res?;
            bail!("Client stopped unexpectedly");
        }
        res = serve(service, cmd_rx, evt_tx) => res,
    }
}

/// Pushes device state to the UI and applies UI commands until something fails.
async fn serve(
    mut service: MaestroService,
    cmd_rx: &async_channel::Receiver<Command>,
    evt_tx: &async_channel::Sender<Event>,
) -> Result<()> {
    let anc = service.read_setting(settings::id::CurrentAncrState).await?;
    let _ = evt_tx.send(Event::Anc(anc)).await;

    let mut runtime_call = service.clone().subscribe_to_runtime_info()?;
    let mut settings_call = service.clone().subscribe_to_settings_changes()?;
    let mut runtime = runtime_call.stream();
    let mut changes = settings_call.stream();

    loop {
        tokio::select! {
            info = runtime.next() => {
                let info = info.context("Runtime info stream ended")??;
                let _ = evt_tx.send(Event::Battery(battery_from(&info))).await;
            }
            change = changes.next() => {
                let change = change.context("Settings stream ended")??;
                if let Some(SettingValue::CurrentAncrState(state)) = setting_from(change) {
                    let _ = evt_tx.send(Event::Anc(state)).await;
                }
            }
            cmd = cmd_rx.recv() => match cmd {
                Ok(Command::SetAnc(state)) => {
                    let res = service.write_setting(SettingValue::CurrentAncrState(state)).await;
                    if let Err(err) = res {
                        tracing::warn!(error = ?err, ?state, "buds rejected ANC mode");
                        let _ = evt_tx.send(Event::AncRejected(state)).await;

                        // Put the UI back on the mode that is actually active.
                        let current = service.read_setting(settings::id::CurrentAncrState).await?;
                        let _ = evt_tx.send(Event::Anc(current)).await;
                    }
                }
                Ok(Command::Reconnect) => {}
                Err(_) => return Ok(()),
            },
        }
    }
}

fn battery_from(info: &RuntimeInfo) -> BatteryInfo {
    // BatteryState: 0 = unknown, 1 = not charging, 2 = charging.
    let conv = |b: &maestro::protocol::types::DeviceBatteryInfo| Battery {
        level: b.level.clamp(0, 100) as u8,
        charging: b.state == 2,
    };

    let bat = info.battery_info.as_ref();
    let placement = info.placement.as_ref();

    BatteryInfo {
        left: bat.and_then(|b| b.left.as_ref()).map(conv),
        right: bat.and_then(|b| b.right.as_ref()).map(conv),
        case: bat.and_then(|b| b.case.as_ref()).map(conv),
        left_in_case: placement.is_some_and(|p| p.left_bud_in_case),
        right_in_case: placement.is_some_and(|p| p.right_bud_in_case),
    }
}

fn setting_from(rsp: SettingsRsp) -> Option<SettingValue> {
    let settings_rsp::ValueOneof::Value(value) = rsp.value_oneof?;
    Some(value.value_oneof?.into())
}

/// Returns the first connected device that advertises the Maestro service.
async fn find_connected_buds(adapter: &Adapter) -> Result<Device> {
    for addr in adapter.device_addresses().await? {
        let dev = adapter.device(addr)?;

        if !dev.is_connected().await.unwrap_or(false) {
            continue;
        }

        let uuids = dev.uuids().await?.unwrap_or_default();
        if uuids.contains(&maestro::UUID) {
            return Ok(dev);
        }
    }

    Err(NoBudsError.into())
}

// The RFCOMM setup below follows pbpctrl's cli/src/bt.rs (MIT/Apache-2.0).

async fn connect_maestro_rfcomm(session: &Session, dev: &Device) -> Result<Stream> {
    let profile = Profile {
        uuid: maestro::UUID,
        role: Some(Role::Client),
        require_authentication: Some(false),
        require_authorization: Some(false),
        auto_connect: Some(false),
        ..Default::default()
    };

    let mut handle = session.register_profile(profile).await?;

    let (_, stream) = tokio::try_join!(
        try_connect_profile(dev),
        accept_profile_request(&mut handle, dev.address()),
    )?;

    Ok(stream)
}

async fn try_connect_profile(dev: &Device) -> Result<()> {
    const MAX_TRIES: u32 = 3;

    let mut tries = 0;
    while let Err(err) = dev.connect_profile(&maestro::UUID).await {
        if tries >= MAX_TRIES {
            return Err(err.into());
        }
        tries += 1;
        tracing::warn!(error = ?err, "connecting to Maestro profile failed, retrying ({tries}/{MAX_TRIES})");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    Ok(())
}

async fn accept_profile_request(handle: &mut ProfileHandle, address: Address) -> Result<Stream> {
    while let Some(req) = handle.next().await {
        if req.device() == address {
            return Ok(req.accept()?);
        }
        req.reject(ReqError::Rejected);
    }

    bail!("Maestro profile closed without a connection")
}
