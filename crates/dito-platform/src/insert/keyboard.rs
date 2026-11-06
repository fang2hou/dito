use std::time::Duration;

use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use dito_core::insert::{TYPE_CHUNK_GAP_MS, TYPE_CHUNK_UNITS, Utf16Chunks};

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
    ev.post(CGEventTapLocation::HID);
    true
}

/// Post `units` as unicode key events (no clipboard involved). Returns false only when events
/// could not be posted at all; whether they land is verified separately.
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
    let command = Some(CGEventFlags::CGEventFlagCommand);
    post_key(KEYCODE_V, true, command, None) && post_key(KEYCODE_V, false, command, None)
}
