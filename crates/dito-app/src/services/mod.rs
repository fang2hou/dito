//! App-level operations on shared state, callable from any view.
//!
//! Split by what an operation does: `history` mutates saved entries, `connection` talks to configured
//! engines, `queries` reads local data, `save_config` and `store_secret` persist settings, and `watch_config`
//! re-renders a view when they change. The re-exports below are the whole public surface: views keep calling
//! `services::<fn>` and never the submodules directly.

mod connection;
mod history;
mod queries;
mod save_config;
mod store_secret;
mod watch_config;

pub use connection::{test_asr_draft, test_connection, try_polish};
pub use history::{delete_history_entry, history_entry, polish_again, transcribe_again};
pub use queries::{WordbookWords, aws_profiles, history_list, history_stats, wordbook_words};
pub use save_config::{save_config, try_save_config};
pub use store_secret::{store_secret, try_store_secret};
pub use watch_config::watch_config;
