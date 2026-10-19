mod config;
mod device;
mod i18n;
mod state;
mod tray;
mod window;

use std::cell::{OnceCell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};

use config::{Config, LangPref};
use device::{Command, Event};
use i18n::{Msg, t};
use state::State;
use tray::{Tray, TrayAction, TrayView};
use window::Window;

const APP_ID: &str = "io.github.neptunonet.Budlink";

/// Owns the app state and keeps the window and the tray icon in sync with it.
struct Controller {
    app: adw::Application,
    cmd_tx: async_channel::Sender<Command>,
    state: RefCell<State>,
    config: RefCell<Config>,
    window: RefCell<Option<Window>>,
    tray: Option<ksni::blocking::Handle<Tray>>,
    // Keeps the app alive with the window closed while the tray icon is up.
    _hold: Option<gio::ApplicationHoldGuard>,
}

impl Controller {
    fn start(app: &adw::Application) -> Rc<Self> {
        let config = Config::load();
        i18n::set(config.language.resolve());

        let (cmd_tx, evt_rx) = device::spawn();
        let state = State::default();

        let (tray_tx, tray_rx) = async_channel::unbounded();
        let tray = match Tray::spawn(TrayView::new(&state), tray_tx) {
            Ok(handle) => Some(handle),
            Err(err) => {
                tracing::warn!(error = %err, "no tray support, closing the window will quit");
                None
            }
        };
        let hold = tray.is_some().then(|| app.hold());

        let ctl = Rc::new(Self {
            app: app.clone(),
            cmd_tx,
            state: RefCell::new(state),
            config: RefCell::new(config),
            window: RefCell::new(None),
            tray,
            _hold: hold,
        });

        ctl.add_actions();

        glib::spawn_future_local({
            let ctl = ctl.clone();
            async move {
                while let Ok(event) = evt_rx.recv().await {
                    ctl.on_event(&event);
                }
            }
        });

        glib::spawn_future_local({
            let ctl = ctl.clone();
            async move {
                while let Ok(action) = tray_rx.recv().await {
                    match action {
                        TrayAction::Open => ctl.present_window(),
                        TrayAction::Quit => ctl.app.quit(),
                        TrayAction::SetAnc(mode) => {
                            let _ = ctl.cmd_tx.try_send(Command::SetAnc(mode));
                        }
                    }
                }
            }
        });

        ctl
    }

    fn add_actions(self: &Rc<Self>) {
        let preferences = gio::ActionEntry::builder("preferences")
            .activate({
                let ctl = self.clone();
                move |_, _, _| ctl.show_preferences()
            })
            .build();
        let about = gio::ActionEntry::builder("about")
            .activate({
                let ctl = self.clone();
                move |_, _, _| ctl.show_about()
            })
            .build();
        let quit = gio::ActionEntry::builder("quit")
            .activate(|app: &adw::Application, _, _| app.quit())
            .build();
        self.app.add_action_entries([preferences, about, quit]);
        self.app.set_accels_for_action("app.quit", &["<Control>q"]);
        self.app.set_accels_for_action("window.close", &["<Control>w"]);
    }

    fn on_event(&self, event: &Event) {
        self.state.borrow_mut().apply(event);

        if let Some(win) = self.window.borrow().as_ref() {
            win.render(&self.state.borrow());
            if let Event::AncRejected(mode) = event {
                win.mode_rejected(*mode);
            }
        }
        self.update_tray();
    }

    fn update_tray(&self) {
        if let Some(handle) = &self.tray {
            tray::update(handle, TrayView::new(&self.state.borrow()));
        }
    }

    fn present_window(self: &Rc<Self>) {
        if let Some(win) = self.window.borrow().as_ref() {
            win.window.present();
            return;
        }

        let win = Window::new(&self.app, &self.cmd_tx);
        win.render(&self.state.borrow());
        win.window.connect_close_request({
            let ctl = Rc::downgrade(self);
            move |_| {
                if let Some(ctl) = ctl.upgrade() {
                    ctl.window.borrow_mut().take();
                }
                glib::Propagation::Proceed
            }
        });
        win.window.present();
        self.window.replace(Some(win));
    }

    fn set_language(&self, pref: LangPref) {
        let mut config = self.config.borrow_mut();
        if config.language == pref {
            return;
        }
        config.language = pref;
        config.save();

        i18n::set(pref.resolve());
        if let Some(win) = self.window.borrow().as_ref() {
            win.retranslate();
            win.render(&self.state.borrow());
        }
        self.update_tray();
    }

    fn show_preferences(self: &Rc<Self>) {
        let languages = gtk::StringList::new(&[t(Msg::SystemDefault), "English", "Español"]);
        let current = self.config.borrow().language;
        let language = adw::ComboRow::builder()
            .title(t(Msg::Language))
            .model(&languages)
            .selected(LangPref::ALL.iter().position(|p| *p == current).unwrap_or(0) as u32)
            .build();

        let group = adw::PreferencesGroup::builder().title(t(Msg::General)).build();
        group.add(&language);
        let page = adw::PreferencesPage::new();
        page.add(&group);

        let dialog = adw::PreferencesDialog::builder().title(t(Msg::Preferences)).build();
        dialog.add(&page);

        language.connect_selected_notify({
            let ctl = self.clone();
            let (dialog, group) = (dialog.clone(), group.clone());
            move |row| {
                let Some(pref) = LangPref::ALL.get(row.selected() as usize) else { return };
                ctl.set_language(*pref);

                // The dialog is already open: translate it in place.
                dialog.set_title(t(Msg::Preferences));
                group.set_title(t(Msg::General));
                row.set_title(t(Msg::Language));
            }
        });

        let parent = self.window.borrow().as_ref().map(|w| w.window.clone());
        dialog.present(parent.as_ref());
    }

    fn show_about(&self) {
        let about = adw::AboutDialog::builder()
            .application_name("Budlink")
            .application_icon(APP_ID)
            .version(env!("CARGO_PKG_VERSION"))
            .developer_name("Juan Ignacio Armas")
            .comments(t(Msg::AppComment))
            .license_type(gtk::License::Gpl30)
            .website(env!("CARGO_PKG_REPOSITORY"))
            .issue_url(concat!(env!("CARGO_PKG_REPOSITORY"), "/issues"))
            .build();
        about.add_acknowledgement_section(
            Some("Maestro protocol (pbpctrl)"),
            &["Maximilian Luz https://github.com/qzed/pbpctrl"],
        );

        let parent = self.window.borrow().as_ref().map(|w| w.window.clone());
        about.present(parent.as_ref());
    }
}

fn main() -> glib::ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "budlink=info".into()),
        )
        .init();

    let app = adw::Application::builder().application_id(APP_ID).build();
    let controller: Rc<OnceCell<Rc<Controller>>> = Rc::default();

    app.connect_startup({
        let controller = controller.clone();
        move |app| {
            let _ = controller.set(Controller::start(app));
        }
    });

    // Launching the app again (from the menu or the tray) shows the window.
    app.connect_activate(move |_| {
        if let Some(ctl) = controller.get() {
            ctl.present_window();
        }
    });

    app.run()
}
