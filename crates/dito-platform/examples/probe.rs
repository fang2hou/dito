//! Manual probe: `cargo run -p dito-platform --example probe [seconds]`
//! Prints permission states, the frontmost app, whether a text field has focus, and then logs
//! shortcut actions for the configured shortcut (default: right Option) for a few seconds.

use std::time::{Duration, Instant};

use dito_core::controller::Frontmost;
use dito_platform::{HotkeyService, MacFrontmost, Permission, Shortcut, insert, permissions};

fn main() {
    let secs: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(8);
    for p in [Permission::Microphone, Permission::Accessibility, Permission::SystemAudio] {
        println!("{p:?}: {:?}", permissions::status(p));
    }
    println!("frontmost: {:?}", MacFrontmost.frontmost());
    println!("text target focused: {}", insert::has_text_target());
    match HotkeyService::start(Shortcut::parse("RightOption").unwrap()) {
        Ok((svc, rx)) => {
            println!("hotkey tap running; press right Option / Esc for {secs}s");
            let end = Instant::now() + Duration::from_secs(secs);
            while Instant::now() < end {
                if let Ok(a) = rx.recv_timeout(Duration::from_millis(200)) {
                    println!("action: {a:?}");
                }
            }
            drop(svc);
        }
        Err(e) => println!("hotkey: {e}"),
    }
}
