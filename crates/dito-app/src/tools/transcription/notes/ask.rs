//! A question asked from the notes panel. It is a real conversation in the chat store, so Chat can go on with it:
//! the first question creates the chat with the meeting attached, every question reads the meeting as it is now,
//! and the reply is written by the same engine as in Chat, on the local chat model only.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use dito_core::chat;
use dito_core::i18n::Tr;
use dito_core::store::{Attachment, Author, ChatMessageRow, MessageState, NewChatMessage};
use dito_core::transcription::{MeetingContext, SpeakerNames};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::TranscriptionEvent;
use crate::services;
use crate::timefmt::{date_time, now};
use crate::tools::chat::Handoff;
use crate::tools::chat::generation::{self, Finished, Generation};
use crate::tools::transcription::TranscriptionPage;
use crate::ui::{InputEvent, TextareaState};

/// The reply is redrawn this often while it streams, as in Chat.
const STREAM_FRAME: Duration = Duration::from_millis(33);

pub(crate) struct Ask {
    pub composer: Entity<TextareaState>,
    /// The meeting this conversation asks about.
    pub meeting: Option<i64>,
    pub chat_id: Option<i64>,
    pub rows: Vec<ChatMessageRow>,
    pub generation: Option<Generation>,
    ticker: Option<Task<()>>,
    /// The panel shows the conversation instead of the notes.
    pub conversation: bool,
    pub scroll: ScrollHandle,
    pub notice: Option<Tr>,
}

impl Ask {
    pub fn new(placeholder: &str, window: &mut Window, cx: &mut Context<TranscriptionPage>) -> Self {
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx).placeholder(placeholder.to_owned()).submit_on_enter(true).auto_grow(1, 4)
        });
        cx.subscribe_in(&composer, window, |page: &mut TranscriptionPage, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::Change => cx.notify(),
            InputEvent::PressEnter { shift: false, .. } => page.ask_send(window, cx),
            _ => {}
        })
        .detach();
        Self {
            composer,
            meeting: None,
            chat_id: None,
            rows: Vec::new(),
            generation: None,
            ticker: None,
            conversation: false,
            scroll: ScrollHandle::new(),
            notice: None,
        }
    }

    pub fn generating(&self) -> bool {
        self.generation.is_some()
    }
}

