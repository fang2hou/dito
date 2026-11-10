//! SQLite-backed history and dictionary, plus the aggregate queries behind the stats page.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::{Connection, OptionalExtension, params};
use serde::de::value::Error as ValueError;
use serde::de::{DeserializeOwned, IntoDeserializer};
use serde::{Deserialize, Serialize};

use crate::prompts::Style;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Database(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// The database was written by a newer build. It is refused untouched: this build cannot know what changed.
    #[error("the database has schema version {found}, but this build supports up to version {supported}")]
    SchemaTooNew { found: u32, supported: u32 },
}

type Result<T> = std::result::Result<T, StoreError>;

/// Typing speed used to estimate "time saved" (characters per minute).
pub const TYPING_CPM: f64 = 60.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HistoryKind {
    #[default]
    Dictation,
    /// The user cancelled mid-recording; the audio is kept so it can be recognized again.
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewHistory {
    pub ts: i64,
    pub kind: HistoryKind,
    pub style: Style,
    pub app: String,
    pub lang: String,
    pub duration_ms: u64,
    pub raw: String,
    pub polished: Option<String>,
    pub error: Option<String>,
    pub audio_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryEntry {
    pub id: i64,
    pub ts: i64,
    pub kind: HistoryKind,
    pub style: Style,
    pub app: String,
    pub lang: String,
    pub duration_ms: u64,
    pub raw: String,
    pub polished: Option<String>,
    pub error: Option<String>,
    pub audio_path: Option<String>,
}

impl HistoryEntry {
    /// Text that was (or would be) delivered.
    pub fn final_text(&self) -> &str {
        self.polished.as_deref().unwrap_or(&self.raw)
    }
}

#[derive(Debug, Clone, Default)]
pub struct HistoryFilter {
    pub query: Option<String>,
    pub since_ts: Option<i64>,
    pub style: Option<Style>,
    /// Display name and bundle id of every app currently configured under Prompts. A recording is that app when its
    /// stored style is the app or when the app name it recorded is one of these names; everything else outside the
    /// built-in styles is Other. Removing an app therefore sends its past entries to Other, and adding it back
    /// restores them without rewriting history.
    pub configured_apps: Vec<(String, String)>,
    pub disabled_builtin_styles: Vec<Style>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stats {
    pub chars: u64,
    pub recordings: u64,
    pub speech_ms: u64,
    /// Characters per minute of speaking.
    pub avg_cpm: f64,
    pub saved_minutes: f64,
    /// (local date `YYYY-MM-DD`, characters), ascending.
    pub per_day: Vec<(String, u64)>,
    /// (app name, recordings), descending.
    pub apps: Vec<(String, u64)>,
    /// (language code, recordings), descending.
    pub langs: Vec<(String, u64)>,
}

pub struct Store {
    conn: Mutex<Connection>,
    tz_offset_secs: i32,
}

/// Schema migrations in the order they shipped. `PRAGMA user_version` holds how many have been applied, so a shipped
/// entry is never edited or reordered: a schema change is a new entry at the end.
const MIGRATIONS: &[&str] = &[
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
];

/// The schema version this build creates and the newest one it can open.
const SCHEMA_VERSION: u32 = MIGRATIONS.len() as u32;

fn user_version(conn: &Connection) -> rusqlite::Result<u32> {
    conn.pragma_query_value(None, "user_version", |row| row.get(0))
}

/// Applies the migrations after `applied`, each in its own transaction together with its version bump, so a failure
/// leaves the database at the last version that applied completely.
fn migrate(conn: &mut Connection, applied: u32) -> Result<()> {
    for (migration, version) in MIGRATIONS.iter().zip(1u32..).skip(applied as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(migration)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
        tracing::info!(version, "applied database migration");
    }
    Ok(())
}

/// The text stored in a column for an enum that serializes as a unit variant name.
fn encode<T: Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .expect("column enums are unit variants")
}

/// Reads stay tolerant: a value this version does not know (a row written by a newer build) reads
/// as the default instead of failing the whole listing.
fn decode<T: DeserializeOwned + Default>(column: &str, stored: &str) -> T {
    let parsed: std::result::Result<T, ValueError> = T::deserialize(stored.to_owned().into_deserializer());
    parsed.unwrap_or_else(|_| {
        tracing::warn!(column, stored, "unknown stored value, reading it as the default");
        T::default()
    })
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
        Ok(Self { conn: Mutex::new(conn), tz_offset_secs })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().expect("store connection mutex poisoned")
    }

    fn day_of(&self, ts: i64) -> String {
        let local = time::OffsetDateTime::from_unix_timestamp(ts + self.tz_offset_secs as i64)
            .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
        format!("{:04}-{:02}-{:02}", local.year(), u8::from(local.month()), local.day())
    }

    pub fn add_history(&self, h: &NewHistory) -> Result<i64> {
        let c = self.conn();
        c.execute(
            "INSERT INTO history (ts, day, kind, style, app, lang, duration_ms, raw, polished, error, audio_path)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                h.ts,
                self.day_of(h.ts),
                encode(h.kind),
                encode(&h.style),
                h.app,
                h.lang,
                h.duration_ms as i64,
                h.raw,
                h.polished,
                h.error,
                h.audio_path
            ],
        )?;
        Ok(c.last_insert_rowid())
    }

    /// Turn a cancelled/failed entry into a normal dictation after a successful retry.
    pub fn complete_history(&self, id: i64, raw: &str, polished: Option<&str>, lang: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE history SET kind='dictation', raw=?2, polished=?3, lang=?4, error=NULL WHERE id=?1",
            params![id, raw, polished, lang],
        )?;
        Ok(())
    }

    pub fn set_polished(&self, id: i64, polished: &str) -> Result<()> {
        self.conn().execute("UPDATE history SET polished=?2 WHERE id=?1", params![id, polished])?;
        Ok(())
    }

    pub fn get_history(&self, id: i64) -> Result<Option<HistoryEntry>> {
        let c = self.conn();
        Ok(c.query_row(&format!("{SELECT_HISTORY} WHERE id = ?1"), [id], row_to_entry).optional()?)
    }

    pub fn list_history(&self, f: &HistoryFilter) -> Result<Vec<HistoryEntry>> {
        let mut sql = format!("{SELECT_HISTORY} WHERE 1=1");
        let mut args: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(q) = f.query.as_deref().filter(|q| !q.trim().is_empty()) {
            sql.push_str(" AND (raw LIKE ?1 ESCAPE '\\' OR polished LIKE ?1 ESCAPE '\\')");
            let esc = q.trim().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
            args.push(format!("%{esc}%").into());
        }
        if let Some(since) = f.since_ts {
            sql.push_str(&format!(" AND ts >= ?{}", args.len() + 1));
            args.push(since.into());
        }
        if let Some(style) = &f.style {
            match style {
                // Other: every recording outside the built-in styles that is neither a configured app's style nor one
                // of the configured app names. The name check keeps entries recorded before the app was configured
                // (or under another bundle id) with their app.
                Style::General => {
                    let active: Vec<_> = [Style::Slack, Style::Teams, Style::Herdr]
                        .into_iter()
                        .filter(|style| {
                            !f.disabled_builtin_styles.contains(style)
                                || f.configured_apps.iter().any(|(_, id)| Style::for_bundle_id(id) == *style)
                        })
                        .collect();
                    let mut clause = String::from("1 = 1");
                    if !active.is_empty() {
                        let first = args.len() + 1;
                        let marks: Vec<_> = (0..active.len()).map(|i| format!("?{}", first + i)).collect();
                        args.extend(active.into_iter().map(|style| encode(style).into()));
                        clause.push_str(&format!(" AND style NOT IN ({})", marks.join(", ")));
                    }
                    if !f.configured_apps.is_empty() {
                        let first = args.len() + 1;
                        let id_marks: Vec<String> =
                            (0..f.configured_apps.len()).map(|i| format!("?{}", first + i)).collect();
                        args.extend(
                            f.configured_apps
                                .iter()
                                .map(|(_, id)| encode(Style::Application(id.clone())))
                                .map(Into::into),
                        );
                        let first = args.len() + 1;
                        let name_marks: Vec<String> =
                            (0..f.configured_apps.len()).map(|i| format!("?{}", first + i)).collect();
                        args.extend(f.configured_apps.iter().map(|(name, _)| name.clone().into()));
                        clause.push_str(&format!(
                            " AND style NOT IN ({}) AND app NOT IN ({})",
                            id_marks.join(", "),
                            name_marks.join(", ")
                        ));
                    }
                    sql.push_str(&format!(" AND ({clause})"));
                }
                // Built-in styles are fixed and are matched by style alone.
                Style::Slack | Style::Teams | Style::Herdr => {
                    sql.push_str(&format!(" AND style = ?{}", args.len() + 1));
                    args.push(encode(style.clone()).into());
                }
                Style::Application(id) => {
                    let name = f.configured_apps.iter().find(|(_, app_id)| app_id == id).map(|(n, _)| n.clone());
                    let first = args.len() + 1;
                    let builtin = Style::for_bundle_id(id);
                    if !matches!(builtin, Style::Application(_) | Style::General) {
                        args.push(encode(style.clone()).into());
                        args.push(encode(builtin).into());
                        sql.push_str(&format!(" AND (style = ?{first} OR style = ?{})", first + 1));
                    } else {
                        match name.filter(|n| !n.is_empty()) {
                            Some(name) => {
                                args.push(encode(style.clone()).into());
                                args.push(name.into());
                                sql.push_str(&format!(
                                " AND (style = ?{first} OR (app = ?{} AND style NOT IN ('slack', 'teams', 'herdr')))",
                                first + 1
                            ));
                            }
                            None => {
                                sql.push_str(&format!(" AND style = ?{first}"));
                                args.push(encode(style.clone()).into());
                            }
                        }
                    }
                }
            }
        }
        sql.push_str(" ORDER BY ts DESC, id DESC");
        if let Some(l) = f.limit {
            sql.push_str(&format!(" LIMIT {l}"));
        }
        let c = self.conn();
        let mut stmt = c.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args), row_to_entry)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn delete_history(&self, id: i64) -> Result<()> {
        self.conn().execute("DELETE FROM history WHERE id=?1", [id])?;
        Ok(())
    }

    /// Returns false when the term already exists (or is blank).
    pub fn add_word(&self, term: &str, now: i64) -> Result<bool> {
        let term = term.trim();
        if term.is_empty() {
            return Ok(false);
        }
        let n = self
            .conn()
            .execute("INSERT OR IGNORE INTO dictionary (term, created) VALUES (?1, ?2)", params![term, now])?;
        Ok(n > 0)
    }

    pub fn remove_word(&self, term: &str) -> Result<()> {
        self.conn().execute("DELETE FROM dictionary WHERE term=?1", [term])?;
        Ok(())
    }

    pub fn words(&self, query: Option<&str>) -> Result<Vec<String>> {
        let c = self.conn();
        let like = format!("%{}%", query.unwrap_or("").trim());
        let mut stmt = c.prepare("SELECT term FROM dictionary WHERE term LIKE ?1 ORDER BY created DESC, term")?;
        Ok(stmt.query_map([like], |r| r.get(0))?.collect::<std::result::Result<_, _>>()?)
    }

    /// One word per line; returns the number of new words.
    pub fn import_words(&self, text: &str, now: i64) -> Result<usize> {
        let mut added = 0;
        for line in text.lines() {
            if self.add_word(line, now)? {
                added += 1;
            }
        }
        Ok(added)
    }

    pub fn export_words(&self) -> Result<String> {
        let mut words = self.words(None)?;
        words.sort();
        Ok(words.join("\n"))
    }

    pub fn stats(&self, since_ts: Option<i64>) -> Result<Stats> {
        let c = self.conn();
        let since = since_ts.unwrap_or(0);
        let (chars, recordings, speech_ms): (i64, i64, i64) = c.query_row(
            "SELECT COALESCE(SUM(LENGTH(COALESCE(polished, raw))),0), COUNT(*), COALESCE(SUM(duration_ms),0)
             FROM history WHERE kind='dictation' AND ts >= ?1",
            [since],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let minutes = speech_ms as f64 / 60_000.0;
        let avg_cpm = if minutes > 0.0 { chars as f64 / minutes } else { 0.0 };
        let saved_minutes = (chars as f64 / TYPING_CPM - minutes).max(0.0);

        let mut per_day = Vec::new();
        let mut stmt = c.prepare(
            "SELECT day, SUM(LENGTH(COALESCE(polished, raw))) FROM history
             WHERE kind='dictation' AND ts >= ?1 GROUP BY day ORDER BY day",
        )?;
        for r in stmt.query_map([since], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64)))? {
            per_day.push(r?);
        }
        let group = |col: &str| -> Result<Vec<(String, u64)>> {
            let mut stmt = c.prepare(&format!(
                "SELECT {col}, COUNT(*) AS n FROM history WHERE kind='dictation' AND ts >= ?1
                 GROUP BY {col} ORDER BY n DESC, {col} LIMIT 6"
            ))?;
            let rows = stmt.query_map([since], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64)))?;
            Ok(rows.collect::<std::result::Result<_, _>>()?)
        };
        Ok(Stats {
            chars: chars as u64,
            recordings: recordings as u64,
            speech_ms: speech_ms as u64,
            avg_cpm,
            saved_minutes,
            per_day,
            apps: group("app")?,
            langs: group("lang")?,
        })
    }
}

