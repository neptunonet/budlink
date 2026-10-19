//! What the app knows about the buds, shared by the window and the tray.

use crate::device::{AncState, BatteryInfo, DisconnectReason, Event};
use crate::i18n::Msg;

/// Noise control modes in display order: state, title and subtitle.
pub const ANC_MODES: [(AncState, Msg, Msg); 4] = [
    (AncState::Active, Msg::AncActive, Msg::AncActiveHint),
    (AncState::Aware, Msg::AncAware, Msg::AncAwareHint),
    (AncState::Adaptive, Msg::AncAdaptive, Msg::AncAdaptiveHint),
    (AncState::Off, Msg::AncOff, Msg::AncOffHint),
];

#[derive(Debug, Clone, Default)]
pub enum Conn {
    #[default]
    Searching,
    Disconnected(DisconnectReason),
    Connected { name: String },
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub conn: Conn,
    pub anc: Option<AncState>,
    pub battery: Option<BatteryInfo>,
    /// Modes the connected buds refused.
    pub rejected: Vec<AncState>,
}

impl State {
    pub fn apply(&mut self, event: &Event) {
        match event {
            Event::Searching => self.conn = Conn::Searching,
            Event::Connected { name } => {
                self.conn = Conn::Connected { name: name.clone() };
                // Support may differ between models.
                self.rejected.clear();
            }
            Event::Disconnected { reason } => {
                self.conn = Conn::Disconnected(reason.clone());
                self.anc = None;
                self.battery = None;
            }
            Event::Battery(info) => self.battery = Some(*info),
            Event::Anc(state) => self.anc = Some(*state),
            Event::AncRejected(state) => {
                if !self.rejected.contains(state) {
                    self.rejected.push(*state);
                }
            }
        }
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.conn, Conn::Connected { .. })
    }

    pub fn supports(&self, mode: AncState) -> bool {
        !self.rejected.contains(&mode)
    }
}
