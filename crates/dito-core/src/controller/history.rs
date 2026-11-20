use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::audio::AudioClip;
use crate::config::Config;
use crate::error::{ErrorKind, ProviderError};
use crate::factory::Engines;
use crate::pipeline::{self, CancellationToken, PipelineError, PipelineJob, PolishJob};
use crate::store::{HistoryEntry, NewHistory, StepMillis, Store, StoreError};

#[derive(Debug, thiserror::Error)]
pub enum HistoryRerunError {
    #[error("history entry has no audio")]
    NoAudio,
    #[error("history audio could not be decoded")]
    InvalidAudio,
    #[error("history entry has no transcript")]
    NoTranscript,
    #[error(transparent)]
    Audio(#[from] std::io::Error),
    #[error(transparent)]
    Pipeline(#[from] PipelineError),
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Task(#[from] tokio::task::JoinError),
}

pub async fn transcribe_again(
    engines: &dyn Engines,
    store: Arc<Store>,
    cfg: &Config,
    entry: HistoryEntry,
) -> Result<(), HistoryRerunError> {
    let path = entry.audio_path.clone().ok_or(HistoryRerunError::NoAudio)?;
    let clip = tokio::task::spawn_blocking(move || {
        let bytes = std::fs::read(path)?;
        AudioClip::from_wav(&bytes).ok_or(HistoryRerunError::InvalidAudio)
    })
    .await??;
    let words = super::actor::load_wordbook(store.clone()).await;
    let out = pipeline::run(
        engines,
        cfg,
        &PipelineJob { clip: &clip, target: entry.style, wordbook: &words },
        &|_| {},
        &CancellationToken::new(),
    )
    .await?;
    let timings = StepMillis { asr: Some(out.asr_ms), polish: out.polished.is_some().then_some(out.polish_ms) };
    let (id, raw, polished, lang) = (entry.id, out.raw, out.polished, out.lang.code().to_string());
    let labels = cfg.engine_labels();
    tokio::task::spawn_blocking(move || {
        store.complete_history(id, &raw, polished.as_deref(), &lang, &labels, &timings)
    })
    .await??;
    Ok(())
}

pub async fn polish_again(
    engines: &dyn Engines,
    store: Arc<Store>,
    cfg: &Config,
    entry: HistoryEntry,
) -> Result<(), HistoryRerunError> {
    if entry.raw.trim().is_empty() {
        return Err(HistoryRerunError::NoTranscript);
    }
    let mut engine_cfg = cfg.clone();
    engine_cfg.polish.enabled = true;
    let text_model = engines
        .text_model(&engine_cfg)
        .await?
        .ok_or_else(|| ProviderError::new(ErrorKind::Unavailable, "no text model is available"))?;
    let words = super::actor::load_wordbook(store.clone()).await;
    let polish_job = PolishJob {
        prompts: &cfg.prompts,
        target: entry.style,
        text: &entry.raw,
        wordbook: pipeline::polish_wordbook(cfg, &words),
        lang: pipeline::transcript_lang(Some(&entry.lang), &entry.raw),
    };
    let started = std::time::Instant::now();
    let text = pipeline::polish_text(text_model.as_ref(), polish_job).await?;
    let polish_ms = started.elapsed().as_millis() as u64;
    let (id, polish_engine) = (entry.id, cfg.text_model.label());
    tokio::task::spawn_blocking(move || store.set_polished(id, &text, &polish_engine, polish_ms)).await??;
    Ok(())
}

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
            insert(&store, &entry);
        });
    }

    /// For an entry that has no recording, such as a command.
    pub(super) fn save_entry(&self, entry: NewHistory) {
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || insert(&store, &entry));
    }
}

