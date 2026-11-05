//! SQLite-backed history and dictionary, plus the aggregate queries behind the stats page.

use std::path::Path;
use std::sync::Mutex;

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::prompts::Style;

/// Typing speed used to estimate "time saved" (characters per minute).
pub const TYPING_CPM: f64 = 60.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryKind {
    Dictation,
    /// The user cancelled mid-recording; the audio is kept so it can be recognized again.
    Cancelled,
    Failed,
}

impl HistoryKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Dictation => "dictation",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }
    fn parse(s: &str) -> Self {
        match s {
            "cancelled" => Self::Cancelled,
            "failed" => Self::Failed,
            _ => Self::Dictation,
        }
    }
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

fn style_str(s: Style) -> &'static str {
    match s {
        Style::General => "general",
        Style::Slack => "slack",
        Style::Teams => "teams",
    }
}

fn style_parse(s: &str) -> Style {
    match s {
        "slack" => Style::Slack,
        "teams" => Style::Teams,
        _ => Style::General,
    }
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

    fn init(conn: Connection, tz_offset_secs: i32) -> Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS history (
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
        )?;
        Ok(Self { conn: Mutex::new(conn), tz_offset_secs })
    }

    fn day_of(&self, ts: i64) -> String {
        let local = time::OffsetDateTime::from_unix_timestamp(ts + self.tz_offset_secs as i64)
            .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
        format!("{:04}-{:02}-{:02}", local.year(), u8::from(local.month()), local.day())
    }

    // ---- history ----

    pub fn add_history(&self, h: &NewHistory) -> Result<i64> {
        let c = self.conn.lock().unwrap();
        c.execute(
            "INSERT INTO history (ts, day, kind, style, app, lang, duration_ms, raw, polished, error, audio_path)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                h.ts,
                self.day_of(h.ts),
                h.kind.as_str(),
                style_str(h.style),
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
        self.conn.lock().unwrap().execute(
            "UPDATE history SET kind='dictation', raw=?2, polished=?3, lang=?4, error=NULL WHERE id=?1",
            params![id, raw, polished, lang],
        )?;
        Ok(())
    }

    pub fn set_polished(&self, id: i64, polished: &str) -> Result<()> {
        self.conn.lock().unwrap().execute("UPDATE history SET polished=?2 WHERE id=?1", params![id, polished])?;
        Ok(())
    }

    pub fn get_history(&self, id: i64) -> Result<Option<HistoryEntry>> {
        let c = self.conn.lock().unwrap();
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
        if let Some(style) = f.style {
            sql.push_str(&format!(" AND style = ?{}", args.len() + 1));
            args.push(style_str(style).to_string().into());
        }
        sql.push_str(" ORDER BY ts DESC, id DESC");
        if let Some(l) = f.limit {
            sql.push_str(&format!(" LIMIT {l}"));
        }
        let c = self.conn.lock().unwrap();
        let mut stmt = c.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args), row_to_entry)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn delete_history(&self, id: i64) -> Result<()> {
        self.conn.lock().unwrap().execute("DELETE FROM history WHERE id=?1", [id])?;
        Ok(())
    }

    // ---- dictionary ----

    /// Returns false when the term already exists (or is blank).
    pub fn add_word(&self, term: &str, now: i64) -> Result<bool> {
        let term = term.trim();
        if term.is_empty() {
            return Ok(false);
        }
        let n = self
            .conn
            .lock()
            .unwrap()
            .execute("INSERT OR IGNORE INTO dictionary (term, created) VALUES (?1, ?2)", params![term, now])?;
        Ok(n > 0)
    }

    pub fn remove_word(&self, term: &str) -> Result<()> {
        self.conn.lock().unwrap().execute("DELETE FROM dictionary WHERE term=?1", [term])?;
        Ok(())
    }

    pub fn words(&self, query: Option<&str>) -> Result<Vec<String>> {
        let c = self.conn.lock().unwrap();
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

    // ---- stats ----

    pub fn stats(&self, since_ts: Option<i64>) -> Result<Stats> {
        let c = self.conn.lock().unwrap();
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
        kind: HistoryKind::parse(&r.get::<_, String>(2)?),
        style: style_parse(&r.get::<_, String>(3)?),
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
}
