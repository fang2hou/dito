//! Calendar arithmetic behind the Stats page (`design:settings`): local days, Monday-first weeks and heat steps.
//!
//! A day is counted in whole days since 1970-01-01 in local time, the same bucket `history.day` stores as text.

/// The period control of the Stats page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Period {
    #[default]
    Week,
    Month,
    All,
}

/// The heatmap never reaches further back than this many days before today.
const HEATMAP_REACH_DAYS: i64 = 364;

pub fn local_day(ts: i64, tz_offset_secs: i32) -> i64 {
    (ts + tz_offset_secs as i64).div_euclid(86_400)
}

/// (year, month 1 to 12, day of month).
pub fn civil(day: i64) -> (i32, u8, u8) {
    let date = date_of(day);
    (date.year(), u8::from(date.month()), date.day())
}

/// The Monday that starts the week of `day`.
pub fn week_start(day: i64) -> i64 {
    // 1970-01-01 was a Thursday.
    day - (day + 3).rem_euclid(7)
}

/// First and last day of the month of `day`.
pub fn month_span(day: i64) -> (i64, i64) {
    let date = date_of(day);
    let first = day - i64::from(date.day()) + 1;
    (first, first + i64::from(date.month().length(date.year())) - 1)
}

/// First and last day the daily view draws: whole Monday-first weeks, except the month, which the calendar pads
/// itself. The heatmap starts at the first week of use and reaches back one year at most.
pub fn shown_days(period: Period, today: i64, first_use: Option<i64>) -> (i64, i64) {
    match period {
        Period::Week => (week_start(today), week_start(today) + 6),
        Period::Month => month_span(today),
        Period::All => {
            let from = first_use.unwrap_or(today).clamp(today - HEATMAP_REACH_DAYS, today);
            (week_start(from), week_start(today) + 6)
        }
    }
}

/// Upper bounds of heat steps 1 to 3: the quartiles of the non-zero values, so the steps follow the user's own
/// range instead of fixed thresholds.
pub fn heat_quartiles(values: impl IntoIterator<Item = u64>) -> [u64; 3] {
    let mut active: Vec<u64> = values.into_iter().filter(|v| *v > 0).collect();
    if active.is_empty() {
        return [0; 3];
    }
    active.sort_unstable();
    [1, 2, 3].map(|quarter| active[(active.len() * quarter).div_ceil(4) - 1])
}

/// 0 for a day without dictation, else 1 to 4.
pub fn heat_step(value: u64, quartiles: &[u64; 3]) -> usize {
    if value == 0 {
        return 0;
    }
    1 + quartiles.iter().filter(|bound| value > **bound).count()
}

pub(super) fn day_key(day: i64) -> String {
    let (year, month, day) = civil(day);
    format!("{year:04}-{month:02}-{day:02}")
}

pub(super) fn day_from_key(key: &str) -> Option<i64> {
    let mut parts = key.splitn(3, '-');
    let year = parts.next()?.parse().ok()?;
    let month = time::Month::try_from(parts.next()?.parse::<u8>().ok()?).ok()?;
    let date = time::Date::from_calendar_date(year, month, parts.next()?.parse().ok()?).ok()?;
    Some((date - time::OffsetDateTime::UNIX_EPOCH.date()).whole_days())
}

fn date_of(day: i64) -> time::Date {
    time::OffsetDateTime::UNIX_EPOCH.date().checked_add(time::Duration::days(day)).unwrap_or(time::Date::MIN)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(key: &str) -> i64 {
        day_from_key(key).expect("a date")
    }

    #[test]
    fn weeks_start_on_monday_in_local_time() {
        let jst = 9 * 3600;
        // 2026-11-08 15:30 UTC is already Monday 2026-11-09 in JST.
        let ts = day("2026-11-08") * 86_400 + 15 * 3600 + 1800;
        assert_eq!(day_key(local_day(ts, 0)), "2026-11-08");
        let local = local_day(ts, jst);
        assert_eq!(day_key(local), "2026-11-09");
        assert_eq!(week_start(local), local, "a Monday starts its own week");
        assert_eq!(day_key(week_start(local_day(ts, 0))), "2026-11-02", "the Sunday before closes the earlier week");
        assert_eq!(day_key(week_start(day("2026-11-15"))), "2026-11-09");
        assert_eq!(day_key(week_start(day("1969-12-31"))), "1969-12-29", "days before the epoch bucket the same way");
    }

    #[test]
    fn shown_days_follow_the_period() {
        let today = day("2026-11-13");
        let keys = |(from, to): (i64, i64)| (day_key(from), day_key(to));
        assert_eq!(keys(shown_days(Period::Week, today, None)), ("2026-11-09".into(), "2026-11-15".into()));
        assert_eq!(keys(shown_days(Period::Month, today, None)), ("2026-11-01".into(), "2026-11-30".into()));
        assert_eq!(keys(month_span(day("2024-02-10"))), ("2024-02-01".into(), "2024-02-29".into()));
        assert_eq!(
            keys(shown_days(Period::All, today, Some(day("2026-10-01")))),
            ("2026-09-28".into(), "2026-11-15".into()),
            "the heatmap starts at the first week of use"
        );
        let (from, to) = shown_days(Period::All, today, Some(day("2020-01-01")));
        assert_eq!((to - from + 1) / 7, 53, "and covers the past year at most");
        assert_eq!(shown_days(Period::All, today, None), shown_days(Period::Week, today, None));
    }

    #[test]
    fn heat_steps_split_the_active_days_by_quartile() {
        let quartiles = heat_quartiles([0, 100, 200, 0, 300, 400, 500, 600, 700, 800]);
        assert_eq!(quartiles, [200, 400, 600]);
        let steps: Vec<_> = [0, 100, 200, 201, 400, 600, 601, 800].iter().map(|v| heat_step(*v, &quartiles)).collect();
        assert_eq!(steps, [0, 1, 1, 2, 2, 3, 4, 4]);
        assert_eq!(heat_step(320, &heat_quartiles([0, 320])), 1, "a single active day is the lightest step");
        assert_eq!(heat_step(0, &heat_quartiles([])), 0);
    }
}
