//! SQLite-backed dictation history.

use rusqlite::{OptionalExtension, params};

use crate::config::EngineLabels;
use crate::prompts::AppTarget;

use super::rows::{SELECT_HISTORY, row_to_entry};
use super::schema::encode;
use super::*;

impl Store {
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
        let (sql, args) = Self::list_query(f);
        let c = self.conn();
        let mut stmt = c.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args), row_to_entry)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    fn list_query(f: &HistoryFilter) -> (String, Vec<rusqlite::types::Value>) {
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
        if let Some(limit) = f.limit {
            // The unary plus hides the limit's value from the planner, which reads a plain parameter as it does a
            // literal. Shown a small limit, it scans and sorts the whole table; without one it walks `history_ts`,
            // which stops after the newest rows.
            sql.push_str(&format!(" LIMIT +?{}", args.len() + 1));
            args.push(i64::try_from(limit).unwrap_or(i64::MAX).into());
        }
        (sql, args)
    }

    pub fn delete_history(&self, id: i64) -> Result<()> {
        self.conn().execute("DELETE FROM history WHERE id=?1", [id])?;
        Ok(())
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::intent::{Decision, Intent, RewriteOp};
    use crate::prompts::AppTarget;

    pub(in crate::store) fn builtin(id: &str) -> AppTarget {
        AppTarget::builtin(id).expect("a built-in style")
    }

    pub(in crate::store) fn entry(ts: i64, app: &str, lang: &str, raw: &str, polished: Option<&str>) -> NewHistory {
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

    pub(in crate::store) fn labels(asr: &str, polish: Option<&str>) -> EngineLabels {
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

    /// The list query as it was before the limit became a hidden parameter: the same text with the limit written in.
    fn with_written_limit(filter: &HistoryFilter) -> (String, Vec<rusqlite::types::Value>) {
        let (sql, mut args) = Store::list_query(filter);
        let Some(limit) = filter.limit else { return (sql, args) };
        let hidden = format!(" LIMIT +?{}", args.len());
        let unlimited = sql.strip_suffix(&hidden).expect("the limit ends the query");
        args.pop();
        (format!("{unlimited} LIMIT {limit}"), args)
    }

    #[test]
    fn a_hidden_limit_lists_what_a_written_limit_listed_and_reads_the_newest_row_through_the_time_index() {
        let s = Store::open_in_memory().unwrap();
        for i in 0..700 {
            // Timestamps repeat and do not follow the ids, so the order rests on both of its terms.
            let raw = if i % 5 == 0 { "needle in the text" } else { "plain" };
            let mut row = entry(i * 37 % 240, ["Editor", "Notes", "Mail"][i as usize % 3], "ja", raw, None);
            row.style = match i % 4 {
                0 => builtin("slack"),
                1 => AppTarget::Application("example.editor".into()),
                _ => AppTarget::General,
            };
            s.add_history(&row).unwrap();
        }
        let shapes = [
            HistoryFilter::default(),
            HistoryFilter { since_ts: Some(120), ..Default::default() },
            HistoryFilter { style: Some(builtin("slack")), ..Default::default() },
            HistoryFilter {
                style: Some(AppTarget::General),
                configured_apps: vec![("Editor".into(), "example.editor".into())],
                ..Default::default()
            },
            HistoryFilter { query: Some("needle".into()), ..Default::default() },
        ];
        for shape in shapes {
            for limit in [Some(1), Some(2), Some(10), Some(500), None] {
                let filter = HistoryFilter { limit, ..shape.clone() };
                let (sql, args) = with_written_limit(&filter);
                let before: Vec<HistoryEntry> = {
                    let c = s.conn();
                    let mut stmt = c.prepare(&sql).unwrap();
                    let rows = stmt.query_map(rusqlite::params_from_iter(args), row_to_entry).unwrap();
                    rows.map(|row| row.unwrap()).collect()
                };
                assert!(!before.is_empty(), "{filter:?}");
                assert!(before.windows(2).all(|pair| (pair[0].ts, pair[0].id) > (pair[1].ts, pair[1].id)));
                assert_eq!(s.list_history(&filter).unwrap(), before, "{filter:?}");
            }
        }

        let (probe, args) = Store::list_query(&HistoryFilter { limit: Some(1), ..Default::default() });
        let plan: String = s
            .conn()
            .query_row(&format!("EXPLAIN QUERY PLAN {probe}"), rusqlite::params_from_iter(args), |row| row.get(3))
            .unwrap();
        assert!(plan.contains("history_ts"), "the newest row is found by reading the whole table: {plan}");
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

    pub(in crate::store) fn command_entry() -> NewHistory {
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
}
