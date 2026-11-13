use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use dito_local::{AsrOutput, AsrRuntime, RuntimeError};
use transcribe_cpp::{Error, Feature, Model, RunOptions, Session, TimestampKind};

use crate::chunking::{join_chunks, split_at_pauses};

/// Any speech family transcribe.cpp runs. The file describes its own architecture, languages and
/// features, so one adapter serves every such catalog entry.
pub struct Speech {
    session: Mutex<Session>,
    languages: Vec<String>,
    detects_language: bool,
    takes_vocabulary: bool,
}

impl Speech {
    pub fn load(model: &Path) -> Result<Self, RuntimeError> {
        if std::env::var_os("DITO_TRANSCRIBE_DEBUG").is_none() {
            transcribe_cpp::disable_logging();
        }
        let model = Model::load(model).map_err(RuntimeError::engine)?;
        let capabilities = model.capabilities().map_err(RuntimeError::engine)?;
        Ok(Self {
            session: Mutex::new(model.session().map_err(RuntimeError::engine)?),
            languages: capabilities.languages,
            detects_language: capabilities.supports_language_detect,
            takes_vocabulary: model.supports(Feature::Vocabulary),
        })
    }
}

impl AsrRuntime for Speech {
    fn transcribe(
        &self,
        samples: &[f32],
        language: Option<&str>,
        hotwords: &[String],
        stop: &AtomicBool,
    ) -> Result<AsrOutput, RuntimeError> {
        // A hint the model was not trained on is dropped instead of failing the dictation.
        let forced = language.filter(|code| self.languages.iter().any(|known| known == code));
        let options = RunOptions {
            timestamps: TimestampKind::None,
            language: forced.map(str::to_string),
            vocabulary: if self.takes_vocabulary { hotwords.to_vec() } else { Vec::new() },
            ..RunOptions::default()
        };
        let mut session = self.session.lock().map_err(|_| RuntimeError::LockPoisoned("session"))?;
        let mut texts = Vec::new();
        let mut detected = None;
        for chunk in split_at_pauses(samples) {
            // A native run is not interrupted here, so `stop` is checked at every seam between chunks.
            if stop.load(Ordering::Relaxed) {
                return Err(RuntimeError::Cancelled);
            }
            let transcript = session.run(chunk, &options).map_err(|error| match error {
                Error::OutputTruncated { .. } => RuntimeError::Truncated,
                other => RuntimeError::engine(other),
            })?;
            detected = detected.or(transcript.language);
            if !transcript.text.trim().is_empty() {
                texts.push(transcript.text.trim().to_string());
            }
        }
        let language = forced.map(str::to_string).or(detected.filter(|_| self.detects_language));
        Ok(AsrOutput { text: join_chunks(texts), language })
    }
}
