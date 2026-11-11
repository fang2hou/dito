use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::audio::AudioClip;
use crate::store::{NewHistory, Store};

/// Writing a recording and a database row blocks, so it runs on the blocking pool; a failure is logged and never
/// reaches the dictation.
pub(super) struct History {
    store: Arc<Store>,
    audio_dir: Option<PathBuf>,
}

impl History {
    pub(super) fn new(store: Arc<Store>, audio_dir: Option<PathBuf>) -> Self {
        Self { store, audio_dir }
    }

    /// `entry.audio_path` is filled in from the saved recording, `audio_name` being its file name.
    pub(super) fn save(&self, clip: Arc<AudioClip>, audio_name: String, mut entry: NewHistory) {
        let store = self.store.clone();
        let audio_dir = self.audio_dir.clone();
        tokio::task::spawn_blocking(move || {
            entry.audio_path = audio_dir.and_then(|dir| write_audio(&dir, &audio_name, &clip));
            if let Err(e) = store.add_history(&entry) {
                tracing::warn!(operation = "add history", error = %e, "history was not saved");
            }
        });
    }
}

fn write_audio(dir: &Path, name: &str, clip: &AudioClip) -> Option<String> {
    let path = dir.join(name);
    let written = std::fs::create_dir_all(dir).and_then(|()| std::fs::write(&path, clip.to_wav()));
    match written {
        Ok(()) => Some(path.to_string_lossy().into_owned()),
        Err(e) => {
            tracing::warn!(operation = "save audio", error = %e, "history recording was not saved");
            None
        }
    }
}
