use std::time::Duration;

use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, EventField};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use dito_core::insert::{TYPE_CHUNK_GAP_MS, TYPE_CHUNK_UNITS, Utf16Chunks};

use crate::hotkey::SYNTHETIC_EVENT_MARK;

const KEYCODE_C: u16 = 8;
const KEYCODE_V: u16 = 9;

fn post_key(keycode: u16, down: bool, flags: Option<CGEventFlags>, text: Option<&[u16]>) -> bool {
    // A private source keeps posted events isolated from the physical keyboard state: a
    // modifier the user is still holding (right Command ending hold-mode, Shift, ...) must not
    // bleed into synthesized keystrokes and turn them into shortcuts.
    let Ok(src) = CGEventSource::new(CGEventSourceStateID::Private) else { return false };
    let Ok(ev) = CGEvent::new_keyboard_event(src, keycode, down) else { return false };
    if let Some(f) = flags {
        ev.set_flags(f);
    }
    if let Some(t) = text {
        ev.set_string_from_utf16_unchecked(t);
    }
    // The shortcut tap must not take these for the user's typing: a Cmd+C right after a command shortcut went down
    // would otherwise abort its chord.
    ev.set_integer_value_field(EventField::EVENT_SOURCE_USER_DATA, SYNTHETIC_EVENT_MARK);
    ev.post(CGEventTapLocation::HID);
    true
}

/// Returns false only when events could not be posted at all; whether they land is verified separately.
pub(super) fn post_type(units: &[u16]) -> bool {
    let mut ok = true;
    for chunk in Utf16Chunks::new(units, TYPE_CHUNK_UNITS) {
        ok &= post_key(0, true, None, Some(chunk));
        ok &= post_key(0, false, None, Some(chunk));
        std::thread::sleep(Duration::from_millis(TYPE_CHUNK_GAP_MS));
    }
    ok
}

pub(super) fn post_paste() -> bool {
    post_command_key(KEYCODE_V)
}

pub(super) fn post_copy() -> bool {
    post_command_key(KEYCODE_C)
}

fn post_command_key(keycode: u16) -> bool {
    let command = Some(CGEventFlags::CGEventFlagCommand);
    post_key(keycode, true, command, None) && post_key(keycode, false, command, None)
}
