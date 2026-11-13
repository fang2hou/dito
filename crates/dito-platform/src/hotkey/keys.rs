use dito_core::i18n::{Lang, Tr};

// Device-dependent modifier bits (IOLLEvent.h): which physical key is down.
pub(super) const L_CTRL: u64 = 0x0000_0001;
pub(super) const L_SHIFT: u64 = 0x0000_0002;
pub(super) const R_SHIFT: u64 = 0x0000_0004;
pub(super) const L_CMD: u64 = 0x0000_0008;
pub(super) const R_CMD: u64 = 0x0000_0010;
pub(super) const L_ALT: u64 = 0x0000_0020;
pub(super) const R_ALT: u64 = 0x0000_0040;
pub(super) const R_CTRL: u64 = 0x0000_2000;
// Device-independent bits.
pub(super) const F_SHIFT: u64 = 0x0002_0000;
pub(super) const F_CTRL: u64 = 0x0004_0000;
pub(super) const F_ALT: u64 = 0x0008_0000;
pub(super) const F_CMD: u64 = 0x0010_0000;
pub(super) const F_FN: u64 = 0x0080_0000;

const MOD_BITS: u64 =
    L_CTRL | L_SHIFT | R_SHIFT | L_CMD | R_CMD | L_ALT | R_ALT | R_CTRL | F_SHIFT | F_CTRL | F_ALT | F_CMD | F_FN;

pub const KEYCODE_ESCAPE: u16 = 53;

