//! The chat tool: conversations with the text model in use (`design:tools/chat`). The page owns its conversation, the
//! reply being written and every transient control; the store keeps conversations (ADR-0013, ADR-0015).

mod composer;
mod files;
mod generation;
mod highlight;
mod markdown;
mod markdown_view;
mod messages;
mod picker;
#[cfg(feature = "screenshots")]
mod pose;
mod sidebar;
mod switch;

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::chat::{self, FileError, Limits};
use dito_core::config::Source;
use dito_core::i18n::{Lang, Tr, fmt};
use dito_core::provider::ThinkingCapabilities;
use dito_core::store::{
    Attachment, Author, ChatMessageRow, ChatSummary, MessageState, NewChatMessage, TranscriptSummary,
};
use gpui_kit::prelude::*;
use gpui_kit::*;

use self::generation::{Finished, Generation};
use self::markdown::Document;
use self::switch::{Phase, Switch};
use crate::app_shell::{RAIL_COLLAPSE, RAIL_EXPAND};
use crate::assets::Ic;
use crate::settings::{self, Ctx};
use crate::state::AppState;
use crate::timefmt::{hhmm, local_day, now, ymd};
use crate::ui::{self, InputEvent, TextareaState};

const PAGE_PAD_X: f32 = 28.;
const TOOLBAR_TOP: f32 = 36.;
const TOOLBAR_H: f32 = 36.;
const VIEWPORT_TOP_GAP: f32 = 20.;
const COMPOSER_BOTTOM: f32 = 28.;
const SIDEBAR_W: f32 = 224.;
/// The reply is redrawn this often while it streams: text arrives faster than the eye follows, and every redraw runs
/// the page.
const STREAM_FRAME: Duration = Duration::from_millis(33);
const BOTTOM_SLACK: f32 = 4.;
const DRAWER_MS: u64 = 180;

pub(crate) struct ChatPage {
    st: Arc<AppState>,
    motion: ui::MotionScope,
    active: bool,
    sessions: Vec<ChatSummary>,
    /// `None` is a new chat, which is saved once its first message is sent.
    current: Option<i64>,
    /// The unsaved new chat exists, shown or not.
    draft_open: bool,
    rows: Vec<ChatMessageRow>,
    docs: HashMap<i64, Rc<Document>>,
    composer: Entity<TextareaState>,
    composer_lang: Lang,
    pending: Vec<Attachment>,
    notice: Option<String>,
    generation: Option<Generation>,
    /// Redraws the page while a reply streams; it ends with the reply or with the view.
    ticker: Option<Task<()>>,
    scroll: ScrollHandle,
    compact: bool,
    drawer: bool,
    menu: Option<sidebar::RowMenu>,
    /// The menu a press outside just closed, so the press on its own button does not open it again.
    menu_closed: Option<(i64, Instant)>,
    rename: Option<sidebar::Rename>,
    picker: Option<picker::Picker>,
    text_menu: Option<TextMenu>,
    thinking_menu: bool,
    /// When a press outside closed the thinking menu, so the press on its own pill does not open it again.
    thinking_closed: Option<Instant>,
    switching: Option<Switching>,
    /// Esc closes the picker, the menu, the rename field and the drawer, from anywhere in the window.
    _escape: Subscription,
}

/// The menu a secondary click opens over selected conversation text. It keeps the text, because the press on its row
/// may end the selection before the click arrives.
struct TextMenu {
    at: Point<Pixels>,
    text: String,
}

/// What the chat needs to know about the text model in use.
struct ModelFacts {
    /// Names the model in `chat.thinking` of the configuration.
    key: String,
    limits: Limits,
    thinking: Option<ThinkingCapabilities>,
    /// The reasoning effort a cloud configuration names, which is the choice until another is saved.
    configured: Option<String>,
}

struct Switching {
    anim: Switch,
    /// What the outgoing phase still shows.
    old_rows: Vec<ChatMessageRow>,
    old_docs: HashMap<i64, Rc<Document>>,
}

