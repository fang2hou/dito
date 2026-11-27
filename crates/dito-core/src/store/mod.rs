//! SQLite-backed history and wordbook.

use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::sync::{Mutex, MutexGuard};

use rusqlite::Connection;

mod activity;
mod chats;
mod history;
mod model;
mod rows;
mod schema;
mod stats;
mod transcripts;
mod wordbook;

pub use activity::{Period, civil, heat_quartiles, heat_step, local_day, month_span, shown_days, week_start};
pub use chats::{Attachment, Author, Chat, ChatMessageRow, ChatSummary, MessageState, NewChatMessage};
pub use model::*;
use schema::{SCHEMA_VERSION, migrate, user_version};
pub use transcripts::{
    ParagraphState, RESUME_WINDOW_SECS, Reopened, Sources, Speaker, Transcript, TranscriptParagraph, TranscriptSummary,
};
pub use wordbook::{WordbookEntry, parse_spoken_list, wordbook_terms};

pub struct Store {
    conn: Mutex<Connection>,
    tz_offset_secs: i32,
    /// Bumped by every wordbook write, so a running session knows when to read the wordbook again.
    wordbook_revision: AtomicU64,
}

impl Store {
    pub fn open(path: impl AsRef<Path>, tz_offset_secs: i32) -> Result<Self> {
        if let Some(dir) = path.as_ref().parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path)?, tz_offset_secs)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?, 0)
    }

    fn init(mut conn: Connection, tz_offset_secs: i32) -> Result<Self> {
        let applied = user_version(&conn)?;
        if applied > SCHEMA_VERSION {
            // Checked before anything is written, the journal mode included, so a refused database stays as it was.
            return Err(StoreError::SchemaTooNew { found: applied, supported: SCHEMA_VERSION });
        }
        conn.execute_batch("PRAGMA journal_mode = WAL;")?;
        migrate(&mut conn, applied)?;
        let store = Self { conn: Mutex::new(conn), tz_offset_secs, wordbook_revision: AtomicU64::new(0) };
        store.recover_chats()?;
        Ok(store)
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().expect("store connection mutex poisoned")
    }

    fn day_of(&self, ts: i64) -> String {
        activity::day_key(local_day(ts, self.tz_offset_secs))
    }
}
