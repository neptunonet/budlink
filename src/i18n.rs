//! UI translations (English and Spanish).
//!
//! The strings live in the binary rather than in gettext catalogs so the
//! language can be switched from the preferences at runtime, without
//! installing locale files or restarting the app.

use std::cell::Cell;

use gtk::glib;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Es,
}

impl Lang {
    /// Picks the first supported language from the user's locale settings.
    pub fn from_system() -> Self {
        for name in glib::language_names() {
            let name = name.as_str();
            if name.starts_with("es") {
                return Lang::Es;
            }
            if name.starts_with("en") {
                return Lang::En;
            }
        }
        Lang::En
    }
}

thread_local! {
    // Only the GTK main thread renders text, so a thread-local is enough.
    static CURRENT: Cell<Lang> = const { Cell::new(Lang::En) };
}

pub fn set(lang: Lang) {
    CURRENT.with(|c| c.set(lang));
}

fn pick(en: &'static str, es: &'static str) -> &'static str {
    match CURRENT.with(Cell::get) {
        Lang::En => en,
        Lang::Es => es,
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Msg {
    AppComment,
    MainMenu,
    Preferences,
    About,
    OpenBudlink,
    Quit,

    Searching,
    NoBudsTitle,
    NoBudsHint,
    TryAgain,
    ReasonNoBuds,
    ReasonReset,
    ReasonClosed,

    NoiseControl,
    AncActive,
    AncActiveHint,
    AncAware,
    AncAwareHint,
    AncAdaptive,
    AncAdaptiveHint,
    AncOff,
    AncOffHint,
    NotSupported,

    Battery,
    Left,
    Right,
    Case,
    Charging,
    InCase,
    CaseHint,
    Unavailable,

    General,
    Language,
    SystemDefault,
    Appearance,
    Light,
    Dark,
}

pub fn t(msg: Msg) -> &'static str {
    use Msg::*;
    match msg {
        AppComment => pick(
            "Control your Pixel Buds from the Linux desktop",
            "Controla tus Pixel Buds desde el escritorio de Linux",
        ),
        MainMenu => pick("Main Menu", "Menú principal"),
        Preferences => pick("Preferences", "Preferencias"),
        About => pick("About Budlink", "Acerca de Budlink"),
        OpenBudlink => pick("Open Budlink", "Abrir Budlink"),
        Quit => pick("Quit", "Salir"),

        Searching => pick("Looking for your buds…", "Buscando tus auriculares…"),
        NoBudsTitle => pick("No Buds Connected", "No hay auriculares conectados"),
        NoBudsHint => pick(
            "Connect your Pixel Buds from the system Bluetooth settings.",
            "Conecta tus Pixel Buds desde la configuración de Bluetooth del sistema.",
        ),
        TryAgain => pick("Try Again", "Reintentar"),
        ReasonNoBuds => pick(
            "No compatible buds are connected.",
            "No hay auriculares compatibles conectados.",
        ),
        ReasonReset => pick(
            "The buds closed the connection. Another app may be controlling them, \
             such as the Pixel Buds app on your phone.",
            "Los auriculares cerraron la conexión. Puede que otra app los esté \
             controlando, como la app Pixel Buds del teléfono.",
        ),
        ReasonClosed => pick("The connection was closed.", "Se cerró la conexión."),

        NoiseControl => pick("Noise Control", "Control de ruido"),
        AncActive => pick("Noise Cancelling", "Cancelación de ruido"),
        AncActiveHint => pick("Blocks outside sound", "Bloquea el sonido exterior"),
        AncAware => pick("Transparency", "Transparencia"),
        AncAwareHint => pick("Lets you hear your surroundings", "Te deja oír lo que pasa alrededor"),
        AncAdaptive => pick("Adaptive", "Adaptativo"),
        AncAdaptiveHint => pick("Adjusts to the noise around you", "Se ajusta al ruido del entorno"),
        AncOff => pick("Off", "Desactivado"),
        AncOffHint => pick("No noise control", "Sin control de ruido"),
        NotSupported => pick("Not supported by these buds", "No disponible en estos auriculares"),

        Battery => pick("Battery", "Batería"),
        Left => pick("Left", "Izquierdo"),
        Right => pick("Right", "Derecho"),
        Case => pick("Case", "Estuche"),
        Charging => pick("Charging", "Cargando"),
        InCase => pick("In case", "En el estuche"),
        CaseHint => pick(
            "Put a bud in the case to read it",
            "Pon un auricular en el estuche para leerla",
        ),
        Unavailable => pick("Unavailable", "No disponible"),

        General => pick("General", "General"),
        Language => pick("Language", "Idioma"),
        SystemDefault => pick("System default", "Predeterminado del sistema"),
        Appearance => pick("Appearance", "Apariencia"),
        Light => pick("Light", "Claro"),
        Dark => pick("Dark", "Oscuro"),
    }
}

/// Toast shown when the buds reject a noise control mode.
pub fn mode_not_supported(mode: &str) -> String {
    match CURRENT.with(Cell::get) {
        Lang::En => format!("{mode} isn’t supported by these buds"),
        Lang::Es => format!("{mode} no está disponible en estos auriculares"),
    }
}
