//! Global shortcut logic, independent of the OS event source so it can be unit-tested.
//!
//! A shortcut is a modifier key on its own (right Option, Fn, ...), a modifier with other
//! modifiers held (⌃ + 右⌥), or a key with modifiers. The state machine consumes raw key
//! events and emits [`Action`]s.

use std::time::{Duration, Instant};

use dito_core::i18n::{Lang, Tr};

// Device-dependent modifier bits (IOLLEvent.h): which physical key is down.
const L_CTRL: u64 = 0x0000_0001;
const L_SHIFT: u64 = 0x0000_0002;
const R_SHIFT: u64 = 0x0000_0004;
const L_CMD: u64 = 0x0000_0008;
const R_CMD: u64 = 0x0000_0010;
const L_ALT: u64 = 0x0000_0020;
const R_ALT: u64 = 0x0000_0040;
const R_CTRL: u64 = 0x0000_2000;
// Device-independent bits.
const F_SHIFT: u64 = 0x0002_0000;
const F_CTRL: u64 = 0x0004_0000;
const F_ALT: u64 = 0x0008_0000;
const F_CMD: u64 = 0x0010_0000;
const F_FN: u64 = 0x0080_0000;

pub const KEYCODE_ESCAPE: u16 = 53;

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

impl Modifier {
    const ALL: [Modifier; 9] = [
        Self::LeftShift,
        Self::RightShift,
        Self::LeftControl,
        Self::RightControl,
        Self::LeftOption,
        Self::RightOption,
        Self::LeftCommand,
        Self::RightCommand,
        Self::Fn,
    ];

    pub fn keycode(self) -> u16 {
        match self {
            Self::LeftShift => 56,
            Self::RightShift => 60,
            Self::LeftControl => 59,
            Self::RightControl => 62,
            Self::LeftOption => 58,
            Self::RightOption => 61,
            Self::LeftCommand => 55,
            Self::RightCommand => 54,
            Self::Fn => 63,
        }
    }

    fn from_keycode(k: u16) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.keycode() == k)
    }

    /// Is this physical key down according to the event flags?
    fn is_down(self, flags: u64) -> bool {
        match self {
            Self::LeftShift => flags & L_SHIFT != 0,
            Self::RightShift => flags & R_SHIFT != 0,
            Self::LeftControl => flags & L_CTRL != 0,
            Self::RightControl => flags & R_CTRL != 0,
            Self::LeftOption => flags & L_ALT != 0,
            Self::RightOption => flags & R_ALT != 0,
            Self::LeftCommand => flags & L_CMD != 0,
            Self::RightCommand => flags & R_CMD != 0,
            Self::Fn => flags & F_FN != 0,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::LeftShift => "LeftShift",
            Self::RightShift => "RightShift",
            Self::LeftControl => "LeftControl",
            Self::RightControl => "RightControl",
            Self::LeftOption => "LeftOption",
            Self::RightOption => "RightOption",
            Self::LeftCommand => "LeftCommand",
            Self::RightCommand => "RightCommand",
            Self::Fn => "Fn",
        }
    }

    fn label(self, lang: Lang) -> String {
        let sided = |side: Tr, symbol: &str| format!("{} {symbol}", lang.tr(side));
        match self {
            Self::LeftShift => sided(Tr::GeneralLeft, "⇧"),
            Self::RightShift => sided(Tr::GeneralRight, "⇧"),
            Self::LeftControl => sided(Tr::GeneralLeft, "⌃"),
            Self::RightControl => sided(Tr::GeneralRight, "⌃"),
            Self::LeftOption => sided(Tr::GeneralLeft, "⌥"),
            Self::RightOption => sided(Tr::GeneralRight, "⌥"),
            Self::LeftCommand => sided(Tr::GeneralLeft, "⌘"),
            Self::RightCommand => sided(Tr::GeneralRight, "⌘"),
            Self::Fn => "Fn".to_string(),
        }
    }
}

/// Modifier set for key combos (side-agnostic).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub shift: bool,
    pub control: bool,
    pub option: bool,
    pub command: bool,
}