impl ChatPage {
    pub(crate) fn new(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let lang = st.cfg.get().general.lang();
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(lang.tr(Tr::ChatPlaceholder))
                .submit_on_enter(true)
                .auto_grow(1, 6)
        });
        cx.subscribe_in(&composer, window, |page: &mut Self, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::Change => cx.notify(),
            InputEvent::PressEnter { shift: false, .. } => page.send(window, cx),
            _ => {}
        })
        .detach();
        let owner = window.window_handle();
        let weak = cx.entity().downgrade();
        let escape = cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle() == owner && event.keystroke.key == "escape" {
                let _ = weak.update(cx, |page, cx| page.dismiss_overlays(cx));
            }
        });
        let mut page = Self {
            st,
            motion: ui::MotionScope::new(),
            active: false,
            sessions: Vec::new(),
            current: None,
            draft_open: true,
            rows: Vec::new(),
            docs: HashMap::new(),
            composer,
            composer_lang: lang,
            pending: Vec::new(),
            notice: None,
            generation: None,
            ticker: None,
            scroll: ScrollHandle::new(),
            compact: false,
            drawer: false,
            menu: None,
            menu_closed: None,
            rename: None,
            picker: None,
            text_menu: None,
            thinking_menu: false,
            thinking_closed: None,
            switching: None,
            _escape: escape,
        };
        page.reload_sessions();
        page
    }

    /// The page became the visible one, or stopped being it. A reply keeps streaming while the page is hidden.
    pub(crate) fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        let was = self.active;
        self.active = active;
        if active {
            if !was {
                self.reload_sessions();
            }
        } else {
            self.menu = None;
            self.picker = None;
            self.rename = None;
            self.drawer = false;
        }
        cx.notify();
    }

    pub(crate) fn reset_motion(&mut self) {
        self.motion.reset();
        self.switching = None;
        self.drawer = false;
        self.menu = None;
        self.picker = None;
    }

    fn reload_sessions(&mut self) {
        self.sessions = self.st.store.list_chats().unwrap_or_else(|e| {
            tracing::warn!("could not list chats: {e}");
            Vec::new()
        });
    }

    fn dismiss_overlays(&mut self, cx: &mut Context<Self>) {
        let open = self.picker.is_some()
            || self.menu.is_some()
            || self.rename.is_some()
            || self.text_menu.is_some()
            || self.thinking_menu
            || self.drawer;
        if open {
            self.picker = None;
            self.text_menu = None;
            self.thinking_menu = false;
            self.menu = None;
            self.rename = None;
            self.drawer = false;
            cx.notify();
        }
    }

    fn generating(&self) -> bool {
        self.generation.is_some()
    }

    fn model_facts(&self) -> ModelFacts {
        let cfg = self.st.cfg.get();
        let engine = &cfg.text_model;
        match engine.source {
            Source::Local => {
                let capabilities = self.st.models.spec(&engine.local_model).and_then(|spec| spec.text_capabilities);
                ModelFacts {
                    key: format!("local:{}", engine.local_model),
                    limits: Limits::local(capabilities.as_ref()),
                    thinking: capabilities.and_then(|c| c.thinking),
                    configured: None,
                }
            }
            Source::Provider => {
                let active = engine.active_text();
                ModelFacts {
                    key: format!("cloud:{}", active.as_ref().map_or("", |t| t.id.as_str())),
                    limits: Limits::CLOUD,
                    thinking: Some(chat::CLOUD_THINKING),
                    configured: active.map(|t| t.params.reasoning_effort.as_str().to_owned()),
                }
            }
        }
    }

    /// The choice saved for this model, else the effort its cloud configuration names, else the model's default.
    fn thinking_choice(&self, facts: &ModelFacts) -> Option<&'static str> {
        let capabilities = facts.thinking.as_ref()?;
        let cfg = self.st.cfg.get();
        let saved = cfg.chat.thinking.get(&facts.key).map(String::as_str).or(facts.configured.as_deref());
        chat::thinking_choice(capabilities, saved)
    }

    fn toggle_thinking_menu(&mut self, cx: &mut Context<Self>) {
        let just_closed = self.thinking_closed.take().is_some_and(|at| at.elapsed() < Duration::from_millis(200));
        self.thinking_menu = !self.thinking_menu && !just_closed;
        cx.notify();
    }

    fn close_thinking_menu(&mut self, cx: &mut Context<Self>) {
        if self.thinking_menu {
            self.thinking_menu = false;
            self.thinking_closed = Some(Instant::now());
            cx.notify();
        }
    }

    fn set_thinking(&mut self, choice: &'static str, cx: &mut Context<Self>) {
        let key = self.model_facts().key;
        crate::services::save_config(&self.st, move |cfg| {
            cfg.chat.thinking.insert(key, choice.to_owned());
        });
        self.thinking_menu = false;
        cx.notify();
    }

    fn model_label(&self) -> String {
        let cfg = self.st.cfg.get();
        let engine = &cfg.text_model;
        match engine.source {
            Source::Local => {
                self.st.models.spec(&engine.local_model).map_or_else(|| engine.local_model.clone(), |s| s.name.clone())
            }
            Source::Provider => engine.active_text().map(|t| t.target.model().to_owned()).unwrap_or_default(),
        }
    }

    fn open_model_settings(&self, cx: &mut Context<Self>) {
        settings::open_text_model(self.st.clone(), cx);
    }

    fn fail(&mut self, key: Tr, cx: &mut Context<Self>) {
        self.notice = Some(self.st.cfg.get().general.lang().tr(key).to_owned());
        cx.notify();
    }

    fn lang(&self) -> Lang {
        self.st.cfg.get().general.lang()
    }

    fn date_label(&self) -> impl Fn(i64) -> String + Copy {
        let off = dito_platform::local_utc_offset_secs();
        move |ts| format!("{} {}", ymd(local_day(ts, off)).replace('-', "/"), hhmm(ts, off))
    }

    fn at_bottom(&self) -> bool {
        let max = f32::from(self.scroll.max_offset().y);
        max <= 0. || f32::from(self.scroll.offset().y) <= -max + BOTTOM_SLACK
    }

    fn replace_messages(&mut self, chat: Option<i64>) {
        self.current = chat;
        self.draft_open |= chat.is_none();
        self.rows.clear();
        self.docs.clear();
        let Some(id) = chat else { return };
        match self.st.store.get_chat(id) {
            Ok(Some(chat)) => {
                for row in &chat.messages {
                    if row.author == Author::Assistant {
                        self.docs.insert(row.id, Rc::new(Document::of(&row.content)));
                    }
                }
                self.rows = chat.messages;
            }
            Ok(None) => self.current = None,
            Err(e) => {
                tracing::warn!("could not read chat {id}: {e}");
                self.current = None;
            }
        }
    }

    /// Shows another chat, or a new one, with the switching motion.
    fn switch_to(&mut self, chat: Option<i64>, cx: &mut Context<Self>) {
        self.drawer = false;
        self.menu = None;
        self.rename = None;
        if self.current == chat && (chat.is_some() || self.rows.is_empty()) {
            cx.notify();
            return;
        }
        self.generation = None;
        self.ticker = None;
        let reduced = ui::Motion::system().is_reduced();
        let now_at = Instant::now();
        let (old_rows, old_docs, from) = match self.switching.take() {
            Some(sw) => {
                let elapsed = sw.anim.elapsed_ms(now_at);
                let poses = sw.anim.poses(elapsed);
                match poses {
                    Some(poses) if poses.phase == Phase::Out => {
                        let from = poses.turns.iter().map(|t| t.opacity).collect();
                        (sw.old_rows, sw.old_docs, from)
                    }
                    Some(poses) => {
                        let from = poses.turns.iter().map(|t| t.opacity).collect();
                        (std::mem::take(&mut self.rows), std::mem::take(&mut self.docs), from)
                    }
                    None => self.snapshot(),
                }
            }
            None => self.snapshot(),
        };
        self.replace_messages(chat);
        let incoming = switch::turns(&self.rows).len();
        self.switching = Some(Switching { anim: Switch::new(now_at, from, incoming, reduced), old_rows, old_docs });
        self.scroll.scroll_to_bottom();
        self.reload_sessions();
        cx.notify();
    }

    /// The turns on screen, taken to fade out: the last few, at full opacity.
    fn snapshot(&mut self) -> (Vec<ChatMessageRow>, HashMap<i64, Rc<Document>>, Vec<f32>) {
        let turns = switch::turns(&self.rows).len().min(switch::MAX_TURNS);
        (std::mem::take(&mut self.rows), std::mem::take(&mut self.docs), vec![1.; turns])
    }

    fn new_chat(&mut self, cx: &mut Context<Self>) {
        self.switch_to(None, cx);
    }

    fn open_chat(&mut self, id: i64, cx: &mut Context<Self>) {
        self.switch_to(Some(id), cx);
    }

    fn delete_chat(&mut self, id: i64, cx: &mut Context<Self>) {
        if self.generation.as_ref().is_some_and(|g| g.chat_id == id) {
            self.generation = None;
            self.ticker = None;
        }
        if let Err(e) = self.st.store.delete_chat(id) {
            tracing::warn!("could not delete chat {id}: {e}");
            self.fail(Tr::ChatSaveFailed, cx);
            return;
        }
        self.menu = None;
        if self.current == Some(id) {
            self.switch_to(None, cx);
        }
        self.reload_sessions();
        cx.notify();
    }

    fn rename_chat(&mut self, id: i64, title: &str, cx: &mut Context<Self>) {
        let title = title.trim();
        if !title.is_empty()
            && let Err(e) = self.st.store.rename_chat(id, title)
        {
            tracing::warn!("could not rename chat {id}: {e}");
            self.fail(Tr::ChatSaveFailed, cx);
        }
        self.rename = None;
        self.reload_sessions();
        cx.notify();
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.generating() {
            return;
        }
        let text = self.composer.read(cx).value().to_string();
        if text.trim().is_empty() && self.pending.is_empty() {
            return;
        }
        let attachments = std::mem::take(&mut self.pending);
        let ts = now();
        let chat_id = match self.current {
            Some(id) => id,
            None => {
                let title = chat::title_from(&text, &attachments).unwrap_or_default();
                match self.st.store.create_chat(&title, ts) {
                    Ok(id) => {
                        self.current = Some(id);
                        self.draft_open = false;
                        id
                    }
                    Err(e) => {
                        tracing::warn!("could not create a chat: {e}");
                        self.pending = attachments;
                        self.fail(Tr::ChatSaveFailed, cx);
                        return;
                    }
                }
            }
        };
        let message = NewChatMessage {
            author: Author::User,
            content: text.trim(),
            state: MessageState::Done,
            attachments: &attachments,
            model: None,
            ts,
        };
        match self.st.store.add_chat_message(chat_id, &message) {
            Ok(id) => self.rows.push(ChatMessageRow {
                id,
                author: Author::User,
                content: text.trim().to_owned(),
                state: MessageState::Done,
                error: None,
                attachments,
                model: None,
                ts,
            }),
            Err(e) => {
                tracing::warn!("could not save a chat message: {e}");
                self.pending = attachments;
                self.fail(Tr::ChatSaveFailed, cx);
                return;
            }
        }
        self.composer.update(cx, |state, cx| state.set_value("", window, cx));
        self.reload_sessions();
        self.begin_reply(chat_id, cx);
    }

    /// Adds the reply's row and starts the request for the conversation as it stands.
    fn begin_reply(&mut self, chat_id: i64, cx: &mut Context<Self>) {
        let facts = self.model_facts();
        let mut request = chat::request_for(&self.rows, self.date_label(), facts.limits.context_chars);
        request.thinking = chat::thinking_request(self.thinking_choice(&facts));
        let model = self.model_label();
        let ts = now();
        let placeholder = NewChatMessage {
            author: Author::Assistant,
            content: "",
            state: MessageState::Generating,
            attachments: &[],
            model: Some(&model),
            ts,
        };
        let id = match self.st.store.add_chat_message(chat_id, &placeholder) {
            Ok(id) => id,
            Err(e) => {
                tracing::warn!("could not save a chat reply: {e}");
                self.fail(Tr::ChatSaveFailed, cx);
                return;
            }
        };
        self.rows.push(ChatMessageRow {
            id,
            author: Author::Assistant,
            content: String::new(),
            state: MessageState::Generating,
            error: None,
            attachments: Vec::new(),
            model: Some(model),
            ts,
        });
        self.docs.insert(id, Rc::new(Document::default()));
        self.generation = Some(Generation::start(&self.st, chat_id, id, request));
        self.scroll.scroll_to_bottom();
        self.watch_generation(cx);
        cx.notify();
    }

    fn watch_generation(&mut self, cx: &mut Context<Self>) {
        self.ticker = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(STREAM_FRAME).await;
                let keep = this.update(cx, |page, cx| page.poll_generation(cx));
                if !keep.unwrap_or(false) {
                    break;
                }
            }
        }));
    }

    /// `true` while the reply is still being written.
    fn poll_generation(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(generation) = &mut self.generation else { return false };
        let id = generation.message_id;
        if let Some(text) = generation.grown() {
            if let Some(row) = self.rows.iter_mut().find(|r| r.id == id) {
                row.content.clone_from(&text);
            }
            if let Some(doc) = self.docs.get_mut(&id) {
                Rc::make_mut(doc).update(&text);
            }
        }
        let finished = generation.finished();
        if let Some(Finished { content, state, error }) = finished {
            if let Some(row) = self.rows.iter_mut().find(|r| r.id == id) {
                row.content.clone_from(&content);
                row.state = state;
                row.error = error;
            }
            self.docs.insert(id, Rc::new(Document::of(&content)));
            self.generation = None;
            self.reload_sessions();
            cx.notify();
            return false;
        }
        cx.notify();
        true
    }

    fn stop(&mut self) {
        if let Some(generation) = &self.generation {
            generation.stop();
        }
    }

    /// Replaces the last reply, kept after a stop or a failure, with a new one.
    fn retry(&mut self, cx: &mut Context<Self>) {
        if self.generating() {
            return;
        }
        let Some(chat_id) = self.current else { return };
        let Some(last) = self.rows.last() else { return };
        if last.author != Author::Assistant || !matches!(last.state, MessageState::Stopped | MessageState::Failed) {
            return;
        }
        if let Err(e) = self.st.store.delete_chat_message(last.id) {
            tracing::warn!("could not remove a chat reply: {e}");
            self.fail(Tr::ChatSaveFailed, cx);
            return;
        }
        let id = last.id;
        self.rows.pop();
        self.docs.remove(&id);
        self.begin_reply(chat_id, cx);
    }

    fn add_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result =
            cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: None });
        let max_bytes = self.model_facts().limits.file_bytes;
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = result.await else { return };
            for path in paths {
                let read = cx.background_executor().spawn(async move { chat::read_text_file(&path, max_bytes) }).await;
                let done = this.update(cx, |page, cx| page.attach_file(read, cx));
                if done.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn attach_file(&mut self, read: Result<Attachment, FileError>, cx: &mut Context<Self>) {
        match read {
            Ok(file) => {
                self.pending.push(file);
                self.notice = None;
                cx.notify();
            }
            Err(FileError::TooLarge) => {
                let lang = self.lang();
                let limit = files::size_label(self.model_facts().limits.file_bytes);
                self.notice = Some(fmt::chat_file_too_large(lang, &limit));
                cx.notify();
            }
            Err(FileError::NotText) => self.fail(Tr::ChatFileNotText, cx),
            Err(FileError::Unreadable) => self.fail(Tr::ChatFileUnreadable, cx),
        }
    }

    fn remove_pending(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.pending.len() {
            self.pending.remove(index);
            cx.notify();
        }
    }

    fn open_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let meetings = self.st.store.list_transcripts().unwrap_or_else(|e| {
            tracing::warn!("could not list transcripts: {e}");
            Vec::new()
        });
        let attached = self
            .pending
            .iter()
            .filter_map(|a| match a {
                Attachment::Meeting { transcript_id, .. } => Some(*transcript_id),
                Attachment::File { .. } => None,
            })
            .collect();
        self.picker = Some(picker::Picker::new(meetings, attached, self.lang(), window, cx));
        cx.notify();
    }

    /// Meetings already attached stay as they are; the unticked are dropped and the newly ticked are read now.
    fn confirm_picker(&mut self, cx: &mut Context<Self>) {
        let Some(picker) = self.picker.take() else { return };
        self.pending.retain(|a| match a {
            Attachment::Meeting { transcript_id, .. } => picker.selected.contains(transcript_id),
            Attachment::File { .. } => true,
        });
        let kept: Vec<i64> = self
            .pending
            .iter()
            .filter_map(|a| match a {
                Attachment::Meeting { transcript_id, .. } => Some(*transcript_id),
                Attachment::File { .. } => None,
            })
            .collect();
        let mut failed = false;
        for id in picker.selected.iter().filter(|id| !kept.contains(id)) {
            match self.meeting_attachment(*id) {
                Some(meeting) => self.pending.push(meeting),
                None => failed = true,
            }
        }
        if failed {
            self.fail(Tr::ChatMeetingUnreadable, cx);
        }
        cx.notify();
    }

    fn meeting_attachment(&self, id: i64) -> Option<Attachment> {
        let transcript = self.st.store.get_transcript(id).ok().flatten()?;
        let lang = self.lang();
        let both = transcript.summary.sources.both();
        let text = chat::meeting_text(transcript.paragraphs.iter().map(|p| {
            let speaker = both.then(|| match p.speaker {
                dito_core::store::Speaker::Me => lang.tr(Tr::TranscriptionMe),
                dito_core::store::Speaker::Others => lang.tr(Tr::TranscriptionOthers),
            });
            (speaker, p.text.as_str())
        }));
        let summary: &TranscriptSummary = &transcript.summary;
        let title = if summary.title.trim().is_empty() {
            self.date_label()(summary.started_ts)
        } else {
            summary.title.trim().to_owned()
        };
        Some(Attachment::Meeting {
            transcript_id: id,
            title,
            started_ts: summary.started_ts,
            duration_ms: summary.length_ms,
            text,
        })
    }

    /// The window's width decides it, as the sidebar's rail does; the same hysteresis keeps a drag across the edge
    /// from flickering.
    fn follow_width(&mut self, window: &Window) {
        let width = f32::from(window.viewport_size().width);
        let compact = if self.compact { width < RAIL_EXPAND } else { width < RAIL_COLLAPSE };
        if compact != self.compact {
            self.compact = compact;
            self.drawer = false;
        }
    }
}

