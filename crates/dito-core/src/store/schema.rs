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

#[cfg(test)]
mod tests {
    use crate::config::EngineLabels;

    use super::super::*;
    use super::*;

    /// The schema as it shipped before versioning. Frozen here on purpose: real databases hold exactly this, whatever
    /// migrations are appended later. It omits `IF NOT EXISTS`, so migration 1 must tolerate tables that already exist.
    const PRE_VERSIONING_SCHEMA: &str = "CREATE TABLE history (
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
     CREATE INDEX history_ts ON history(ts DESC);
     CREATE TABLE dictionary (
       term TEXT PRIMARY KEY,
       created INTEGER NOT NULL
     );";

    #[test]
    fn a_fresh_store_is_at_the_latest_schema_version() {
        let s = Store::open_in_memory().unwrap();
        assert_eq!(user_version(&s.conn()).unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn an_older_database_keeps_its_rows_and_reaches_the_latest_version() {
        // Version 0 predates versioning, version 1 adopted that schema, version 2 added the engine columns and version 3
        // renamed the wordbook.
        for applied in [0u32, 1, 2, 3] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("h.db");
            {
                let old = Connection::open(&path).unwrap();
                old.execute_batch(PRE_VERSIONING_SCHEMA).unwrap();
                if applied >= 2 {
                    old.execute_batch(MIGRATIONS[1]).unwrap();
                }
                if applied == 3 {
                    old.execute_batch(MIGRATIONS[2]).unwrap();
                }
                old.execute(
                    "INSERT INTO history (ts, day, kind, style, app, lang, duration_ms, raw)
                     VALUES (10, '1970-01-01', 'dictation', 'general', 'Notes', 'ja', 6000, 'from the old build')",
                    (),
                )
                .unwrap();
                let words = if applied == 3 { "wordbook" } else { "dictionary" };
                old.execute(&format!("INSERT INTO {words} (term, created) VALUES ('Acme', 10)"), ()).unwrap();
                old.pragma_update(None, "user_version", applied).unwrap();
                assert_eq!(user_version(&old).unwrap(), applied);
            }

            let s = Store::open(&path, 0).unwrap();
            assert_eq!(user_version(&s.conn()).unwrap(), SCHEMA_VERSION, "upgrading from version {applied}");
            assert_eq!(s.words(None).unwrap(), ["Acme"]);
            let rows = s.list_history(&HistoryFilter::default()).unwrap();
            assert_eq!(rows.iter().map(|e| e.raw.as_str()).collect::<Vec<_>>(), ["from the old build"]);
            assert_eq!(rows[0].engines, EngineLabels::default(), "a row from before the engine columns is unlabelled");
            assert_eq!(rows[0].timings, StepMillis::default(), "a row from before the timing columns has none");
            assert_eq!((rows[0].source.as_deref(), rows[0].command.as_ref()), (None, None));
            drop(s);

            let reopened = Store::open(&path, 0).unwrap();
            assert_eq!(user_version(&reopened.conn()).unwrap(), SCHEMA_VERSION, "a current database opens unchanged");
            assert_eq!(reopened.list_history(&HistoryFilter::default()).unwrap().len(), 1);
        }
    }

    #[test]
    fn a_version_six_database_gains_the_chat_tables_and_keeps_its_transcripts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("h.db");
        {
            let old = Connection::open(&path).unwrap();
            for migration in &MIGRATIONS[..6] {
                old.execute_batch(migration).unwrap();
            }
            old.execute("INSERT INTO transcripts (started_ts, title, with_mic) VALUES (7, 'weekly', 0)", ()).unwrap();
            old.pragma_update(None, "user_version", 6).unwrap();
        }

        let s = Store::open(&path, 0).unwrap();
        assert_eq!(user_version(&s.conn()).unwrap(), SCHEMA_VERSION);
        assert_eq!(s.list_transcripts().unwrap()[0].title, "weekly");
        assert!(s.list_chats().unwrap().is_empty());
        let chat = s.create_chat("first", 1).unwrap();
        assert_eq!(s.get_chat(chat).unwrap().unwrap().summary.title, "first");
    }

    #[test]
    fn a_database_from_a_newer_build_is_refused_and_left_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("h.db");
        let newer = SCHEMA_VERSION + 1;
        {
            let future = Connection::open(&path).unwrap();
            future.execute_batch("CREATE TABLE added_by_a_newer_build (x INTEGER)").unwrap();
            future.pragma_update(None, "user_version", newer).unwrap();
        }

        let err = Store::open(&path, 0).err().expect("a newer database is refused");
        let StoreError::SchemaTooNew { found, supported } = err else { panic!("unexpected error: {err}") };
        assert_eq!((found, supported), (newer, SCHEMA_VERSION));

        let conn = Connection::open(&path).unwrap();
        assert_eq!(user_version(&conn).unwrap(), newer, "the version is not lowered");
        let objects: i64 = conn.query_row("SELECT COUNT(*) FROM sqlite_master", (), |row| row.get(0)).unwrap();
        assert_eq!(objects, 1, "no migration ran: only the newer build's table exists");
    }
}
