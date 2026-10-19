//! Panel icon (StatusNotifierItem), shown by KDE and by GNOME with the
//! AppIndicator extension (enabled by default on Ubuntu).
//!
//! The tray service runs on its own thread. The GTK thread renders the
//! translated texts into a [`TrayView`] and pushes it with [`Tray::update`];
//! clicks come back as [`TrayAction`]s through a channel.

use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::{RadioGroup, RadioItem, StandardItem};
use ksni::{MenuItem, ToolTip};

use crate::device::{AncState, Battery};
use crate::i18n::{Msg, t};
use crate::state::{ANC_MODES, Conn, State};

const ICON: &str = "io.github.neptunonet.Budlink-symbolic";

#[derive(Debug, Clone, Copy)]
pub enum TrayAction {
    Open,
    Quit,
    SetAnc(AncState),
}

/// Everything the tray displays, already translated.
#[derive(Debug, Clone, Default)]
pub struct TrayView {
    status: String,
    battery: Vec<String>,
    /// Label and whether the buds support it, in [`ANC_MODES`] order.
    modes: Vec<(String, bool)>,
    selected: Option<usize>,
    open: String,
    quit: String,
}

impl TrayView {
    pub fn new(state: &State) -> Self {
        let connected = state.is_connected();
        let status = match &state.conn {
            Conn::Connected { name } => name.clone(),
            Conn::Searching => t(Msg::Searching).to_string(),
            Conn::Disconnected(_) => t(Msg::NoBudsTitle).to_string(),
        };

        let mut battery = Vec::new();
        if let Some(info) = state.battery.filter(|_| connected) {
            let fmt = |label: Msg, b: Option<Battery>| match b {
                Some(b) if b.charging => format!("{}: {} % ⚡", t(label), b.level),
                Some(b) => format!("{}: {} %", t(label), b.level),
                None => format!("{}: —", t(label)),
            };
            battery.push(fmt(Msg::Left, info.left));
            battery.push(fmt(Msg::Right, info.right));
            battery.push(fmt(Msg::Case, info.case));
        }

        let modes = if connected {
            ANC_MODES
                .iter()
                .map(|(mode, title, _)| (t(*title).to_string(), state.supports(*mode)))
                .collect()
        } else {
            Vec::new()
        };
        let selected = state
            .anc
            .and_then(|anc| ANC_MODES.iter().position(|(mode, _, _)| *mode == anc));

        Self {
            status,
            battery,
            modes,
            selected,
            open: t(Msg::OpenBudlink).to_string(),
            quit: t(Msg::Quit).to_string(),
        }
    }
}

pub struct Tray {
    view: TrayView,
    actions: async_channel::Sender<TrayAction>,
}

impl Tray {
    /// Registers the panel icon. Fails when the desktop has no tray support.
    pub fn spawn(
        view: TrayView,
        actions: async_channel::Sender<TrayAction>,
    ) -> Result<Handle<Tray>, ksni::Error> {
        Tray { view, actions }.spawn()
    }

    fn send(&self, action: TrayAction) {
        let _ = self.actions.try_send(action);
    }
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "budlink".into()
    }

    fn title(&self) -> String {
        "Budlink".into()
    }

    fn icon_name(&self) -> String {
        ICON.into()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            icon_name: ICON.into(),
            title: "Budlink".into(),
            description: std::iter::once(self.view.status.as_str())
                .chain(self.view.battery.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join("\n"),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(TrayAction::Open);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let info = |label: &str| {
            StandardItem { label: label.to_string(), enabled: false, ..Default::default() }.into()
        };

        let mut items: Vec<MenuItem<Self>> = vec![info(&self.view.status)];
        items.extend(self.view.battery.iter().map(|line| info(line)));

        if !self.view.modes.is_empty() {
            items.push(MenuItem::Separator);
            items.push(
                RadioGroup {
                    selected: self.view.selected.unwrap_or(usize::MAX),
                    select: Box::new(|tray: &mut Self, index| {
                        if let Some((mode, _, _)) = ANC_MODES.get(index) {
                            tray.send(TrayAction::SetAnc(*mode));
                        }
                    }),
                    options: self
                        .view
                        .modes
                        .iter()
                        .map(|(label, enabled)| RadioItem {
                            label: label.clone(),
                            enabled: *enabled,
                            ..Default::default()
                        })
                        .collect(),
                }
                .into(),
            );
        }

        items.push(MenuItem::Separator);
        items.push(
            StandardItem {
                label: self.view.open.clone(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayAction::Open)),
                ..Default::default()
            }
            .into(),
        );
        items.push(
            StandardItem {
                label: self.view.quit.clone(),
                icon_name: "application-exit".into(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayAction::Quit)),
                ..Default::default()
            }
            .into(),
        );
        items
    }
}

/// Pushes a new view to a running tray.
pub fn update(handle: &Handle<Tray>, view: TrayView) {
    handle.update(move |tray| tray.view = view);
}
