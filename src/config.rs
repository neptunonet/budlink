//! User preferences, stored as `key=value` lines in
//! `~/.config/budlink/settings`.

use std::fs;
use std::path::PathBuf;

use gtk::glib;

use crate::i18n::Lang;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LangPref {
    #[default]
    System,
    En,
    Es,
}

impl LangPref {
    pub const ALL: [LangPref; 3] = [LangPref::System, LangPref::En, LangPref::Es];

    pub fn resolve(self) -> Lang {
        match self {
            LangPref::System => Lang::from_system(),
            LangPref::En => Lang::En,
            LangPref::Es => Lang::Es,
        }
    }

    fn key(self) -> &'static str {
        match self {
            LangPref::System => "system",
            LangPref::En => "en",
            LangPref::Es => "es",
        }
    }
}

/// Light or dark style. `System` follows the desktop setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub const ALL: [Appearance; 3] = [Appearance::System, Appearance::Light, Appearance::Dark];

    pub fn color_scheme(self) -> adw::ColorScheme {
        match self {
            Appearance::System => adw::ColorScheme::Default,
            Appearance::Light => adw::ColorScheme::ForceLight,
            Appearance::Dark => adw::ColorScheme::ForceDark,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Appearance::System => "system",
            Appearance::Light => "light",
            Appearance::Dark => "dark",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Config {
    pub language: LangPref,
    pub appearance: Appearance,
}

fn path() -> PathBuf {
    glib::user_config_dir().join("budlink").join("settings")
}

impl Config {
    /// Reads the settings file. Missing or unknown values fall back to defaults.
    pub fn load() -> Self {
        let mut config = Config::default();
        let Ok(text) = fs::read_to_string(path()) else {
            return config;
        };

        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            let value = value.trim();
            match key.trim() {
                "language" => {
                    if let Some(pref) = LangPref::ALL.into_iter().find(|p| p.key() == value) {
                        config.language = pref;
                    }
                }
                "appearance" => {
                    if let Some(pref) = Appearance::ALL.into_iter().find(|p| p.key() == value) {
                        config.appearance = pref;
                    }
                }
                _ => {}
            }
        }
        config
    }

    pub fn save(&self) {
        let path = path();
        let text = format!(
            "language={}\nappearance={}\n",
            self.language.key(),
            self.appearance.key()
        );
        let res = path
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::write(&path, text));
        if let Err(err) = res {
            tracing::warn!(error = %err, path = %path.display(), "could not save settings");
        }
    }
}
