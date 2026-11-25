//! Transcribe, then polish, for one recorded clip; recording and delivery belong to the controller.

use std::borrow::Cow;
use std::time::Instant;

pub use tokio_util::sync::CancellationToken;

use crate::audio::AudioClip;
use crate::config::Config;
use crate::error::{ErrorKind, ProviderError, ProviderResult};
use crate::factory::Engines;
use crate::health::EngineStatus;
use crate::i18n::Lang;
use crate::prompts::{AppTarget, PromptInput, Prompts, SCAFFOLD_MARKERS, build_prompt, detect_lang};
use crate::provider::{AsrRequest, TextModel, TextRequest};
use crate::spoken_forms;
use crate::trace_log;

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

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("pipeline cancelled")]
    Cancelled,
    #[error(transparent)]
    Failed(ProviderError),
}

pub struct PipelineJob<'a> {
    pub clip: &'a AudioClip,
    pub target: AppTarget,
    pub wordbook: &'a [String],
}

pub(crate) async fn or_cancel<T>(cancel: &CancellationToken, fut: impl Future<Output = T>) -> Result<T, PipelineError> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(PipelineError::Cancelled),
        v = fut => Ok(v),
    }
}

/// A polish failure never loses the transcript.
pub async fn run(
    engines: &dyn Engines,
    cfg: &Config,
    job: &PipelineJob<'_>,
    on_phase: &(dyn Fn(PipelinePhase) + Send + Sync),
    cancel: &CancellationToken,
) -> Result<Output, PipelineError> {
    reject_silent(job.clip)?;
    on_phase(PipelinePhase::Transcribing);
    let mut out = transcribe(engines, cfg, job.clip, job.wordbook, cancel).await?;
    polish(engines, cfg, job, on_phase, cancel, &mut out).await?;
    Ok(out)
}

/// The ASR stage alone, with the same silence gate: command mode reads an instruction exactly as it was said.
pub async fn transcribe_clip(
    engines: &dyn Engines,
    cfg: &Config,
    clip: &AudioClip,
    wordbook: &[String],
    cancel: &CancellationToken,
) -> Result<Output, PipelineError> {
    reject_silent(clip)?;
    transcribe(engines, cfg, clip, wordbook, cancel).await
}

fn reject_silent(clip: &AudioClip) -> Result<(), PipelineError> {
    if !clip.is_effectively_silent() {
        return Ok(());
    }
    trace_log!(
        duration_ms = clip.duration_ms(),
        rms = %format!("{:.4}", clip.rms()),
        peak = %format!("{:.4}", clip.peak()),
        "clip rejected as silent"
    );
    Err(PipelineError::Failed(ProviderError::new(ErrorKind::NoSpeech, "silent or too short")))
}

async fn transcribe(
    engines: &dyn Engines,
    cfg: &Config,
    clip: &AudioClip,
    wordbook: &[String],
    cancel: &CancellationToken,
) -> Result<Output, PipelineError> {
    let started = Instant::now();
    let asr = engines.transcriber(cfg).await.map_err(PipelineError::Failed)?;
    let language = cfg.general.preferred_language.code();
    trace_log!(
        engine = %cfg.asr.label(),
        audio_ms = clip.duration_ms(),
        language = ?language,
        hotwords = wordbook.len(),
        "asr request"
    );
    let mut transcript =
        match or_cancel(cancel, asr.transcribe(&AsrRequest { clip, language, hotwords: wordbook })).await? {
            Ok(transcript) => transcript,
            Err(error) => {
                trace_log!(elapsed_ms = started.elapsed().as_millis() as u64, %error, "asr failed");
                return Err(PipelineError::Failed(error));
            }
        };
    let asr_ms = started.elapsed().as_millis() as u64;
    trace_log!(asr_ms, detected = ?transcript.language, transcript = ?transcript.text, "asr transcript");
    let raw_before = std::mem::take(&mut transcript.text);
    // The language is read from what ASR heard: a transcript of number words alone is English, and `24/7` is not.
    let lang = transcript_lang(transcript.language.as_deref(), &raw_before);
    transcript.text = restore_written_forms(raw_before, wordbook, "asr");
    if transcript.text.trim().is_empty() {
        return Err(PipelineError::Failed(ProviderError::new(ErrorKind::NoSpeech, "nothing was transcribed")));
    }
    Ok(Output { raw: transcript.text, polished: None, polish_error: None, lang, asr_ms, polish_ms: 0 })
}