impl Mods {
    pub fn from_flags(flags: u64) -> Self {
        Self {
            shift: flags & F_SHIFT != 0,
            control: flags & F_CTRL != 0,
            option: flags & F_ALT != 0,
            command: flags & F_CMD != 0,
        }
    }

    fn is_empty(self) -> bool {
        self == Self::default()
    }

    /// The modifier set with `m`'s own (side-agnostic) contribution cleared: what is held *besides* `m`.
    fn without(self, m: Modifier) -> Self {
        let mut s = self;
        match m {
            Modifier::LeftControl | Modifier::RightControl => s.control = false,
            Modifier::LeftOption | Modifier::RightOption => s.option = false,
            Modifier::LeftShift | Modifier::RightShift => s.shift = false,
            Modifier::LeftCommand | Modifier::RightCommand => s.command = false,
            Modifier::Fn => {}
        }
        s
    }
}

/// Every modifier bit, device-dependent (which physical key) and independent (which logical modifier).
const MOD_BITS: u64 =
    L_CTRL | L_SHIFT | R_SHIFT | L_CMD | R_CMD | L_ALT | R_ALT | R_CTRL | F_SHIFT | F_CTRL | F_ALT | F_CMD | F_FN;

fn no_mods_down(flags: u64) -> bool {
    flags & MOD_BITS == 0
}

const KEY_NAMES: &[(&str, u16)] = &[
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

fn key_name(code: u16) -> Option<&'static str> {
    KEY_NAMES.iter().find(|(_, c)| *c == code).map(|(n, _)| *n)
}

/// Keys that can be a global shortcut without any modifier: they do not type anything.
fn is_standalone_key(code: u16) -> bool {
    key_name(code).is_some_and(|n| {
        n == "Eisu" || n == "Kana" || (n.starts_with('F') && n[1..].bytes().all(|b| b.is_ascii_digit()) && n.len() > 1)
    })
}

/// Character a punctuation key prints on an ANSI keyboard; the platform layer replaces it with the real layout's.
fn ansi_symbol(name: &str) -> Option<&'static str> {
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

/// Does this key print a character (so its label depends on the keyboard layout)?
fn prints_character(code: u16) -> bool {
    key_name(code).is_some_and(|n| n.len() == 1 || ansi_symbol(n).is_some())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    /// One modifier key, on its own or with other modifiers held (e.g. ⌃ + 右⌥).
    Modifier {
        m: Modifier,
        mods: Mods,
    },
    Combo {
        mods: Mods,
        key: u16,
    },
}

impl Shortcut {
    /// Stable id stored in the config, e.g. `RightOption`, `Control+RightOption`, `Control+Option+Space`.
    pub fn id(&self) -> String {
        fn mod_parts(mods: &Mods) -> Vec<&'static str> {
            let mut parts = Vec::new();
            if mods.control {
                parts.push("Control");
            }
            if mods.option {
                parts.push("Option");
            }
            if mods.shift {
                parts.push("Shift");
            }
            if mods.command {
                parts.push("Command");
            }
            parts
        }
        match self {
            Self::Modifier { m, mods } => {
                let mut parts = mod_parts(mods);
                parts.push(m.id());
                parts.join("+")
            }
            Self::Combo { mods, key } => {
                let key_id = key_name(*key).map_or_else(|| format!("Key{key}"), str::to_string);
                let mut parts: Vec<String> = mod_parts(mods).into_iter().map(String::from).collect();
                parts.push(key_id);
                parts.join("+")
            }
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        if let Some(m) = Modifier::ALL.into_iter().find(|m| m.id() == id) {
            return Some(Self::Modifier { m, mods: Mods::default() });
        }
        let mut mods = Mods::default();
        let mut key = None;
        let mut modifier = None;
        for part in id.split('+') {
            match part {
                "Control" => mods.control = true,
                "Option" => mods.option = true,
                "Shift" => mods.shift = true,
                "Command" => mods.command = true,
                name => {
                    if let Some(m) = Modifier::ALL.into_iter().find(|m| m.id() == name) {
                        modifier = Some(m);
                    } else {
                        key = KEY_NAMES
                            .iter()
                            .find(|(n, _)| *n == name)
                            .map(|(_, c)| *c)
                            .or_else(|| name.strip_prefix("Key").and_then(|c| c.parse().ok()));
                    }
                }
            }
        }
        match (modifier, key) {
            (None, Some(key)) => (!mods.is_empty() || is_standalone_key(key)).then_some(Self::Combo { mods, key }),
            (Some(m), None) => Some(Self::Modifier { m, mods }),
            _ => None,
        }
    }

