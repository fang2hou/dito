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

    /// Name shown in the language picker, always in its own language.
    pub fn native_name(self) -> &'static str {
        match self {
            Self::Ja => "日本語",
            Self::En => "English",
            Self::Zh => "简体中文",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Ja => "ja",
            Self::En => "en",
            Self::Zh => "zh",
        }
    }

    /// The copy for `message` in this language.
    pub fn tr(self, message: Tr) -> &'static str {
        message.text(self)
    }

    /// Parse a BCP-47 style tag such as `ja-JP`, `en-US` or `zh-Hans-CN`.
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