/// Leaves `out` untouched when polishing is off.
async fn polish(
    engines: &dyn Engines,
    cfg: &Config,
    job: &PipelineJob<'_>,
    on_phase: &(dyn Fn(PipelinePhase) + Send + Sync),
    cancel: &CancellationToken,
    out: &mut Output,
) -> Result<(), PipelineError> {
    if !cfg.polish.enabled {
        trace_log!("polish skipped, it is switched off");
        return Ok(());
    }

    let started = Instant::now();
    on_phase(PipelinePhase::Polishing { input_chars: out.raw.chars().count(), started });
    let resolved = resolve_polish_model(engines, cfg, cancel).await?;
    let prompt = build_prompt(&PromptInput {
        prompts: &cfg.prompts,
        target: job.target.clone(),
        text: &out.raw,
        wordbook: polish_wordbook(cfg, job.wordbook),
        lang: out.lang,
    });
    let request = TextRequest { system: &prompt.system, examples: prompt.examples, text: &prompt.user };
    let result = attempt_polish(engines, cfg, cancel, resolved, &request).await?;
    match result {
        Ok(Some(text)) => out.polished = Some(restore_written_forms(text, job.wordbook, "polish")),
        Ok(None) => {}
        Err(error) => out.polish_error = Some(error),
    }
    out.polish_ms = started.elapsed().as_millis() as u64;
    Ok(())
}

