//! Microphone and system audio capture to 16 kHz mono f32.

mod capture;
mod live;
mod resample;
mod transport;

pub use capture::{AutoOverrides, Capture, InputDevice, list_input_devices};
pub use live::LiveCapture;