fn insert(store: &Store, entry: &NewHistory) {
    if let Err(e) = store.add_history(entry) {
        tracing::warn!(operation = "add history", error = %e, "history was not saved");
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use tempfile::TempDir;

    use super::*;
    use crate::config::EngineLabels;
    use crate::error::ProviderResult;
    use crate::factory::FixedEngines;
    use crate::prompts::AppTarget;
    use crate::provider::mock::{MockTextModel, MockTranscriber};
    use crate::provider::{AsrRequest, TextModel, TextRequest, Transcriber, Transcript};
    use crate::store::HistoryKind;

    fn entry(store: &Store, audio_path: Option<String>, raw: &str, polished: Option<&str>) -> HistoryEntry {
        let id = store
            .add_history(&NewHistory {
                ts: 10,
                kind: HistoryKind::Dictation,
                style: AppTarget::General,
                app: "Editor".into(),
                lang: "en".into(),
                duration_ms: 1_000,
                raw: raw.into(),
                polished: polished.map(Into::into),
                error: None,
                audio_path,
                engines: EngineLabels { asr: Some("old-asr".into()), polish: Some("old-polish".into()) },
                timings: StepMillis { asr: Some(321), polish: Some(654) },
                source: None,
                command: None,
            })
            .unwrap();
        store.get_history(id).unwrap().unwrap()
    }

    fn recording(dir: &TempDir) -> String {
        let path = dir.path().join("history.wav");
        std::fs::write(&path, AudioClip::new(vec![0.3; 16_000]).to_wav()).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[tokio::test]
    async fn transcribe_again_replaces_text_without_replacing_the_row() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open_in_memory().unwrap());
        let before = entry(&store, Some(recording(&dir)), "old raw", Some("old polished"));
        let engines = FixedEngines::new(Arc::new(MockTranscriber::new("new raw")), Arc::new(MockTextModel::default()));

        transcribe_again(&engines, store.clone(), &Config::default(), before.clone()).await.unwrap();

        let after = store.get_history(before.id).unwrap().unwrap();
        assert_eq!(after.id, before.id);
        assert_eq!(after.audio_path, before.audio_path);
        assert_eq!(after.raw, "new raw");
        assert_eq!(after.polished.as_deref(), Some("new raw."));
    }

    #[tokio::test]
    async fn polish_again_changes_only_the_polished_result() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let before = entry(&store, None, "keep this raw", Some("old polished"));
        let engines = FixedEngines::new(
            Arc::new(MockTranscriber::new("unused")),
            Arc::new(MockTextModel::default().script([Ok("new polished".into())])),
        );
        let mut cfg = Config::default();
        cfg.polish.enabled = false;

        polish_again(&engines, store.clone(), &cfg, before.clone()).await.unwrap();

        let after = store.get_history(before.id).unwrap().unwrap();
        assert_eq!(after.id, before.id);
        assert_eq!(after.raw, before.raw);
        assert_eq!(after.polished.as_deref(), Some("new polished"));
        assert_eq!(after.engines.asr, before.engines.asr);
        assert_eq!(after.timings.asr, before.timings.asr);
    }

    #[tokio::test]
    async fn transcribe_again_without_audio_is_typed_and_does_not_change_the_row() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let before = entry(&store, None, "unchanged", Some("still unchanged"));
        let engines = FixedEngines::new(Arc::new(MockTranscriber::new("unused")), Arc::new(MockTextModel::default()));

        let result = transcribe_again(&engines, store.clone(), &Config::default(), before.clone()).await;

        assert!(matches!(result, Err(HistoryRerunError::NoAudio)));
        assert_eq!(store.get_history(before.id).unwrap().unwrap(), before);
    }

    #[tokio::test]
    async fn an_engine_failure_does_not_change_the_row() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open_in_memory().unwrap());
        let before = entry(&store, Some(recording(&dir)), "unchanged", Some("still unchanged"));
        let engines = FixedEngines::new(
            Arc::new(MockTranscriber::new("unused").script([Err(ErrorKind::Auth)])),
            Arc::new(MockTextModel::default()),
        );

        let result = transcribe_again(&engines, store.clone(), &Config::default(), before.clone()).await;

        assert!(matches!(
            result,
            Err(HistoryRerunError::Pipeline(PipelineError::Failed(ProviderError { kind: ErrorKind::Auth, .. })))
        ));
        assert_eq!(store.get_history(before.id).unwrap().unwrap(), before);
    }

    #[derive(Default)]
    struct WordbookSpy {
        hotwords: Mutex<Vec<String>>,
        prompt: Mutex<String>,
    }

    #[async_trait]
    impl Transcriber for WordbookSpy {
        async fn transcribe(&self, req: &AsrRequest<'_>) -> ProviderResult<Transcript> {
            *self.hotwords.lock().unwrap() = req.hotwords.to_vec();
            Ok(Transcript { text: "Acme".into(), language: Some("en".into()) })
        }
    }

    #[async_trait]
    impl TextModel for WordbookSpy {
        async fn polish(&self, req: &TextRequest<'_>) -> ProviderResult<String> {
            *self.prompt.lock().unwrap() = req.system.into();
            Ok("Acme.".into())
        }
    }

    #[tokio::test]
    async fn the_wordbook_reaches_both_engines() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open_in_memory().unwrap());
        store.add_word("Acme", 1).unwrap();
        let history = entry(&store, Some(recording(&dir)), "old", None);
        let spy = Arc::new(WordbookSpy::default());
        let engines = FixedEngines::new(spy.clone(), spy.clone());
        let cfg = Config { wordbook_for_polish: true, ..Default::default() };

        transcribe_again(&engines, store, &cfg, history).await.unwrap();

        assert_eq!(spy.hotwords.lock().unwrap().as_slice(), ["Acme"]);
        assert!(spy.prompt.lock().unwrap().contains("Acme"));
    }
}