/// Writes each wordbook term as the term where the text spells it as it sounds. A hotword is only a hint to ASR and a
/// style rule may spell `1on1` out again, so the transcript and the polished text both go through here, whether or not
/// the wordbook is given to polish.
fn restore_written_forms(text: String, wordbook: &[String], stage: &'static str) -> String {
    let restored = match spoken_forms::normalize(&text, wordbook) {
        Cow::Borrowed(_) => None,
        Cow::Owned(restored) => Some(restored),
    };
    match restored {
        None => text,
        Some(restored) => {
            trace_log!(stage, before = ?text, after = ?restored, "spoken forms restored");
            restored
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Recovery {
    Reuse,
    Resolve,
    Preconnect,
    None,
}

impl Recovery {
    fn of(status: Option<&EngineStatus>) -> Self {
        match status {
            Some(EngineStatus::Running) => Self::Reuse,
            Some(EngineStatus::Loading | EngineStatus::Stopped | EngineStatus::Stopping | EngineStatus::Failed(_)) => {
                Self::Resolve
            }
            Some(EngineStatus::Remote) | None => Self::Preconnect,
            Some(EngineStatus::NotInstalled) => Self::None,
        }
    }

    fn initial_action(self) -> &'static str {
        match self {
            Self::Preconnect => "warm up and resolve",
            Self::Reuse | Self::Resolve => "resolve text model",
            Self::None => "do not retry",
        }
    }

    fn attempt_action(self) -> &'static str {
        match self {
            Self::Reuse => "reuse running model",
            Self::Resolve => "resolve text model",
            Self::Preconnect => "preconnect cached model",
            Self::None => "do not retry",
        }
    }
}

enum ResolvedModel {
    First(std::sync::Arc<dyn TextModel>),
    Recovered(std::sync::Arc<dyn TextModel>),
    Skipped,
    Failed(ProviderError),
}

async fn resolve_polish_model(
    engines: &dyn Engines,
    cfg: &Config,
    cancel: &CancellationToken,
) -> Result<ResolvedModel, PipelineError> {
    let first = or_cancel(cancel, engines.text_model(cfg)).await?;
    match first {
        Ok(Some(model)) => Ok(ResolvedModel::First(model)),
        Ok(None) => Ok(ResolvedModel::Skipped),
        Err(error) if !error.is_retryable() => {
            trace_log!(error = %error, "polish engine unavailable, the transcript is delivered");
            Ok(ResolvedModel::Failed(error))
        }
        Err(error) => {
            let status = engines.text_model_status(cfg);
            let recovery = Recovery::of(status.as_ref());
            if recovery == Recovery::None {
                return Ok(ResolvedModel::Failed(error));
            }
            trace_log!(error = ?error.kind, engine_status = ?status, action = recovery.initial_action(), "polish retry started");
            if recovery == Recovery::Preconnect
                && let Err(cancelled) = or_cancel(cancel, engines.warm_up(cfg)).await
            {
                trace_polish_retry(false, None);
                return Err(cancelled);
            }
            let resolved = match or_cancel(cancel, engines.text_model(cfg)).await {
                Ok(resolved) => resolved,
                Err(cancelled) => {
                    trace_polish_retry(false, None);
                    return Err(cancelled);
                }
            };
            match resolved {
                Ok(Some(model)) => Ok(ResolvedModel::Recovered(model)),
                Ok(None) => {
                    trace_polish_retry(false, Some(error.kind));
                    Ok(ResolvedModel::Failed(error))
                }
                Err(recovery_error) => {
                    trace_polish_retry(false, Some(recovery_error.kind));
                    Ok(ResolvedModel::Failed(recovery_error))
                }
            }
        }
    }
}

async fn attempt_polish(
    engines: &dyn Engines,
    cfg: &Config,
    cancel: &CancellationToken,
    resolved: ResolvedModel,
    request: &TextRequest<'_>,
) -> Result<ProviderResult<Option<String>>, PipelineError> {
    let (model, recovered) = match resolved {
        ResolvedModel::First(model) => (model, false),
        ResolvedModel::Recovered(model) => (model, true),
        ResolvedModel::Skipped => return Ok(Ok(None)),
        ResolvedModel::Failed(error) => return Ok(Err(error)),
    };
    let first = if recovered {
        match checked_attempt(cancel, model.as_ref(), request).await {
            Err(cancelled) => {
                trace_polish_retry(false, None);
                return Err(cancelled);
            }
            Ok(first) => first,
        }
    } else {
        checked_attempt(cancel, model.as_ref(), request).await?
    };
    if recovered {
        trace_polish_retry(first.is_ok(), first.as_ref().err().map(|error| error.kind));
        return Ok(first.map(Some));
    }
    match first {
        Ok(text) => Ok(Ok(Some(text))),
        Err(error) if error.is_retryable() => retry_polish(engines, cfg, cancel, model, request, error).await,
        Err(error) => Ok(Err(error)),
    }
}

async fn retry_polish(
    engines: &dyn Engines,
    cfg: &Config,
    cancel: &CancellationToken,
    model: std::sync::Arc<dyn TextModel>,
    request: &TextRequest<'_>,
    error: ProviderError,
) -> Result<ProviderResult<Option<String>>, PipelineError> {
    let status = engines.text_model_status(cfg);
    let recovery = Recovery::of(status.as_ref());
    if recovery == Recovery::None {
        return Ok(Err(error));
    }
    trace_log!(error = ?error.kind, engine_status = ?status, action = recovery.attempt_action(), "polish retry started");
    let operation = async {
        Ok(match recovery {
            Recovery::Reuse => checked_attempt(cancel, model.as_ref(), request).await?,
            Recovery::Preconnect => {
                or_cancel(cancel, model.preconnect()).await?;
                checked_attempt(cancel, model.as_ref(), request).await?
            }
            Recovery::Resolve => match or_cancel(cancel, engines.text_model(cfg)).await? {
                Ok(Some(model)) => checked_attempt(cancel, model.as_ref(), request).await?,
                Ok(None) => Err(error),
                Err(recovery_error) => Err(recovery_error),
            },
            Recovery::None => Err(error),
        })
    };
    let result = match operation.await {
        Ok(result) => result,
        Err(cancelled) => {
            trace_polish_retry(false, None);
            return Err(cancelled);
        }
    };
    trace_polish_retry(result.is_ok(), result.as_ref().err().map(|error| error.kind));
    Ok(result.map(Some))
}

fn trace_polish_retry(succeeded: bool, error: Option<ErrorKind>) {
    trace_log!(succeeded, error = ?error, "polish retry finished");
}

fn validate_polish_output(text: String) -> ProviderResult<String> {
    if text.trim().is_empty() {
        return Err(ProviderError::retryable(ErrorKind::BadResponse, "empty model output"));
    }
    if text.lines().any(|line| SCAFFOLD_MARKERS.contains(&line) || matches!(line, "<think>" | "</think>")) {
        return Err(ProviderError::retryable(ErrorKind::BadResponse, "polish output leaked its scaffold"));
    }
    Ok(text)
}

async fn checked_attempt(
    cancel: &CancellationToken,
    text_model: &dyn TextModel,
    request: &TextRequest<'_>,
) -> Result<ProviderResult<String>, PipelineError> {
    or_cancel(cancel, checked_polish(text_model, request)).await
}

async fn checked_polish(text_model: &dyn TextModel, request: &TextRequest<'_>) -> ProviderResult<String> {
    validate_polish_output(call_text_model(text_model, "polish", request).await?)
}

pub struct PolishJob<'a> {
    /// Passed in rather than read from the config because the Prompts page previews unsaved drafts.
    pub prompts: &'a Prompts,
    pub target: AppTarget,
    pub text: &'a str,
    pub wordbook: &'a [String],
    /// Whose prompt set supplies the user's rules: the language of `text`.
    pub lang: Lang,
}

