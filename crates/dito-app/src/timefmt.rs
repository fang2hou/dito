//! Local-time helpers working on unix seconds and a fixed UTC offset.

pub use dito_core::unix_seconds as now;

pub fn clock(ms: u64) -> String {
    let secs = ms / 1000;
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
    } else {
        format!("{:02}:{:02}", secs / 60, secs % 60)
    }
}

/// `2026/11/13 14:02` in the user's time zone.
pub fn date_time(ts: i64, off: i32) -> String {
    format!("{} {}", ymd(local_day(ts, off)).replace('-', "/"), hhmm(ts, off))
}

/// Days since 1970-01-01 in local time.
pub fn local_day(ts: i64, off: i32) -> i64 {
    (ts + off as i64).div_euclid(86_400)
}

pub fn hhmm(ts: i64, off: i32) -> String {
    let s = (ts + off as i64).rem_euclid(86_400);
    format!("{:02}:{:02}", s / 3600, (s % 3600) / 60)
}

/// `HH:MM:SS.mmm` of unix milliseconds.
pub fn clock_millis(ms: i64, off: i32) -> String {
    let ms = (ms + off as i64 * 1000).rem_euclid(86_400_000);
    format!("{:02}:{:02}:{:02}.{:03}", ms / 3_600_000, ms % 3_600_000 / 60_000, ms % 60_000 / 1000, ms % 1000)
}

pub fn ymd(day: i64) -> String {
    let d = time::OffsetDateTime::UNIX_EPOCH + time::Duration::days(day);
    format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day())
}

pub fn day_start(day: i64, off: i32) -> i64 {
    day * 86_400 - off as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_formats_durations() {
        assert_eq!(clock(2_536_000), "42:16");
        assert_eq!(clock(3_725_000), "1:02:05");
    }

    #[test]
    fn date_time_uses_local_date_and_time() {
        let jst = 9 * 3600;
        let ts = 20 * 3600 + 14 * 60;
        assert_eq!(date_time(ts, jst), "1970/01/02 05:14");
    }

    #[test]
    fn local_day_and_clock_follow_the_offset() {
        let jst = 9 * 3600;
        let ts = 20 * 3600; // 1970-01-01 20:00 UTC = 01-02 05:00 JST
        assert_eq!(local_day(ts, jst), 1);
        assert_eq!(hhmm(ts, jst), "05:00");
        assert_eq!(clock_millis(ts * 1000 + 1_234, jst), "05:00:01.234");
        assert_eq!(ymd(1), "1970-01-02");
        assert_eq!(day_start(1, jst), 86_400 - jst as i64);
    }
}