impl TranscriptionPage {
    /// Sends the question in the field about the meeting on screen.
    pub(in crate::tools::transcription) fn ask_send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.notes.ask.generating() {
            return;
        }
        let text = self.notes.ask.composer.read(cx).value().trim().to_owned();
        let Some(meeting) = self.open.as_ref().map(|open| open.summary.id) else { return };
        if text.is_empty() {
            return;
        }
        // The notes never reach a cloud model, and neither do questions asked from them.
        if !services::chat_model_is_local(&self.st) {
            self.notes.ask.notice = Some(Tr::TranscriptionNotesNeedLocal);
            cx.notify();
            return;
        }
        let lang = self.st.cfg.get().general.lang();
        let off = dito_platform::local_utc_offset_secs();
        let context = match MeetingContext::read(&self.st.store, &self.status, meeting) {
            Ok(Some(context)) => context,
            _ => return self.ask_fail(Tr::ChatMeetingUnreadable, cx),
        };
        let names = SpeakerNames { me: lang.tr(Tr::TranscriptionMe), others: lang.tr(Tr::TranscriptionOthers) };
        let title = if context.title.is_empty() { date_time(context.started_ts, off) } else { context.title.clone() };
        let attachment = Attachment::Meeting {
            transcript_id: meeting,
            title,
            started_ts: context.started_ts,
            duration_ms: context.duration_ms,
            text: context.text(names, true, off),
        };
        let attachments = [attachment];
        let ts = now();
        let chat_id = match self.notes.ask.chat_id {
            Some(id) => id,
            None => match self.st.store.create_chat(&chat::title_from(&text, &attachments).unwrap_or_default(), ts) {
                Ok(id) => id,
                Err(e) => {
                    tracing::warn!("could not create a chat: {e}");
                    return self.ask_fail(Tr::ChatSaveFailed, cx);
                }
            },
        };
        let message = NewChatMessage {
            author: Author::User,
            content: &text,
            state: MessageState::Done,
            attachments: &attachments,
            model: None,
            ts,
        };
        let id = match self.st.store.add_chat_message(chat_id, &message) {
            Ok(id) => id,
            Err(e) => {
                tracing::warn!("could not save a chat message: {e}");
                return self.ask_fail(Tr::ChatSaveFailed, cx);
            }
        };
        let ask = &mut self.notes.ask;
        ask.chat_id = Some(chat_id);
        ask.meeting = Some(meeting);
        ask.notice = None;
        ask.conversation = true;
        ask.rows.push(ChatMessageRow {
            id,
            author: Author::User,
            content: text,
            state: MessageState::Done,
            error: None,
            attachments: attachments.to_vec(),
            model: None,
            ts,
        });
        ask.composer.update(cx, |state, cx| state.set_value("", window, cx));
        match generation::begin(&self.st, chat_id, &self.notes.ask.rows) {
            Ok((row, generation)) => {
                let ask = &mut self.notes.ask;
                ask.rows.push(row);
                ask.generation = Some(generation);
                ask.scroll.scroll_to_bottom();
                self.watch_ask(cx);
            }
            Err(e) => {
                tracing::warn!("could not save a chat reply: {e}");
                self.ask_fail(Tr::ChatSaveFailed, cx);
            }
        }
        cx.notify();
    }

    fn ask_fail(&mut self, notice: Tr, cx: &mut Context<Self>) {
        self.notes.ask.notice = Some(notice);
        cx.notify();
    }

    /// Redraws the panel while the reply streams; the timer is the page's, so it ends with the reply or the page.
    fn watch_ask(&mut self, cx: &mut Context<Self>) {
        self.notes.ask.ticker = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(STREAM_FRAME).await;
                let more = this.update(cx, |page, cx| page.poll_ask(cx));
                if !matches!(more, Ok(true)) {
                    break;
                }
            }
        }));
    }

    /// `true` while the reply is still being written.
    fn poll_ask(&mut self, cx: &mut Context<Self>) -> bool {
        let ask = &mut self.notes.ask;
        let Some(generation) = &mut ask.generation else { return false };
        let id = generation.message_id;
        if let Some(text) = generation.grown()
            && let Some(row) = ask.rows.iter_mut().find(|r| r.id == id)
        {
            row.content = text;
        }
        if let Some(Finished { content, state, error }) = generation.finished() {
            if let Some(row) = ask.rows.iter_mut().find(|r| r.id == id) {
                row.content = content;
                row.state = state;
                row.error = error;
            }
            ask.generation = None;
            cx.notify();
            return false;
        }
        cx.notify();
        true
    }

    /// The conversation goes back behind the notes; nothing is lost, and the next question continues it.
    pub(in crate::tools::transcription) fn ask_back(&mut self, cx: &mut Context<Self>) {
        self.notes.ask.conversation = false;
        cx.notify();
    }

    /// Hands the conversation to Chat, with the reply that is still being written.
    pub(in crate::tools::transcription) fn ask_continue(&mut self, cx: &mut Context<Self>) {
        let ask = &mut self.notes.ask;
        let (Some(chat_id), Some(meeting_id)) = (ask.chat_id, ask.meeting) else { return };
        let handoff = Handoff { chat_id, meeting_id, generation: Rc::new(RefCell::new(ask.generation.take())) };
        self.ask_reset();
        cx.emit(TranscriptionEvent::OpenInChat(handoff));
        cx.notify();
    }

    /// Forgets the conversation shown in the panel. The chat itself stays in the store.
    pub(in crate::tools::transcription) fn ask_reset(&mut self) {
        let ask = &mut self.notes.ask;
        ask.chat_id = None;
        ask.meeting = None;
        ask.rows.clear();
        ask.generation = None;
        ask.ticker = None;
        ask.conversation = false;
        ask.notice = None;
    }
}
