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
    /// The command shortcut, with the same hold semantics as the main one.
    CommandDown,
    CommandUp,
    /// The keys the command shortcut adds to the dictation shortcut went down within the chord window: the press that
    /// started a dictation was the first half of the command shortcut. It is one press, not a second one, and ends
    /// with [`Action::CommandUp`].
    ShortcutToCommand,
    /// The same, from the command shortcut to the dictation shortcut, when the dictation shortcut is the command
    /// shortcut plus modifiers. It ends with [`Action::ShortcutUp`].
    CommandToShortcut,
    /// The Tools shortcut fired once. It has no up event: it opens a window, it does not hold anything.
    ToolsPressed,
    /// Another key was pressed right after a lone modifier went down: it was a typing chord, not a press.
    ChordAbort,
    Escape,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Main,
    Command,
}

const SLOTS: [Slot; 2] = [Slot::Main, Slot::Command];

impl Slot {
    fn down(self) -> Action {
        match self {
            Self::Main => Action::ShortcutDown,
            Self::Command => Action::CommandDown,
        }
    }

    fn up(self) -> Action {
        match self {
            Self::Main => Action::ShortcutUp,
            Self::Command => Action::CommandUp,
        }
    }

    fn other(self) -> Self {
        match self {
            Self::Main => Self::Command,
            Self::Command => Self::Main,
        }
    }

    /// What the press became when it was handed over to this shortcut.
    fn handed_over(self) -> Action {
        match self {
            Self::Main => Action::CommandToShortcut,
            Self::Command => Action::ShortcutToCommand,
        }
    }
}

#[derive(Debug)]
struct Trigger {
    shortcut: Shortcut,
    held: bool,
    held_since: Option<Instant>,
}

impl Trigger {
    fn new(shortcut: Shortcut) -> Self {
        Self { shortcut, held: false, held_since: None }
    }

    fn release(&mut self) {
        self.held = false;
        self.held_since = None;
    }
}

/// The Tools shortcut opens a window, so it fires once and holds nothing. A combo fires on its key down; a lone
/// modifier fires when it is released as a tap.
#[derive(Debug)]
struct ToolsTrigger {
    shortcut: Shortcut,
    /// A combo key is down: its repeats and its key up are ours too.
    held: bool,
    /// A modifier press is still a tap: nothing has been typed during it.
    armed: bool,
}

impl ToolsTrigger {
    fn new(shortcut: Shortcut) -> Self {
        Self { shortcut, held: false, armed: false }
    }

    fn release(&mut self) {
        self.held = false;
        self.armed = false;
    }
}

pub struct Machine {
    main: Trigger,
    command: Option<Trigger>,
    tools: Option<ToolsTrigger>,
    capturing: bool,
    capture_candidate: Option<(Modifier, Mods)>,
    capture_dirty: bool,
    /// Set only while a dictation is on screen.
    pub swallow_escape: bool,
    /// Set while a text field of the HUD holds composing text: Esc then belongs to the input method, which cancels the
    /// composition, so it is neither reported nor swallowed.
    pub escape_to_ime: bool,
}

impl Machine {
    pub fn new(shortcut: Shortcut) -> Self {
        Self {
            main: Trigger::new(shortcut),
            command: None,
            tools: None,
            capturing: false,
            capture_candidate: None,
            capture_dirty: false,
            swallow_escape: false,
            escape_to_ime: false,
        }
    }

    pub fn set_shortcut(&mut self, s: Shortcut) {
        self.main = Trigger::new(s);
        if let Some(command) = self.command.as_mut().filter(|c| c.shortcut == s) {
            command.release();
        }
        if let Some(tools) = self.tools.as_mut().filter(|t| t.shortcut == s) {
            tools.release();
        }
    }

    /// `None` turns the command shortcut off. The same shortcut as the main one is ignored: the main one wins.
    pub fn set_command_shortcut(&mut self, s: Option<Shortcut>) {
        self.command = s.map(Trigger::new);
        if let Some(tools) = self.tools.as_mut().filter(|t| Some(t.shortcut) == s) {
            tools.release();
        }
    }

