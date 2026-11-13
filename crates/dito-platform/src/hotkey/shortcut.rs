use dito_core::config::DEFAULT_SHORTCUT;
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

    /// This shortcut with Shift held as well. `None` when Shift is already part of it (a Shift key as the trigger, or
    /// a Shift co-modifier): the result would be the same shortcut, or one the machine can never match.
    pub fn with_shift(self) -> Option<Self> {
        match self {
            Self::Modifier { m, mods } if !m.is_shift() && !mods.shift => {
                Some(Self::Modifier { m, mods: Mods { shift: true, ..mods } })
            }
            Self::Combo { mods, key } if !mods.shift => Some(Self::Combo { mods: Mods { shift: true, ..mods }, key }),
            _ => None,
        }
    }

    /// Whether a press of this shortcut may turn out to be the first half of `to`: the same modifier key with more
    /// co-modifiers, so the extra ones can still go down right after it. Combos never hand over.
    pub fn may_hand_over_to(&self, to: &Self) -> bool {
        let (Self::Modifier { m: from_key, mods: from_mods }, Self::Modifier { m: to_key, mods: to_mods }) = (self, to)
        else {
            return false;
        };
        from_key == to_key && from_mods != to_mods && to_mods.contains(*from_mods)
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

/// The dictation shortcut a saved id stands for; an id that does not parse falls back to the default.
pub fn dictation_shortcut(id: &str) -> Shortcut {
    Shortcut::parse(id).unwrap_or_else(|| Shortcut::parse(DEFAULT_SHORTCUT).expect("the default shortcut parses"))
}

/// The shortcut that starts a command. An empty saved id is unset: it is the dictation shortcut plus Shift, so it
/// follows the dictation shortcut, and there is none when that already uses Shift. A saved id that does not parse, or
/// that is the dictation shortcut itself, leaves command mode off: one key must never start both.
pub fn command_shortcut(dictation: &str, command: &str) -> Option<Shortcut> {
    let dictation = dictation_shortcut(dictation);
    let command = if command.is_empty() { dictation.with_shift()? } else { Shortcut::parse(command)? };
    (command != dictation).then_some(command)
}

/// Whether a press of the dictation shortcut may turn out to be the first half of the command shortcut, which is what
/// the controller's hold on the capsule is for.
pub fn dictation_may_hand_over(dictation: &str, command: &str) -> bool {
    command_shortcut(dictation, command).is_some_and(|command| dictation_shortcut(dictation).may_hand_over_to(&command))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_shortcut_is_shift_plus_the_dictation_shortcut_until_one_is_saved() {
        let id = |dictation, command| command_shortcut(dictation, command).map(|s| s.id());
        assert_eq!(id("RightCommand", "").as_deref(), Some("Shift+RightCommand"));
        assert_eq!(id("Fn", "").as_deref(), Some("Shift+Fn"));
        assert_eq!(id("Control+RightOption", "").as_deref(), Some("Control+Shift+RightOption"));
        assert_eq!(id("Control+Option+Space", "").as_deref(), Some("Control+Option+Shift+Space"));
        assert_eq!(id("F5", "").as_deref(), Some("Shift+F5"));
        assert_eq!(id("Nope", "").as_deref(), Some("Shift+RightCommand"), "follows the dictation fallback");
        // Shift is already part of the dictation shortcut: nothing to derive.
        for dictation in ["RightShift", "Shift+Fn", "Shift+Command+A"] {
            assert_eq!(id(dictation, ""), None, "{dictation}");
        }
    }

    #[test]
    fn only_a_modifier_shortcut_with_the_same_key_and_more_modifiers_may_hand_over() {
        let may = |dictation, command| dictation_may_hand_over(dictation, command);
        assert!(may("RightCommand", ""), "the default Shift variant");
        assert!(may("RightCommand", "Shift+RightCommand"));
        assert!(may("Fn", "Control+Fn"));
        assert!(!may("RightCommand", "RightOption"), "another key");
        assert!(!may("Shift+Fn", ""), "no command shortcut");
        assert!(!may("Control+Option+Space", ""), "combos never hand over");
        assert!(!may("RightCommand", "Nope"));
    }

    #[test]
    fn a_saved_command_shortcut_stays_and_never_equals_the_dictation_one() {
        let id = |dictation, command| command_shortcut(dictation, command).map(|s| s.id());
        assert_eq!(id("RightCommand", "RightOption").as_deref(), Some("RightOption"));
        assert_eq!(id("Fn", "Shift+Fn").as_deref(), Some("Shift+Fn"), "a saved one does not follow");
        assert_eq!(id("RightOption", "RightOption"), None);
        assert_eq!(id("Shift+Command+A", "Command+Shift+A"), None, "id order does not matter");
        assert_eq!(id("RightCommand", "Nope"), None);
    }

    #[test]
    fn shift_variants_round_trip_and_keep_their_labels() {
        for id in ["Shift+RightCommand", "Shift+Fn", "Control+Shift+RightOption", "Shift+F5", "Control+Shift+Space"] {
            let parsed = Shortcut::parse(id).unwrap();
            assert_eq!(parsed.id(), id, "{id}");
            assert_eq!(Shortcut::parse(&parsed.id()), Some(parsed));
        }
        assert_eq!(Shortcut::parse("Shift+RightCommand").unwrap().label(Lang::En), "⇧ Right ⌘");
        assert_eq!(Shortcut::parse("Shift+F5").unwrap().label(Lang::En), "⇧ F5");
        assert!(Shortcut::parse("Shift+A").is_some(), "Shift plus a letter is a combo");
        assert!(Shortcut::parse("Shift+Space+RightOption").is_none(), "a trigger key and a modifier are exclusive");
    }

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
