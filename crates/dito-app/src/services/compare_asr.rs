//! One contender of a model comparison transcribing the round's recording (`design:settings/compare-models`).

use std::sync::Arc;
use std::time::Instant;

use dito_core::asr_compare::Contender;
use dito_core::audio::AudioClip;
use dito_core::config::Source;
use dito_core::factory::LocalBackend;
use dito_core::health::EngineStatus;
use dito_core::provider::AsrRequest;

use crate::state::AppState;

/// The text, or why there is none. `millis` times the request alone, not the loading of a local model.
pub struct Heard {
    pub text: Result<String, String>,
    pub millis: u64,
}

/// Transcribes as dictation would: the preferred language and the wordbook as hotwords. A local model that this
/// loaded, and that nothing uses, is unloaded again, so a comparison holds at most one extra model in memory.
pub async fn compare_transcribe(st: Arc<AppState>, contender: Contender, clip: Arc<AudioClip>) -> Heard {
    let st2 = st.clone();
    let job = st.rt.spawn(async move {
        let cfg = st2.cfg.get();
        let loaded_before = match &contender {
            Contender::Local(id) => st2.local.status(id) == EngineStatus::Running,
            Contender::Cloud(_) => true,
        };
        let store = st2.store.clone();
        let words = st2.rt.spawn_blocking(move || store.words(None).unwrap_or_default()).await.unwrap_or_default();
        // Rebuilt so every cloud contender is timed from a new connection: a shared engine would favor the one
        // dictation has just used.
        let heard = match st2.factory.rebuild_transcriber_for(&contender.engine(&cfg.asr)).await {
            Ok(engine) => {
                let started = Instant::now();
                let request =
                    AsrRequest { clip: &clip, language: cfg.general.preferred_language.code(), hotwords: &words };
                let text = engine.transcribe(&request).await;
                let millis = started.elapsed().as_millis() as u64;
                match text {
                    Ok(transcript) if transcript.text.trim().is_empty() => {
                        Heard { text: Err("nothing was transcribed".into()), millis }
                    }
                    Ok(transcript) => Heard { text: Ok(transcript.text.trim().to_owned()), millis },
                    Err(error) => Heard { text: Err(error.to_string()), millis },
                }
            }
            Err(error) => Heard { text: Err(error.to_string()), millis: 0 },
        };
        if let Contender::Local(id) = &contender {
            let in_use = cfg.asr.source == Source::Local && cfg.asr.local_model == *id;
            if !loaded_before && !in_use {
                st2.local.unload(id);
            }
        }
        heard
    });
    job.await.unwrap_or_else(|error| Heard { text: Err(error.to_string()), millis: 0 })
}
