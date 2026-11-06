//! Microphone capture to 16 kHz mono f32.

mod capture;
mod resample;
mod transport;

pub use capture::{AutoOverrides, Capture, InputDevice, list_input_devices};