    /// Short label for the shortcut field, e.g. `右 ⌥` or `⌃⌥ Space`. Character keys use their ANSI symbol.
    pub fn label(&self, lang: Lang) -> String {
        self.label_with(lang, |_| None)
    }

    /// Like [`label`](Self::label), but `layout_char` may supply the character a key prints on the real keyboard.
    pub fn label_with(&self, lang: Lang, layout_char: impl Fn(u16) -> Option<String>) -> String {
        match self {
            Self::Modifier { m, mods } => {
                let mut s = String::new();
                if mods.control {
                    s.push('⌃');
                }
                if mods.option {
                    s.push('⌥');
                }
                if mods.shift {
                    s.push('⇧');
                }
                if mods.command {
                    s.push('⌘');
                }
                if !s.is_empty() {
                    s.push(' ');
                }
                s.push_str(&m.label(lang));
                s
            }
            Self::Combo { mods, key } => {
                let mut s = String::new();
                if mods.control {
                    s.push('⌃');
                }
                if mods.option {
                    s.push('⌥');
                }
                if mods.shift {
                    s.push('⇧');
                }
                if mods.command {
                    s.push('⌘');
                }
                if !s.is_empty() {
                    s.push(' ');
                }
                let printed = if prints_character(*key) { layout_char(*key) } else { None };
                match (printed, key_name(*key)) {
                    (Some(c), _) => s.push_str(&c.to_uppercase()),
                    (None, Some(n)) => s.push_str(ansi_symbol(n).unwrap_or(n)),
                    (None, None) => s.push_str(&format!("Key{key}")),
                }
                s
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawEvent {
    FlagsChanged { keycode: u16, flags: u64 },
    KeyDown { keycode: u16, flags: u64, repeat: bool },
    KeyUp { keycode: u16, flags: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    ShortcutDown,
    ShortcutUp,
    /// Another key was pressed right after a lone modifier started a recording: it was a typing chord.
    ChordAbort,
    Escape,
    /// While capturing: the user pressed this shortcut.
    Captured(Shortcut),
    CaptureCancelled,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Output {
    pub actions: Vec<Action>,
    /// Do not deliver the event to other apps.
    pub swallow: bool,
}

/// A lone modifier followed by a key within this window is treated as a typing chord, not a dictation.
const CHORD_WINDOW: Duration = Duration::from_millis(400);

pub struct Machine {
    shortcut: Shortcut,
    held: bool,
    held_since: Option<Instant>,
    capturing: bool,
    capture_candidate: Option<(Modifier, Mods)>,
    capture_dirty: bool,
    /// Consume Esc (only while a dictation is on screen).
    pub swallow_escape: bool,
}

impl Machine {
    pub fn new(shortcut: Shortcut) -> Self {
        Self {
            shortcut,
            held: false,
            held_since: None,
            capturing: false,
            capture_candidate: None,
            capture_dirty: false,
            swallow_escape: false,
        }
    }

    pub fn set_shortcut(&mut self, s: Shortcut) {
        self.shortcut = s;
        self.held = false;
        self.held_since = None;
    }

    pub fn begin_capture(&mut self) {
        self.capturing = true;
        self.capture_candidate = None;
        self.capture_dirty = false;
    }

    pub fn cancel_capture(&mut self) {
        self.capturing = false;
        self.capture_candidate = None;
    }

    pub fn is_capturing(&self) -> bool {
        self.capturing
    }

    pub fn feed(&mut self, ev: RawEvent, now: Instant) -> Output {
        if self.capturing {
            return self.feed_capture(ev);
        }
        let mut out = Output::default();
        match ev {
            RawEvent::FlagsChanged { flags, .. } => {
                if let Shortcut::Modifier { m, mods } = self.shortcut {
                    // Active while the trigger key itself is down and exactly the co-modifiers are
                    // held: releasing the trigger or a co-modifier, or pressing an extra one, ends it.
                    let active = m.is_down(flags) && Mods::from_flags(flags).without(m) == mods;
                    if active && !self.held {
                        self.held = true;
                        self.held_since = Some(now);
                        out.actions.push(Action::ShortcutDown);
                    } else if !active && self.held {
                        self.held = false;
                        self.held_since = None;
                        out.actions.push(Action::ShortcutUp);
                    }
                }
            }
            RawEvent::KeyDown { keycode, flags, repeat } => {
                if keycode == KEYCODE_ESCAPE && Mods::from_flags(flags).is_empty() {
                    out.actions.push(Action::Escape);
                    out.swallow = self.swallow_escape;
                } else if let Shortcut::Combo { mods, key } = self.shortcut {
                    if keycode == key && Mods::from_flags(flags) == mods {
                        out.swallow = true;
                        if !repeat && !self.held {
                            self.held = true;
                            out.actions.push(Action::ShortcutDown);
                        }
                    }
                } else if self.held && self.held_since.is_some_and(|t| now.duration_since(t) < CHORD_WINDOW) {
                    // Option+E and friends: the modifier was part of typing.
                    self.held_since = None;
                    out.actions.push(Action::ChordAbort);
                }
            }
            RawEvent::KeyUp { keycode, .. } => {
                if let Shortcut::Combo { key, .. } = self.shortcut
                    && keycode == key
                    && self.held
                {
                    self.held = false;
                    out.swallow = true;
                    out.actions.push(Action::ShortcutUp);
                }
            }
        }
        out
    }

    fn feed_capture(&mut self, ev: RawEvent) -> Output {
        let mut out = Output { swallow: true, ..Default::default() };
        match ev {
            RawEvent::KeyDown { keycode, flags, repeat } => {
                if repeat {
                    return out;
                }
                self.capture_dirty = true;
                if keycode == KEYCODE_ESCAPE {
                    self.capturing = false;
                    out.actions.push(Action::CaptureCancelled);
                    return out;
                }
                let mods = Mods::from_flags(flags);
                if !mods.is_empty() || is_standalone_key(keycode) {
                    self.capturing = false;
                    out.actions.push(Action::Captured(Shortcut::Combo { mods, key: keycode }));
                }
            }
            RawEvent::FlagsChanged { keycode, flags } => {
                let Some(m) = Modifier::from_keycode(keycode) else { return out };
                if m.is_down(flags) {
                    // The last modifier pressed is the trigger; whatever else is held then are its co-modifiers.
                    self.capture_candidate = Some((m, Mods::from_flags(flags).without(m)));
                    self.capture_dirty = false;
                } else if let Some((candidate, mods)) = self.capture_candidate
                    && !self.capture_dirty
                    && no_mods_down(flags)
                {
                    // Capture once every modifier is released, so the press/release order of the
                    // chord does not decide which key becomes the trigger.
                    self.capturing = false;
                    self.capture_candidate = None;
                    out.actions.push(Action::Captured(Shortcut::Modifier { m: candidate, mods }));
                }
            }
            RawEvent::KeyUp { .. } => {}
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    fn lone(m: Modifier) -> Shortcut {
        Shortcut::Modifier { m, mods: Mods::default() }
    }

    fn flags_change(m: Modifier, down: bool) -> RawEvent {
        flags_change_with(m, down, 0)
    }

    /// `m` toggles while `extra` modifier flags stay held.
    fn flags_change_with(m: Modifier, down: bool, extra: u64) -> RawEvent {
        let bit = match m {
            Modifier::LeftShift => L_SHIFT | F_SHIFT,
            Modifier::RightShift => R_SHIFT | F_SHIFT,
            Modifier::LeftControl => L_CTRL | F_CTRL,
            Modifier::RightControl => R_CTRL | F_CTRL,
            Modifier::LeftOption => L_ALT | F_ALT,
            Modifier::RightOption => R_ALT | F_ALT,
            Modifier::LeftCommand => L_CMD | F_CMD,
            Modifier::RightCommand => R_CMD | F_CMD,
            Modifier::Fn => F_FN,
        };
        RawEvent::FlagsChanged { keycode: m.keycode(), flags: if down { bit | extra } else { extra } }
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

    #[test]
    fn labels_use_mac_symbols() {
        let right_option = Shortcut::parse("RightOption").unwrap();
        assert_eq!(right_option.label(Lang::Ja), "右 ⌥");
        assert_eq!(right_option.label(Lang::En), "Right ⌥");
        assert_eq!(Shortcut::parse("Control+Option+Space").unwrap().label(Lang::En), "⌃⌥ Space");
    }

    #[test]
    fn lone_modifier_emits_down_then_up_and_ignores_the_other_side() {
        let mut m = Machine::new(lone(Modifier::RightOption));
        let now = t0();
        assert_eq!(m.feed(flags_change(Modifier::LeftOption, true), now).actions, []);
        assert_eq!(m.feed(flags_change(Modifier::RightOption, true), now).actions, [Action::ShortcutDown]);
        assert_eq!(m.feed(flags_change(Modifier::RightOption, true), now).actions, [], "no duplicate down");
        assert_eq!(m.feed(flags_change(Modifier::RightOption, false), now).actions, [Action::ShortcutUp]);
    }

    #[test]
    fn a_key_right_after_the_modifier_aborts_as_a_typing_chord() {
        let mut m = Machine::new(lone(Modifier::RightOption));
        let now = t0();
        m.feed(flags_change(Modifier::RightOption, true), now);
        let key = RawEvent::KeyDown { keycode: 14, flags: R_ALT | F_ALT, repeat: false };
        assert_eq!(m.feed(key, now + Duration::from_millis(100)).actions, [Action::ChordAbort]);
        assert_eq!(m.feed(key, now + Duration::from_millis(150)).actions, [], "only once per press");
    }

    #[test]
    fn a_key_long_after_the_modifier_is_not_a_chord() {
        let mut m = Machine::new(lone(Modifier::RightOption));
        let now = t0();
        m.feed(flags_change(Modifier::RightOption, true), now);
        let key = RawEvent::KeyDown { keycode: 14, flags: R_ALT, repeat: false };
        assert_eq!(m.feed(key, now + Duration::from_secs(2)).actions, []);
    }

    #[test]
    fn combo_requires_exact_modifiers_swallows_and_ignores_repeats() {
        let mut m = Machine::new(Shortcut::parse("Control+Option+Space").unwrap());
        let now = t0();
        let down = |flags, repeat| RawEvent::KeyDown { keycode: 49, flags, repeat };
        assert_eq!(m.feed(down(F_CTRL, false), now), Output::default(), "missing Option");
        let out = m.feed(down(F_CTRL | F_ALT, false), now);
        assert_eq!((out.actions, out.swallow), (vec![Action::ShortcutDown], true));
        assert_eq!(m.feed(down(F_CTRL | F_ALT, true), now).actions, [], "auto-repeat");
        let up = m.feed(RawEvent::KeyUp { keycode: 49, flags: F_CTRL | F_ALT }, now);
        assert_eq!((up.actions, up.swallow), (vec![Action::ShortcutUp], true));
    }

    #[test]
    fn escape_is_reported_and_only_swallowed_when_asked() {
        let mut m = Machine::new(lone(Modifier::Fn));
        let esc = RawEvent::KeyDown { keycode: KEYCODE_ESCAPE, flags: 0, repeat: false };
        let o = m.feed(esc, t0());
        assert_eq!((o.actions, o.swallow), (vec![Action::Escape], false));
        m.swallow_escape = true;
        assert!(m.feed(esc, t0()).swallow);
        let with_cmd = RawEvent::KeyDown { keycode: KEYCODE_ESCAPE, flags: F_CMD, repeat: false };
        assert!(m.feed(with_cmd, t0()).actions.is_empty(), "Cmd+Esc is not ours");
    }

    #[test]
    fn capture_takes_a_lone_modifier_on_release() {
        let mut m = Machine::new(lone(Modifier::Fn));
        m.begin_capture();
        let now = t0();
        let o = m.feed(flags_change(Modifier::RightCommand, true), now);
        assert!(o.swallow && o.actions.is_empty());
        let o = m.feed(flags_change(Modifier::RightCommand, false), now);
        assert_eq!(o.actions, [Action::Captured(lone(Modifier::RightCommand))]);
        assert!(!m.is_capturing());
    }

    #[test]
    fn capture_takes_a_combo_and_not_a_modifier_that_was_part_of_it() {
        let mut m = Machine::new(lone(Modifier::Fn));
        m.begin_capture();
        let now = t0();
        m.feed(flags_change(Modifier::RightCommand, true), now);
        let o = m.feed(RawEvent::KeyDown { keycode: 49, flags: F_CMD, repeat: false }, now);
        assert_eq!(
            o.actions,
            [Action::Captured(Shortcut::Combo { mods: Mods { command: true, ..Default::default() }, key: 49 })]
        );
        // Releasing the modifier afterwards must not capture again.
        assert!(m.feed(flags_change(Modifier::RightCommand, false), now).actions.is_empty());
    }

    #[test]
    fn punctuation_and_unnamed_keys_can_be_captured_in_combos() {
        // JIS "@" is key code 33 (ANSI "["); 10 has no name at all (ISO section sign).
        let mut m = Machine::new(lone(Modifier::Fn));
        m.begin_capture();
        let o = m.feed(RawEvent::KeyDown { keycode: 33, flags: F_CTRL, repeat: false }, t0());
        let s = Shortcut::parse("Control+LeftBracket").unwrap();
        assert_eq!(o.actions, [Action::Captured(s)]);
        assert_eq!(s.label(Lang::Ja), "⌃ [");
        assert_eq!(s.label_with(Lang::Ja, |c| (c == 33).then(|| "@".to_string())), "⌃ @");

        m.begin_capture();
        let o = m.feed(RawEvent::KeyDown { keycode: 10, flags: F_CMD, repeat: false }, t0());
        let unnamed = Shortcut::Combo { mods: Mods { command: true, ..Default::default() }, key: 10 };
        assert_eq!(o.actions, [Action::Captured(unnamed)]);
        assert_eq!(unnamed.id(), "Command+Key10");
        assert_eq!(Shortcut::parse("Command+Key10"), Some(unnamed));
    }

    #[test]
    fn jis_mode_keys_work_alone_but_printing_keys_do_not() {
        assert_eq!(Shortcut::parse("Eisu").unwrap().id(), "Eisu");
        assert_eq!(Shortcut::parse("F13").unwrap().id(), "F13");
        assert!(Shortcut::parse("LeftBracket").is_none());
        assert!(Shortcut::parse("ForwardDelete").is_none(), "F-prefixed names are not function keys");
        let mut m = Machine::new(Shortcut::parse("Eisu").unwrap());
        let down = RawEvent::KeyDown { keycode: 102, flags: 0, repeat: false };
        assert_eq!(m.feed(down, t0()).actions, [Action::ShortcutDown]);
    }

    #[test]
    fn capture_rejects_bare_letters_and_esc_cancels() {
        let mut m = Machine::new(lone(Modifier::Fn));
        m.begin_capture();
        let a = RawEvent::KeyDown { keycode: 0, flags: 0, repeat: false };
        assert!(m.feed(a, t0()).actions.is_empty() && m.is_capturing());
        let f5 = RawEvent::KeyDown { keycode: 96, flags: 0, repeat: false };
        assert_eq!(m.feed(f5, t0()).actions, [Action::Captured(Shortcut::parse("F5").unwrap())]);
        m.begin_capture();
        let esc = RawEvent::KeyDown { keycode: KEYCODE_ESCAPE, flags: 0, repeat: false };
        assert_eq!(m.feed(esc, t0()).actions, [Action::CaptureCancelled]);
        assert!(!m.is_capturing());
    }

    #[test]
    fn capture_keeps_a_held_modifier_as_part_of_the_shortcut() {
        let mut m = Machine::new(lone(Modifier::Fn));
        m.begin_capture();
        let now = t0();
        m.feed(flags_change(Modifier::LeftControl, true), now);
        m.feed(flags_change_with(Modifier::RightOption, true, L_CTRL | F_CTRL), now);
        // Releasing the trigger while Control is still down must not capture a lone 右⌥ yet.
        let o = m.feed(flags_change_with(Modifier::RightOption, false, L_CTRL | F_CTRL), now);
        assert_eq!(o.actions, []);
        assert!(m.is_capturing());
        let o = m.feed(flags_change(Modifier::LeftControl, false), now);
        let s = Shortcut::Modifier { m: Modifier::RightOption, mods: Mods { control: true, ..Default::default() } };
        assert_eq!(o.actions, [Action::Captured(s)]);
        assert!(!m.is_capturing());
        assert_eq!(s.id(), "Control+RightOption");
        assert_eq!(Shortcut::parse("Control+RightOption"), Some(s));
        assert_eq!(s.label(Lang::Ja), "⌃ 右 ⌥");
    }

    #[test]
    fn capture_is_independent_of_the_release_order() {
        let mut m = Machine::new(lone(Modifier::Fn));
        m.begin_capture();
        let now = t0();
        m.feed(flags_change(Modifier::LeftControl, true), now);
        m.feed(flags_change_with(Modifier::RightOption, true, L_CTRL | F_CTRL), now);
        // Releasing the co-modifier first must not capture Control as a lone shortcut.
        let o = m.feed(flags_change_with(Modifier::LeftControl, false, R_ALT | F_ALT), now);
        assert_eq!(o.actions, []);
        assert!(m.is_capturing());
        let o = m.feed(flags_change(Modifier::RightOption, false), now);
        assert_eq!(
            o.actions,
            [Action::Captured(Shortcut::Modifier {
                m: Modifier::RightOption,
                mods: Mods { control: true, ..Default::default() }
            })]
        );
    }

    #[test]
    fn modifier_shortcut_with_co_modifiers_needs_all_of_them() {
        let mut m = Machine::new(Shortcut::parse("Control+RightOption").unwrap());
        let now = t0();
        assert_eq!(m.feed(flags_change(Modifier::RightOption, true), now).actions, [], "co-modifier missing");
        assert_eq!(
            m.feed(flags_change_with(Modifier::LeftControl, true, R_ALT | F_ALT), now).actions,
            [Action::ShortcutDown]
        );
        // Releasing the co-modifier first ends it even though the trigger is still held.
        assert_eq!(
            m.feed(flags_change_with(Modifier::LeftControl, false, R_ALT | F_ALT), now).actions,
            [Action::ShortcutUp]
        );
        // An extra modifier disqualifies the chord.
        assert_eq!(
            m.feed(flags_change_with(Modifier::LeftControl, true, R_ALT | F_ALT | L_SHIFT | F_SHIFT), now).actions,
            []
        );
        assert_eq!(m.feed(flags_change_with(Modifier::LeftControl, false, R_ALT | F_ALT), now).actions, []);
        // The chord completes again once exactly the co-modifier is held.
        assert_eq!(
            m.feed(flags_change_with(Modifier::LeftControl, true, R_ALT | F_ALT), now).actions,
            [Action::ShortcutDown]
        );
        assert_eq!(m.feed(flags_change(Modifier::RightOption, false), now).actions, [Action::ShortcutUp]);
    }

    #[test]
    fn lone_modifier_does_not_fire_while_other_modifiers_are_held() {
        let mut m = Machine::new(lone(Modifier::RightOption));
        let now = t0();
        let with_ctrl = flags_change_with(Modifier::RightOption, true, L_CTRL | F_CTRL);
        assert_eq!(m.feed(with_ctrl, now).actions, [], "control held: not a lone press");
        m.feed(flags_change(Modifier::LeftControl, false), now);
        assert_eq!(m.feed(flags_change(Modifier::RightOption, true), now).actions, [Action::ShortcutDown]);
    }
}
