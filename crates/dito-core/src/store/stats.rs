//! SQLite-backed dictation statistics.

use rusqlite::params;

use super::*;

/// More than any UTC offset a row's `day` was stored under. `day` is `ts` shifted by the offset at the time of
/// recording, which can differ from today's, so the daily queries keep their `day` filter and add a `ts` range this
/// much wider, which lets them read through `history_ts` instead of scanning the table.
const DAY_SLACK_SECS: i64 = 2 * 86_400;

impl Store {
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
             WHERE kind='dictation' AND day BETWEEN ?1 AND ?2 AND ts >= ?3 AND ts < ?4 GROUP BY day",
        )?;
        let rows = stmt.query_map(
            params![
                activity::day_key(first),
                activity::day_key(last),
                first * 86_400 - DAY_SLACK_SECS,
                (last + 1) * 86_400 + DAY_SLACK_SECS
            ],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64)),
        )?;
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
        // A later dictation has an earlier day only if the offset moved back in between, so the rows within two
        // slacks of the earliest timestamp decide the minimum.
        let key: Option<String> = self.conn().query_row(
            "SELECT MIN(day) FROM history WHERE kind='dictation'
             AND ts <= (SELECT MIN(ts) FROM history WHERE kind='dictation') + ?1",
            [2 * DAY_SLACK_SECS],
            |r| r.get(0),
        )?;
        Ok(key.as_deref().and_then(activity::day_from_key))
    }
}

#[cfg(test)]
mod tests {
    use super::super::activity::week_start;
    use super::super::history::tests::{command_entry, entry};
    use super::*;

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

    #[test]
    fn daily_queries_match_a_table_scan_across_offsets_and_day_boundaries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("h.db");
        let (hour, day) = (3600, 86_400);
        let open = |offset_hours: i32| Store::open(&path, offset_hours * 3600).unwrap();
        let other_kinds = [HistoryKind::Failed, HistoryKind::Cancelled, HistoryKind::Command];
        // Recorded under offsets that moved both ways, as after travel, so no single offset maps `ts` to `day`.
        for (i, offset_hours) in [14, -12, 0, 9, -11, 13].into_iter().enumerate() {
            let s = open(offset_hours);
            let midnight = (10 + 2 * i as i64) * day - i64::from(offset_hours) * hour;
            let moments = [midnight - 1, midnight, midnight + day / 2, midnight + day - 1, midnight + day];
            for (n, ts) in moments.into_iter().enumerate() {
                s.add_history(&entry(ts, "A", "ja", &"x".repeat(1 + n + 10 * i), None)).unwrap();
                let mut other = entry(ts, "A", "ja", "not a dictation", None);
                other.kind = other_kinds[n % 3];
                s.add_history(&other).unwrap();
            }
        }
        // The earliest dictation is on day 5; one an hour later, after the offset moved back, is on day 4. Rows of
        // other kinds come before both, on an earlier day too.
        let earliest = 5 * day - 2 * hour;
        {
            let s = open(14);
            s.add_history(&entry(earliest, "A", "ja", "day five", None)).unwrap();
            let mut failed = entry(earliest - 10, "A", "ja", "", None);
            failed.kind = HistoryKind::Failed;
            s.add_history(&failed).unwrap();
        }
        let s = open(-12);
        s.add_history(&entry(earliest + hour, "A", "ja", "day four", None)).unwrap();
        let mut cancelled = entry(earliest - day, "A", "ja", "", None);
        cancelled.kind = HistoryKind::Cancelled;
        s.add_history(&cancelled).unwrap();

        let days = 26;
        let mut scanned = vec![0u64; days];
        let scanned_first: Option<String> = {
            let c = s.conn();
            let mut stmt = c
                .prepare(
                    "SELECT day, SUM(LENGTH(COALESCE(polished, raw))) FROM history
                     WHERE kind='dictation' AND day BETWEEN ?1 AND ?2 GROUP BY day",
                )
                .unwrap();
            let range = [activity::day_key(0), activity::day_key(days as i64 - 1)];
            let rows = stmt.query_map(range, |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))).unwrap();
            for row in rows {
                let (key, chars) = row.unwrap();
                scanned[activity::day_from_key(&key).unwrap() as usize] = chars as u64;
            }
            c.query_row("SELECT MIN(day) FROM history WHERE kind='dictation'", [], |r| r.get(0)).unwrap()
        };
        assert_eq!(scanned.iter().filter(|chars| **chars > 0).count(), 15, "the seed covers many days");

        assert_eq!(s.first_day().unwrap(), scanned_first.as_deref().and_then(activity::day_from_key));
        assert_eq!(s.first_day().unwrap(), Some(4));
        for first in 0..days {
            for last in first..days {
                let read = s.chars_per_day(first as i64, last as i64).unwrap();
                assert_eq!(read, scanned[first..=last], "days {first} to {last}");
            }
        }
    }
}
