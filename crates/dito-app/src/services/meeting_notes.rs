//! Where the live meeting notes get their model: the chat model in use, on this Mac or in the cloud, as for any chat
//! (ADR-0008).

use std::sync::Arc;

use dito_core::config::Source;
use dito_core::factory::LocalBackend;
use dito_core::health::EngineStatus;
use dito_core::meeting_notes::{ModelFuture, NotesModel, NotesModelSource, Unavailable};

use super::{chat_model_facts, chat_model_is_local, chat_reply_started};
use crate::state::AppState;

struct ChatNotes {
    st: Arc<AppState>,
}

pub fn notes_model_source(st: &Arc<AppState>) -> Arc<dyn NotesModelSource> {
    Arc::new(ChatNotes { st: st.clone() })
}

impl NotesModelSource for ChatNotes {
    fn loading(&self) -> bool {
        let engine = self.st.cfg.get().chat_engine().into_owned();
        chat_model_is_local(&self.st) && self.st.local.status(&engine.local_model) != EngineStatus::Running
    }

    fn model(&self) -> ModelFuture<'_> {
        Box::pin(async {
            let engine = self.st.cfg.get().chat_engine().into_owned();
            let facts = chat_model_facts(&self.st);
            // Held until the request ends, so the keeper never unloads the model under it.
            let hold = chat_reply_started(&self.st);
            let model =
                self.st.factory.text_model_for(&engine).await.map_err(|e| Unavailable::ModelFailed(e.to_string()))?;
            let max_tokens = match engine.source {
                Source::Local => engine.local_params_for(&engine.local_model).max_tokens,
                Source::Provider => engine.active_text().and_then(|text| text.params.max_tokens),
            };
            Ok(NotesModel {
                model,
                limits: facts.limits,
                thinking_off: facts.thinking.is_some_and(|t| t.can_disable),
                max_tokens,
                hold: Some(Box::new(hold)),
            })
        })
    }
}
