//! Press and release the right Command key N times, one second apart (drives a running Dito for smoke tests).
//!   cargo run -p dito-platform --example press_shortcut -- 2

use std::time::Duration;

use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, EventField};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

fn post(down: bool) {
    let src = CGEventSource::new(CGEventSourceStateID::HIDSystemState).expect("event source");
    let ev = CGEvent::new(src).expect("event");
    ev.set_type(CGEventType::FlagsChanged);
    ev.set_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE, 54);
    ev.set_flags(if down {
        CGEventFlags::from_bits_retain(CGEventFlags::CGEventFlagCommand.bits() | 0x10)
    } else {
        CGEventFlags::CGEventFlagNull
    });
    ev.post(CGEventTapLocation::HID);
}

fn main() {
    let n: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(2);
    for i in 0..n {
        post(true);
        std::thread::sleep(Duration::from_millis(80));
        post(false);
        println!("press {}", i + 1);
        std::thread::sleep(Duration::from_millis(1800));
    }
}
