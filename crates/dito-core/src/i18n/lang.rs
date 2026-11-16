use serde::{Deserialize, Serialize};

use super::Tr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Lang {
    #[default]
    Ja,
    En,
    Zh,
    ZhHant,
    Ko,
    Mn,
    Fr,
}

impl Lang {
    pub const ALL: [Lang; 7] = [Self::Ja, Self::En, Self::Zh, Self::ZhHant, Self::Ko, Self::Mn, Self::Fr];

    /// Languages that have their own polish prompt sets (`Prompts::ja`, `en`, `zh`).
    pub const PROMPT_LANGUAGES: [Lang; 3] = [Self::Ja, Self::En, Self::Zh];

    /// Self-name for the display-language picker, retaining the UI locale's script variant rather than the broader
    /// speech language.
    pub fn native_name(self) -> &'static str {
        self.tr(match self {
            Self::Ja => Tr::GeneralJapanese,
            Self::En => Tr::GeneralEnglish,
            Self::Zh => Tr::GeneralSimplifiedChinese,
            Self::ZhHant => Tr::GeneralTraditionalChinese,
            Self::Ko => Tr::GeneralKorean,
            Self::Mn => Tr::GeneralMongolian,
            Self::Fr => Tr::GeneralFrench,
        })
    }

    /// Name of the spoken or written language localized into `ui`.
    pub fn name(self, ui: Lang) -> &'static str {
        ui.tr(match self {
            Self::Ja => Tr::GeneralJapanese,
            Self::En => Tr::GeneralEnglish,
            Self::Zh => Tr::GeneralChinese,
            Self::ZhHant => Tr::GeneralTraditionalChinese,
            Self::Ko => Tr::GeneralKorean,
            Self::Mn => Tr::GeneralMongolian,
            Self::Fr => Tr::GeneralFrench,
        })
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Ja => "ja",
            Self::En => "en",
            Self::Zh => "zh",
            Self::ZhHant => "zh-Hant",
            Self::Ko => "ko",
            Self::Mn => "mn",
            Self::Fr => "fr",
        }
    }

    /// Maps any UI locale onto one of [`Self::PROMPT_LANGUAGES`] (`ZhHant` -> `Zh`, `Ja` -> `Ja`, others -> `En`).
    pub fn prompt_language(self) -> Self {
        match self {
            Self::Ja => Self::Ja,
            Self::Zh | Self::ZhHant => Self::Zh,
            Self::En | Self::Ko | Self::Mn | Self::Fr => Self::En,
        }
    }

    pub fn tr(self, message: Tr) -> &'static str {
        message.text(self)
    }

    pub fn from_tag(tag: &str) -> Option<Lang> {
        let base = tag.split(['.', '@']).next()?;
        let mut parts = base.split(['-', '_']).filter(|s| !s.is_empty());
        let primary = parts.next()?.to_ascii_lowercase();
        match primary.as_str() {
            "ja" => Some(Self::Ja),
            "en" => Some(Self::En),
            "zh" => {
                let traditional = parts.any(|p| matches!(p.to_ascii_lowercase().as_str(), "hant" | "tw" | "hk" | "mo"));
                Some(if traditional { Self::ZhHant } else { Self::Zh })
            }
            "ko" => Some(Self::Ko),
            "mn" => {
                if parts.any(|p| p.eq_ignore_ascii_case("mong")) {
                    None
                } else {
                    Some(Self::Mn)
                }
            }
            "fr" => Some(Self::Fr),
            _ => None,
        }
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
        assert_eq!(Lang::from_tag("zh_CN.UTF-8"), Some(Lang::Zh));
        assert_eq!(Lang::from_tag("zh-Hant-TW"), Some(Lang::ZhHant));
        assert_eq!(Lang::from_tag("zh_TW.UTF-8"), Some(Lang::ZhHant));
        assert_eq!(Lang::from_tag("zh-HK"), Some(Lang::ZhHant));
        assert_eq!(Lang::from_tag("zh_hant"), Some(Lang::ZhHant));
        assert_eq!(Lang::from_tag("ko-KR"), Some(Lang::Ko));
        assert_eq!(Lang::from_tag("mn-MN"), Some(Lang::Mn));
        assert_eq!(Lang::from_tag("mn-Cyrl-MN"), Some(Lang::Mn));
        assert_eq!(Lang::from_tag("mn-Mong-CN"), None);
        assert_eq!(Lang::from_tag("fr-FR"), Some(Lang::Fr));
        assert_eq!(Lang::from_tag("fr_CA.UTF-8"), Some(Lang::Fr));
        assert_eq!(Lang::from_tag("de-DE"), None);
    }
}
