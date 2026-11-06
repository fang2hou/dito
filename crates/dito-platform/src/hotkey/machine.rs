use std::time::{Duration, Instant};

use super::keys::{KEYCODE_ESCAPE, Modifier, Mods, is_standalone_key, no_mods_down};
use super::shortcut::Shortcut;

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
    use dito_core::i18n::Lang;

    use super::super::keys::{F_ALT, F_CMD, F_CTRL, F_SHIFT, L_CTRL, L_SHIFT, R_ALT};
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
        RawEvent::FlagsChanged { keycode: m.keycode(), flags: if down { m.event_flags() | extra } else { extra } }
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
}
