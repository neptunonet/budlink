use std::cell::Cell;

use adw::prelude::*;
use gtk::{gio, glib};

use crate::device::{AncState, Battery, Command, DisconnectReason};
use crate::i18n::{self, Msg, t};
use crate::state::{ANC_MODES, Conn, State};

struct BatteryRow {
    title: Msg,
    row: adw::ActionRow,
    bar: gtk::LevelBar,
    label: gtk::Label,
}

impl BatteryRow {
    fn new(title: Msg) -> Self {
        let bar = gtk::LevelBar::builder()
            .min_value(0.0)
            .max_value(100.0)
            .width_request(120)
            .valign(gtk::Align::Center)
            .build();
        let label = gtk::Label::builder().width_chars(5).xalign(1.0).build();
        label.add_css_class("numeric");

        let row = adw::ActionRow::new();
        row.add_suffix(&bar);
        row.add_suffix(&label);

        Self { title, row, bar, label }
    }

    /// `in_case` is `None` for the case itself.
    fn render(&self, battery: Option<Battery>, in_case: Option<bool>) {
        self.row.set_title(t(self.title));
        match battery {
            Some(b) => {
                self.bar.set_value(b.level.into());
                self.label.set_text(&format!("{} %", b.level));
                self.row.set_subtitle(match (b.charging, in_case) {
                    (true, _) => t(Msg::Charging),
                    (false, Some(true)) => t(Msg::InCase),
                    _ => "",
                });
                self.row.set_sensitive(true);
            }
            None => {
                self.bar.set_value(0.0);
                self.label.set_text("—");
                self.row.set_subtitle(t(if in_case.is_none() { Msg::CaseHint } else { Msg::Unavailable }));
                self.row.set_sensitive(false);
            }
        }
    }
}

struct AncRow {
    state: AncState,
    title: Msg,
    subtitle: Msg,
    row: adw::ActionRow,
    check: gtk::CheckButton,
}

pub struct Window {
    pub window: adw::ApplicationWindow,
    header: adw::HeaderBar,
    menu_button: gtk::MenuButton,
    stack: gtk::Stack,
    toasts: adw::ToastOverlay,
    status: adw::StatusPage,
    retry: gtk::Button,
    anc_group: adw::PreferencesGroup,
    anc_rows: Vec<AncRow>,
    battery_group: adw::PreferencesGroup,
    left: BatteryRow,
    right: BatteryRow,
    case: BatteryRow,
    /// Set while the UI mirrors state coming from the buds, so the radio
    /// handlers don't echo it back as a command.
    syncing: std::rc::Rc<Cell<bool>>,
}

