//! Transcribe → polish for one recorded clip. No recording, no delivery: those belong to the controller.
//! The polish step is also available alone ([`polish_text`]) for text that is already transcribed.

use std::time::Instant;

pub use tokio_util::sync::CancellationToken;

use crate::audio::AudioClip;
use crate::config::Config;
use crate::error::{ErrorKind, ProviderError, ProviderResult};
use crate::factory::Engines;
use crate::i18n::Lang;
use crate::prompts::{PromptInput, Prompts, Style, build_system_prompt, build_system_prompt_for_language, detect_lang};
use crate::provider::{AsrRequest, PolishRequest, Polisher};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelinePhase {
    Transcribing,
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

pub struct PipelineJob<'a> {
    pub clip: &'a AudioClip,
    pub style: Style,
    pub wordbook: &'a [String],
}

async fn or_cancel<T>(cancel: &CancellationToken, fut: impl Future<Output = T>) -> Result<T, PipelineError> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(PipelineError::Cancelled),
        v = fut => Ok(v),
    }
}

/// Transcribe and (if enabled) polish. A polish failure never loses the transcript.
pub async fn run(
    engines: &dyn Engines,
    cfg: &Config,
    job: &PipelineJob<'_>,
    on_phase: &(dyn Fn(PipelinePhase) + Send + Sync),
    cancel: &CancellationToken,
) -> Result<Output, PipelineError> {
    reject_silent(job.clip)?;
    on_phase(PipelinePhase::Transcribing);
    let mut out = transcribe(engines, cfg, job, cancel).await?;
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

async fn transcribe(
    engines: &dyn Engines,
    cfg: &Config,
    job: &PipelineJob<'_>,
    cancel: &CancellationToken,
) -> Result<Output, PipelineError> {
    let started = Instant::now();
    let asr = engines.transcriber(cfg).await.map_err(PipelineError::Failed)?;
    let transcript = or_cancel(
        cancel,
        asr.transcribe(&AsrRequest {
            clip: job.clip,
            language: cfg.general.preferred_language.code(),
            hotwords: job.wordbook,
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
    job: &PipelineJob<'_>,
    on_phase: &(dyn Fn(PipelinePhase) + Send + Sync),
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
    on_phase(PipelinePhase::Polishing { input_chars: out.raw.chars().count(), started });
    let polish_job = PolishJob {
        prompts: &cfg.prompts,
        style: job.style.clone(),
        text: &out.raw,
        wordbook: polish_wordbook(cfg, job.wordbook),
        lang: None,
    };
    match or_cancel(cancel, polish_text(polisher.as_ref(), polish_job)).await? {
        Ok(text) => out.polished = Some(text),
        Err(e) => out.polish_error = Some(e),
    }
    out.polish_ms = started.elapsed().as_millis() as u64;
    Ok(())
}

/// What one polish call needs: the text and everything its system prompt is built from.
pub struct PolishJob<'a> {
    /// Passed in rather than read from the config because the Prompts page previews unsaved drafts.
    pub prompts: &'a Prompts,
    pub style: Style,
    pub text: &'a str,
    /// Spelling references for the prompt: the saved wordbook run through [`polish_wordbook`], or none.
    pub wordbook: &'a [String],
    /// Prompt language; `None` detects it from `text`, as dictation does.
    pub lang: Option<Lang>,
}

/// The wordbook words the polish prompt may carry: all of `words` when the user turned on
/// `wordbook_for_polish`, none otherwise. Recognition hotwords are not subject to it.
pub fn polish_wordbook<'a>(cfg: &Config, words: &'a [String]) -> &'a [String] {
    if cfg.wordbook_for_polish { words } else { &[] }
}

/// Polish `job.text` with `polisher`. The one place that builds the system prompt and calls the model; dictation,
/// polishing a history entry again and the Prompts page preview all use it.
pub async fn polish_text(polisher: &dyn Polisher, job: PolishJob<'_>) -> ProviderResult<String> {
    let PolishJob { prompts, style, text, wordbook, lang } = job;
    let input = PromptInput { prompts, style, text, wordbook };
    let system = match lang {
        Some(lang) => build_system_prompt_for_language(&input, lang),
        None => build_system_prompt(&input),
    };
    polisher.polish(&PolishRequest { system: &system, text }).await
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
        run(e, cfg, &PipelineJob { clip, style: Style::General, wordbook: &[] }, &|_| {}, &CancellationToken::new())
            .await
    }

    #[tokio::test]
    async fn recognizes_then_polishes_in_order() {
        let e = engines(MockTranscriber::new("えーと 会議です"), MockPolisher::default());
        let phases = std::sync::Mutex::new(Vec::new());
        let out = run(
            &e,
            &Config::default(),
            &PipelineJob { clip: &tone(), style: Style::General, wordbook: &[] },
            &|p| phases.lock().unwrap().push(p),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert!(matches!(
            phases.lock().unwrap().as_slice(),
            [PipelinePhase::Transcribing, PipelinePhase::Polishing { input_chars: 8, .. }]
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
        let on_phase = move |p: PipelinePhase| {
            if matches!(p, PipelinePhase::Polishing { .. }) {
                s2.fetch_add(1, Ordering::SeqCst);
                c2.cancel();
            }
        };
        let r = run(
            &e,
            &Config::default(),
            &PipelineJob { clip: &tone(), style: Style::General, wordbook: &[] },
            &on_phase,
            &cancel,
        )
        .await;
        assert!(matches!(r, Err(PipelineError::Cancelled)));
        assert_eq!(seen.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn wordbook_reaches_polish_only_when_enabled() {
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
        let job = PipelineJob { clip: &clip, style: Style::General, wordbook: &dict };
        let mut cfg = Config::default();
        run(&e, &cfg, &job, &|_| {}, &CancellationToken::new()).await.unwrap();
        assert!(!spy.0.lock().unwrap().contains("<reference_vocabulary>"));
        cfg.wordbook_for_polish = true;
        run(&e, &cfg, &job, &|_| {}, &CancellationToken::new()).await.unwrap();
        assert!(spy.0.lock().unwrap().contains("Acme"));
    }

    #[tokio::test]
    async fn polish_text_uses_the_given_prompt_language_instead_of_detecting_it() {
        struct Capture(std::sync::Mutex<(String, String)>);
        #[async_trait::async_trait]
        impl Polisher for Capture {
            async fn polish(&self, r: &PolishRequest<'_>) -> ProviderResult<String> {
                *self.0.lock().unwrap() = (r.system.to_string(), r.text.to_string());
                Ok("ok".into())
            }
        }
        let capture = Capture(Default::default());
        let prompts = Prompts::default();
        let text = "hello there";
        let job = |lang| PolishJob { prompts: &prompts, style: Style::General, text, wordbook: &[], lang };

        polish_text(&capture, job(None)).await.unwrap();
        let (system, sent) = capture.0.lock().unwrap().clone();
        assert!(system.starts_with(prompts.en.base.trim()), "no language given: the English text picks English");
        assert_eq!(sent, text);

        polish_text(&capture, job(Some(Lang::Ja))).await.unwrap();
        let (system, sent) = capture.0.lock().unwrap().clone();
        assert!(system.starts_with(prompts.ja.base.trim()), "a given language wins over the text");
        assert_eq!(sent, text);
    }
}
