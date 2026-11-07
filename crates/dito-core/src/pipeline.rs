//! Recognize → polish for one recorded clip. No recording, no delivery: those belong to the controller.

use std::time::Instant;

pub use tokio_util::sync::CancellationToken;

use crate::audio::AudioClip;
use crate::config::Config;
use crate::error::{ErrorKind, ProviderError};
use crate::factory::Engines;
use crate::i18n::Lang;
use crate::prompts::{PromptInput, Style, build_system_prompt, detect_lang};
use crate::provider::{AsrRequest, PolishRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Recognizing,
    Polishing { input_chars: usize, started: Instant },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub raw: String,
    /// `None` when polishing is off or failed (the raw text is then what gets delivered).
    pub polished: Option<String>,
    pub polish_error: Option<ProviderError>,
    pub lang: Lang,
    pub asr_ms: u64,
    pub polish_ms: u64,
}

impl Output {
    pub fn text(&self) -> &str {
        self.polished.as_deref().unwrap_or(&self.raw)
    }
}

#[derive(Debug)]
pub enum PipelineError {
    Cancelled,
    Failed(ProviderError),
}

pub struct Job<'a> {
    pub clip: &'a AudioClip,
    pub style: Style,
    pub dictionary: &'a [String],
}

async fn or_cancel<T>(cancel: &CancellationToken, fut: impl Future<Output = T>) -> Result<T, PipelineError> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(PipelineError::Cancelled),
        v = fut => Ok(v),
    }
}

/// Recognize and (if enabled) polish. A polish failure never loses the recognized text.
pub async fn run(
    engines: &dyn Engines,
    cfg: &Config,
    job: &Job<'_>,
    on_phase: &(dyn Fn(Phase) + Send + Sync),
    cancel: &CancellationToken,
) -> Result<Output, PipelineError> {
    reject_silent(job.clip)?;
    on_phase(Phase::Recognizing);
    let mut out = recognize(engines, cfg, job, cancel).await?;
    polish(engines, cfg, job, on_phase, cancel, &mut out).await?;
    Ok(out)
}

fn reject_silent(clip: &AudioClip) -> Result<(), PipelineError> {
    if !clip.is_effectively_silent() {
        return Ok(());
    }
    tracing::debug!("clip rejected as silent: {}ms rms {:.4} peak {:.4}", clip.duration_ms(), clip.rms(), clip.peak());
    Err(PipelineError::Failed(ProviderError::new(ErrorKind::NoSpeech, "silent or too short")))
}

async fn recognize(
    engines: &dyn Engines,
    cfg: &Config,
    job: &Job<'_>,
    cancel: &CancellationToken,
) -> Result<Output, PipelineError> {
    let started = Instant::now();
    let asr = engines.transcriber(cfg).await.map_err(PipelineError::Failed)?;
    let transcript = or_cancel(
        cancel,
        asr.transcribe(&AsrRequest {
            clip: job.clip,
            language: cfg.general.preferred_language.code(),
            hotwords: job.dictionary,
        }),
    )
    .await?
    .map_err(PipelineError::Failed)?;
    let asr_ms = started.elapsed().as_millis() as u64;
    if transcript.text.trim().is_empty() {
        return Err(PipelineError::Failed(ProviderError::new(ErrorKind::NoSpeech, "nothing was recognized")));
    }
    let lang = transcript.language.as_deref().and_then(lang_from_code).unwrap_or_else(|| detect_lang(&transcript.text));
    Ok(Output { raw: transcript.text, polished: None, polish_error: None, lang, asr_ms, polish_ms: 0 })
}

/// Fills `out.polished` or `out.polish_error`; leaves `out` untouched when polishing is off.
async fn polish(
    engines: &dyn Engines,
    cfg: &Config,
    job: &Job<'_>,
    on_phase: &(dyn Fn(Phase) + Send + Sync),
    cancel: &CancellationToken,
    out: &mut Output,
) -> Result<(), PipelineError> {
    let polisher = match engines.polisher(cfg).await {
        Ok(Some(polisher)) => polisher,
        Ok(None) => return Ok(()),
        Err(e) => {
            out.polish_error = Some(e);
            return Ok(());
        }
    };

    let started = Instant::now();
    on_phase(Phase::Polishing { input_chars: out.raw.chars().count(), started });
    let words: &[String] = if cfg.dictionary_for_polish { job.dictionary } else { &[] };
    let system = build_system_prompt(&PromptInput {
        prompts: &cfg.prompts,
        style: job.style.clone(),
        text: &out.raw,
        dictionary: words,
    });
    match or_cancel(cancel, polisher.polish(&PolishRequest { system: &system, text: &out.raw })).await? {
        Ok(text) => out.polished = Some(text),
        Err(e) => out.polish_error = Some(e),
    }
    out.polish_ms = started.elapsed().as_millis() as u64;
    Ok(())
}