    /// `None` turns the Tools shortcut off. It fires on its own, so it never touches the dictation or command press.
    pub fn set_tools_shortcut(&mut self, s: Option<Shortcut>) {
        self.tools = s.map(ToolsTrigger::new);
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

    fn command_is_distinct(&self) -> bool {
        self.command.as_ref().is_some_and(|c| c.shortcut != self.main.shortcut)
    }

    /// The Tools shortcut is inert while it is the dictation or the command shortcut's key: those win.
    fn tools_mut(&mut self) -> Option<&mut ToolsTrigger> {
        let shortcut = self.tools.as_ref()?.shortcut;
        let taken = shortcut == self.main.shortcut || self.command.as_ref().is_some_and(|c| c.shortcut == shortcut);
        if taken {
            return None;
        }
        self.tools.as_mut()
    }

    fn trigger_mut(&mut self, slot: Slot) -> Option<&mut Trigger> {
        match slot {
            Slot::Main => Some(&mut self.main),
            Slot::Command if self.command_is_distinct() => self.command.as_mut(),
            Slot::Command => None,
        }
    }

    fn trigger(&self, slot: Slot) -> Option<&Trigger> {
        match slot {
            Slot::Main => Some(&self.main),
            Slot::Command if self.command_is_distinct() => self.command.as_ref(),
            Slot::Command => None,
        }
    }

    /// Whether the press that `from` is holding may become a press of `to`: `to` is `from` plus more modifiers, and
    /// `from` went down only a moment ago, so the extra modifiers were the second half of one chord.
    fn can_hand_over(&self, from: Slot, to: Slot, now: Instant) -> bool {
        let (Some(a), Some(b)) = (self.trigger(from), self.trigger(to)) else { return false };
        a.held
            && a.shortcut.may_hand_over_to(&b.shortcut)
            && a.held_since.is_some_and(|since| now.duration_since(since) < CHORD_WINDOW)
    }

    /// The two shortcuts never fire together: a held one blocks the other from starting.
    fn other_held(&self, slot: Slot) -> bool {
        match slot {
            Slot::Main => self.command_is_distinct() && self.command.as_ref().is_some_and(|c| c.held),
            Slot::Command => self.main.held,
        }
    }

    pub fn feed(&mut self, ev: RawEvent, now: Instant) -> Output {
        if self.capturing {
            return self.feed_capture(ev);
        }
        let mut out = Output::default();
        match ev {
            RawEvent::FlagsChanged { keycode, flags } => self.feed_flags(keycode, flags, now, &mut out),
            RawEvent::KeyDown { keycode, flags, repeat } => {
                if keycode == KEYCODE_ESCAPE && Mods::from_flags(flags).is_empty() {
                    if !self.escape_to_ime {
                        out.actions.push(Action::Escape);
                        out.swallow = self.swallow_escape;
                    }
                } else {
                    self.feed_key_down(keycode, flags, repeat, now, &mut out);
                }
            }
            RawEvent::KeyUp { keycode, .. } => {
                for slot in SLOTS {
                    if let Some(t) = self.trigger_mut(slot)
                        && let Shortcut::Combo { key, .. } = t.shortcut
                        && keycode == key
                        && t.held
                    {
                        t.release();
                        out.swallow = true;
                        out.actions.push(slot.up());
                    }
                }
                // The Tools combo fired on its way down; its key up only stops the repeats.
                if let Some(t) = self.tools_mut()
                    && let Shortcut::Combo { key, .. } = t.shortcut
                    && keycode == key
                    && t.held
                {
                    t.held = false;
                    out.swallow = true;
                }
            }
        }
        out
    }

    fn feed_flags(&mut self, keycode: u16, flags: u64, now: Instant, out: &mut Output) {
        let held = Mods::from_flags(flags);
        // A trigger starts when its key is down with exactly its co-modifiers, and lasts while its key and those
        // co-modifiers stay down. A modifier added meanwhile does not end it: Shift pressed late next to a held right
        // ⌘ is ignored, and releasing right ⌘ still ends the press.
        let starts = |t: &Trigger| match t.shortcut {
            Shortcut::Modifier { m, mods } => m.is_down(flags) && held.without(m) == mods,
            Shortcut::Combo { .. } => false,
        };
        let lasts = |t: &Trigger| match t.shortcut {
            Shortcut::Modifier { m, mods } => m.is_down(flags) && held.without(m).contains(mods),
            Shortcut::Combo { .. } => true,
        };
        for slot in SLOTS {
            if let Some(t) = self.trigger_mut(slot)
                && t.held
                && !lasts(t)
            {
                t.release();
                out.actions.push(slot.up());
            }
        }
        // A Tools modifier press fires when it ends, and only if nothing was typed meanwhile: it has no down event and
        // never swallows, so the modifier keeps working as one.
        let tools_last = |t: &ToolsTrigger| match t.shortcut {
            Shortcut::Modifier { m, mods } => m.is_down(flags) && held.without(m).contains(mods),
            Shortcut::Combo { .. } => true,
        };
        if let Some(t) = self.tools_mut()
            && t.armed
            && !tools_last(t)
        {
            t.armed = false;
            out.actions.push(Action::ToolsPressed);
        }
        // Only a key press starts a trigger. Lifting Shift from Shift+Right ⌘ while Right ⌘ stays down must not start
        // the plain Right ⌘ shortcut.
        if !Modifier::from_keycode(keycode).is_some_and(|m| m.is_down(flags)) {
            return;
        }
        let tools_starts = |t: &ToolsTrigger| match t.shortcut {
            Shortcut::Modifier { m, mods } => m.is_down(flags) && held.without(m) == mods,
            Shortcut::Combo { .. } => false,
        };
        if let Some(t) = self.tools_mut()
            && !t.armed
            && tools_starts(t)
        {
            t.armed = true;
        }
        for slot in SLOTS {
            let blocked = self.other_held(slot);
            let hand_over = blocked && self.can_hand_over(slot.other(), slot, now);
            let Some(t) = self.trigger_mut(slot) else { continue };
            if t.held || !starts(t) || (blocked && !hand_over) {
                continue;
            }
            t.held = true;
            t.held_since = Some(now);
            if hand_over {
                // Right ⌘ pressed a moment before Shift was the first half of Shift+Right ⌘: one press, carried over.
                if let Some(from) = self.trigger_mut(slot.other()) {
                    from.release();
                }
                out.actions.push(slot.handed_over());
            } else {
                out.actions.push(slot.down());
            }
        }
    }

    fn feed_key_down(&mut self, keycode: u16, flags: u64, repeat: bool, now: Instant, out: &mut Output) {
        // A key pressed during a Tools modifier press makes it typing, not a tap.
        if let Some(t) = self.tools_mut() {
            t.armed = false;
        }
        let mut matched = false;
        for slot in SLOTS {
            let blocked = self.other_held(slot);
            if let Some(t) = self.trigger_mut(slot)
                && let Shortcut::Combo { mods, key } = t.shortcut
                && keycode == key
                && Mods::from_flags(flags) == mods
            {
                matched = true;
                out.swallow = true;
                if !repeat && !t.held && !blocked {
                    t.held = true;
                    out.actions.push(slot.down());
                }
            }
        }
        if matched {
            return;
        }
        // A combo fires on its key down and stops there: repeats are swallowed without a second press, and the key
        // stays ours until it comes up.
        if let Some(t) = self.tools_mut()
            && let Shortcut::Combo { mods, key } = t.shortcut
            && keycode == key
            && Mods::from_flags(flags) == mods
        {
            out.swallow = true;
            if !repeat && !t.held {
                t.held = true;
                out.actions.push(Action::ToolsPressed);
            }
            return;
        }
        for slot in SLOTS {
            if let Some(t) = self.trigger_mut(slot)
                && t.held
                && t.held_since.is_some_and(|since| now.duration_since(since) < CHORD_WINDOW)
            {
                // Option+E and friends: the modifier was part of typing.
                t.held_since = None;
                out.actions.push(Action::ChordAbort);
                break;
            }
        }
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

    use super::super::keys::{F_ALT, F_CMD, F_CTRL, F_SHIFT, L_ALT, L_CTRL, L_SHIFT, R_ALT, R_CMD};
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
    fn escape_is_left_to_the_input_method_while_composing() {
        let mut m = Machine::new(lone(Modifier::Fn));
        m.swallow_escape = true;
        m.escape_to_ime = true;
        let o = m.feed(RawEvent::KeyDown { keycode: KEYCODE_ESCAPE, flags: 0, repeat: false }, t0());
        assert_eq!((o.actions, o.swallow), (vec![], false));
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
        assert_eq!(
            m.feed(flags_change_with(Modifier::LeftControl, false, R_ALT | F_ALT), now).actions,
            [Action::ShortcutUp]
        );
        assert_eq!(
            m.feed(flags_change_with(Modifier::LeftControl, true, R_ALT | F_ALT | L_SHIFT | F_SHIFT), now).actions,
            []
        );
        assert_eq!(m.feed(flags_change_with(Modifier::LeftControl, false, R_ALT | F_ALT), now).actions, []);
        assert_eq!(
            m.feed(flags_change_with(Modifier::LeftControl, true, R_ALT | F_ALT), now).actions,
            [Action::ShortcutDown]
        );
        assert_eq!(m.feed(flags_change(Modifier::RightOption, false), now).actions, [Action::ShortcutUp]);
    }

    fn two(main: &str, command: &str) -> Machine {
        let mut m = Machine::new(Shortcut::parse(main).unwrap());
        m.set_command_shortcut(Some(Shortcut::parse(command).unwrap()));
        m
    }

    #[test]
    fn command_shortcut_emits_its_own_down_and_up() {
        let mut m = two("RightOption", "RightCommand");
        let now = t0();
        assert_eq!(m.feed(flags_change(Modifier::RightCommand, true), now).actions, [Action::CommandDown]);
        assert_eq!(m.feed(flags_change(Modifier::RightCommand, true), now).actions, [], "no duplicate down");
        assert_eq!(m.feed(flags_change(Modifier::RightCommand, false), now).actions, [Action::CommandUp]);
        assert_eq!(m.feed(flags_change(Modifier::RightOption, true), now).actions, [Action::ShortcutDown]);
        assert_eq!(m.feed(flags_change(Modifier::RightOption, false), now).actions, [Action::ShortcutUp]);
    }

    #[test]
    fn no_command_shortcut_means_no_command_actions() {
        let mut m = Machine::new(lone(Modifier::RightOption));
        m.set_command_shortcut(Some(lone(Modifier::RightCommand)));
        m.set_command_shortcut(None);
        assert_eq!(m.feed(flags_change(Modifier::RightCommand, true), t0()).actions, []);
    }

    #[test]
    fn the_same_shortcut_for_both_goes_to_the_main_one() {
        let mut m = two("RightOption", "RightOption");
        let now = t0();
        assert_eq!(m.feed(flags_change(Modifier::RightOption, true), now).actions, [Action::ShortcutDown]);
        assert_eq!(m.feed(flags_change(Modifier::RightOption, false), now).actions, [Action::ShortcutUp]);

        let mut m = two("Control+Option+Space", "Control+Option+Space");
        let down = RawEvent::KeyDown { keycode: 49, flags: F_CTRL | F_ALT, repeat: false };
        assert_eq!(m.feed(down, now).actions, [Action::ShortcutDown]);
        let up = RawEvent::KeyUp { keycode: 49, flags: F_CTRL | F_ALT };
        assert_eq!(m.feed(up, now).actions, [Action::ShortcutUp]);
    }

    #[test]
    fn making_the_main_shortcut_equal_to_the_command_one_releases_the_command_one() {
        let mut m = two("RightOption", "RightCommand");
        let now = t0();
        m.feed(flags_change(Modifier::RightCommand, true), now);
        m.set_shortcut(lone(Modifier::RightCommand));
        assert_eq!(m.feed(flags_change(Modifier::RightCommand, false), now).actions, []);
        assert_eq!(m.feed(flags_change(Modifier::RightCommand, true), now).actions, [Action::ShortcutDown]);
    }

    #[test]
    fn a_held_shortcut_blocks_the_other_one() {
        let mut m = two("RightOption", "LeftOption");
        let now = t0();
        assert_eq!(m.feed(flags_change(Modifier::RightOption, true), now).actions, [Action::ShortcutDown]);
        let both = flags_change_with(Modifier::LeftOption, true, R_ALT | F_ALT);
        assert_eq!(m.feed(both, now).actions, [], "the command shortcut cannot start while the main one is held");

        let mut m = two("Control+Option+Space", "Control+Option+Return");
        let down = |key| RawEvent::KeyDown { keycode: key, flags: F_CTRL | F_ALT, repeat: false };
        assert_eq!(m.feed(down(36), now).actions, [Action::CommandDown]);
        let out = m.feed(down(49), now);
        assert_eq!((out.actions, out.swallow), (vec![], true), "blocked, but the key is still ours");
        let up = m.feed(RawEvent::KeyUp { keycode: 36, flags: F_CTRL | F_ALT }, now);
        assert_eq!((up.actions, up.swallow), (vec![Action::CommandUp], true));
        assert_eq!(m.feed(down(49), now).actions, [Action::ShortcutDown]);
    }

    #[test]
    fn a_modifier_added_after_the_chord_window_is_ignored_by_a_held_shortcut() {
        let mut m = two("RightOption", "Control+RightOption");
        let now = t0();
        assert_eq!(m.feed(flags_change(Modifier::RightOption, true), now).actions, [Action::ShortcutDown]);
        let with_control = flags_change_with(Modifier::LeftControl, true, R_ALT | F_ALT);
        let late = now + Duration::from_millis(500);
        assert_eq!(m.feed(with_control, late).actions, [], "no handover, no second press");
        // Lifting the late modifier changes nothing, and the trigger key still ends the press.
        assert_eq!(m.feed(flags_change_with(Modifier::LeftControl, false, R_ALT | F_ALT), late).actions, []);
        assert_eq!(m.feed(flags_change(Modifier::RightOption, false), late).actions, [Action::ShortcutUp]);

        // Releasing the trigger key first ends the press too, and the modifier still held starts nothing.
        m.feed(flags_change(Modifier::RightOption, true), now);
        m.feed(with_control, late);
        let trigger_up = flags_change_with(Modifier::RightOption, false, L_CTRL | F_CTRL);
        assert_eq!(m.feed(trigger_up, late).actions, [Action::ShortcutUp]);
        assert_eq!(m.feed(flags_change(Modifier::LeftControl, false), late).actions, []);
    }

    #[test]
    fn a_modifier_added_right_after_the_main_one_hands_its_press_over_to_the_command_shortcut() {
        let mut m = two("RightOption", "Control+RightOption");
        let now = t0();
        m.feed(flags_change(Modifier::RightOption, true), now);
        let with_control = flags_change_with(Modifier::LeftControl, true, R_ALT | F_ALT);
        let quick = now + Duration::from_millis(100);
        assert_eq!(m.feed(with_control, quick).actions, [Action::ShortcutToCommand]);
        assert_eq!(m.feed(flags_change(Modifier::RightOption, false), quick).actions, [Action::CommandUp]);

        // A quick release alone is still a plain release.
        m.feed(flags_change(Modifier::RightOption, true), now);
        assert_eq!(m.feed(flags_change(Modifier::RightOption, false), quick).actions, [Action::ShortcutUp]);
    }

    /// Dictation on right ⌘, the default command shortcut on Shift plus the same key.
    fn shift_variant() -> Machine {
        two("RightCommand", "Shift+RightCommand")
    }

    #[test]
    fn shift_plus_a_lone_modifier_starts_the_command_whichever_key_goes_down_first() {
        let shift_down = flags_change(Modifier::LeftShift, true);
        let with_shift = |m, down| flags_change_with(m, down, L_SHIFT | F_SHIFT);
        let now = t0();

        let mut m = shift_variant();
        assert_eq!(m.feed(shift_down, now).actions, []);
        assert_eq!(m.feed(with_shift(Modifier::RightCommand, true), now).actions, [Action::CommandDown]);
        assert_eq!(m.feed(with_shift(Modifier::RightCommand, false), now).actions, [Action::CommandUp]);
        assert_eq!(m.feed(flags_change(Modifier::LeftShift, false), now).actions, []);

        // Right ⌘ first, Shift a moment later: the press the dictation shortcut took is handed over, with no abort.
        let quick = now + Duration::from_millis(100);
        assert_eq!(m.feed(flags_change(Modifier::RightCommand, true), now).actions, [Action::ShortcutDown]);
        let shift_after = flags_change_with(Modifier::LeftShift, true, R_CMD | F_CMD);
        assert_eq!(m.feed(shift_after, quick).actions, [Action::ShortcutToCommand]);
        assert_eq!(m.feed(flags_change(Modifier::RightCommand, false), quick).actions, [Action::CommandUp]);
    }

    #[test]
    fn a_handed_over_press_ends_the_same_way_whichever_key_is_released_first() {
        let now = t0();
        let quick = now + Duration::from_millis(100);
        let shift_after = flags_change_with(Modifier::LeftShift, true, R_CMD | F_CMD);

        let mut m = shift_variant();
        m.feed(flags_change(Modifier::RightCommand, true), now);
        assert_eq!(m.feed(shift_after, quick).actions, [Action::ShortcutToCommand]);
        let lifted = flags_change_with(Modifier::LeftShift, false, R_CMD | F_CMD);
        assert_eq!(m.feed(lifted, quick).actions, [Action::CommandUp], "Shift first");
        assert_eq!(m.feed(flags_change(Modifier::RightCommand, false), quick).actions, []);

        m.feed(flags_change(Modifier::RightCommand, true), now);
        m.feed(shift_after, quick);
        let cmd_up = flags_change_with(Modifier::RightCommand, false, L_SHIFT | F_SHIFT);
        assert_eq!(m.feed(cmd_up, quick).actions, [Action::CommandUp], "right ⌘ first");
        assert_eq!(m.feed(flags_change(Modifier::LeftShift, false), quick).actions, []);
    }

    #[test]
    fn shift_added_after_the_chord_window_does_not_touch_the_dictation_press() {
        let now = t0();
        let late = now + Duration::from_millis(600);
        let mut m = shift_variant();
        assert_eq!(m.feed(flags_change(Modifier::RightCommand, true), now).actions, [Action::ShortcutDown]);
        let shift_after = flags_change_with(Modifier::LeftShift, true, R_CMD | F_CMD);
        assert_eq!(m.feed(shift_after, late).actions, []);
        assert_eq!(m.feed(flags_change_with(Modifier::LeftShift, false, R_CMD | F_CMD), late).actions, []);
        assert_eq!(m.feed(flags_change(Modifier::RightCommand, false), late).actions, [Action::ShortcutUp]);
    }

    #[test]
    fn the_command_shortcut_hands_over_to_a_dictation_shortcut_that_adds_modifiers() {
        let mut m = two("Control+RightOption", "RightOption");
        let now = t0();
        assert_eq!(m.feed(flags_change(Modifier::RightOption, true), now).actions, [Action::CommandDown]);
        let with_control = flags_change_with(Modifier::LeftControl, true, R_ALT | F_ALT);
        assert_eq!(m.feed(with_control, now + Duration::from_millis(100)).actions, [Action::CommandToShortcut]);
        assert_eq!(m.feed(flags_change(Modifier::RightOption, false), now).actions, [Action::ShortcutUp]);
    }

    #[test]
    fn a_key_after_a_handed_over_press_still_counts_as_typing_within_the_window() {
        let now = t0();
        let mut m = shift_variant();
        m.feed(flags_change(Modifier::RightCommand, true), now);
        let shift_after = flags_change_with(Modifier::LeftShift, true, R_CMD | F_CMD);
        m.feed(shift_after, now + Duration::from_millis(100));
        let key = RawEvent::KeyDown { keycode: 14, flags: R_CMD | F_CMD | L_SHIFT | F_SHIFT, repeat: false };
        assert_eq!(m.feed(key, now + Duration::from_millis(200)).actions, [Action::ChordAbort]);
    }

    #[test]
    fn lifting_shift_first_ends_the_command_without_starting_a_dictation() {
        let now = t0();
        let mut m = shift_variant();
        m.feed(flags_change(Modifier::LeftShift, true), now);
        let with_shift = flags_change_with(Modifier::RightCommand, true, L_SHIFT | F_SHIFT);
        assert_eq!(m.feed(with_shift, now).actions, [Action::CommandDown]);
        let lifted = flags_change_with(Modifier::LeftShift, false, R_CMD | F_CMD);
        assert_eq!(m.feed(lifted, now).actions, [Action::CommandUp], "Right ⌘ alone does not start the main shortcut");
        assert_eq!(m.feed(flags_change(Modifier::RightCommand, false), now).actions, []);
        assert_eq!(m.feed(flags_change(Modifier::RightCommand, true), now).actions, [Action::ShortcutDown]);
    }

    #[test]
    fn shift_plus_a_key_combo_is_told_apart_from_the_same_combo_without_shift() {
        let mut m = two("Control+Option+Space", "Control+Option+Shift+Space");
        let now = t0();
        let down = |flags| RawEvent::KeyDown { keycode: 49, flags, repeat: false };
        let up = |flags| RawEvent::KeyUp { keycode: 49, flags };
        let out = m.feed(down(F_CTRL | F_ALT | F_SHIFT), now);
        assert_eq!((out.actions, out.swallow), (vec![Action::CommandDown], true));
        assert_eq!(m.feed(up(F_CTRL | F_ALT | F_SHIFT), now).actions, [Action::CommandUp]);
        assert_eq!(m.feed(down(F_CTRL | F_ALT), now).actions, [Action::ShortcutDown]);
        assert_eq!(m.feed(up(F_CTRL | F_ALT), now).actions, [Action::ShortcutUp]);
    }

    #[test]
    fn capture_takes_shift_with_a_lone_modifier_or_a_key() {
        let mut m = Machine::new(lone(Modifier::Fn));
        let now = t0();
        m.begin_capture();
        m.feed(flags_change(Modifier::LeftShift, true), now);
        m.feed(flags_change_with(Modifier::RightCommand, true, L_SHIFT | F_SHIFT), now);
        m.feed(flags_change_with(Modifier::RightCommand, false, L_SHIFT | F_SHIFT), now);
        let o = m.feed(flags_change(Modifier::LeftShift, false), now);
        let captured = Shortcut::parse("Shift+RightCommand").unwrap();
        assert_eq!(o.actions, [Action::Captured(captured)]);

        m.begin_capture();
        let o = m.feed(RawEvent::KeyDown { keycode: 96, flags: F_SHIFT, repeat: false }, now);
        assert_eq!(o.actions, [Action::Captured(Shortcut::parse("Shift+F5").unwrap())]);
        m.begin_capture();
        let o = m.feed(RawEvent::KeyDown { keycode: 49, flags: F_SHIFT, repeat: false }, now);
        assert_eq!(o.actions, [Action::Captured(Shortcut::parse("Shift+Space").unwrap())]);
    }

    #[test]
    fn command_combo_requires_exact_modifiers_swallows_and_ignores_repeats() {
        let mut m = two("RightOption", "Control+Option+Space");
        let now = t0();
        let down = |flags, repeat| RawEvent::KeyDown { keycode: 49, flags, repeat };
        assert_eq!(m.feed(down(F_CTRL, false), now), Output::default(), "missing Option");
        let out = m.feed(down(F_CTRL | F_ALT, false), now);
        assert_eq!((out.actions, out.swallow), (vec![Action::CommandDown], true));
        assert_eq!(m.feed(down(F_CTRL | F_ALT, true), now).actions, [], "auto-repeat");
        let up = m.feed(RawEvent::KeyUp { keycode: 49, flags: F_CTRL | F_ALT }, now);
        assert_eq!((up.actions, up.swallow), (vec![Action::CommandUp], true));
    }

    #[test]
    fn a_key_right_after_the_command_modifier_aborts_as_a_typing_chord() {
        let mut m = two("Control+Option+Space", "RightOption");
        let now = t0();
        m.feed(flags_change(Modifier::RightOption, true), now);
        let key = RawEvent::KeyDown { keycode: 14, flags: R_ALT | F_ALT, repeat: false };
        assert_eq!(m.feed(key, now + Duration::from_millis(100)).actions, [Action::ChordAbort]);
        assert_eq!(m.feed(key, now + Duration::from_millis(150)).actions, [], "only once per press");
        assert_eq!(m.feed(flags_change(Modifier::RightOption, false), now).actions, [Action::CommandUp]);
    }

    #[test]
    fn a_late_key_after_the_command_modifier_is_not_a_chord() {
        let mut m = two("RightOption", "RightCommand");
        let now = t0();
        m.feed(flags_change(Modifier::RightCommand, true), now);
        let key = RawEvent::KeyDown { keycode: 14, flags: R_CMD | F_CMD, repeat: false };
        assert_eq!(m.feed(key, now + Duration::from_millis(500)).actions, []);
    }

    fn three(main: &str, command: &str, tools: &str) -> Machine {
        let mut m = two(main, command);
        m.set_tools_shortcut(Some(Shortcut::parse(tools).unwrap()));
        m
    }

    #[test]
    fn the_tools_combo_fires_once_on_its_key_down_and_swallows_its_key() {
        let mut m = three("RightOption", "RightCommand", "Control+Option+Space");
        let now = t0();
        let down = |flags, repeat| RawEvent::KeyDown { keycode: 49, flags, repeat };
        assert_eq!(m.feed(down(F_CTRL, false), now).actions, [], "missing Option");
        let out = m.feed(down(F_CTRL | F_ALT, false), now);
        assert_eq!((out.actions, out.swallow), (vec![Action::ToolsPressed], true));
        let repeat = m.feed(down(F_CTRL | F_ALT, true), now);
        assert_eq!((repeat.actions, repeat.swallow), (vec![], true), "auto-repeat neither fires nor leaks");
        let up = m.feed(RawEvent::KeyUp { keycode: 49, flags: F_CTRL | F_ALT }, now);
        assert_eq!((up.actions, up.swallow), (vec![], true), "the key up is ours and fires nothing");
        assert_eq!(m.feed(down(F_CTRL | F_ALT, false), now).actions, [Action::ToolsPressed], "a new press fires again");
    }

    #[test]
    fn a_tools_modifier_fires_on_release_and_not_when_a_key_joined_it() {
        let mut m = three("RightOption", "RightCommand", "LeftOption");
        let now = t0();
        let down = m.feed(flags_change(Modifier::LeftOption, true), now);
        assert_eq!((down.actions, down.swallow), (vec![], false), "no down event, and nothing swallowed");
        assert_eq!(m.feed(flags_change(Modifier::LeftOption, false), now).actions, [Action::ToolsPressed]);

        // A key during the press makes it typing: the release fires nothing.
        m.feed(flags_change(Modifier::LeftOption, true), now);
        let key = RawEvent::KeyDown { keycode: 14, flags: L_ALT | F_ALT, repeat: false };
        m.feed(key, now + Duration::from_millis(100));
        let up = m.feed(flags_change(Modifier::LeftOption, false), now + Duration::from_millis(150));
        assert_eq!((up.actions, up.swallow), (vec![], false), "a typing chord is not a Tools press");
    }

    #[test]
    fn a_tools_shortcut_that_is_the_dictation_or_command_one_never_fires() {
        let now = t0();
        let mut m = three("RightOption", "RightCommand", "RightOption");
        assert_eq!(m.feed(flags_change(Modifier::RightOption, true), now).actions, [Action::ShortcutDown]);
        assert_eq!(m.feed(flags_change(Modifier::RightOption, false), now).actions, [Action::ShortcutUp]);

        let mut m = three("Fn", "RightCommand", "RightCommand");
        assert_eq!(m.feed(flags_change(Modifier::RightCommand, true), now).actions, [Action::CommandDown]);
        assert_eq!(m.feed(flags_change(Modifier::RightCommand, false), now).actions, [Action::CommandUp]);

        let mut m = three("Control+Option+Space", "RightCommand", "Control+Option+Space");
        let down = RawEvent::KeyDown { keycode: 49, flags: F_CTRL | F_ALT, repeat: false };
        assert_eq!(m.feed(down, now).actions, [Action::ShortcutDown], "the dictation combo keeps its press");

        // Given a shortcut of its own, the same machine fires again.
        m.set_tools_shortcut(Some(Shortcut::parse("LeftOption").unwrap()));
        assert_eq!(m.feed(flags_change(Modifier::LeftOption, true), now).actions, []);
        assert_eq!(m.feed(flags_change(Modifier::LeftOption, false), now).actions, [Action::ToolsPressed]);
    }
}
