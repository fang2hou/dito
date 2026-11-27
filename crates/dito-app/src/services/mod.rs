//! App-level operations on shared state. The re-exports below are the whole public surface: views call
//! `services::<fn>`, never the submodules.

mod chat_model;
mod compare_asr;
mod connection;
mod find_words;
mod history;
mod local_api;
mod meeting_notes;
mod own_window_sink;
mod queries;
mod save_config;
mod store_secret;
mod watch_config;

pub use chat_model::{
    ChatModelKeeper, ModelFacts, chat_left, chat_model_facts, chat_model_is_local, chat_model_label, chat_opened,
    chat_reply_started, chat_thinking_choice, follow_chat_model,
};
pub use compare_asr::{Heard, compare_transcribe};
pub use connection::{test_asr_draft, test_connection, try_polish};
pub use find_words::{WordSearchError, find_words};
pub use history::{delete_history_entry, history_entry, polish_again, transcribe_again};
pub use local_api::InstalledCatalog;
pub use meeting_notes::notes_model_source;
pub use own_window_sink::{OwnWindowInserts, own_window_sink, serve_own_window_inserts};
pub use queries::{
    StatsSnapshot, WordbookWords, aws_profiles, chars_since, history_list, stats_snapshot, wordbook_words,
};
pub use save_config::{save_config, try_save_config};
pub use store_secret::{store_secret, try_store_secret};
pub use watch_config::watch_config;