fn lang_from_code(code: &str) -> Option<Lang> {
    match code.to_ascii_lowercase().get(..2)? {
        "ja" => Some(Lang::Ja),
        "en" => Some(Lang::En),
        "zh" => Some(Lang::Zh),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::factory::FixedEngines;
    use crate::provider::mock::{MockPolisher, MockTranscriber};

    fn tone() -> AudioClip {
        AudioClip::new(vec![0.3; 16_000])
    }

    fn engines(asr: MockTranscriber, polish: MockPolisher) -> FixedEngines {
        FixedEngines { asr: Arc::new(asr), polish: Arc::new(polish) }
    }

    async fn go(e: &FixedEngines, cfg: &Config, clip: &AudioClip) -> Result<Output, PipelineError> {
        run(e, cfg, &Job { clip, style: Style::General, dictionary: &[] }, &|_| {}, &CancellationToken::new()).await
    }

    #[tokio::test]
    async fn recognizes_then_polishes_in_order() {
        let e = engines(MockTranscriber::new("えーと 会議です"), MockPolisher::default());
        let phases = std::sync::Mutex::new(Vec::new());
        let out = run(
            &e,
            &Config::default(),
            &Job { clip: &tone(), style: Style::General, dictionary: &[] },
            &|p| phases.lock().unwrap().push(p),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert!(matches!(
            phases.lock().unwrap().as_slice(),
            [Phase::Recognizing, Phase::Polishing { input_chars: 8, .. }]
        ));
        assert_eq!(out.raw, "えーと 会議です");
        assert_eq!(out.text(), "会議です。");
        assert_eq!(out.lang, Lang::Ja);
    }

    #[tokio::test]
    async fn disabled_polish_delivers_the_raw_text_without_calling_the_model() {
        let polish = Arc::new(MockPolisher::default());
        let e = FixedEngines { asr: Arc::new(MockTranscriber::new("hello")), polish: polish.clone() };
        let mut cfg = Config::default();
        cfg.polish.enabled = false;
        let out = go(&e, &cfg, &tone()).await.unwrap();
        assert_eq!(out.text(), "hello");
        assert_eq!(polish.calls(), 0);
    }

    #[tokio::test]
    async fn polish_failure_falls_back_to_the_recognized_text() {
        let e = engines(MockTranscriber::new("hello"), MockPolisher::default().script([Err(ErrorKind::RateLimit)]));
        let out = go(&e, &Config::default(), &tone()).await.unwrap();
        assert_eq!(out.text(), "hello");
        assert_eq!(out.polish_error.as_ref().unwrap().kind, ErrorKind::RateLimit);
    }

    #[tokio::test]
    async fn asr_failures_surface_with_their_class() {
        let e = engines(MockTranscriber::new("x").script([Err(ErrorKind::Auth)]), MockPolisher::default());
        match go(&e, &Config::default(), &tone()).await {
            Err(PipelineError::Failed(err)) => assert_eq!(err.kind, ErrorKind::Auth),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_empty_transcript_is_no_speech_and_is_not_polished() {
        let polish = Arc::new(MockPolisher::default());
        let e = FixedEngines { asr: Arc::new(MockTranscriber::new("  ")), polish: polish.clone() };
        match go(&e, &Config::default(), &tone()).await {
            Err(PipelineError::Failed(err)) => assert_eq!(err.kind, ErrorKind::NoSpeech),
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(polish.calls(), 0);
    }

    #[tokio::test]
    async fn silent_audio_never_reaches_the_engine() {
        let asr = Arc::new(MockTranscriber::new("x"));
        let e = FixedEngines { asr: asr.clone(), polish: Arc::new(MockPolisher::default()) };
        match go(&e, &Config::default(), &AudioClip::silence(1.0)).await {
            Err(PipelineError::Failed(err)) => assert_eq!(err.kind, ErrorKind::NoSpeech),
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(asr.calls(), 0);
    }

    #[tokio::test]
    async fn cancelling_during_polish_aborts_and_reports_cancelled() {
        let e = engines(
            MockTranscriber::new("hello"),
            MockPolisher::default().with_delay(std::time::Duration::from_secs(30)),
        );
        let cancel = CancellationToken::new();
        let seen = Arc::new(AtomicUsize::new(0));
        let (c2, s2) = (cancel.clone(), seen.clone());
        let on_phase = move |p: Phase| {
            if matches!(p, Phase::Polishing { .. }) {
                s2.fetch_add(1, Ordering::SeqCst);
                c2.cancel();
            }
        };
        let r = run(
            &e,
            &Config::default(),
            &Job { clip: &tone(), style: Style::General, dictionary: &[] },
            &on_phase,
            &cancel,
        )
        .await;
        assert!(matches!(r, Err(PipelineError::Cancelled)));
        assert_eq!(seen.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn dictionary_reaches_polish_only_when_enabled() {
        struct Spy(std::sync::Mutex<String>);
        #[async_trait::async_trait]
        impl crate::provider::Polisher for Spy {
            async fn polish(&self, r: &PolishRequest<'_>) -> crate::error::ProviderResult<String> {
                *self.0.lock().unwrap() = r.system.to_string();
                Ok("ok".into())
            }
        }
        let spy = Arc::new(Spy(Default::default()));
        let e = FixedEngines { asr: Arc::new(MockTranscriber::new("Acme")), polish: spy.clone() };
        let dict = vec!["Acme".to_string()];
        let clip = tone();
        let job = Job { clip: &clip, style: Style::General, dictionary: &dict };
        let mut cfg = Config::default();
        run(&e, &cfg, &job, &|_| {}, &CancellationToken::new()).await.unwrap();
        assert!(!spy.0.lock().unwrap().contains("<reference_vocabulary>"));
        cfg.dictionary_for_polish = true;
        run(&e, &cfg, &job, &|_| {}, &CancellationToken::new()).await.unwrap();
        assert!(spy.0.lock().unwrap().contains("Acme"));
    }
}