/// ASR hotwords are not subject to `wordbook_for_polish`.
pub fn polish_wordbook<'a>(cfg: &Config, words: &'a [String]) -> &'a [String] {
    if cfg.wordbook_for_polish { words } else { &[] }
}

/// The one place that builds the system prompt and calls the model; dictation, polishing a history entry again and the
/// Prompts page preview all use it.
pub async fn polish_text(text_model: &dyn TextModel, job: PolishJob<'_>) -> ProviderResult<String> {
    let PolishJob { prompts, target: style, text, wordbook, lang } = job;
    let prompt = build_prompt(&PromptInput { prompts, target: style, lang, text, wordbook });
    call_text_model(
        text_model,
        "polish",
        &TextRequest { system: &prompt.system, examples: prompt.examples, text: &prompt.user },
    )
    .await
}

/// Every model call that rewrites text goes through here, so the debug log holds the full system prompt and user message
/// that were sent for `what` (a dictation polish or the name of a command tool), the model's output and how long it took.
pub(crate) async fn call_text_model(
    text_model: &dyn TextModel,
    what: &str,
    request: &TextRequest<'_>,
) -> ProviderResult<String> {
    trace_log!(
        tool = what,
        system = ?request.system,
        examples = request.examples.len(),
        user = ?request.text,
        "polish request"
    );
    let started = Instant::now();
    let result = text_model.polish(request).await;
    let elapsed_ms = started.elapsed().as_millis() as u64;
    match &result {
        Ok(output) => trace_log!(tool = what, elapsed_ms, output = ?output, "polish output"),
        Err(error) => trace_log!(tool = what, elapsed_ms, %error, "polish failed"),
    }
    result
}

