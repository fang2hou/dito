//! Transcription sessions and their paragraphs.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::Store;
use super::model::Result;
use super::schema::{decode, encode};

/// Who spoke a paragraph. Without the microphone everything is `Others`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Speaker {
    #[default]
    Others,
    Me,
}

/// What a session records: the Mac's audio (`Others`), the microphone (`Me`), or both. At least one is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sources {
    pub system: bool,
    pub microphone: bool,
}

impl Default for Sources {
    fn default() -> Self {
        Self { system: true, microphone: false }
    }
}

impl Sources {
    /// Speakers are told apart only when both are recorded; with one source everything is that source's speaker.
    pub fn both(self) -> bool {
        self.system && self.microphone
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ParagraphState {
    /// Still collecting utterances; `text` is the recognized words so far.
    Listening,
    Polishing,
    /// Final. Only a done paragraph is edited by the user.
    #[default]
    Done,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptSummary {
    pub id: i64,
    pub started_ts: i64,
    /// `None` while recording.
    pub ended_ts: Option<i64>,
    pub title: String,
    pub sources: Sources,
    /// The first paragraph's text, for the list.
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptParagraph {
    pub id: i64,
    pub speaker: Speaker,
    /// Milliseconds since the session started.
    pub start_ms: u64,
    pub end_ms: u64,
    pub raw: String,
    pub text: String,
    pub state: ParagraphState,
    /// Why polishing did not run or failed; `text` is then the recognized words.
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Transcript {
    pub summary: TranscriptSummary,
    pub paragraphs: Vec<TranscriptParagraph>,
}

const SELECT_SUMMARY: &str = "SELECT t.id, t.started_ts, t.ended_ts, t.title, t.with_mic, t.with_system,
       COALESCE((SELECT p.text FROM transcript_paragraphs p WHERE p.transcript_id = t.id
                 ORDER BY p.start_ms, p.id LIMIT 1), '')
     FROM transcripts t";

fn row_to_summary(r: &rusqlite::Row<'_>) -> rusqlite::Result<TranscriptSummary> {
    Ok(TranscriptSummary {
        id: r.get(0)?,
        started_ts: r.get(1)?,
        ended_ts: r.get(2)?,
        title: r.get(3)?,
        sources: Sources { microphone: r.get::<_, i64>(4)? != 0, system: r.get::<_, i64>(5)? != 0 },
        preview: r.get(6)?,
    })
}

impl Store {
    pub fn create_transcript(&self, started_ts: i64, sources: Sources) -> Result<i64> {
        let c = self.conn();
        c.execute(
            "INSERT INTO transcripts (started_ts, with_mic, with_system) VALUES (?1, ?2, ?3)",
            params![started_ts, sources.microphone, sources.system],
        )?;
        Ok(c.last_insert_rowid())
    }

    pub fn end_transcript(&self, id: i64, ended_ts: i64) -> Result<()> {
        self.conn().execute("UPDATE transcripts SET ended_ts = ?2 WHERE id = ?1", params![id, ended_ts])?;
        Ok(())
    }

    /// A session cut short by a crash or quit is closed, and its unfinished paragraphs keep their recognized words.
    pub fn recover_transcripts(&self, now_ts: i64) -> Result<()> {
        let c = self.conn();
        c.execute("UPDATE transcripts SET ended_ts = ?1 WHERE ended_ts IS NULL", [now_ts])?;
        c.execute("UPDATE transcript_paragraphs SET state = ?1 WHERE state != ?1", [encode(ParagraphState::Done)])?;
        Ok(())
    }

    pub fn list_transcripts(&self) -> Result<Vec<TranscriptSummary>> {
        let c = self.conn();
        let mut stmt = c.prepare(&format!("{SELECT_SUMMARY} ORDER BY t.started_ts DESC, t.id DESC"))?;
        let rows = stmt.query_map([], row_to_summary)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn get_transcript(&self, id: i64) -> Result<Option<Transcript>> {
        let c = self.conn();
        let Some(summary) =
            c.query_row(&format!("{SELECT_SUMMARY} WHERE t.id = ?1"), [id], row_to_summary).optional()?
        else {
            return Ok(None);
        };
        let mut stmt = c.prepare(
            "SELECT id, speaker, start_ms, end_ms, raw, text, state, error FROM transcript_paragraphs
             WHERE transcript_id = ?1 ORDER BY start_ms, id",
        )?;
        let paragraphs = stmt
            .query_map([id], |r| {
                Ok(TranscriptParagraph {
                    id: r.get(0)?,
                    speaker: decode("speaker", &r.get::<_, String>(1)?),
                    start_ms: r.get::<_, i64>(2)? as u64,
                    end_ms: r.get::<_, i64>(3)? as u64,
                    raw: r.get(4)?,
                    text: r.get(5)?,
                    state: decode("state", &r.get::<_, String>(6)?),
                    error: r.get(7)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(Some(Transcript { summary, paragraphs }))
    }

    pub fn set_transcript_title(&self, id: i64, title: &str) -> Result<()> {
        self.conn().execute("UPDATE transcripts SET title = ?2 WHERE id = ?1", params![id, title])?;
        Ok(())
    }

    /// The sources a session recorded, which may have been turned on and off while it ran.
    pub fn set_transcript_sources(&self, id: i64, sources: Sources) -> Result<()> {
        self.conn().execute(
            "UPDATE transcripts SET with_mic = ?2, with_system = ?3 WHERE id = ?1",
            params![id, sources.microphone, sources.system],
        )?;
        Ok(())
    }

    pub fn delete_transcript(&self, id: i64) -> Result<()> {
        let mut c = self.conn();
        let tx = c.transaction()?;
        tx.execute("DELETE FROM transcript_paragraphs WHERE transcript_id = ?1", [id])?;
        tx.execute("DELETE FROM transcripts WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn add_paragraph(
        &self,
        transcript_id: i64,
        speaker: Speaker,
        start_ms: u64,
        end_ms: u64,
        raw: &str,
    ) -> Result<i64> {
        let c = self.conn();
        c.execute(
            "INSERT INTO transcript_paragraphs (transcript_id, speaker, start_ms, end_ms, raw, text, state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6)",
            params![
                transcript_id,
                encode(speaker),
                start_ms as i64,
                end_ms as i64,
                raw,
                encode(ParagraphState::Listening)
            ],
        )?;
        Ok(c.last_insert_rowid())
    }

    /// Recognized words appended while the paragraph is still listening. The start moves when the paragraph's
    /// earlier sentences became a paragraph of their own.
    pub fn extend_paragraph(&self, id: i64, start_ms: u64, end_ms: u64, raw: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE transcript_paragraphs SET start_ms = ?2, end_ms = ?3, raw = ?4, text = ?4 WHERE id = ?1",
            params![id, start_ms as i64, end_ms as i64, raw],
        )?;
        Ok(())
    }

    pub fn set_paragraph_polishing(&self, id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE transcript_paragraphs SET state = ?2 WHERE id = ?1",
            params![id, encode(ParagraphState::Polishing)],
        )?;
        Ok(())
    }

    pub fn finish_paragraph(&self, id: i64, text: &str, error: Option<&str>) -> Result<()> {
        self.conn().execute(
            "UPDATE transcript_paragraphs SET text = ?2, error = ?3, state = ?4 WHERE id = ?1",
            params![id, text, error, encode(ParagraphState::Done)],
        )?;
        Ok(())
    }

    /// The user's edit of a finished paragraph.
    pub fn edit_paragraph(&self, id: i64, text: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE transcript_paragraphs SET text = ?2 WHERE id = ?1 AND state = ?3",
            params![id, text, encode(ParagraphState::Done)],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_keeps_its_paragraphs_in_order_and_recovers_after_a_crash() {
        let s = Store::open_in_memory().unwrap();
        let both = Sources { system: true, microphone: true };
        let id = s.create_transcript(1_000, both).unwrap();
        let late = s.add_paragraph(id, Speaker::Others, 5_000, 6_000, "あとの段落").unwrap();
        let early = s.add_paragraph(id, Speaker::Me, 1_000, 2_000, "えー最初の段落").unwrap();
        s.extend_paragraph(early, 1_000, 3_000, "えー最初の段落です").unwrap();
        s.set_paragraph_polishing(early).unwrap();
        s.finish_paragraph(early, "最初の段落です。", None).unwrap();

        let t = s.get_transcript(id).unwrap().unwrap();
        assert_eq!(t.paragraphs.iter().map(|p| p.id).collect::<Vec<_>>(), vec![early, late]);
        assert_eq!(t.paragraphs[0].raw, "えー最初の段落です", "the recognized words stay next to the polished text");
        assert_eq!(t.summary.preview, "最初の段落です。");
        assert_eq!(t.summary.sources, both);
        assert_eq!(t.paragraphs[1].state, ParagraphState::Listening);

        s.edit_paragraph(late, "editing a listening paragraph is ignored").unwrap();
        s.recover_transcripts(9_000).unwrap();
        let t = s.get_transcript(id).unwrap().unwrap();
        assert_eq!(t.summary.ended_ts, Some(9_000));
        assert_eq!(t.paragraphs[1].text, "あとの段落");
        assert_eq!(t.paragraphs[1].state, ParagraphState::Done);

        s.delete_transcript(id).unwrap();
        assert!(s.get_transcript(id).unwrap().is_none());
        assert!(s.list_transcripts().unwrap().is_empty());
    }
}