const SELECT_HISTORY: &str =
    "SELECT id, ts, kind, style, app, lang, duration_ms, raw, polished, error, audio_path FROM history";

fn row_to_entry(r: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEntry> {
    Ok(HistoryEntry {
        id: r.get(0)?,
        ts: r.get(1)?,
        kind: decode("kind", &r.get::<_, String>(2)?),
        style: decode("style", &r.get::<_, String>(3)?),
        app: r.get(4)?,
        lang: r.get(5)?,
        duration_ms: r.get::<_, i64>(6)? as u64,
        raw: r.get(7)?,
        polished: r.get(8)?,
        error: r.get(9)?,
        audio_path: r.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(ts: i64, app: &str, lang: &str, raw: &str, polished: Option<&str>) -> NewHistory {
        NewHistory {
            ts,
            kind: HistoryKind::Dictation,
            style: Style::General,
            app: app.into(),
            lang: lang.into(),
            duration_ms: 6_000,
            raw: raw.into(),
            polished: polished.map(Into::into),
            error: None,
            audio_path: None,
        }
    }

    #[test]
    fn custom_application_style_round_trips_in_history() {
        let store = Store::open_in_memory().unwrap();
        let mut row = entry(1, "Editor", "en", "hello", None);
        row.style = Style::Application("example.editor".into());
        store.add_history(&row).unwrap();
        let rows = store.list_history(&HistoryFilter::default()).unwrap();
        assert_eq!(rows[0].style, row.style);
    }

    #[test]
    fn history_search_matches_raw_and_polished_and_escapes_wildcards() {
        let s = Store::open_in_memory().unwrap();
        s.add_history(&entry(10, "Slack", "ja", "えーと 会議", Some("会議があります。"))).unwrap();
        s.add_history(&entry(20, "Mail", "en", "100% done", None)).unwrap();
        let f = |q: &str| HistoryFilter { query: Some(q.into()), ..Default::default() };
        assert_eq!(s.list_history(&f("えーと")).unwrap().len(), 1);
        assert_eq!(s.list_history(&f("あります")).unwrap().len(), 1);
        assert_eq!(s.list_history(&f("%")).unwrap().len(), 1, "% must match literally");
        assert_eq!(s.list_history(&f("_")).unwrap().len(), 0, "_ must match literally");
    }

    #[test]
    fn history_filters_by_period_style_and_orders_newest_first() {
        let s = Store::open_in_memory().unwrap();
        s.add_history(&entry(10, "A", "ja", "old", None)).unwrap();
        let mut slack = entry(30, "B", "ja", "new", None);
        slack.style = Style::Slack;
        s.add_history(&slack).unwrap();
        let all = s.list_history(&HistoryFilter::default()).unwrap();
        assert_eq!(all.iter().map(|e| e.raw.as_str()).collect::<Vec<_>>(), ["new", "old"]);
        let recent = s.list_history(&HistoryFilter { since_ts: Some(20), ..Default::default() }).unwrap();
        assert_eq!(recent.len(), 1);
        let styled = s.list_history(&HistoryFilter { style: Some(Style::Slack), ..Default::default() }).unwrap();
        assert_eq!(styled[0].raw, "new");
    }

    #[test]
    fn removing_and_readding_builtin_profiles_reclassifies_history_without_deleting_it() {
        let store = Store::open_in_memory().unwrap();
        let mut recorded = entry(10, "Herdr", "ja", "preserved", None);
        recorded.style = Style::Herdr;
        store.add_history(&recorded).unwrap();
        let mut filter = HistoryFilter {
            style: Some(Style::General),
            disabled_builtin_styles: vec![Style::Herdr],
            ..Default::default()
        };
        assert_eq!(store.list_history(&filter).unwrap().len(), 1);
        filter.configured_apps.push(("Agent work".into(), "dev.herdr.app".into()));
        assert!(store.list_history(&filter).unwrap().is_empty());
        filter.style = Some(Style::Application("dev.herdr.app".into()));
        assert_eq!(store.list_history(&filter).unwrap()[0].raw, "preserved");
    }

    #[test]
    fn app_classes_follow_the_configured_list_by_style_and_name() {
        let s = Store::open_in_memory().unwrap();
        s.add_history(&entry(10, "Notes", "ja", "general", None)).unwrap();
        let mut legacy = entry(15, "App Store", "ja", "legacy", None);
        legacy.style = Style::General;
        s.add_history(&legacy).unwrap();
        let mut slack = entry(20, "Slack", "ja", "slack", None);
        slack.style = Style::Slack;
        s.add_history(&slack).unwrap();
        let mut kept = entry(30, "App Store", "ja", "kept", None);
        kept.style = Style::Application("com.apple.AppStore".into());
        s.add_history(&kept).unwrap();
        let mut gone = entry(40, "Old", "ja", "gone", None);
        gone.style = Style::Application("example.gone".into());
        s.add_history(&gone).unwrap();
        let configured = vec![("App Store".to_string(), "com.apple.AppStore".to_string())];

        let other =
            HistoryFilter { style: Some(Style::General), configured_apps: configured.clone(), ..Default::default() };
        let rows = s.list_history(&other).unwrap();
        let raws: Vec<&str> = rows.iter().map(|e| e.raw.as_str()).collect();
        assert_eq!(raws, ["gone", "general"], "a removed app joins Other; the configured app's entries stay out");

        let app = HistoryFilter {
            style: Some(Style::Application("com.apple.AppStore".into())),
            configured_apps: configured.clone(),
            ..Default::default()
        };
        let rows = s.list_history(&app).unwrap();
        assert_eq!(
            rows.iter().map(|e| e.raw.as_str()).collect::<Vec<_>>(),
            ["kept", "legacy"],
            "name matches recover old rows"
        );

        let re_added = HistoryFilter {
            style: Some(Style::Application("com.example.newstore".into())),
            configured_apps: vec![("App Store".into(), "com.example.newstore".into())],
            ..Default::default()
        };
        let rows = s.list_history(&re_added).unwrap();
        assert_eq!(
            rows.iter().map(|e| e.raw.as_str()).collect::<Vec<_>>(),
            ["kept", "legacy"],
            "an app re-added under another bundle id still claims its past entries by name"
        );

        let none_configured = HistoryFilter { style: Some(Style::General), ..Default::default() };
        let rows = s.list_history(&none_configured).unwrap();
        let raws: Vec<&str> = rows.iter().map(|e| e.raw.as_str()).collect();
        assert_eq!(
            raws,
            ["gone", "kept", "legacy", "general"],
            "with no configured apps every recording outside the built-in styles is Other"
        );
        assert!(
            s.list_history(&HistoryFilter { style: Some(Style::Slack), ..Default::default() }).unwrap()[0].raw
                == "slack"
        );
        assert!(
            !s.list_history(&none_configured).unwrap().iter().any(|e| e.style == Style::Slack),
            "a built-in style never joins Other"
        );
    }

    #[test]
    fn cancelled_entry_becomes_a_dictation_after_retry() {
        let s = Store::open_in_memory().unwrap();
        let mut c = entry(5, "Notes", "", "", None);
        c.kind = HistoryKind::Cancelled;
        c.audio_path = Some("/tmp/a.wav".into());
        let id = s.add_history(&c).unwrap();
        s.complete_history(id, "こんにちは", Some("こんにちは。"), "ja").unwrap();
        let e = s.get_history(id).unwrap().unwrap();
        assert_eq!(e.kind, HistoryKind::Dictation);
        assert_eq!(e.final_text(), "こんにちは。");
        assert_eq!(e.audio_path.as_deref(), Some("/tmp/a.wav"), "audio is kept for playback");
    }

    #[test]
    fn unknown_stored_kinds_and_styles_read_as_defaults() {
        let s = Store::open_in_memory().unwrap();
        let id = s.add_history(&entry(5, "A", "ja", "x", None)).unwrap();
        s.conn().execute("UPDATE history SET kind='from-the-future', style='mystery' WHERE id=?1", [id]).unwrap();
        let e = s.get_history(id).unwrap().unwrap();
        assert_eq!((e.kind, e.style), (HistoryKind::Dictation, Style::General));
    }

    #[test]
    fn dictionary_dedupes_trims_and_round_trips_import_export() {
        let s = Store::open_in_memory().unwrap();
        assert!(s.add_word("  Acme ", 1).unwrap());
        assert!(!s.add_word("Acme", 2).unwrap());
        assert!(!s.add_word("   ", 3).unwrap());
        assert_eq!(s.import_words("Contoso\nAcme\n\nBedrock\n", 4).unwrap(), 2);
        assert_eq!(s.export_words().unwrap(), "Acme\nBedrock\nContoso");
        s.remove_word("Contoso").unwrap();
        assert_eq!(s.words(Some("B")).unwrap(), ["Bedrock"]);
    }

    #[test]
    fn stats_ignore_non_dictations_and_group_by_local_day() {
        let s = Store::open_in_memory().unwrap();
        let day = 86_400;
        s.add_history(&entry(day, "Slack", "ja", "raw", Some("0123456789"))).unwrap();
        s.add_history(&entry(day + 60, "Slack", "ja", "abcde", None)).unwrap();
        s.add_history(&entry(2 * day, "Mail", "en", "xy", None)).unwrap();
        let mut failed = entry(day, "Slack", "ja", "zzzzzz", None);
        failed.kind = HistoryKind::Failed;
        s.add_history(&failed).unwrap();

        let st = s.stats(None).unwrap();
        assert_eq!(st.chars, 17);
        assert_eq!(st.recordings, 3);
        assert_eq!(st.per_day, vec![("1970-01-02".into(), 15), ("1970-01-03".into(), 2)]);
        assert_eq!(st.apps[0], ("Slack".into(), 2));
        assert_eq!(st.langs[0], ("ja".into(), 2));
        assert!((st.avg_cpm - 17.0 / 0.3).abs() < 1e-6);
        assert_eq!(s.stats(Some(2 * day)).unwrap().recordings, 1);
    }

    #[test]
    fn timezone_offset_shifts_the_day_bucket() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path().join("h.db"), 9 * 3600).unwrap();
        // 1970-01-01 20:00 UTC is already 01-02 in JST.
        s.add_history(&entry(20 * 3600, "A", "ja", "x", None)).unwrap();
        assert_eq!(s.stats(None).unwrap().per_day[0].0, "1970-01-02");
    }

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
    fn a_database_from_before_versioning_keeps_its_rows_and_reaches_the_latest_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("h.db");
        {
            let old = Connection::open(&path).unwrap();
            old.execute_batch(PRE_VERSIONING_SCHEMA).unwrap();
            old.execute(
                "INSERT INTO history (ts, day, kind, style, app, lang, duration_ms, raw)
                 VALUES (10, '1970-01-01', 'dictation', 'general', 'Notes', 'ja', 6000, 'from the old build')",
                (),
            )
            .unwrap();
            assert_eq!(user_version(&old).unwrap(), 0, "a database from before versioning has no version");
        }

        let s = Store::open(&path, 0).unwrap();
        assert_eq!(user_version(&s.conn()).unwrap(), SCHEMA_VERSION);
        let rows = s.list_history(&HistoryFilter::default()).unwrap();
        assert_eq!(rows.iter().map(|e| e.raw.as_str()).collect::<Vec<_>>(), ["from the old build"]);
        drop(s);

        let reopened = Store::open(&path, 0).unwrap();
        assert_eq!(user_version(&reopened.conn()).unwrap(), SCHEMA_VERSION, "a current database opens unchanged");
        assert_eq!(reopened.list_history(&HistoryFilter::default()).unwrap().len(), 1);
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
