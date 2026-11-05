//! UI copy. The single source is [`messages.toml`](./messages.toml): one template ID per
//! message with every language side by side. `build.rs` compiles it into the [`Tr`] enum
//! and the `fmt::*` formatting functions, so:
//!
//! * referencing an ID that does not exist is a compile error,
//! * a missing language or an empty value fails the build,
//! * placeholder drift between languages fails the build,
//! * all copy is embedded in the binary — no file I/O at runtime.
//!
//! Adding a language = a column in the TOML + one entry in build.rs's `LANGS`.
//! Call sites: `lang.tr(Tr::NavSettings)`; parameterized copy: `i18n::fmt::using_model(lang, name)`.

use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

mod generated {
    include!(concat!(env!("OUT_DIR"), "/i18n_gen.rs"));
}

pub use generated::Tr;

/// Formatting functions for templates with placeholders (`fmt.*` in messages.toml).
pub mod fmt {
    pub use super::generated::*;
}

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

const UNSET: u8 = u8::MAX;
static SYSTEM: AtomicU8 = AtomicU8::new(UNSET);

/// Tell the UI which language the operating system prefers (the platform layer calls this at startup).
pub fn set_system_language(lang: Lang) {
    SYSTEM.store(Lang::ALL.iter().position(|l| *l == lang).unwrap_or(1) as u8, Ordering::Relaxed);
}

/// The system language, English when it is unknown or unsupported.
pub fn system_language() -> Lang {
    match SYSTEM.load(Ordering::Relaxed) {
        UNSET => ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|k| std::env::var(k).ok())
            .find_map(|v| Lang::from_tag(&v))
            .unwrap_or(Lang::En),
        i => Lang::ALL[i as usize],
    }
}

/// A messy spoken sentence in `lang`, for trying out polishing.
pub fn sample_utterance(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "えーと 明日の3時に会議、いや4時に会議があるので、資料を、あの、事前に共有しておいてください",
        Lang::En => {
            "Um, we have a meeting at 3 tomorrow, no, make that 4, so please, uh, share the materials beforehand"
        }
        Lang::Zh => "嗯，明天三点开会，不对，是四点开会，所以请把资料，那个，提前共享一下",
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

    #[test]
    fn system_language_is_followed_until_the_user_picks_one() {
        use crate::config::{DisplayLanguage, General};
        set_system_language(Lang::En);
        let mut g = General::default();
        assert_eq!(g.lang(), Lang::En, "fresh config follows the system");
        set_system_language(Lang::Zh);
        assert_eq!(g.lang(), Lang::Zh);
        g.display_language = DisplayLanguage::Ja;
        assert_eq!(g.lang(), Lang::Ja, "an explicit choice wins over the system");
    }

    #[test]
    fn every_message_resolves_in_every_language() {
        for m in Tr::ALL {
            for lang in Lang::ALL {
                assert!(!lang.tr(m).is_empty(), "{} has an empty {}", m.id(), lang.code());
            }
        }
    }

    #[test]
    fn formatting_functions_interpolate_every_language() {
        for lang in Lang::ALL {
            assert!(fmt::using_model(lang, "Qwen").contains("Qwen"), "{:?} dropped the model", lang);
            assert!(fmt::about_seconds_left(lang, "9").contains('9'), "{:?} dropped n", lang);
        }
    }
}