impl Window {
    pub fn new(app: &adw::Application, cmd_tx: &async_channel::Sender<Command>) -> Self {
        let syncing = std::rc::Rc::new(Cell::new(false));

        // Status page: shown while searching or disconnected.
        let retry = gtk::Button::builder()
            .halign(gtk::Align::Center)
            .css_classes(["pill", "suggested-action"])
            .visible(false)
            .build();
        retry.connect_clicked({
            let cmd_tx = cmd_tx.clone();
            move |_| {
                let _ = cmd_tx.try_send(Command::Reconnect);
            }
        });
        let status = adw::StatusPage::builder()
            .icon_name("io.github.neptunonet.Budlink-symbolic")
            .child(&retry)
            .build();

        // Device page.
        let anc_group = adw::PreferencesGroup::new();
        let mut anc_rows: Vec<AncRow> = Vec::new();
        for (state, title, subtitle) in ANC_MODES {
            let check = gtk::CheckButton::new();
            if let Some(first) = anc_rows.first() {
                check.set_group(Some(&first.check));
            }
            check.connect_toggled({
                let cmd_tx = cmd_tx.clone();
                let syncing = syncing.clone();
                move |check| {
                    if check.is_active() && !syncing.get() {
                        let _ = cmd_tx.try_send(Command::SetAnc(state));
                    }
                }
            });

            let row = adw::ActionRow::builder().activatable_widget(&check).build();
            row.add_prefix(&check);
            anc_group.add(&row);

            anc_rows.push(AncRow { state, title, subtitle, row, check });
        }

        let left = BatteryRow::new(Msg::Left);
        let right = BatteryRow::new(Msg::Right);
        let case = BatteryRow::new(Msg::Case);
        let battery_group = adw::PreferencesGroup::new();
        battery_group.add(&left.row);
        battery_group.add(&right.row);
        battery_group.add(&case.row);

        let page = adw::PreferencesPage::new();
        page.add(&anc_group);
        page.add(&battery_group);

        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .build();
        stack.add_named(&status, Some("status"));
        stack.add_named(&page, Some("device"));

        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&stack));

        let menu_button = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .primary(true)
            .build();
        let header = adw::HeaderBar::new();
        header.pack_end(&menu_button);

        let content = adw::ToolbarView::new();
        content.add_top_bar(&header);
        content.set_content(Some(&toasts));

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Budlink")
            .default_width(460)
            .default_height(560)
            .content(&content)
            .build();

        let win = Self {
            window,
            header,
            menu_button,
            stack,
            toasts,
            status,
            retry,
            anc_group,
            anc_rows,
            battery_group,
            left,
            right,
            case,
            syncing,
        };
        win.retranslate();
        win
    }

    /// Sets the texts that don't depend on the buds' state.
    pub fn retranslate(&self) {
        let menu = gio::Menu::new();
        menu.append(Some(t(Msg::Preferences)), Some("app.preferences"));
        menu.append(Some(t(Msg::About)), Some("app.about"));
        self.menu_button.set_menu_model(Some(&menu));
        self.menu_button.set_tooltip_text(Some(t(Msg::MainMenu)));

        self.retry.set_label(t(Msg::TryAgain));
        self.anc_group.set_title(t(Msg::NoiseControl));
        self.battery_group.set_title(t(Msg::Battery));
        for anc in &self.anc_rows {
            anc.row.set_title(t(anc.title));
        }
    }

    pub fn render(&self, state: &State) {
        match &state.conn {
            Conn::Searching => {
                self.status.set_title(t(Msg::Searching));
                self.status.set_description(None);
                self.retry.set_visible(false);
                self.stack.set_visible_child_name("status");
                self.header.set_title_widget(None::<&gtk::Widget>);
            }
            Conn::Disconnected(reason) => {
                let reason = match reason {
                    DisconnectReason::NoBuds => t(Msg::ReasonNoBuds).to_string(),
                    DisconnectReason::Reset => t(Msg::ReasonReset).to_string(),
                    DisconnectReason::Closed => t(Msg::ReasonClosed).to_string(),
                    DisconnectReason::Other(err) => err.clone(),
                };
                self.status.set_title(t(Msg::NoBudsTitle));
                self.status.set_description(Some(&format!(
                    "{}\n\n<small>{}</small>",
                    glib::markup_escape_text(t(Msg::NoBudsHint)),
                    glib::markup_escape_text(&reason)
                )));
                self.retry.set_visible(true);
                self.stack.set_visible_child_name("status");
                self.header.set_title_widget(None::<&gtk::Widget>);
            }
            Conn::Connected { name } => {
                self.header
                    .set_title_widget(Some(&adw::WindowTitle::new("Budlink", name)));
                self.stack.set_visible_child_name("device");
            }
        }

        self.syncing.set(true);
        for anc in &self.anc_rows {
            let supported = state.supports(anc.state);
            anc.row.set_sensitive(supported);
            anc.row
                .set_subtitle(t(if supported { anc.subtitle } else { Msg::NotSupported }));
            if state.anc == Some(anc.state) {
                anc.check.set_active(true);
            }
        }
        self.syncing.set(false);

        let info = state.battery.unwrap_or_default();
        self.left.render(info.left, Some(info.left_in_case));
        self.right.render(info.right, Some(info.right_in_case));
        self.case.render(info.case, None);
    }

    pub fn mode_rejected(&self, mode: AncState) {
        if let Some(anc) = self.anc_rows.iter().find(|a| a.state == mode) {
            self.toasts
                .add_toast(adw::Toast::new(&i18n::mode_not_supported(t(anc.title))));
        }
    }
}
