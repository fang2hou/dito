use dito_core::i18n::Lang;

use super::keys::{KEY_NAMES, Modifier, Mods, ansi_symbol, is_standalone_key, key_name, prints_character};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    Modifier { m: Modifier, mods: Mods },
    Combo { mods: Mods, key: u16 },
}

impl Shortcut {
    /// Stable id stored in the config, e.g. `RightOption`, `Control+RightOption`, `Control+Option+Space`.
    pub fn id(&self) -> String {
        let (mods, last) = match self {
            Self::Modifier { m, mods } => (mods, m.id().to_string()),
            Self::Combo { mods, key } => (mods, key_name(*key).map_or_else(|| format!("Key{key}"), str::to_string)),
        };
        mods.names().map(str::to_string).chain([last]).collect::<Vec<_>>().join("+")
    }

    pub fn parse(id: &str) -> Option<Self> {
        if let Some(m) = Modifier::from_id(id) {
            return Some(Self::Modifier { m, mods: Mods::default() });
        }
        let mut mods = Mods::default();
        let mut key = None;
        let mut modifier = None;
        for part in id.split('+') {
            if mods.add_named(part) {
                continue;
            }
            if let Some(m) = Modifier::from_id(part) {
                modifier = Some(m);
            } else {
                key = KEY_NAMES
                    .iter()
                    .find(|(n, _)| *n == part)
                    .map(|(_, c)| *c)
                    .or_else(|| part.strip_prefix("Key").and_then(|c| c.parse().ok()));
            }
        }
        match (modifier, key) {
            (None, Some(key)) => (!mods.is_empty() || is_standalone_key(key)).then_some(Self::Combo { mods, key }),
            (Some(m), None) => Some(Self::Modifier { m, mods }),
            _ => None,
        }
    }

    /// Character keys use their ANSI symbol.
    pub fn label(&self, lang: Lang) -> String {
        self.label_with(lang, |_| None)
    }

    /// Like [`label`](Self::label), but `layout_char` may supply the character a key prints on the real keyboard.
    pub fn label_with(&self, lang: Lang, layout_char: impl Fn(u16) -> Option<String>) -> String {
        match self {
            Self::Modifier { m, mods } => format!("{}{}", mods.label_prefix(), m.label(lang)),
            Self::Combo { mods, key } => {
                let printed = if prints_character(*key) { layout_char(*key) } else { None };
                let key_label = match (printed, key_name(*key)) {
                    (Some(c), _) => c.to_uppercase(),
                    (None, Some(n)) => ansi_symbol(n).unwrap_or(n).to_string(),
                    (None, None) => format!("Key{key}"),
                };
                format!("{}{key_label}", mods.label_prefix())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_reject_junk() {
        for id in [
            "RightOption",
            "Fn",
            "LeftCommand",
            "Control+RightOption",
            "Shift+Fn",
            "Control+Option+Space",
            "Shift+Command+A",
            "F5",
        ] {
            assert_eq!(Shortcut::parse(id).unwrap().id(), id, "{id}");
        }
        assert!(Shortcut::parse("Space").is_none(), "a bare letter/space cannot be a global shortcut");
        assert!(Shortcut::parse("Nope").is_none());
        assert!(Shortcut::parse("").is_none());
    }
}
