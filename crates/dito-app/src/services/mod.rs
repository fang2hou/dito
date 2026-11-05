//! App-level operations that touch the network or disk, callable from any view.
//!
//! Split by what an operation does: `history` mutates saved entries, `connection` talks to
//! configured engines, `queries` reads local data. The re-exports below are the whole public
//! surface — views keep calling `services::<fn>` and never the submodules directly.

mod connection;
mod history;
mod queries;

pub use connection::{test_connection, try_polish};
pub use history::{delete_history_entry, history_entry, polish_again, recognize_again};
pub use queries::{aws_profiles, history_list, history_stats};
