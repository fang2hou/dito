//! The SQLite schema and its versioned migrations.

use rusqlite::Connection;
use serde::Serialize;
use serde::de::value::Error as ValueError;
use serde::de::{DeserializeOwned, IntoDeserializer};

use super::model::Result;

/// Schema migrations in the order they shipped. `PRAGMA user_version` holds how many have been applied, so a shipped
/// entry is never edited or reordered: a schema change is a new entry at the end.
pub(super) const MIGRATIONS: &[&str] = &[
    // 1: the schema from before versioning. `IF NOT EXISTS` lets a database created by that build adopt it unchanged.
    "CREATE TABLE IF NOT EXISTS history (
       id INTEGER PRIMARY KEY AUTOINCREMENT,
       ts INTEGER NOT NULL,
       day TEXT NOT NULL,
       kind TEXT NOT NULL,
       style TEXT NOT NULL,
       app TEXT NOT NULL,
       lang TEXT NOT NULL,
       duration_ms INTEGER NOT NULL,
       raw TEXT NOT NULL,
       polished TEXT,
       error TEXT,
       audio_path TEXT
     );
     CREATE INDEX IF NOT EXISTS history_ts ON history(ts DESC);
     CREATE TABLE IF NOT EXISTS dictionary (
       term TEXT PRIMARY KEY,
       created INTEGER NOT NULL
     );",
    // 2: which engines produced an entry. Nullable, so entries from before it stay unlabelled.
    "ALTER TABLE history ADD COLUMN asr_engine TEXT;
     ALTER TABLE history ADD COLUMN polish_engine TEXT;",
    // 3: use the product name without rewriting the saved words.
    "ALTER TABLE dictionary RENAME TO wordbook;",
    // 4: step timings of every entry, and command rows: the text a command worked on and its details as JSON. All
    // nullable, so earlier entries stay as they were.
    "ALTER TABLE history ADD COLUMN asr_ms INTEGER;
     ALTER TABLE history ADD COLUMN polish_ms INTEGER;
     ALTER TABLE history ADD COLUMN source TEXT;
     ALTER TABLE history ADD COLUMN command TEXT;",
    // 5: transcription sessions and their paragraphs. `text` is what the user reads and edits; `raw` keeps the
    // recognized words so a failed polish never loses them. `IF NOT EXISTS` lets a database that ran the
    // pre-release numbering of these tables be set back to version 3 and catch up.
    "CREATE TABLE IF NOT EXISTS transcripts (
       id INTEGER PRIMARY KEY AUTOINCREMENT,
       started_ts INTEGER NOT NULL,
       ended_ts INTEGER,
       title TEXT NOT NULL DEFAULT '',
       with_mic INTEGER NOT NULL,
       with_system INTEGER NOT NULL DEFAULT 1
     );
     CREATE INDEX IF NOT EXISTS transcripts_started ON transcripts(started_ts DESC);
     CREATE TABLE IF NOT EXISTS transcript_paragraphs (
       id INTEGER PRIMARY KEY AUTOINCREMENT,
       transcript_id INTEGER NOT NULL,
       speaker TEXT NOT NULL,
       start_ms INTEGER NOT NULL,
       end_ms INTEGER NOT NULL,
       raw TEXT NOT NULL,
       text TEXT NOT NULL,
       state TEXT NOT NULL,
       error TEXT
     );
     CREATE INDEX IF NOT EXISTS transcript_paragraphs_order ON transcript_paragraphs(transcript_id, start_ms);",
    // 6: the time a transcript has recorded across its sessions, without the gaps between them. Nullable: a
    // transcript from before it ran one session, so its span from start to end is its length.
    "ALTER TABLE transcripts ADD COLUMN recorded_ms INTEGER;",
    // 7: chat conversations. `attachments` is a JSON list of the files and meetings a message carried, with their text
    // copied in; `model` labels the model that wrote a reply.
    "CREATE TABLE chats (
       id INTEGER PRIMARY KEY AUTOINCREMENT,
       title TEXT NOT NULL,
       created_ts INTEGER NOT NULL,
       updated_ts INTEGER NOT NULL
     );
     CREATE INDEX chats_updated ON chats(updated_ts DESC);
     CREATE TABLE chat_messages (
       id INTEGER PRIMARY KEY AUTOINCREMENT,
       chat_id INTEGER NOT NULL,
       author TEXT NOT NULL,
       content TEXT NOT NULL,
       state TEXT NOT NULL,
       error TEXT,
       attachments TEXT NOT NULL,
       model TEXT,
       ts INTEGER NOT NULL
     );
     CREATE INDEX chat_messages_chat ON chat_messages(chat_id, id);",
];

pub(super) const SCHEMA_VERSION: u32 = MIGRATIONS.len() as u32;

pub(super) fn user_version(conn: &Connection) -> rusqlite::Result<u32> {
    conn.pragma_query_value(None, "user_version", |row| row.get(0))
}

/// Each migration runs in its own transaction together with its version bump, so a failure leaves the database at the
/// last version that applied completely.
pub(super) fn migrate(conn: &mut Connection, applied: u32) -> Result<()> {
    for (migration, version) in MIGRATIONS.iter().zip(1u32..).skip(applied as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(migration)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
        tracing::info!(version, "applied database migration");
    }
    Ok(())
}

pub(super) fn encode<T: Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .expect("column enums are unit variants")
}

/// Reads stay tolerant: a value this version does not know (a row written by a newer build) reads
/// as the default instead of failing the whole listing.
pub(super) fn decode<T: DeserializeOwned + Default>(column: &str, stored: &str) -> T {
    let parsed: std::result::Result<T, ValueError> = T::deserialize(stored.to_owned().into_deserializer());
    parsed.unwrap_or_else(|_| {
        tracing::warn!(column, stored, "unknown stored value, reading it as the default");
        T::default()
    })
}
