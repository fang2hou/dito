//! What the screenshot harness steers on the chat page.

use dito_core::store::{Attachment, Author, ChatMessageRow, MessageState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::ChatPage;
use super::generation::Generation;

impl ChatPage {
    /// Shows a chat at once, without the switching motion.
    pub(crate) fn pose_open(&mut self, id: Option<i64>, cx: &mut Context<Self>) {
        self.current = None;
        self.rows.clear();
        self.switch_to(id, cx);
        self.switching = None;
    }

    /// Starts the switching motion, for frames taken while it runs.
    pub(crate) fn pose_switch(&mut self, id: Option<i64>, cx: &mut Context<Self>) {
        self.switch_to(id, cx);
    }

    pub(crate) fn pose_type(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = text.to_owned();
        self.composer.update(cx, |state, cx| state.set_value(text, window, cx));
    }

    pub(crate) fn pose_send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.send(window, cx);
    }

    pub(crate) fn pose_drawer(&mut self, open: bool, cx: &mut Context<Self>) {
        self.drawer = open;
        cx.notify();
    }

    pub(crate) fn pose_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_picker(window, cx);
    }

    pub(crate) fn pose_attach(&mut self, attachment: Attachment, cx: &mut Context<Self>) {
        self.pending.push(attachment);
        cx.notify();
    }

    pub(crate) fn pose_menu(&mut self, id: i64, cx: &mut Context<Self>) {
        self.menu = Some(super::sidebar::RowMenu { id, armed: false });
        cx.notify();
    }

    pub(crate) fn pose_rename(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        self.start_rename(id, window, cx);
    }

    pub(crate) fn pose_retry(&mut self, cx: &mut Context<Self>) {
        self.retry(cx);
    }

    pub(crate) fn pose_scroll_up(&mut self, cx: &mut Context<Self>) {
        let offset = self.scroll.offset();
        self.scroll.set_offset(point(offset.x, offset.y + px(160.)));
        cx.notify();
    }

    /// Clears what a pose left open or typed.
    pub(crate) fn pose_reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pending.clear();
        self.picker = None;
        self.menu = None;
        self.notice = None;
        self.composer.update(cx, |state, cx| state.set_value("", window, cx));
        cx.notify();
    }

    /// A question and a reply that has `written` so far: still generating (the band `ms` into its sweep), stopped
    /// or failed.
    pub(crate) fn pose_reply(
        &mut self,
        question: &str,
        written: &str,
        state: MessageState,
        ms: u64,
        cx: &mut Context<Self>,
    ) {
        let chat = self.current.unwrap_or(0);
        self.rows.push(ChatMessageRow {
            id: -1,
            author: Author::User,
            content: question.to_owned(),
            state: MessageState::Done,
            error: None,
            attachments: Vec::new(),
            model: None,
            ts: 0,
        });
        self.rows.push(ChatMessageRow {
            id: -2,
            author: Author::Assistant,
            content: written.to_owned(),
            state,
            error: (state == MessageState::Failed).then(|| "Network: connection refused".to_owned()),
            attachments: Vec::new(),
            model: Some("Qwen3.5-4B".to_owned()),
            ts: 0,
        });
        self.docs.insert(-2, std::rc::Rc::new(super::markdown::Document::of(written)));
        self.generation = (state == MessageState::Generating).then(|| Generation::posed(chat, -2, written, ms));
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    pub(crate) fn pose_sessions(&self) -> Vec<i64> {
        self.sessions.iter().map(|s| s.id).collect()
    }
}
