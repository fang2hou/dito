//! Where the live meeting notes get their model: the chat model in use, and only when it runs on this Mac. A cloud
//! chat model is never asked, whatever the other settings say, so the notes keep the meeting off the network
//! (ADR-0002, ADR-0008).

use std::sync::Arc;

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
            if !chat_model_is_local(&self.st) {
                return Err(Unavailable::NeedsLocalModel);
            }
            let engine = self.st.cfg.get().chat_engine().into_owned();
            let facts = chat_model_facts(&self.st);
            // Held until the request ends, so the keeper never unloads the model under it.
            let hold = chat_reply_started(&self.st);
            let model =
                self.st.factory.text_model_for(&engine).await.map_err(|e| Unavailable::ModelFailed(e.to_string()))?;
            Ok(NotesModel {
                model,
                limits: facts.limits,
                thinking_off: facts.thinking.is_some_and(|t| t.can_disable),
                hold: Some(Box::new(hold)),
            })
        })
    }
}
