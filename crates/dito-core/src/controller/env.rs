use std::path::PathBuf;
use std::sync::Arc;

use super::recorder_error::RecorderError;
use crate::audio::AudioClip;
use crate::config::{ConfigStore, InsertMethod};
use crate::factory::Engines;
use crate::store::Store;
use crate::surrounding::Surrounding;

pub trait Recorder: Send + Sync {
    fn start(&self) -> Result<(), RecorderError>;
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
    /// Puts `text` on the clipboard and nowhere else, whether or not a text target is focused. `Copied` or `Failed`.
    fn copy(&self, text: &str) -> Delivery;
}

/// Reads the selection when a command starts and again when its recording ends.
pub trait SelectionSource: Send + Sync {
    /// The selected text and the text around it in the focused element of the app in front, read through
    /// Accessibility, or copied from an app without Accessibility text. `None`: nothing is selected and the field has
    /// no text, or it cannot be read; the second reading of a run then keeps the first.
    fn capture(&self) -> Option<Arc<dyn CapturedSelection>>;
}

/// What was read from the focused element.
pub trait CapturedSelection: Send + Sync {
    /// The selected text; empty when nothing is selected and only the field's text was read.
    fn text(&self) -> &str;
    /// The rest of the field's text, as reference only: it is never the text a tool works on. `None` when the field
    /// exposes no text beyond the selection.
    fn surrounding(&self) -> Option<&Surrounding>;
    /// Whether the text can be written over. A preview, a web page or a read-only view cannot: a tool that would
    /// replace the selection leaves its result to be copied instead.
    fn editable(&self) -> bool;
    /// Gives keyboard focus back to the source app if a Dito window took it, then writes `text` over the selection.
    /// Blocking. Only called when there is a selection.
    fn replace(&self, text: &str, method: InsertMethod) -> Delivery;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppInfo {
    pub name: String,
    pub bundle_id: String,
}

pub trait Frontmost: Send + Sync {
    fn frontmost(&self) -> AppInfo;
}

/// Warms the insertion target's accessibility state when recording starts, so lazily built trees (Electron/Chromium)
/// are ready by delivery.
pub trait FocusWarmer: Send + Sync {
    fn warm(&self);
}

pub trait MusicMute: Send + Sync {
    /// Dropping the session restores the previous state.
    fn mute_if_active(&self) -> Box<dyn MuteSession>;
}

pub trait MuteSession: Send {}

#[derive(Clone)]
pub struct Env {
    pub recorder: Arc<dyn Recorder>,
    pub sink: Arc<dyn Sink>,
    pub frontmost: Arc<dyn Frontmost>,
    pub focus_warmer: Arc<dyn FocusWarmer>,
    pub music_mute: Arc<dyn MusicMute>,
    pub selection: Arc<dyn SelectionSource>,
}

pub struct Deps {
    pub env: Env,
    pub config: Arc<ConfigStore>,
    pub store: Arc<Store>,
    pub engines: Arc<dyn Engines>,
    /// Where recordings are kept for replay and re-transcription. `None` disables saving.
    pub audio_dir: Option<PathBuf>,
}