impl Render for ChatPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        ui::render_with_scope(self.motion.handle(), || self.view(window, cx))
    }
}

impl ChatPage {
    fn view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        self.follow_width(window);
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        if self.composer_lang != c.lang {
            self.composer_lang = c.lang;
            let placeholder = c.t(Tr::ChatPlaceholder);
            self.composer.update(cx, |state, cx| state.set_placeholder(placeholder, window, cx));
        }
        if self.generating() && self.at_bottom() {
            self.scroll.scroll_to_bottom();
        }
        if self.switching.as_ref().is_some_and(|s| s.anim.poses(s.anim.elapsed_ms(Instant::now())).is_none()) {
            self.switching = None;
        }
        if self.switching.is_some() {
            // A chat opens at its latest turn: the layout that swaps the rows in must still be asked for the bottom.
            self.scroll.scroll_to_bottom();
            window.request_animation_frame();
        }
        let main = self.main_column(&c, window, cx);
        let mut root = div()
            .size_full()
            .relative()
            .flex()
            .bg(p.bg)
            // The message field usually holds the focus, and its own Copy would answer first with nothing to copy.
            .capture_action(cx.listener(|_, _: &gpui_kit::base::input::Copy, window, cx| {
                if copy_selection(window, cx) {
                    cx.stop_propagation();
                }
            }))
            .child(main);
        if !self.compact {
            root = root.child(sidebar::panel(self, &c, cx, false));
        }
        root.children(sidebar::drawer(self, &c, window, cx)).children(self.text_menu_view(&c, cx))
    }

    fn open_text_menu(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let text = selected_text(window, cx);
        if text.is_empty() {
            return;
        }
        self.text_menu = Some(TextMenu { at: ev.position, text });
        cx.notify();
    }

    fn text_menu_view(&self, c: &Ctx, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let menu = self.text_menu.as_ref()?;
        let p = c.p;
        let close = ui::cbk(cx, |page, _, cx| {
            page.text_menu = None;
            cx.notify();
        });
        let copy = ui::menu_row(ElementId::from("chat-text-menu-copy"), &p)
            .on_click(ui::click_cb(ui::cbk(cx, |page, _, cx| {
                if let Some(menu) = page.text_menu.take() {
                    cx.write_to_clipboard(ClipboardItem::new_string(menu.text));
                }
                cx.notify();
            })))
            .child(ui::text(c.t(Tr::HudCopy), 13., FontWeight::NORMAL, p.text).line_height(relative(1.)));
        let panel = ui::menu_panel(ElementId::from("chat-text-menu"), &p, close).w(px(160.)).child(copy);
        Some(deferred(anchored().position(menu.at).snap_to_window_with_margin(px(8.)).child(panel)).with_priority(2))
    }

    fn main_column(&mut self, c: &Ctx, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let p = c.p;
        let toolbar = self.toolbar(c, cx);
        let body = messages::viewport(self, c, cx);
        let banner = self.notice.clone().map(|message| {
            ui::dismissible_banner(
                &p,
                "chat-notice-close",
                message,
                c.t(Tr::HudClose),
                ui::cbk(cx, |page, _, cx| {
                    page.notice = None;
                    cx.notify();
                }),
            )
        });
        div()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .relative()
            .flex()
            .flex_col()
            .px(px(PAGE_PAD_X))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::open_text_menu))
            .child(ui::drag_area("chat-drag").absolute().top_0().left_0().right_0().h(px(TOOLBAR_TOP)))
            .child(div().h(px(TOOLBAR_TOP)).flex_none())
            .child(toolbar)
            .child(div().h(px(VIEWPORT_TOP_GAP)).flex_none())
            .child(body)
            .children(banner.map(|b| div().w_full().pb(px(12.)).child(b)))
            .child(composer::view(self, c, window, cx))
            .child(div().h(px(COMPOSER_BOTTOM)).flex_none())
    }

    fn toolbar(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = c.p;
        let mut bar = div()
            .h(px(TOOLBAR_H))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(12.))
            .child(ui::page_title(&p, c.t(Tr::NavChat)).line_height(px(TOOLBAR_H)).flex_1().min_w(px(0.)))
            .child(self.model_button(c, cx));
        if self.compact {
            bar = bar.child(sidebar::history_button(c, cx));
        }
        bar
    }

    /// The model in use and where it runs: it opens Settings at the model page, and there is no dropdown.
    fn model_button(&self, c: &Ctx, cx: &mut Context<Self>) -> impl IntoElement {
        let p = c.p;
        let id = ElementId::from("chat-model-button");
        let cfg = self.st.cfg.get();
        let (source_icon, source) = match cfg.text_model.source {
            Source::Local => (Ic::Cpu, c.t(Tr::ModelsLocal)),
            Source::Provider => (Ic::Globe, c.t(Tr::ModelsProvider)),
        };
        div()
            .id(id.clone())
            .relative()
            .h(px(36.))
            .px(px(12.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(12.))
            .cursor_pointer()
            .on_hover(ui::hover_tracker(id.clone()))
            .on_click(ui::click_cb(ui::cbk(cx, |page, _, cx| page.open_model_settings(cx))))
            .child(ui::hover_fill(&id, Some(12.), p.seg))
            .child(ui::text(self.model_label(), 13., FontWeight::MEDIUM, p.text).line_height(px(18.)))
            .child(
                div()
                    .h(px(24.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded_full()
                    .bg(p.card)
                    .border_1()
                    .border_color(p.border_strong)
                    .child(ui::icon(source_icon, 14., p.muted))
                    .child(ui::text(source, 12., FontWeight::NORMAL, p.muted).line_height(px(18.))),
            )
    }
}

fn selected_text(window: &mut Window, cx: &mut App) -> String {
    gpui_kit::base::TextSelection::selected_text(window, cx).trim().to_owned()
}

fn copy_selection(window: &mut Window, cx: &mut App) -> bool {
    let text = selected_text(window, cx);
    if text.is_empty() {
        return false;
    }
    cx.write_to_clipboard(ClipboardItem::new_string(text));
    true
}
