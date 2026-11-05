//! Posts synthetic events into the real event stream and expects the tap to report them.
//! Uses F20 — no physical keyboard has that key — so the injected events cannot collide with
//! real user shortcuts (right-modifier dictation tools and the like).
//! Needs Accessibility for the test runner; skipped (not failed) without it.

use std::time::Duration;

use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, EventField};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use dito_platform::hotkey::Action;
use dito_platform::{HotkeyService, Permission, Shortcut, Status, permissions};

/// F20, from the key table in `dito_platform::hotkey`.
const KEYCODE_F20: i64 = 90;

fn post_key(down: bool) {
    let src = CGEventSource::new(CGEventSourceStateID::HIDSystemState).expect("event source");
    let ev = CGEvent::new(src).expect("event");
    ev.set_type(if down { CGEventType::KeyDown } else { CGEventType::KeyUp });
    ev.set_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE, KEYCODE_F20);
    ev.set_flags(CGEventFlags::CGEventFlagNull);
    ev.post(CGEventTapLocation::HID);
}

#[test]
fn f20_press_and_release_reach_the_tap() {
    if permissions::status(Permission::Accessibility) != Status::Granted {
        eprintln!("skipped: Accessibility not granted to this process");
        return;
    }
    let (_svc, rx) = HotkeyService::start(Shortcut::parse("F20").unwrap()).expect("tap");
    std::thread::sleep(Duration::from_millis(300));
    post_key(true);
    assert_eq!(rx.recv_timeout(Duration::from_secs(3)), Ok(Action::ShortcutDown));
    post_key(false);
    assert_eq!(rx.recv_timeout(Duration::from_secs(3)), Ok(Action::ShortcutUp));
}
