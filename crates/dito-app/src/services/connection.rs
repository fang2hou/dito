//! Operations that talk to the currently configured engines: the connection test and the
//! prompt page's "try it" panel.

use std::sync::Arc;

use dito_core::error::ErrorKind;
use dito_core::prompts::{PromptInput, build_system_prompt, detect_lang};
use dito_core::provider::PolishRequest;
use dito_core::provider::{TestOutcome, test_polisher, test_transcriber_with_language};
use dito_core::secrets::Role;

use crate::state::AppState;

/// Run the one-button connection test for `role` against the currently configured engine.
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

/// Polish `text` with the given prompt set (the prompt page's "try it" panel).
pub async fn try_polish(
    st: Arc<AppState>,
    prompts: dito_core::prompts::Prompts,
    style: dito_core::prompts::Style,
    text: String,
) -> Result<(String, u64), String> {
    let st2 = st.clone();
    st.rt
        .spawn(async move {
            let cfg = st2.cfg.get();
            let polisher = st2.factory.polisher_for(&cfg.polish.engine).await.map_err(|e| e.to_string())?;
            let _ = detect_lang(&text);
            let system = build_system_prompt(&PromptInput { prompts: &prompts, style, text: &text, dictionary: &[] });
            let started = std::time::Instant::now();
            let out =
                polisher.polish(&PolishRequest { system: &system, text: &text }).await.map_err(|e| e.to_string())?;
            Ok((out, started.elapsed().as_millis() as u64))
        })
        .await
        .map_err(|e| e.to_string())?
}
