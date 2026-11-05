//! Hardware smoke test: `cargo run -p dito-audio --example record -- 2`
use std::time::Duration;

use dito_audio::{Capture, list_input_devices};
use dito_core::controller::Recorder;

fn main() {
    let secs: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(2);
    for d in list_input_devices() {
        println!("input device: {:<32} auto-probe by default: {}", d.name, d.auto_default);
    }
    let cap = Capture::default();
    if let Err(e) = cap.start() {
        eprintln!("start failed: {e}");
        std::process::exit(1);
    }
    for _ in 0..secs * 4 {
        std::thread::sleep(Duration::from_millis(250));
        println!("level {:.4}", cap.level());
    }
    let clip = cap.stop();
    println!(
        "{} samples, {} ms, rms {:.4}, silent={}",
        clip.samples.len(),
        clip.duration_ms(),
        clip.rms(),
        clip.is_effectively_silent()
    );
}
