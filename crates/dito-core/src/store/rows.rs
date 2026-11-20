//! Mapping a `history` row to a `HistoryEntry`.

use crate::config::EngineLabels;

use super::schema::decode;
use super::{CommandDetails, HistoryEntry, StepMillis};

pub(super) const SELECT_HISTORY: &str =
    "SELECT id, ts, kind, style, app, lang, duration_ms, raw, polished, error, audio_path,
    asr_engine, polish_engine, asr_ms, polish_ms, source, command FROM history";

pub(super) fn row_to_entry(r: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEntry> {
    let millis = |index: usize| r.get::<_, Option<i64>>(index).map(|ms| ms.map(|ms| ms as u64));
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
        timings: StepMillis { asr: millis(13)?, polish: millis(14)? },
        source: r.get(15)?,
        command: r.get::<_, Option<String>>(16)?.and_then(|json| details(&json)),
    })
}

/// Reads stay tolerant, as with `decode`: details a newer build wrote in a shape this one does not know read as
/// missing, and the row is still listed with its instruction and result.
fn details(json: &str) -> Option<CommandDetails> {
    serde_json::from_str(json)
        .map_err(
            |error| tracing::warn!(column = "command", %error, "unreadable command details, reading them as missing"),
        )
        .ok()
}
