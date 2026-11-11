//! Operations that talk to the configured engines.

use std::sync::Arc;

use dito_core::error::ErrorKind;
use dito_core::pipeline::{self, PolishJob};
use dito_core::provider::{TestOutcome, test_polisher, test_transcriber_with_language};
use dito_core::secrets::Role;

use crate::state::AppState;

pub async fn test_connection(st: Arc<AppState>, role: Role) -> TestOutcome {
    let cfg = st.cfg.get();
    let engine = match role {
        Role::Asr => &cfg.asr,
        Role::Polish => &cfg.polish.engine,
    };
    let language = cfg.general.preferred_language.code();
    let st2 = st.clone();
    let engine = engine.clone();
    let job = st.rt.spawn(async move {
        match role {
            Role::Asr => match st2.factory.transcriber_for(&engine).await {
                Ok(t) => test_transcriber_with_language(t.as_ref(), language).await,
                Err(e) => TestOutcome::Failed { kind: e.kind, message: e.message },
            },
            Role::Polish => match st2.factory.polisher_for(&engine).await {
                Ok(p) => test_polisher(p.as_ref()).await,
                Err(e) => TestOutcome::Failed { kind: e.kind, message: e.message },
            },
        }
    });
    job.await.unwrap_or_else(|e| TestOutcome::Failed { kind: ErrorKind::Other, message: e.to_string() })
}

/// Uses the draft and its transient key; saved settings stay untouched.
pub async fn test_asr_draft(
    st: Arc<AppState>,
    config: serde_json::Value,
    key: String,
    language: dito_core::config::PreferredLanguage,
) -> TestOutcome {
    st.rt
        .spawn(async move {
            match dito_core::provider::configured_asr::ConfiguredAsr::new(config, key) {
                Ok(engine) => test_transcriber_with_language(&engine, language.code()).await,
                Err(error) => TestOutcome::Failed { kind: error.kind, message: error.message },
            }
        })
        .await
        .unwrap_or_else(|error| TestOutcome::Failed { kind: ErrorKind::Other, message: error.to_string() })
}

/// Uses the given prompts rather than the saved ones.
pub async fn try_polish(
    st: Arc<AppState>,
    prompts: dito_core::prompts::Prompts,
    style: dito_core::prompts::Style,
    lang: dito_core::i18n::Lang,
    text: String,
) -> Result<(String, u64), String> {
    let st2 = st.clone();
    st.rt
        .spawn(async move {
            let cfg = st2.cfg.get();
            let polisher = st2.factory.polisher_for(&cfg.polish.engine).await.map_err(|e| e.to_string())?;
            let polish_job = PolishJob { prompts: &prompts, style, text: &text, wordbook: &[], lang: Some(lang) };
            let started = std::time::Instant::now();
            let out = pipeline::polish_text(polisher.as_ref(), polish_job).await.map_err(|e| e.to_string())?;
            Ok((out, started.elapsed().as_millis() as u64))
        })
        .await
        .map_err(|e| e.to_string())?
}
