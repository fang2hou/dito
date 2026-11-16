//! Chat conversations and their messages.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::Store;
use super::model::Result;
use super::schema::{decode, encode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Author {
    #[default]
    User,
    Assistant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MessageState {
    #[default]
    Done,
    /// A reply still being written; one found after a restart was cut short and reads as stopped.
    Generating,
    /// The user stopped the reply; `content` keeps what was written.
    Stopped,
    Failed,
}

/// What a message carries along with its words. The text is copied in when the message is sent, so a later edit or
/// deletion of the source never changes what the model was shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Attachment {
    File { name: String, bytes: u64, text: String },
    Meeting { transcript_id: i64, title: String, started_ts: i64, duration_ms: Option<u64>, text: String },
}

impl Attachment {
    pub fn name(&self) -> &str {
        match self {
            Self::File { name, .. } => name,
            Self::Meeting { title, .. } => title,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatSummary {
    pub id: i64,
    pub title: String,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatMessageRow {
    pub id: i64,
    pub author: Author,
    pub content: String,
    pub state: MessageState,
    pub error: Option<String>,
    pub attachments: Vec<Attachment>,
    /// The model that wrote a reply, as the page labelled it then.
    pub model: Option<String>,
    pub ts: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chat {
    pub summary: ChatSummary,
    pub messages: Vec<ChatMessageRow>,
}

pub struct NewChatMessage<'a> {
    pub author: Author,
    pub content: &'a str,
    pub state: MessageState,
    pub attachments: &'a [Attachment],
    pub model: Option<&'a str>,
    pub ts: i64,
}

fn row_to_summary(r: &rusqlite::Row<'_>) -> rusqlite::Result<ChatSummary> {
    Ok(ChatSummary { id: r.get(0)?, title: r.get(1)?, updated_ts: r.get(2)? })
}

/// Attachments a build cannot read (written by a newer build) leave the message listed without them.
fn read_attachments(id: i64, stored: &str) -> Vec<Attachment> {
    serde_json::from_str(stored).unwrap_or_else(|e| {
        tracing::warn!(id, "unreadable chat attachments: {e}");
        Vec::new()
    })
}

impl Store {
    pub fn create_chat(&self, title: &str, ts: i64) -> Result<i64> {
        let c = self.conn();
        c.execute("INSERT INTO chats (title, created_ts, updated_ts) VALUES (?1, ?2, ?2)", params![title, ts])?;
        Ok(c.last_insert_rowid())
    }

    /// Newest activity first.
    pub fn list_chats(&self) -> Result<Vec<ChatSummary>> {
        let c = self.conn();
        let mut stmt = c.prepare("SELECT id, title, updated_ts FROM chats ORDER BY updated_ts DESC, id DESC")?;
        let rows = stmt.query_map([], row_to_summary)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn get_chat(&self, id: i64) -> Result<Option<Chat>> {
        let c = self.conn();
        let Some(summary) =
            c.query_row("SELECT id, title, updated_ts FROM chats WHERE id = ?1", [id], row_to_summary).optional()?
        else {
            return Ok(None);
        };
        let mut stmt = c.prepare(
            "SELECT id, author, content, state, error, attachments, model, ts FROM chat_messages
             WHERE chat_id = ?1 ORDER BY id",
        )?;
        let messages = stmt
            .query_map([id], |r| {
                let message_id: i64 = r.get(0)?;
                Ok(ChatMessageRow {
                    id: message_id,
                    author: decode("chat_messages.author", &r.get::<_, String>(1)?),
                    content: r.get(2)?,
                    state: decode("chat_messages.state", &r.get::<_, String>(3)?),
                    error: r.get(4)?,
                    attachments: read_attachments(message_id, &r.get::<_, String>(5)?),
                    model: r.get(6)?,
                    ts: r.get(7)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(Some(Chat { summary, messages }))
    }

    pub fn rename_chat(&self, id: i64, title: &str) -> Result<()> {
        self.conn().execute("UPDATE chats SET title = ?2 WHERE id = ?1", params![id, title])?;
        Ok(())
    }

    pub fn delete_chat(&self, id: i64) -> Result<()> {
        let mut c = self.conn();
        let tx = c.transaction()?;
        tx.execute("DELETE FROM chat_messages WHERE chat_id = ?1", [id])?;
        tx.execute("DELETE FROM chats WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn add_chat_message(&self, chat_id: i64, m: &NewChatMessage<'_>) -> Result<i64> {
        let attachments =
            serde_json::to_string(m.attachments).map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))?;
        let mut c = self.conn();
        let tx = c.transaction()?;
        tx.execute(
            "INSERT INTO chat_messages (chat_id, author, content, state, attachments, model, ts)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![chat_id, encode(m.author), m.content, encode(m.state), attachments, m.model, m.ts],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute("UPDATE chats SET updated_ts = MAX(updated_ts, ?2) WHERE id = ?1", params![chat_id, m.ts])?;
        tx.commit()?;
        Ok(id)
    }

    /// Writes what a reply ended as: its text, the state and the error of a failure.
    pub fn finish_chat_message(&self, id: i64, content: &str, state: MessageState, error: Option<&str>) -> Result<()> {
        self.conn().execute(
            "UPDATE chat_messages SET content = ?2, state = ?3, error = ?4 WHERE id = ?1",
            params![id, content, encode(state), error],
        )?;
        Ok(())
    }

    pub fn delete_chat_message(&self, id: i64) -> Result<()> {
        self.conn().execute("DELETE FROM chat_messages WHERE id = ?1", [id])?;
        Ok(())
    }

    /// A reply cut short by a quit or crash keeps the words it had and reads as stopped.
    pub(super) fn recover_chats(&self) -> Result<()> {
        self.conn().execute(
            "UPDATE chat_messages SET state = ?2 WHERE state = ?1",
            params![encode(MessageState::Generating), encode(MessageState::Stopped)],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, text: &str) -> Attachment {
        Attachment::File { name: name.into(), bytes: text.len() as u64, text: text.into() }
    }

    fn user<'a>(content: &'a str, attachments: &'a [Attachment], ts: i64) -> NewChatMessage<'a> {
        NewChatMessage { author: Author::User, content, state: MessageState::Done, attachments, model: None, ts }
    }

    #[test]
    fn a_chat_keeps_its_messages_with_their_attachments_in_order() {
        let s = Store::open_in_memory().unwrap();
        let chat = s.create_chat("議事メモの要約", 100).unwrap();
        let notes = [
            file("議事メモ.md", "# メモ"),
            Attachment::Meeting {
                transcript_id: 4,
                title: "週次定例".into(),
                started_ts: 50,
                duration_ms: Some(3_120_000),
                text: "決定事項".into(),
            },
        ];
        s.add_chat_message(chat, &user("まとめて", &notes, 101)).unwrap();
        let reply = s
            .add_chat_message(
                chat,
                &NewChatMessage {
                    author: Author::Assistant,
                    content: "",
                    state: MessageState::Generating,
                    attachments: &[],
                    model: Some("Qwen3.5-4B"),
                    ts: 102,
                },
            )
            .unwrap();
        s.finish_chat_message(reply, "三つあります。", MessageState::Done, None).unwrap();

        let got = s.get_chat(chat).unwrap().unwrap();
        assert_eq!(got.summary, ChatSummary { id: chat, title: "議事メモの要約".into(), updated_ts: 102 });
        assert_eq!(got.messages.len(), 2);
        assert_eq!(got.messages[0].attachments, notes);
        assert_eq!(got.messages[1].content, "三つあります。");
        assert_eq!(got.messages[1].model.as_deref(), Some("Qwen3.5-4B"));
        assert_eq!(got.messages[1].state, MessageState::Done);
    }

    #[test]
    fn chats_list_by_latest_activity_and_deleting_one_removes_its_messages() {
        let s = Store::open_in_memory().unwrap();
        let old = s.create_chat("old", 10).unwrap();
        let new = s.create_chat("new", 20).unwrap();
        assert_eq!(s.list_chats().unwrap().iter().map(|c| c.id).collect::<Vec<_>>(), [new, old]);
        s.add_chat_message(old, &user("hello", &[], 30)).unwrap();
        assert_eq!(s.list_chats().unwrap().iter().map(|c| c.id).collect::<Vec<_>>(), [old, new]);

        s.rename_chat(old, "renamed").unwrap();
        assert_eq!(s.get_chat(old).unwrap().unwrap().summary.title, "renamed");
        s.delete_chat(old).unwrap();
        assert!(s.get_chat(old).unwrap().is_none());
        assert_eq!(s.list_chats().unwrap().len(), 1);
    }

    #[test]
    fn a_reply_found_generating_after_a_restart_reads_as_stopped_with_its_words() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("h.db");
        let (chat, reply) = {
            let s = Store::open(&path, 0).unwrap();
            let chat = s.create_chat("c", 1).unwrap();
            let reply = s
                .add_chat_message(
                    chat,
                    &NewChatMessage {
                        author: Author::Assistant,
                        content: "partial",
                        state: MessageState::Generating,
                        attachments: &[],
                        model: None,
                        ts: 2,
                    },
                )
                .unwrap();
            (chat, reply)
        };
        let s = Store::open(&path, 0).unwrap();
        let message = &s.get_chat(chat).unwrap().unwrap().messages[0];
        assert_eq!((message.id, message.state, message.content.as_str()), (reply, MessageState::Stopped, "partial"));
    }

    #[test]
    fn attachments_a_build_cannot_read_leave_the_message_listed() {
        let s = Store::open_in_memory().unwrap();
        let chat = s.create_chat("c", 1).unwrap();
        let id = s.add_chat_message(chat, &user("hi", &[], 2)).unwrap();
        s.conn()
            .execute("UPDATE chat_messages SET attachments = '[{\"kind\":\"from_the_future\"}]' WHERE id = ?1", [id])
            .unwrap();
        let message = &s.get_chat(chat).unwrap().unwrap().messages[0];
        assert_eq!((message.content.as_str(), message.attachments.len()), ("hi", 0));
    }
}