/// The language an engine reported for a transcript, or the one its script suggests when the engine gave none or one
/// Dito has no prompt set for.
pub fn transcript_lang(reported: Option<&str>, text: &str) -> Lang {
    reported.and_then(lang_from_code).unwrap_or_else(|| detect_lang(text))
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
    use crate::provider::mock::{MockTextModel, MockTranscriber};

    fn tone() -> AudioClip {
        AudioClip::new(vec![0.3; 16_000])
    }

    fn engines(asr: MockTranscriber, polish: MockTextModel) -> FixedEngines {
        FixedEngines::new(Arc::new(asr), Arc::new(polish))
    }

    async fn go(e: &dyn Engines, cfg: &Config, clip: &AudioClip) -> Result<Output, PipelineError> {
        run(
            e,
            cfg,
            &PipelineJob { clip, target: AppTarget::General, wordbook: &[] },
            &|_| {},
            &CancellationToken::new(),
        )
        .await
    }

    #[tokio::test]
    async fn transcribes_then_polishes_in_order() {
        let e = engines(MockTranscriber::new("えーと 会議です"), MockTextModel::default());
        let phases = std::sync::Mutex::new(Vec::new());
        let out = run(
            &e,
            &Config::default(),
            &PipelineJob { clip: &tone(), target: AppTarget::General, wordbook: &[] },
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
        let polish = Arc::new(MockTextModel::default());
        let e = FixedEngines::new(Arc::new(MockTranscriber::new("hello")), polish.clone());
        let mut cfg = Config::default();
        cfg.polish.enabled = false;
        let out = go(&e, &cfg, &tone()).await.unwrap();
        assert_eq!(out.text(), "hello");
        assert_eq!(polish.calls(), 0);
    }

    #[tokio::test]
    async fn polish_failure_falls_back_to_the_transcript() {
        let e = engines(MockTranscriber::new("hello"), MockTextModel::default().script([Err(ErrorKind::RateLimit)]));
        let out = go(&e, &Config::default(), &tone()).await.unwrap();
        assert_eq!(out.text(), "hello");
        assert_eq!(out.polish_error.as_ref().unwrap().kind, ErrorKind::RateLimit);
    }

    #[tokio::test]
    async fn a_retryable_polish_failure_retries_once_and_can_succeed() {
        let polish = Arc::new(MockTextModel::default().script_provider([
            Err(ProviderError::retryable(ErrorKind::Network, "before response")),
            Ok("cleaned".into()),
        ]));
        let e = FixedEngines::new(Arc::new(MockTranscriber::new("hello")), polish.clone());

        let out = go(&e, &Config::default(), &tone()).await.unwrap();

        assert_eq!(out.text(), "cleaned");
        assert_eq!(out.polish_error, None);
        assert_eq!(polish.calls(), 2);
    }

    #[tokio::test]
    async fn a_second_polish_failure_replaces_the_first_and_delivers_raw_text() {
        let polish = Arc::new(MockTextModel::default().script_provider([
            Err(ProviderError::retryable(ErrorKind::Network, "first")),
            Err(ProviderError::retryable(ErrorKind::Timeout, "second")),
        ]));
        let e = FixedEngines::new(Arc::new(MockTranscriber::new("hello")), polish.clone());

        let out = go(&e, &Config::default(), &tone()).await.unwrap();

        assert_eq!(out.text(), "hello");
        assert_eq!(out.polish_error.as_ref().unwrap().message, "second");
        assert_eq!(polish.calls(), 2);
    }

    #[tokio::test]
    async fn non_retryable_polish_failures_are_not_retried() {
        for error in [
            ProviderError::new(ErrorKind::Auth, "bad credentials"),
            ProviderError::new(ErrorKind::RateLimit, "slow down"),
            ProviderError::new(ErrorKind::Timeout, "response started"),
        ] {
            let polish = Arc::new(MockTextModel::default().script_provider([Err(error)]));
            let e = FixedEngines::new(Arc::new(MockTranscriber::new("hello")), polish.clone());
            let out = go(&e, &Config::default(), &tone()).await.unwrap();
            assert_eq!(out.text(), "hello");
            assert_eq!(polish.calls(), 1);
        }
    }

    #[tokio::test]
    async fn empty_output_is_retried_before_a_good_output_is_accepted() {
        let polish = Arc::new(MockTextModel::default().script_provider([Ok("  ".into()), Ok("good".into())]));
        let e = FixedEngines::new(Arc::new(MockTranscriber::new("hello")), polish.clone());

        let out = go(&e, &Config::default(), &tone()).await.unwrap();

        assert_eq!(out.text(), "good");
        assert_eq!(polish.calls(), 2);
    }

    struct RecoveringEngines {
        base: FixedEngines,
        status: Option<EngineStatus>,
        resolutions: AtomicUsize,
        fail_resolution: usize,
    }

    #[async_trait::async_trait]
    impl Engines for RecoveringEngines {
        async fn transcriber(&self, cfg: &Config) -> ProviderResult<std::sync::Arc<dyn crate::provider::Transcriber>> {
            self.base.transcriber(cfg).await
        }

        async fn text_model(&self, cfg: &Config) -> ProviderResult<Option<std::sync::Arc<dyn TextModel>>> {
            let resolution = self.resolutions.fetch_add(1, Ordering::SeqCst) + 1;
            if resolution == self.fail_resolution {
                return Err(ProviderError::retryable(ErrorKind::Unavailable, "reload failed"));
            }
            self.base.text_model(cfg).await
        }

        async fn router(&self, cfg: &Config) -> ProviderResult<std::sync::Arc<dyn crate::provider::Router>> {
            self.base.router(cfg).await
        }

        fn text_model_status(&self, _: &Config) -> Option<EngineStatus> {
            self.status.clone()
        }
    }

    #[tokio::test]
    async fn a_stopped_local_model_is_resolved_again_before_retry() {
        let polish = Arc::new(MockTextModel::default().script_provider([
            Err(ProviderError::retryable(ErrorKind::Unavailable, "stopped")),
            Ok("recovered".into()),
        ]));
        let engines = RecoveringEngines {
            base: FixedEngines::new(Arc::new(MockTranscriber::new("hello")), polish.clone()),
            status: Some(EngineStatus::Stopped),
            resolutions: AtomicUsize::new(0),
            fail_resolution: usize::MAX,
        };

        let out = go(&engines, &Config::default(), &tone()).await.unwrap();

        assert_eq!(out.text(), "recovered");
        assert_eq!(polish.calls(), 2);
        assert_eq!(engines.resolutions.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_retryable_initial_model_resolution_is_attempted_once_more() {
        let polish = Arc::new(MockTextModel::default().script_provider([Ok("loaded".into())]));
        let engines = RecoveringEngines {
            base: FixedEngines::new(Arc::new(MockTranscriber::new("hello")), polish.clone()),
            status: Some(EngineStatus::Stopped),
            resolutions: AtomicUsize::new(0),
            fail_resolution: 1,
        };

        let out = go(&engines, &Config::default(), &tone()).await.unwrap();

        assert_eq!(out.text(), "loaded");
        assert_eq!(polish.calls(), 1);
        assert_eq!(engines.resolutions.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_failed_local_model_recovery_delivers_raw_text_without_a_second_call() {
        let polish = Arc::new(
            MockTextModel::default()
                .script_provider([Err(ProviderError::retryable(ErrorKind::Unavailable, "stopped"))]),
        );
        let engines = RecoveringEngines {
            base: FixedEngines::new(Arc::new(MockTranscriber::new("hello")), polish.clone()),
            status: Some(EngineStatus::Stopped),
            resolutions: AtomicUsize::new(0),
            fail_resolution: 2,
        };

        let out = go(&engines, &Config::default(), &tone()).await.unwrap();

        assert_eq!(out.text(), "hello");
        assert_eq!(out.polish_error.as_ref().unwrap().message, "reload failed");
        assert_eq!(polish.calls(), 1);
    }

    #[tokio::test]
    async fn a_model_that_is_not_installed_is_not_retried() {
        let polish = Arc::new(
            MockTextModel::default()
                .script_provider([Err(ProviderError::retryable(ErrorKind::Unavailable, "not installed"))]),
        );
        let engines = RecoveringEngines {
            base: FixedEngines::new(Arc::new(MockTranscriber::new("hello")), polish.clone()),
            status: Some(EngineStatus::NotInstalled),
            resolutions: AtomicUsize::new(0),
            fail_resolution: usize::MAX,
        };

        let out = go(&engines, &Config::default(), &tone()).await.unwrap();

        assert_eq!(out.text(), "hello");
        assert_eq!(polish.calls(), 1);
        assert_eq!(engines.resolutions.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn unchanged_output_is_accepted_without_a_retry() {
        let polish = Arc::new(MockTextModel::default().script_provider([Ok("hello".into())]));
        let e = FixedEngines::new(Arc::new(MockTranscriber::new("hello")), polish.clone());

        let out = go(&e, &Config::default(), &tone()).await.unwrap();

        assert_eq!(out.text(), "hello");
        assert_eq!(polish.calls(), 1);
    }

    #[test]
    fn polish_output_check_rejects_only_known_broken_shapes() {
        for marker in SCAFFOLD_MARKERS.into_iter().chain(["<think>", "</think>"]) {
            let error = validate_polish_output(format!("before\n{marker}\nafter")).unwrap_err();
            assert!(error.is_retryable(), "{marker}");
        }
        for text in ["hello", "x", "hello hello hello", &"x".repeat(20_000)] {
            assert_eq!(validate_polish_output(text.to_string()).unwrap(), text);
        }
    }

    #[tokio::test]
    async fn asr_failures_surface_with_their_class() {
        let e = engines(MockTranscriber::new("x").script([Err(ErrorKind::Auth)]), MockTextModel::default());
        match go(&e, &Config::default(), &tone()).await {
            Err(PipelineError::Failed(err)) => assert_eq!(err.kind, ErrorKind::Auth),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_empty_transcript_is_no_speech_and_is_not_polished() {
        let polish = Arc::new(MockTextModel::default());
        let e = FixedEngines::new(Arc::new(MockTranscriber::new("  ")), polish.clone());
        match go(&e, &Config::default(), &tone()).await {
            Err(PipelineError::Failed(err)) => assert_eq!(err.kind, ErrorKind::NoSpeech),
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(polish.calls(), 0);
    }

    #[tokio::test]
    async fn silent_audio_never_reaches_the_engine() {
        let asr = Arc::new(MockTranscriber::new("x"));
        let e = FixedEngines::new(asr.clone(), Arc::new(MockTextModel::default()));
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
            MockTextModel::default().with_delay(std::time::Duration::from_secs(30)),
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
            &PipelineJob { clip: &tone(), target: AppTarget::General, wordbook: &[] },
            &on_phase,
            &cancel,
        )
        .await;
        assert!(matches!(r, Err(PipelineError::Cancelled)));
        assert_eq!(seen.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cancelling_during_the_second_attempt_reports_cancelled() {
        struct CancelSecond {
            calls: AtomicUsize,
            cancel: CancellationToken,
        }
        #[async_trait::async_trait]
        impl TextModel for CancelSecond {
            async fn polish(&self, _: &TextRequest<'_>) -> ProviderResult<String> {
                if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    return Err(ProviderError::retryable(ErrorKind::Network, "before response"));
                }
                self.cancel.cancel();
                std::future::pending().await
            }
        }
        let cancel = CancellationToken::new();
        let model = Arc::new(CancelSecond { calls: AtomicUsize::new(0), cancel: cancel.clone() });
        let e = FixedEngines::new(Arc::new(MockTranscriber::new("hello")), model.clone());

        let result = run(
            &e,
            &Config::default(),
            &PipelineJob { clip: &tone(), target: AppTarget::General, wordbook: &[] },
            &|_| {},
            &cancel,
        )
        .await;

        assert!(matches!(result, Err(PipelineError::Cancelled)));
        assert_eq!(model.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn wordbook_terms_are_written_as_themselves_before_and_after_polish() {
        // Spells the term out again, and remembers the text the model was given.
        struct SpellsOut(std::sync::Mutex<String>);
        #[async_trait::async_trait]
        impl TextModel for SpellsOut {
            async fn polish(&self, r: &TextRequest<'_>) -> ProviderResult<String> {
                *self.0.lock().unwrap() = r.text.to_string();
                Ok("Let's do a one-on-one.".into())
            }
        }
        async fn dictate(e: &dyn Engines, cfg: &Config, wordbook: &[String]) -> Output {
            run(
                e,
                cfg,
                &PipelineJob { clip: &tone(), target: AppTarget::General, wordbook },
                &|_| {},
                &CancellationToken::new(),
            )
            .await
            .unwrap()
        }
        let said = || Arc::new(MockTranscriber::new("let's do a one on one"));
        let dict = vec!["1on1".to_string()];
        let mut cfg = Config::default();
        cfg.polish.enabled = false;

        let e = FixedEngines::new(said(), Arc::new(MockTextModel::default()));
        let out = dictate(&e, &cfg, &dict).await;
        assert_eq!((out.raw.as_str(), out.polished.as_deref()), ("let's do a 1on1", None));

        cfg.polish.enabled = true;
        let model = Arc::new(SpellsOut(std::sync::Mutex::new(String::new())));
        let e = FixedEngines::new(said(), model.clone());
        let out = dictate(&e, &cfg, &dict).await;
        assert_eq!(crate::prompts::unfenced(&model.0.lock().unwrap()), "let's do a 1on1");
        assert_eq!(out.polished.as_deref(), Some("Let's do a 1on1."));
    }

    #[tokio::test]
    async fn polish_rules_follow_the_language_of_what_asr_heard() {
        struct Spy(std::sync::Mutex<String>);
        #[async_trait::async_trait]
        impl TextModel for Spy {
            async fn polish(&self, r: &TextRequest<'_>) -> ProviderResult<String> {
                *self.0.lock().unwrap() = r.system.to_string();
                Ok("ok".into())
            }
        }
        let spy = Arc::new(Spy(Default::default()));
        let e = FixedEngines::new(Arc::new(MockTranscriber::new("twenty four seven")), spy.clone());
        let cfg = Config::default();
        let dict = vec!["24/7".to_string()];
        let clip = tone();
        let out = run(
            &e,
            &cfg,
            &PipelineJob { clip: &clip, target: AppTarget::General, wordbook: &dict },
            &|_| {},
            &CancellationToken::new(),
        )
        .await
        .unwrap();

        assert_eq!(out.raw, "24/7");
        assert_eq!(out.lang, Lang::En);
        let system = spy.0.lock().unwrap();
        assert!(system.contains(cfg.prompts.en.base.trim()) && !system.contains(cfg.prompts.ja.base.trim()));
    }

    #[tokio::test]
    async fn wordbook_reaches_polish_only_when_enabled() {
        struct Spy(std::sync::Mutex<String>);
        #[async_trait::async_trait]
        impl crate::provider::TextModel for Spy {
            async fn polish(&self, r: &TextRequest<'_>) -> crate::error::ProviderResult<String> {
                *self.0.lock().unwrap() = r.system.to_string();
                Ok("ok".into())
            }
        }
        let spy = Arc::new(Spy(Default::default()));
        let e = FixedEngines::new(Arc::new(MockTranscriber::new("Acme")), spy.clone());
        let dict = vec!["Acme".to_string()];
        let clip = tone();
        let job = PipelineJob { clip: &clip, target: AppTarget::General, wordbook: &dict };
        let mut cfg = Config::default();
        run(&e, &cfg, &job, &|_| {}, &CancellationToken::new()).await.unwrap();
        assert!(!spy.0.lock().unwrap().contains("<reference_vocabulary>"));
        cfg.wordbook_for_polish = true;
        run(&e, &cfg, &job, &|_| {}, &CancellationToken::new()).await.unwrap();
        assert!(spy.0.lock().unwrap().contains("Acme"));
    }

    #[tokio::test]
    async fn polish_text_sends_the_rules_of_the_given_language_and_the_transcript_as_the_last_turn() {
        struct Capture(std::sync::Mutex<(String, usize, String)>);
        #[async_trait::async_trait]
        impl TextModel for Capture {
            async fn polish(&self, r: &TextRequest<'_>) -> ProviderResult<String> {
                *self.0.lock().unwrap() = (r.system.to_string(), r.examples.len(), r.text.to_string());
                Ok("ok".into())
            }
        }
        let capture = Capture(Default::default());
        let prompts = Prompts::default();
        let text = "hello there";
        let job = |lang| PolishJob { prompts: &prompts, target: AppTarget::General, text, wordbook: &[], lang };

        polish_text(&capture, job(Lang::En)).await.unwrap();
        let (english, examples, sent) = capture.0.lock().unwrap().clone();
        assert!(english.contains(prompts.en.base.trim()) && !english.contains(prompts.ja.base.trim()));
        assert!(examples > 0, "the demonstrations travel with the request");
        assert_eq!(crate::prompts::unfenced(&sent), text);

        polish_text(&capture, job(Lang::Ja)).await.unwrap();
        let japanese = capture.0.lock().unwrap().0.clone();
        assert!(japanese.contains(prompts.ja.base.trim()), "the language is the caller's, never read from the text");
    }

    #[test]
    fn a_transcript_takes_the_language_its_engine_reported() {
        assert_eq!(transcript_lang(Some("zh-CN"), "うん、接下来"), Lang::Zh);
        assert_eq!(transcript_lang(None, "明日の会議"), Lang::Ja);
        assert_eq!(transcript_lang(Some("ko"), "hello"), Lang::En, "no prompt set for it: the script decides");
    }
}
