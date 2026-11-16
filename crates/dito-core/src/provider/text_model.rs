use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;

use crate::error::ProviderResult;

/// One demonstration: a user turn and the assistant reply a model should have given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Example {
    pub said: &'static str,
    pub written: &'static str,
}

pub struct TextRequest<'a> {
    pub system: &'a str,
    /// Sent as earlier turns of the conversation, in order, before `text`.
    pub examples: &'static [Example],
    /// The last user turn.
    pub text: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

impl ChatMessage {
    pub fn new(role: ChatRole, content: impl Into<String>) -> Self {
        Self { role, content: content.into() }
    }
}

/// How hard a model thinks before it answers. A level is the `key` of one of the model's
/// [`ThinkingLevel`](super::ThinkingLevel)s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Thinking {
    Off,
    On,
    Level(String),
}

/// A multi-turn conversation. Unset sampling fields take the engine's configured parameters.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    /// Unset keeps what the engine is configured to do.
    pub thinking: Option<Thinking>,
}

/// Stops a streaming reply. Engines check it between tokens and return what they have so far.
#[derive(Debug, Clone, Default)]
pub struct ChatCancel(Arc<AtomicBool>);

impl ChatCancel {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Receives each piece of a reply as it is generated, in order.
pub type ChatDelta<'a> = &'a mut (dyn FnMut(&str) + Send);

#[async_trait]
pub trait TextModel: Send + Sync {
    async fn polish(&self, req: &TextRequest<'_>) -> ProviderResult<String>;

    /// Replies to a conversation, passing each piece to `on_delta` and returning the whole reply. A cancelled reply
    /// returns the text generated so far. An engine without streaming answers in one piece through `polish`.
    async fn chat(&self, req: &ChatRequest, on_delta: ChatDelta<'_>, cancel: &ChatCancel) -> ProviderResult<String> {
        let system: Vec<&str> =
            req.messages.iter().filter(|m| m.role == ChatRole::System).map(|m| m.content.as_str()).collect();
        let mut turns = String::new();
        let dialogue: Vec<&ChatMessage> = req.messages.iter().filter(|m| m.role != ChatRole::System).collect();
        let (last, earlier) = dialogue.split_last().map(|(l, e)| (l.content.as_str(), e)).unwrap_or(("", &[]));
        for m in earlier {
            let who = if m.role == ChatRole::User { "User" } else { "Assistant" };
            turns.push_str(&format!("{who}: {}\n\n", m.content));
        }
        turns.push_str(last);
        if cancel.is_cancelled() {
            return Ok(String::new());
        }
        let reply = self.polish(&TextRequest { system: &system.join("\n\n"), examples: &[], text: &turns }).await?;
        on_delta(&reply);
        Ok(reply)
    }
}
