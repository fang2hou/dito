use dito_core::store::HistoryEntry;

use crate::timefmt::{hhmm, local_day, ymd};

pub(super) fn history_csv(rows: &[HistoryEntry], utc_offset_secs: i32) -> String {
    let quoted = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut out = String::from("date,time,app,style,text,original\n");
    for e in rows {
        out.push_str(&format!(
            "{},{},{},{},{},{}\n",
            ymd(local_day(e.ts, utc_offset_secs)),
            hhmm(e.ts, utc_offset_secs),
            quoted(&e.app),
            e.style.label(),
            quoted(e.final_text()),
            quoted(&e.raw)
        ));
    }
    out
}
