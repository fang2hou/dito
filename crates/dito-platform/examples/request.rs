//! Ask for a permission the way the app's "許可する" button does, then watch its status for a few seconds.
//!   cargo run -p dito-platform --example request -- accessibility|input|mic [seconds]

use std::time::Duration;

use dito_platform::{Permission, permissions};

fn main() {
    let mut args = std::env::args().skip(1);
    let which = match args.next().as_deref() {
        Some("accessibility") => Permission::Accessibility,
        Some("mic") => Permission::Microphone,
        _ => {
            eprintln!("usage: request accessibility|input|mic [seconds]");
            std::process::exit(2);
        }
    };
    let secs: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(5);
    println!("before: {:?}", permissions::status(which));
    permissions::request(which);
    for _ in 0..secs {
        std::thread::sleep(Duration::from_secs(1));
        println!("status: {:?}", permissions::status(which));
    }
}
