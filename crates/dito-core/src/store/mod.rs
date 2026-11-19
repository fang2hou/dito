//! SQLite-backed history and wordbook.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::{Connection, OptionalExtension, Statement, params};

use crate::config::EngineLabels;
use crate::prompts::AppTarget;

mod activity;
mod chats;
mod model;
mod rows;
mod schema;
mod transcripts;

pub use activity::{Period, civil, heat_quartiles, heat_step, local_day, month_span, shown_days, week_start};
pub use chats::{Attachment, Author, Chat, ChatMessageRow, ChatSummary, MessageState, NewChatMessage};
pub use model::*;
use rows::{SELECT_HISTORY, row_to_entry};
use schema::{SCHEMA_VERSION, encode, migrate, user_version};
pub use transcripts::{
    ParagraphState, RESUME_WINDOW_SECS, Reopened, Sources, Speaker, Transcript, TranscriptParagraph, TranscriptSummary,
};

pub struct Store {
    conn: Mutex<Connection>,
    tz_offset_secs: i32,
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
        let store = Self { conn: Mutex::new(conn), tz_offset_secs };
        store.recover_chats()?;
        Ok(store)
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().expect("store connection mutex poisoned")
    }

    fn day_of(&self, ts: i64) -> String {
        activity::day_key(local_day(ts, self.tz_offset_secs))
    }

    pub fn add_history(&self, h: &NewHistory) -> Result<i64> {
        let command = h
            .command
            .as_ref()
            .map(|details| {
                serde_json::to_string(details).map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))
            })
            .transpose()?;
        let c = self.conn();
        c.execute(
            "INSERT INTO history (ts, day, kind, style, app, lang, duration_ms, raw, polished, error, audio_path,
                                  asr_engine, polish_engine, asr_ms, polish_ms, source, command)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
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
                h.audio_path,
                h.engines.asr,
                h.engines.polish,
                h.timings.asr.map(|ms| ms as i64),
                h.timings.polish.map(|ms| ms as i64),
                h.source,
                command
            ],
        )?;
        Ok(c.last_insert_rowid())
    }

    /// `engines` and `timings` are the retry's and replace the earlier attempt's, so a polish label the retry lacks is
    /// cleared.
    pub fn complete_history(
        &self,
        id: i64,
        raw: &str,
        polished: Option<&str>,
        lang: &str,
        engines: &EngineLabels,
        timings: &StepMillis,
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE history
             SET kind='dictation', raw=?2, polished=?3, lang=?4, asr_engine=?5, polish_engine=?6, error=NULL,
                 asr_ms=?7, polish_ms=?8
             WHERE id=?1",
            params![
                id,
                raw,
                polished,
                lang,
                engines.asr,
                engines.polish,
                timings.asr.map(|ms| ms as i64),
                timings.polish.map(|ms| ms as i64)
            ],
        )?;
        Ok(())
    }

    /// The transcript is unchanged, so its engine label and recognition time are left alone.
    pub fn set_polished(&self, id: i64, polished: &str, polish_engine: &str, polish_ms: u64) -> Result<()> {
        self.conn().execute(
            "UPDATE history SET polished=?2, polish_engine=?3, polish_ms=?4 WHERE id=?1",
            params![id, polished, polish_engine, polish_ms as i64],
        )?;
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
            // For a command row these are the instruction, the result and the text it worked on.
            sql.push_str(
                " AND (raw LIKE ?1 ESCAPE '\\' OR polished LIKE ?1 ESCAPE '\\' OR source LIKE ?1 ESCAPE '\\')",
            );
            let esc = q.trim().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
            args.push(format!("%{esc}%").into());
        }
        if let Some(since) = f.since_ts {
            sql.push_str(&format!(" AND ts >= ?{}", args.len() + 1));
            args.push(since.into());
        }
        if let Some(style) = &f.style {
            match style {
                // Other: every recording outside the active built-ins whose target and recorded app name match none
                // of the configured apps. Matching by app name keeps entries recorded before the app was configured
                // (or under a different bundle id) grouped with that app.
                AppTarget::General => {
                    let active: Vec<_> = AppTarget::builtins()
                        .filter(|style| {
                            !f.disabled_builtin_styles.contains(style)
                                || f.configured_apps.iter().any(|(_, id)| AppTarget::for_bundle_id(id) == *style)
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
                                .map(|(_, id)| encode(AppTarget::Application(id.clone())))
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
                AppTarget::Builtin(_) => {
                    sql.push_str(&format!(" AND style = ?{}", args.len() + 1));
                    args.push(encode(style.clone()).into());
                }
                AppTarget::Application(id) => {
                    let name = f.configured_apps.iter().find(|(_, app_id)| app_id == id).map(|(n, _)| n.clone());
                    let first = args.len() + 1;
                    let builtin = AppTarget::for_bundle_id(id);
                    if !matches!(builtin, AppTarget::Application(_) | AppTarget::General) {
                        args.push(encode(style.clone()).into());
                        args.push(encode(builtin).into());
                        sql.push_str(&format!(" AND (style = ?{first} OR style = ?{})", first + 1));
                    } else {
                        match name.filter(|n| !n.is_empty()) {
                            Some(name) => {
                                args.push(encode(style.clone()).into());
                                args.push(name.into());
                                let ids: Vec<_> = AppTarget::builtins().map(encode).collect();
                                let marks: Vec<_> = (0..ids.len()).map(|i| format!("?{}", first + 2 + i)).collect();
                                args.extend(ids.into_iter().map(Into::into));
                                sql.push_str(&format!(
                                    " AND (style = ?{first} OR (app = ?{} AND style NOT IN ({})))",
                                    first + 1,
                                    marks.join(", ")
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
        let conn = self.conn();
        insert_word(&mut conn.prepare(INSERT_WORD)?, term, now)
    }

    pub fn remove_words(&self, terms: &[String]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        {
            let mut delete = tx.prepare("DELETE FROM wordbook WHERE term=?1")?;
            for term in terms {
                delete.execute([term])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn words(&self, query: Option<&str>) -> Result<Vec<String>> {
        let c = self.conn();
        let like = format!("%{}%", query.unwrap_or("").trim());
        let mut stmt = c.prepare("SELECT term FROM wordbook WHERE term LIKE ?1 ORDER BY created DESC, term")?;
        Ok(stmt.query_map([like], |r| r.get(0))?.collect::<std::result::Result<_, _>>()?)
    }

    /// Adds every new line or, on an error, none of them. Returns how many were added.
    pub fn import_words(&self, text: &str, now: i64) -> Result<usize> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let mut added = 0;
        {
            let mut insert = tx.prepare(INSERT_WORD)?;
            for line in text.lines() {
                if insert_word(&mut insert, line, now)? {
                    added += 1;
                }
            }
        }
        tx.commit()?;
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
            apps: group("app")?,
            langs: group("lang")?,
        })
    }

    /// The `chars` of [`stats`](Self::stats) alone, in one statement.
    pub fn chars_since(&self, since_ts: i64) -> Result<u64> {
        let chars: i64 = self.conn().query_row(
            "SELECT COALESCE(SUM(LENGTH(COALESCE(polished, raw))),0) FROM history WHERE kind='dictation' AND ts >= ?1",
            [since_ts],
            |r| r.get(0),
        )?;
        Ok(chars as u64)
    }

    /// Dictated characters on each local day from `first` to `last`, one value per day.
    pub fn chars_per_day(&self, first: i64, last: i64) -> Result<Vec<u64>> {
        let mut days = vec![0; usize::try_from(last - first + 1).unwrap_or(0)];
        let c = self.conn();
        let mut stmt = c.prepare(
            "SELECT day, SUM(LENGTH(COALESCE(polished, raw))) FROM history
             WHERE kind='dictation' AND day BETWEEN ?1 AND ?2 GROUP BY day",
        )?;
        let rows = stmt.query_map([activity::day_key(first), activity::day_key(last)], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
        })?;
        for row in rows {
            let (key, chars) = row?;
            let slot = activity::day_from_key(&key).and_then(|day| usize::try_from(day - first).ok());
            if let Some(slot) = slot.and_then(|slot| days.get_mut(slot)) {
                *slot = chars;
            }
        }
        Ok(days)
    }

    /// The local day of the earliest dictation.
    pub fn first_day(&self) -> Result<Option<i64>> {
        let key: Option<String> =
            self.conn().query_row("SELECT MIN(day) FROM history WHERE kind='dictation'", [], |r| r.get(0))?;
        Ok(key.as_deref().and_then(activity::day_from_key))
    }
}

const INSERT_WORD: &str = "INSERT OR IGNORE INTO wordbook (term, created) VALUES (?1, ?2)";

fn insert_word(insert: &mut Statement<'_>, term: &str, now: i64) -> Result<bool> {
    let term = term.trim();
    if term.is_empty() {
        return Ok(false);
    }
    Ok(insert.execute(params![term, now])? > 0)
}

#[cfg(test)]
mod tests {
    use super::schema::MIGRATIONS;
    use super::*;
    use crate::intent::{Decision, Intent, RewriteOp};

    fn builtin(id: &str) -> AppTarget {
        AppTarget::builtin(id).expect("a built-in style")
    }

    fn entry(ts: i64, app: &str, lang: &str, raw: &str, polished: Option<&str>) -> NewHistory {
        NewHistory {
            ts,
            kind: HistoryKind::Dictation,
            style: AppTarget::General,
            app: app.into(),
            lang: lang.into(),
            duration_ms: 6_000,
            raw: raw.into(),
            polished: polished.map(Into::into),
            error: None,
            audio_path: None,
            engines: EngineLabels::default(),
            timings: StepMillis::default(),
            source: None,
            command: None,
        }
    }

    fn labels(asr: &str, polish: Option<&str>) -> EngineLabels {
        EngineLabels { asr: Some(asr.into()), polish: polish.map(Into::into) }
    }

    #[test]
    fn custom_application_style_and_engine_labels_round_trip_in_history() {
        let store = Store::open_in_memory().unwrap();
        let mut row = entry(1, "Editor", "en", "hello", None);
        row.style = AppTarget::Application("example.editor".into());
        row.engines = labels("asr.funasr:paraformer-v2", Some("azure:gpt-example"));
        let id = store.add_history(&row).unwrap();
        let rows = store.list_history(&HistoryFilter::default()).unwrap();
        assert_eq!(rows[0].style, row.style);
        assert_eq!(rows[0].engines, row.engines);

        store.set_polished(id, "polished again", "bedrock:example-model", 900).unwrap();
        let again = store.get_history(id).unwrap().unwrap();
        assert_eq!(again.polished.as_deref(), Some("polished again"));
        assert_eq!(again.timings, StepMillis { asr: None, polish: Some(900) });
        assert_eq!(
            again.engines,
            labels("asr.funasr:paraformer-v2", Some("bedrock:example-model")),
            "polishing again relabels the polish engine and leaves the recognition label"
        );
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
        slack.style = builtin("slack");
        s.add_history(&slack).unwrap();
        let all = s.list_history(&HistoryFilter::default()).unwrap();
        assert_eq!(all.iter().map(|e| e.raw.as_str()).collect::<Vec<_>>(), ["new", "old"]);
        let recent = s.list_history(&HistoryFilter { since_ts: Some(20), ..Default::default() }).unwrap();
        assert_eq!(recent.len(), 1);
        let styled = s.list_history(&HistoryFilter { style: Some(builtin("slack")), ..Default::default() }).unwrap();
        assert_eq!(styled[0].raw, "new");
    }

    #[test]
    fn removing_and_readding_builtin_styles_reclassifies_history_without_deleting_it() {
        let store = Store::open_in_memory().unwrap();
        let mut recorded = entry(10, "Herdr", "ja", "preserved", None);
        recorded.style = builtin("herdr");
        store.add_history(&recorded).unwrap();
        let mut filter = HistoryFilter {
            style: Some(AppTarget::General),
            disabled_builtin_styles: vec![builtin("herdr")],
            ..Default::default()
        };
        assert_eq!(store.list_history(&filter).unwrap().len(), 1);
        filter.configured_apps.push(("Agent work".into(), "dev.herdr.app".into()));
        assert!(store.list_history(&filter).unwrap().is_empty());
        filter.style = Some(AppTarget::Application("dev.herdr.app".into()));
        assert_eq!(store.list_history(&filter).unwrap()[0].raw, "preserved");
    }

    #[test]
    fn app_classes_follow_the_configured_list_by_style_and_name() {
        let s = Store::open_in_memory().unwrap();
        s.add_history(&entry(10, "Notes", "ja", "general", None)).unwrap();
        let mut legacy = entry(15, "App Store", "ja", "legacy", None);
        legacy.style = AppTarget::General;
        s.add_history(&legacy).unwrap();
        let mut slack = entry(20, "Slack", "ja", "slack", None);
        slack.style = builtin("slack");
        s.add_history(&slack).unwrap();
        let mut kept = entry(30, "App Store", "ja", "kept", None);
        kept.style = AppTarget::Application("com.apple.AppStore".into());
        s.add_history(&kept).unwrap();
        let mut gone = entry(40, "Old", "ja", "gone", None);
        gone.style = AppTarget::Application("example.gone".into());
        s.add_history(&gone).unwrap();
        let configured = vec![("App Store".to_string(), "com.apple.AppStore".to_string())];

        let other = HistoryFilter {
            style: Some(AppTarget::General),
            configured_apps: configured.clone(),
            ..Default::default()
        };
        let rows = s.list_history(&other).unwrap();
        let raws: Vec<&str> = rows.iter().map(|e| e.raw.as_str()).collect();
        assert_eq!(raws, ["gone", "general"], "a removed app joins Other; the configured app's entries stay out");

        let app = HistoryFilter {
            style: Some(AppTarget::Application("com.apple.AppStore".into())),
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
            style: Some(AppTarget::Application("com.example.newstore".into())),
            configured_apps: vec![("App Store".into(), "com.example.newstore".into())],
            ..Default::default()
        };
        let rows = s.list_history(&re_added).unwrap();
        assert_eq!(
            rows.iter().map(|e| e.raw.as_str()).collect::<Vec<_>>(),
            ["kept", "legacy"],
            "an app re-added under another bundle id still claims its past entries by name"
        );

        let none_configured = HistoryFilter { style: Some(AppTarget::General), ..Default::default() };
        let rows = s.list_history(&none_configured).unwrap();
        let raws: Vec<&str> = rows.iter().map(|e| e.raw.as_str()).collect();
        assert_eq!(
            raws,
            ["gone", "kept", "legacy", "general"],
            "with no configured apps every recording outside the built-in styles is Other"
        );
        assert!(
            s.list_history(&HistoryFilter { style: Some(builtin("slack")), ..Default::default() }).unwrap()[0].raw
                == "slack"
        );
        assert!(
            !s.list_history(&none_configured).unwrap().iter().any(|e| e.style == builtin("slack")),
            "a built-in style never joins Other"
        );
    }

    #[test]
    fn cancelled_entry_becomes_a_dictation_after_retry() {
        let s = Store::open_in_memory().unwrap();
        let mut c = entry(5, "Notes", "", "", None);
        c.kind = HistoryKind::Cancelled;
        c.audio_path = Some("/tmp/a.wav".into());
        c.engines = labels("local:old-asr", Some("local:old-polish"));
        let id = s.add_history(&c).unwrap();
        let retry = labels("local:new-asr", Some("local:new-polish"));
        let timings = StepMillis { asr: Some(400), polish: Some(700) };
        s.complete_history(id, "こんにちは", Some("こんにちは。"), "ja", &retry, &timings).unwrap();
        let e = s.get_history(id).unwrap().unwrap();
        assert_eq!(e.kind, HistoryKind::Dictation);
        assert_eq!(e.timings, timings);
        assert_eq!(e.final_text(), "こんにちは。");
        assert_eq!(e.audio_path.as_deref(), Some("/tmp/a.wav"), "audio is kept for playback");
        assert_eq!(e.engines, retry, "the retry's engines replace the cancelled attempt's");
    }

    fn command_entry() -> NewHistory {
        let mut ranked = [0.02; Intent::COUNT];
        ranked[Intent::Rewrite as usize] = 0.9;
        NewHistory {
            kind: HistoryKind::Command,
            raw: "もっと短く".into(),
            polished: Some("短い文。".into()),
            source: Some("これは長い文章です。".into()),
            duration_ms: 2_500,
            timings: StepMillis { asr: Some(300), polish: Some(1_200) },
            engines: labels("local:asr", Some("local:chat")),
            command: Some(CommandDetails {
                origin: Some(InstructionOrigin::Spoken),
                decision: Some(Decision {
                    intent: Intent::Rewrite,
                    confidence: 0.9,
                    probabilities: ranked,
                    rewrite_op: Some((RewriteOp::Shorten, 0.8)),
                }),
                choice: Some(ToolChoice::Confident),
                tool: Some(Intent::Rewrite),
                source: Some(SourceKind::Selection),
                outcome: CommandOutcome::Replaced,
                route_ms: Some(150),
                router_engine: Some("local:chat".into()),
            }),
            ..entry(5, "Notes", "ja", "", None)
        }
    }

    #[test]
    fn a_command_entry_round_trips_with_its_details() {
        let s = Store::open_in_memory().unwrap();
        let row = command_entry();
        let id = s.add_history(&row).unwrap();
        let read = s.get_history(id).unwrap().unwrap();
        assert_eq!(read.kind, HistoryKind::Command);
        assert_eq!((read.raw.as_str(), read.final_text()), ("もっと短く", "短い文。"));
        assert_eq!(read.source, row.source);
        assert_eq!(read.timings, row.timings);
        assert_eq!(read.engines, row.engines);
        assert_eq!(read.command, row.command);
        assert_eq!(read.audio_path, None);
    }

    #[test]
    fn search_finds_a_command_by_its_instruction_source_and_result() {
        let s = Store::open_in_memory().unwrap();
        s.add_history(&command_entry()).unwrap();
        let find =
            |q: &str| s.list_history(&HistoryFilter { query: Some(q.into()), ..Default::default() }).unwrap().len();
        assert_eq!((find("もっと"), find("長い文章"), find("短い文。"), find("無関係")), (1, 1, 1, 0));
    }

    #[test]
    fn command_rows_are_not_dictation_statistics() {
        let s = Store::open_in_memory().unwrap();
        s.add_history(&command_entry()).unwrap();
        assert_eq!(s.stats(None).unwrap().recordings, 0);
        assert_eq!(s.first_day().unwrap(), None);
    }

    #[test]
    fn details_a_build_cannot_read_leave_the_command_row_listed() {
        let s = Store::open_in_memory().unwrap();
        let id = s.add_history(&command_entry()).unwrap();
        s.conn().execute("UPDATE history SET command='{\"outcome\":\"from-the-future\"}' WHERE id=?1", [id]).unwrap();
        let read = s.get_history(id).unwrap().unwrap();
        assert_eq!((read.kind, read.command.as_ref()), (HistoryKind::Command, None));
        assert_eq!(read.final_text(), "短い文。");
    }

    #[test]
    fn unknown_stored_kinds_and_styles_read_as_defaults() {
        let s = Store::open_in_memory().unwrap();
        let id = s.add_history(&entry(5, "A", "ja", "x", None)).unwrap();
        s.conn().execute("UPDATE history SET kind='from-the-future', style='mystery' WHERE id=?1", [id]).unwrap();
        let e = s.get_history(id).unwrap().unwrap();
        assert_eq!((e.kind, e.style), (HistoryKind::Dictation, AppTarget::General));
    }

    #[test]
    fn wordbook_dedupes_trims_and_round_trips_import_export() {
        let s = Store::open_in_memory().unwrap();
        assert!(s.add_word("  Acme ", 1).unwrap());
        assert!(!s.add_word("Acme", 2).unwrap());
        assert!(!s.add_word("   ", 3).unwrap());
        assert_eq!(s.import_words("Contoso\nAcme\n\nBedrock\n", 4).unwrap(), 2);
        assert_eq!(s.export_words().unwrap(), "Acme\nBedrock\nContoso");
        s.remove_words(&["Contoso".into(), "Acme".into(), "missing".into()]).unwrap();
        assert_eq!(s.words(None).unwrap(), ["Bedrock"]);
    }

    #[test]
    fn wordbook_import_counts_only_new_words_and_leaves_known_ones_as_they_were() {
        let s = Store::open_in_memory().unwrap();
        s.add_word("Acme", 1).unwrap();
        assert_eq!(s.import_words("Contoso\n\n  Bedrock \nAcme\nContoso\n   \nDito", 2).unwrap(), 3);
        // Newest first, so a known word that kept its time comes last.
        assert_eq!(s.words(None).unwrap(), ["Bedrock", "Contoso", "Dito", "Acme"]);
    }

    #[test]
    fn wordbook_batch_deletion_rolls_back_on_error() {
        let s = Store::open_in_memory().unwrap();
        s.import_words("Acme\nContoso", 1).unwrap();
        s.conn()
            .execute_batch(
                "CREATE TRIGGER reject_delete BEFORE DELETE ON wordbook
                 WHEN OLD.term = 'Contoso' BEGIN SELECT RAISE(ABORT, 'test failure'); END;",
            )
            .unwrap();

        assert!(s.remove_words(&["Acme".into(), "Contoso".into()]).is_err());
        assert_eq!(s.words(None).unwrap(), ["Acme", "Contoso"]);
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
        assert_eq!(s.chars_per_day(0, 3).unwrap(), [0, 15, 2, 0], "one value per day, failures left out");
        assert_eq!(s.first_day().unwrap(), Some(1));
        assert_eq!(st.apps[0], ("Slack".into(), 2));
        assert_eq!(st.langs[0], ("ja".into(), 2));
        assert!((st.avg_cpm - 17.0 / 0.3).abs() < 1e-6);
        assert_eq!(s.stats(Some(2 * day)).unwrap().recordings, 1);
    }

    #[test]
    fn chars_since_matches_the_stats_total() {
        let s = Store::open_in_memory().unwrap();
        s.add_history(&entry(10, "Slack", "ja", "raw", Some("0123456789"))).unwrap();
        s.add_history(&entry(20, "Slack", "ja", "abcde", None)).unwrap();
        let mut failed = entry(20, "Slack", "ja", "zzzzzz", None);
        failed.kind = HistoryKind::Failed;
        s.add_history(&failed).unwrap();
        s.add_history(&NewHistory { ts: 25, ..command_entry() }).unwrap();

        for (since, chars) in [(0, 15), (11, 5), (21, 0)] {
            assert_eq!(s.chars_since(since).unwrap(), chars);
            assert_eq!(s.stats(Some(since)).unwrap().chars, chars);
        }
    }

    #[test]
    fn timezone_offset_shifts_the_day_bucket() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path().join("h.db"), 9 * 3600).unwrap();
        // 1970-01-01 20:00 UTC is already 01-02 in JST.
        s.add_history(&entry(20 * 3600, "A", "ja", "x", None)).unwrap();
        assert_eq!(s.first_day().unwrap(), Some(1));
        // Sunday 1970-01-04 23:30 JST closes the week that Monday 1970-01-05 00:30 JST opens.
        s.add_history(&entry(3 * 86_400 + 14 * 3600 + 1800, "A", "ja", "xx", None)).unwrap();
        s.add_history(&entry(3 * 86_400 + 15 * 3600 + 1800, "A", "ja", "xxx", None)).unwrap();
        assert_eq!(week_start(3), -3);
        assert_eq!(s.chars_per_day(-3, 3).unwrap(), [0, 0, 0, 0, 1, 0, 2]);
        assert_eq!(week_start(4), 4);
        assert_eq!(s.chars_per_day(4, 10).unwrap(), [3, 0, 0, 0, 0, 0, 0]);
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
