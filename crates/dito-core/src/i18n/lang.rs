use serde::{Deserialize, Serialize};

use super::Tr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Lang {
    #[default]
    Ja,
    En,
    Zh,
}

impl Lang {
    pub const ALL: [Lang; 3] = [Self::Ja, Self::En, Self::Zh];

    /// Self-name for the display-language picker, retaining the UI locale's script variant rather than the broader
    /// speech language.
    pub fn native_name(self) -> &'static str {
        self.tr(match self {
            Self::Ja => Tr::GeneralJapanese,
            Self::En => Tr::GeneralEnglish,
            Self::Zh => Tr::GeneralSimplifiedChinese,
        })
    }

    /// Name of the spoken or written language localized into `ui`.
    pub fn name(self, ui: Lang) -> &'static str {
        ui.tr(match self {
            Self::Ja => Tr::GeneralJapanese,
            Self::En => Tr::GeneralEnglish,
            Self::Zh => Tr::GeneralChinese,
        })
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Ja => "ja",
            Self::En => "en",
            Self::Zh => "zh",
        }
    }

    pub fn tr(self, message: Tr) -> &'static str {
        message.text(self)
    }

    pub fn from_tag(tag: &str) -> Option<Lang> {
        let primary = tag.split(['-', '_', '.']).next()?.to_ascii_lowercase();
        Self::ALL.into_iter().find(|l| l.code() == primary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_tags_map_to_supported_languages() {
        assert_eq!(Lang::from_tag("ja-JP"), Some(Lang::Ja));
        assert_eq!(Lang::from_tag("en_US.UTF-8"), Some(Lang::En));
        assert_eq!(Lang::from_tag("zh-Hans-CN"), Some(Lang::Zh));
        assert_eq!(Lang::from_tag("fr-FR"), None);
    }
}
