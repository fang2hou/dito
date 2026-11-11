use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::HudPosition;
use super::hud::DEFAULT_HUD_OFFSET;
use crate::i18n::{self, Lang, Tr};

pub const DEFAULT_SHORTCUT: &str = "RightCommand";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RecordMode {
    #[default]
    Toggle,
    Hold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MusicMuteMode {
    Off,
    #[default]
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InsertMethod {
    /// Type the text as synthetic key events.
    #[default]
    KeyInput,
    /// Put the text on the clipboard and send Cmd+V.
    Paste,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

/// Language the speaker most likely uses; `Auto` lets the engine detect it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PreferredLanguage {
    #[default]
    Auto,
    Ja,
    En,
    Zh,
}

impl PreferredLanguage {
    pub const ALL: [Self; 4] = [Self::Auto, Self::Ja, Self::En, Self::Zh];

    pub fn name(self, ui: Lang) -> &'static str {
        match self {
            Self::Auto => ui.tr(Tr::GeneralAuto),
            Self::Ja => Lang::Ja.name(ui),
            Self::En => Lang::En.name(ui),
            Self::Zh => Lang::Zh.name(ui),
        }
    }

    pub fn code(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::Ja => Some("ja"),
            Self::En => Some("en"),
            Self::Zh => Some("zh"),
        }
    }
}

/// Language of the app's own UI: follows the system unless the user picks one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DisplayLanguage {
    #[default]
    System,
    Ja,
    En,
    Zh,
}

impl DisplayLanguage {
    pub const ALL: [DisplayLanguage; 4] = [Self::System, Self::Ja, Self::En, Self::Zh];

    pub fn resolve(self) -> Lang {
        match self {
            Self::System => i18n::system_language(),
            Self::Ja => Lang::Ja,
            Self::En => Lang::En,
            Self::Zh => Lang::Zh,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    pub shortcut: String,
    pub record_mode: RecordMode,
    pub insert_method: InsertMethod,
    pub music_mute: MusicMuteMode,
    pub microphone: String,
    /// Per-device choice (by name) of whether automatic microphone picking may use it. Devices
    /// not listed follow the default for their connection type.
    pub microphone_auto: BTreeMap<String, bool>,
    pub preferred_language: PreferredLanguage,
    pub hud_languages: Vec<PreferredLanguage>,
    pub launch_at_login: bool,
    /// Off: no Dock icon while the app runs in the background. It still appears while a window is open.
    pub show_dock_icon: bool,
    pub appearance: Appearance,
    /// HUD appearance follows the OS independently of the settings window.
    pub hud_appearance: Appearance,
    pub hud_position: HudPosition,
    /// Distance in points between the countdown's bottom and the usable screen edge.
    /// Retained for migration of settings written before discrete height levels existed.
    pub hud_offset: u32,
    /// Discrete position, -2 through 12. Missing in older configuration files.
    pub hud_height_level: Option<i8>,
    pub display_language: DisplayLanguage,
}

impl General {
    /// Canonical order, even when a hand-edited config contains duplicates or a different order.
    pub fn hud_languages(&self) -> impl Iterator<Item = PreferredLanguage> + '_ {
        PreferredLanguage::ALL.into_iter().filter(|language| self.hud_languages.contains(language))
    }

    pub fn toggle_hud_language(&mut self, language: PreferredLanguage) {
        if self.hud_languages.contains(&language) {
            self.hud_languages.retain(|&item| item != language);
        } else {
            self.hud_languages.push(language);
        }
    }

    /// The language the UI is shown in right now.
    pub fn lang(&self) -> Lang {
        self.display_language.resolve()
    }
}

impl Default for General {
    fn default() -> Self {
        Self {
            shortcut: DEFAULT_SHORTCUT.into(),
            record_mode: RecordMode::Toggle,
            insert_method: InsertMethod::KeyInput,
            music_mute: MusicMuteMode::Auto,
            microphone: String::new(),
            microphone_auto: BTreeMap::new(),
            preferred_language: PreferredLanguage::Auto,
            hud_languages: PreferredLanguage::ALL.to_vec(),
            launch_at_login: true,
            show_dock_icon: true,
            appearance: Appearance::System,
            hud_appearance: Appearance::Dark,
            hud_position: HudPosition::Centered,
            hud_offset: DEFAULT_HUD_OFFSET,
            hud_height_level: None,
            display_language: DisplayLanguage::System,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{General, PreferredLanguage};

    #[test]
    fn legacy_configs_keep_all_shortcuts_and_an_empty_selection_stays_empty() {
        let legacy: General = toml::from_str("preferred_language = 'en'").unwrap();
        assert!(legacy.hud_languages().eq(PreferredLanguage::ALL));
        let hidden: General = toml::from_str("hud_languages = []").unwrap();
        assert_eq!(hidden.hud_languages().count(), 0);
    }

    #[test]
    fn shortcut_changes_are_independent_of_the_preferred_language() {
        let mut general: General = toml::from_str(
            "preferred_language = 'en'
hud_languages = ['zh', 'auto', 'auto']",
        )
        .unwrap();
        assert!(general.hud_languages().eq([PreferredLanguage::Auto, PreferredLanguage::Zh]));
        general.toggle_hud_language(PreferredLanguage::Auto);
        assert!(general.hud_languages().eq([PreferredLanguage::Zh]));
        assert_eq!(general.preferred_language, PreferredLanguage::En);
    }
}