pub(super) fn no_mods_down(flags: u64) -> bool {
    flags & MOD_BITS == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Group {
    Control,
    Option,
    Shift,
    Command,
}

impl Group {
    const DISPLAY_ORDER: [Group; 4] = [Self::Control, Self::Option, Self::Shift, Self::Command];

    fn name(self) -> &'static str {
        match self {
            Self::Control => "Control",
            Self::Option => "Option",
            Self::Shift => "Shift",
            Self::Command => "Command",
        }
    }

    fn symbol(self) -> char {
        match self {
            Self::Control => '⌃',
            Self::Option => '⌥',
            Self::Shift => '⇧',
            Self::Command => '⌘',
        }
    }

    fn flag(self) -> u64 {
        match self {
            Self::Control => F_CTRL,
            Self::Option => F_ALT,
            Self::Shift => F_SHIFT,
            Self::Command => F_CMD,
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::DISPLAY_ORDER.into_iter().find(|g| g.name() == name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
}

impl Side {
    fn tr(self) -> Tr {
        match self {
            Self::Left => Tr::GeneralLeft,
            Self::Right => Tr::GeneralRight,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    LeftShift,
    RightShift,
    LeftControl,
    RightControl,
    LeftOption,
    RightOption,
    LeftCommand,
    RightCommand,
    Fn,
}

struct ModifierSpec {
    modifier: Modifier,
    keycode: u16,
    flag: u64,
    id: &'static str,
    /// `None` for Fn, which has no side and no logical modifier.
    key: Option<(Group, Side)>,
}

const MODIFIER_SPECS: [ModifierSpec; 9] = [
    ModifierSpec {
        modifier: Modifier::LeftShift,
        keycode: 56,
        flag: L_SHIFT,
        id: "LeftShift",
        key: Some((Group::Shift, Side::Left)),
    },
    ModifierSpec {
        modifier: Modifier::RightShift,
        keycode: 60,
        flag: R_SHIFT,
        id: "RightShift",
        key: Some((Group::Shift, Side::Right)),
    },
    ModifierSpec {
        modifier: Modifier::LeftControl,
        keycode: 59,
        flag: L_CTRL,
        id: "LeftControl",
        key: Some((Group::Control, Side::Left)),
    },
    ModifierSpec {
        modifier: Modifier::RightControl,
        keycode: 62,
        flag: R_CTRL,
        id: "RightControl",
        key: Some((Group::Control, Side::Right)),
    },
    ModifierSpec {
        modifier: Modifier::LeftOption,
        keycode: 58,
        flag: L_ALT,
        id: "LeftOption",
        key: Some((Group::Option, Side::Left)),
    },
    ModifierSpec {
        modifier: Modifier::RightOption,
        keycode: 61,
        flag: R_ALT,
        id: "RightOption",
        key: Some((Group::Option, Side::Right)),
    },
    ModifierSpec {
        modifier: Modifier::LeftCommand,
        keycode: 55,
        flag: L_CMD,
        id: "LeftCommand",
        key: Some((Group::Command, Side::Left)),
    },
    ModifierSpec {
        modifier: Modifier::RightCommand,
        keycode: 54,
        flag: R_CMD,
        id: "RightCommand",
        key: Some((Group::Command, Side::Right)),
    },
    ModifierSpec { modifier: Modifier::Fn, keycode: 63, flag: F_FN, id: "Fn", key: None },
];

impl Modifier {
    fn spec(self) -> &'static ModifierSpec {
        MODIFIER_SPECS.iter().find(|s| s.modifier == self).expect("every modifier has a spec")
    }

    pub(super) fn from_id(id: &str) -> Option<Self> {
        MODIFIER_SPECS.iter().find(|s| s.id == id).map(|s| s.modifier)
    }

    pub fn keycode(self) -> u16 {
        self.spec().keycode
    }

    pub(super) fn from_keycode(keycode: u16) -> Option<Self> {
        MODIFIER_SPECS.iter().find(|s| s.keycode == keycode).map(|s| s.modifier)
    }

    pub(super) fn is_down(self, flags: u64) -> bool {
        flags & self.spec().flag != 0
    }

    pub(super) fn is_shift(self) -> bool {
        matches!(self.spec().key, Some((Group::Shift, _)))
    }

    pub fn id(self) -> &'static str {
        self.spec().id
    }

    pub(super) fn label(self, lang: Lang) -> String {
        let spec = self.spec();
        match spec.key {
            Some((group, side)) => format!("{} {}", lang.tr(side.tr()), group.symbol()),
            None => spec.id.to_string(),
        }
    }

    #[cfg(test)]
    pub(super) fn event_flags(self) -> u64 {
        let spec = self.spec();
        spec.flag | spec.key.map_or(0, |(group, _)| group.flag())
    }
}

/// Side-agnostic: either Shift counts as Shift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub shift: bool,
    pub control: bool,
    pub option: bool,
    pub command: bool,
}

impl Mods {
    pub fn from_flags(flags: u64) -> Self {
        let mut mods = Self::default();
        for group in Group::DISPLAY_ORDER {
            mods.set(group, flags & group.flag() != 0);
        }
        mods
    }

    pub(super) fn is_empty(self) -> bool {
        self == Self::default()
    }

    /// Whether every modifier of `other` is held here, possibly with more.
    pub(super) fn contains(self, other: Self) -> bool {
        Group::DISPLAY_ORDER.into_iter().all(|g| !other.has(g) || self.has(g))
    }

    fn has(self, group: Group) -> bool {
        match group {
            Group::Control => self.control,
            Group::Option => self.option,
            Group::Shift => self.shift,
            Group::Command => self.command,
        }
    }

    fn set(&mut self, group: Group, on: bool) {
        match group {
            Group::Control => self.control = on,
            Group::Option => self.option = on,
            Group::Shift => self.shift = on,
            Group::Command => self.command = on,
        }
    }

    fn held(self) -> impl Iterator<Item = Group> {
        Group::DISPLAY_ORDER.into_iter().filter(move |g| self.has(*g))
    }

    pub(super) fn without(mut self, m: Modifier) -> Self {
        if let Some((group, _)) = m.spec().key {
            self.set(group, false);
        }
        self
    }

    /// Accepts `Control`, `Option`, `Shift` and `Command`; false for any other name.
    pub(super) fn add_named(&mut self, name: &str) -> bool {
        let Some(group) = Group::from_name(name) else { return false };
        self.set(group, true);
        true
    }

    pub(super) fn names(self) -> impl Iterator<Item = &'static str> {
        self.held().map(Group::name)
    }

    /// The held modifiers as symbols followed by a space, or nothing when none is held.
    pub(super) fn label_prefix(self) -> String {
        let symbols: String = self.held().map(Group::symbol).collect();
        if symbols.is_empty() { symbols } else { format!("{symbols} ") }
    }
}

pub(super) const KEY_NAMES: &[(&str, u16)] = &[
    ("A", 0),
    ("S", 1),
    ("D", 2),
    ("F", 3),
    ("H", 4),
    ("G", 5),
    ("Z", 6),
    ("X", 7),
    ("C", 8),
    ("V", 9),
    ("B", 11),
    ("Q", 12),
    ("W", 13),
    ("E", 14),
    ("R", 15),
    ("Y", 16),
    ("T", 17),
    ("1", 18),
    ("2", 19),
    ("3", 20),
    ("4", 21),
    ("6", 22),
    ("5", 23),
    ("9", 25),
    ("7", 26),
    ("8", 28),
    ("0", 29),
    ("O", 31),
    ("U", 32),
    ("I", 34),
    ("P", 35),
    ("Return", 36),
    ("L", 37),
    ("J", 38),
    ("K", 40),
    ("N", 45),
    ("M", 46),
    ("Tab", 48),
    ("Space", 49),
    ("Delete", 51),
    ("Equal", 24),
    ("Minus", 27),
    ("RightBracket", 30),
    ("LeftBracket", 33),
    ("Quote", 39),
    ("Semicolon", 41),
    ("Backslash", 42),
    ("Comma", 43),
    ("Slash", 44),
    ("Period", 47),
    ("Grave", 50),
    ("Yen", 93),
    ("Underscore", 94),
    ("JisComma", 95),
    ("Eisu", 102),
    ("Kana", 104),
    ("Home", 115),
    ("PageUp", 116),
    ("ForwardDelete", 117),
    ("End", 119),
    ("PageDown", 121),
    ("Left", 123),
    ("Right", 124),
    ("Down", 125),
    ("Up", 126),
    ("F13", 105),
    ("F14", 107),
    ("F15", 113),
    ("F16", 106),
    ("F17", 64),
    ("F18", 79),
    ("F19", 80),
    ("F20", 90),
    ("F5", 96),
    ("F6", 97),
    ("F7", 98),
    ("F3", 99),
    ("F8", 100),
    ("F9", 101),
    ("F11", 103),
    ("F10", 109),
    ("F12", 111),
    ("F4", 118),
    ("F2", 120),
    ("F1", 122),
];

pub(super) fn key_name(code: u16) -> Option<&'static str> {
    KEY_NAMES.iter().find(|(_, c)| *c == code).map(|(n, _)| *n)
}

/// Keys that can be a global shortcut without any modifier: they do not type anything.
pub(super) fn is_standalone_key(code: u16) -> bool {
    key_name(code).is_some_and(|n| {
        n == "Eisu" || n == "Kana" || (n.starts_with('F') && n[1..].bytes().all(|b| b.is_ascii_digit()) && n.len() > 1)
    })
}

/// The platform layer replaces the ANSI character with the real layout's.
pub(super) fn ansi_symbol(name: &str) -> Option<&'static str> {
    Some(match name {
        "Equal" => "=",
        "Minus" => "-",
        "RightBracket" => "]",
        "LeftBracket" => "[",
        "Quote" => "'",
        "Semicolon" => ";",
        "Backslash" => "\\",
        "Comma" => ",",
        "Slash" => "/",
        "Period" => ".",
        "Grave" => "`",
        "Yen" => "¥",
        "Underscore" => "_",
        "JisComma" => ",",
        _ => return None,
    })
}

/// The label of a printing key depends on the keyboard layout.
pub(super) fn prints_character(code: u16) -> bool {
    key_name(code).is_some_and(|n| n.len() == 1 || ansi_symbol(n).is_some())
}
