//! Microphone capture → 16 kHz mono f32.

pub mod capture;
pub mod resample;
mod transport;

pub use capture::{AutoOverrides, Capture, InputDevice, list_input_devices};
