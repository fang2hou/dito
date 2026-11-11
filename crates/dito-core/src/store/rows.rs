//! Mapping a `history` row to a `HistoryEntry`.

use super::schema::decode;
use super::{EngineLabels, HistoryEntry};

pub(super) const SELECT_HISTORY: &str =
    "SELECT id, ts, kind, style, app, lang, duration_ms, raw, polished, error, audio_path,
    asr_engine, polish_engine FROM history";

pub(super) fn row_to_entry(r: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEntry> {
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
        engines: EngineLabels { asr: r.get(11)?, polish: r.get(12)? },
    })
}
