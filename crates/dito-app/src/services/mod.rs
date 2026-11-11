//! App-level operations on shared state. The re-exports below are the whole public surface: views call
//! `services::<fn>`, never the submodules.

mod connection;
mod history;
mod queries;
mod save_config;
mod store_secret;
mod watch_config;

pub use connection::{test_asr_draft, test_connection, try_polish};
pub use history::{delete_history_entry, history_entry, polish_again, transcribe_again};
pub use queries::{StatsSnapshot, WordbookWords, aws_profiles, history_list, stats_snapshot, wordbook_words};
pub use save_config::{save_config, try_save_config};
pub use store_secret::{store_secret, try_store_secret};
pub use watch_config::watch_config;
