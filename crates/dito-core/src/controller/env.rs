use std::path::PathBuf;
use std::sync::Arc;

use super::recorder_error::RecorderError;
use crate::audio::AudioClip;
use crate::config::{ConfigStore, InsertMethod};
use crate::factory::Engines;
use crate::store::Store;

pub trait Recorder: Send + Sync {
    fn start(&self) -> Result<(), RecorderError>;
    /// Stop and return everything recorded since `start`.
    fn stop(&self) -> AudioClip;
    /// Smoothed input level 0..1 for the waveform.
    fn level(&self) -> f32;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
    Inserted,
    /// There was no place to insert; the text is on the clipboard.
    Copied,
    Failed(String),
}

pub trait Sink: Send + Sync {
    fn deliver(&self, text: &str, method: InsertMethod) -> Delivery;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppInfo {
    pub name: String,
    pub bundle_id: String,
}

pub trait Frontmost: Send + Sync {
    fn frontmost(&self) -> AppInfo;
}

/// Warms the insertion target's accessibility state when recording starts, so lazily built
/// trees (Electron/Chromium) are ready by delivery. Platform-agnostic no-op in tests.
pub trait FocusWarmer: Send + Sync {
    fn warm(&self);
}

/// Silences other audio (typically music) while the microphone records.
pub trait MusicMute: Send + Sync {
    /// Mute whatever is playing, if anything; dropping the session restores the previous state.
    fn mute_if_active(&self) -> Box<dyn MuteSession>;
}

/// Marker implemented by the guard types [`MusicMute`] returns; the concrete `Drop` restores state.
pub trait MuteSession: Send {}

#[derive(Clone)]
pub struct Env {
    pub recorder: Arc<dyn Recorder>,
    pub sink: Arc<dyn Sink>,
    pub frontmost: Arc<dyn Frontmost>,
    pub focus_warmer: Arc<dyn FocusWarmer>,
    pub music_mute: Arc<dyn MusicMute>,
}

pub struct Deps {
    pub env: Env,
    pub config: Arc<ConfigStore>,
    pub store: Arc<Store>,
    pub engines: Arc<dyn Engines>,
    /// Where recordings are kept for replay and re-recognition. `None` disables saving.
    pub audio_dir: Option<PathBuf>,
}
