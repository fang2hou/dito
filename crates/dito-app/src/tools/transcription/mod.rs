//! The Transcription tool: the readiness gate a session needs, the meeting list, one transcript as an editable
//! document, and the bottom panel that starts and stops a session and unfolds into the live speaker lanes.

mod detail;
mod dock;
mod lanes;
mod list;
mod readiness;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::i18n::Tr;
use dito_core::store::{ParagraphState, Speaker, Transcript, TranscriptParagraph, TranscriptSummary};
use dito_core::transcription::{SessionState, TranscriptionStatus};
use dito_local::SILERO_VAD;
use dito_platform::{Permission, Status, permissions};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::services;
use crate::settings::Ctx;
use crate::state::AppState;
use crate::timefmt::{hhmm, local_day, now, ymd};
use crate::ui::{self, InputEvent, InputState, TextareaState};

/// The page gutters and rhythm, the same as every other page of the window.
const PAGE_PAD_X: f32 = 56.;
const PAGE_PAD_TOP: f32 = 40.;
const PAGE_PAD_BOTTOM: f32 = 28.;
const PAGE_GAP: f32 = 18.;
/// The sweep over unsettled words is redrawn this often instead of every display frame: it is a slow color change,
/// and each redraw renders the whole page.
const SWEEP_FRAME: Duration = Duration::from_millis(33);
/// How close to the end of the page still counts as reading the newest paragraphs.
const BOTTOM_SLACK: f32 = 4.;

/// What the page shows: the meeting list, or one transcript.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum View {
    List,
    Detail,
}

struct ParagraphView {
    data: TranscriptParagraph,
    /// Only a finished paragraph is editable.
    editor: Option<Entity<TextareaState>>,
}

pub(super) struct OpenTranscript {
    summary: TranscriptSummary,
    title: Entity<InputState>,
    paragraphs: Vec<ParagraphView>,
}

/// The permissions a session needs, polled while the gate is visible: macOS changes them outside the app.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Perms {
    microphone: Status,
    system_audio: Status,
}

impl Perms {
    fn read() -> Self {
        Self {
            microphone: permissions::status(Permission::Microphone),
            system_audio: permissions::status(Permission::SystemAudio),
        }
    }

    fn ready(self) -> bool {
        self.microphone == Status::Granted && self.system_audio == Status::Granted
    }
}

pub struct TranscriptionPage {
    st: Arc<AppState>,
    motion: ui::MotionScope,
    status: TranscriptionStatus,
    list: Vec<TranscriptSummary>,
    open: Option<OpenTranscript>,
    view: View,
    delete_armed: bool,
    /// The runtime error the user closed; a different one shows again.
    dismissed: Option<String>,
    perms: Perms,
    /// Only a visible page polls the permissions and keeps its live words moving.
    active: bool,
    born: Instant,
    /// Redraws the sweep while unsettled words are shown.
    sweep: Option<Task<()>>,
    /// The open transcript's own scroll.
    scroll: ScrollHandle,
    list_scroll: ScrollHandle,
    /// The bottom panel's size and where its move stands.
    dock: dock::DockAnim,
    /// The bottom panel's real width, measured once it has painted: the shell's sidebar collapses and expands at
    /// different window widths, so the panel cannot guess it from the window.
    dock_width: Rc<Cell<Option<f32>>>,
    /// The lanes stay drawn while the panel folds, so they shrink into the bar instead of vanishing.
    dock_lanes: bool,
    /// Lets the screenshot harness pose the tool past the readiness gate on any machine.
    #[cfg(feature = "screenshots")]
    assume_ready: bool,
}

impl TranscriptionPage {
    pub fn new(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        services::watch_config(&st, cx);
        let mut rx = st.transcription.subscribe();
        cx.spawn_in(window, async move |this, cx| {
            while rx.changed().await.is_ok() {
                let status = rx.borrow_and_update().clone();
                if this.update_in(cx, |page, window, cx| page.on_status(status, window, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        // The elapsed time in the panel moves once a second while a session runs.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let alive = this.update(cx, |page, cx| {
                    if matches!(page.status.state, SessionState::Recording { .. }) {
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
        // The gate follows the detector's download while it is shown.
        let mut models = st.models.subscribe();
        cx.spawn(async move |this, cx| {
            while models.changed().await.is_ok() {
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();
        // A permission granted in System Settings shows up without reopening the window.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let alive = this.update(cx, |page, cx| {
                    if !page.active {
                        return;
                    }
                    let perms = Perms::read();
                    if perms != page.perms {
                        page.perms = perms;
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
        let status = st.transcription.status();
        let mut page = Self {
            st,
            motion: ui::MotionScope::new(),
            status,
            list: Vec::new(),
            open: None,
            view: View::List,
            delete_armed: false,
            dismissed: None,
            perms: Perms::read(),
            active: false,
            born: Instant::now(),
            sweep: None,
            scroll: ScrollHandle::new(),
            list_scroll: ScrollHandle::new(),
            dock: dock::DockAnim::new(),
            dock_width: Rc::new(Cell::new(None)),
            dock_lanes: false,
            #[cfg(feature = "screenshots")]
            assume_ready: false,
        };
        page.reload_list();
        page
    }

    /// The open transcript follows the session's status even while the page is hidden; only the list is re-read.
    pub fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        let was = self.active;
        self.active = active;
        if active {
            if !was {
                self.perms = Perms::read();
            }
            self.reload_list();
            cx.notify();
        }
    }

    /// True while a session records: the window's sidebar shows a red dot for it.
    pub fn is_recording(&self) -> bool {
        matches!(self.status.state, SessionState::Recording { .. })
    }

    pub fn reset_motion(&mut self) {
        self.motion.reset();
        self.dock.settle();
        self.born = Instant::now();
    }

    #[cfg(feature = "screenshots")]
    pub fn motion_scope(&self) -> ui::ScopeHandle {
        self.motion.handle()
    }

    #[cfg(feature = "screenshots")]
    pub fn assume_ready(&mut self, ready: bool, cx: &mut Context<Self>) {
        self.assume_ready = ready;
        cx.notify();
    }

    #[cfg(feature = "screenshots")]
    pub fn open_newest(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.list.first().map(|row| row.id) {
            self.open_transcript(id, window, cx);
        }
    }

    /// The newest transcript as if it were still being recorded: a row waiting for polishing, then the open
    /// paragraph's settled words and the words still being heard in the lanes.
    #[cfg(feature = "screenshots")]
    pub fn preview_live(&mut self, tail: [&str; 3], window: &mut Window, cx: &mut Context<Self>) {
        let [polishing, settled, unsettled] = tail;
        self.open_newest(window, cx);
        let Some(open) = &mut self.open else { return };
        open.summary.ended_ts = None;
        open.paragraphs.push(ParagraphView {
            data: TranscriptParagraph {
                id: -2,
                speaker: Speaker::Others,
                start_ms: 52_000,
                end_ms: 58_000,
                raw: polishing.into(),
                text: polishing.into(),
                state: ParagraphState::Polishing,
                error: None,
            },
            editor: None,
        });
        let (id, sources) = (open.summary.id, open.summary.sources);
        // The clock reads 03:42 like the design: the stored entry started hours ago, the pose starts fresh.
        self.status.state = SessionState::Recording { id, started_ts: crate::timefmt::now() - 222, sources };
        self.status.closed = open.paragraphs.len();
        self.status.live = vec![dito_core::transcription::LiveText {
            speaker: Speaker::Others,
            start_ms: 60_000,
            settled: settled.into(),
            unsettled: unsettled.into(),
        }];
        self.view = View::Detail;
        cx.notify();
    }

    /// Follows another session's status, as the app's own is followed.
    #[cfg(feature = "screenshots")]
    pub fn follow(
        &mut self,
        handle: dito_core::transcription::TranscriptionHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut rx = handle.subscribe();
        cx.spawn_in(window, async move |this, cx| {
            while rx.changed().await.is_ok() {
                let status = rx.borrow_and_update().clone();
                if this.update_in(cx, |page, window, cx| page.on_status(status, window, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn on_status(&mut self, status: TranscriptionStatus, window: &mut Window, cx: &mut Context<Self>) {
        let (before, after) = (self.status.state.transcript_id(), status.state.transcript_id());
        let changed = status.revision != self.status.revision;
        self.status = status;
        match after {
            // A new session opens its own entry, so the user watches it fill.
            Some(id) if before != Some(id) => self.open_transcript(id, window, cx),
            _ if changed => {
                self.reload_list();
                self.refresh_open(window, cx);
            }
            _ => {}
        }
        cx.notify();
    }

    fn reload_list(&mut self) {
        self.list = self.st.store.list_transcripts().unwrap_or_else(|e| {
            tracing::warn!("could not list transcripts: {e}");
            Vec::new()
        });
    }

    fn open_transcript(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        self.reload_list();
        self.delete_armed = false;
        let Some(t) = self.load(id) else {
            self.open = None;
            self.view = View::List;
            cx.notify();
            return;
        };
        let lang = self.st.cfg.get().general.lang();
        let title_value = t.summary.title.clone();
        let title = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(lang.tr(Tr::TranscriptionTitlePlaceholder))
                .default_value(title_value)
        });
        ui::notify_on_focus(&title, window, cx);
        let store = self.st.store.clone();
        cx.subscribe_in(&title, window, move |page: &mut Self, state, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change) {
                let value = state.read(cx).value().to_string();
                if let Err(e) = store.set_transcript_title(id, &value) {
                    tracing::warn!("could not save a transcript title: {e}");
                }
                if let Some(open) = &mut page.open {
                    open.summary.title = value.clone();
                }
                if let Some(row) = page.list.iter_mut().find(|row| row.id == id) {
                    row.title = value;
                }
                cx.notify();
            }
        })
        .detach();
        let paragraphs = t.paragraphs.into_iter().map(|p| self.paragraph_view(p, window, cx)).collect();
        self.open = Some(OpenTranscript { summary: t.summary, title, paragraphs });
        self.view = View::Detail;
        // A live entry opens at its end, where the words arrive.
        if self.status.state.transcript_id() == Some(id) {
            self.scroll.scroll_to_bottom();
        } else {
            self.scroll.set_offset(point(px(0.), px(0.)));
        }
        cx.notify();
    }

    fn load(&self, id: i64) -> Option<Transcript> {
        self.st.store.get_transcript(id).unwrap_or_else(|e| {
            tracing::warn!("could not read transcript {id}: {e}");
            None
        })
    }

    /// Re-reads the open transcript. Editors of finished paragraphs are kept, so typing is never interrupted.
    fn refresh_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.open.as_ref().map(|o| o.summary.id) else { return };
        let Some(t) = self.load(id) else {
            self.open = None;
            self.view = View::List;
            return;
        };
        let mut old = self.open.take().expect("checked above");
        let mut paragraphs = Vec::with_capacity(t.paragraphs.len());
        for data in t.paragraphs {
            let kept = old.paragraphs.iter().position(|v| v.data.id == data.id && v.editor.is_some());
            match kept {
                Some(i) => {
                    let mut view = old.paragraphs.swap_remove(i);
                    view.data = data;
                    paragraphs.push(view);
                }
                None => paragraphs.push(self.paragraph_view(data, window, cx)),
            }
        }
        old.summary = TranscriptSummary { title: old.summary.title, ..t.summary };
        old.paragraphs = paragraphs;
        self.open = Some(old);
    }

    fn paragraph_view(&self, data: TranscriptParagraph, window: &mut Window, cx: &mut Context<Self>) -> ParagraphView {
        let editor = (data.state == ParagraphState::Done).then(|| {
            let value = data.text.clone();
            let editor = cx.new(|cx| TextareaState::new(window, cx).default_value(value));
            let (store, id) = (self.st.store.clone(), data.id);
            cx.subscribe_in(&editor, window, move |_: &mut Self, state, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    if let Err(e) = store.edit_paragraph(id, &state.read(cx).value()) {
                        tracing::warn!("could not save a paragraph edit: {e}");
                    }
                    cx.notify();
                }
            })
            .detach();
            editor
        });
        ParagraphView { data, editor }
    }

    fn close_detail(&mut self, cx: &mut Context<Self>) {
        self.view = View::List;
        self.delete_armed = false;
        self.reload_list();
        cx.notify();
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        self.dismissed = None;
        self.perms = Perms::read();
        if !self.perms.ready() {
            if self.perms.microphone == Status::NotDetermined {
                permissions::request(Permission::Microphone);
            }
            if self.perms.system_audio == Status::NotDetermined {
                permissions::request(Permission::SystemAudio);
            }
            cx.notify();
            return;
        }
        if !self.vad_ready() {
            cx.notify();
            return;
        }
        self.st.transcription.start(self.st.cfg.get().transcription.sources());
    }

    fn delete_open(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.open.as_ref().map(|o| o.summary.id) else { return };
        if !self.delete_armed {
            self.delete_armed = true;
            cx.notify();
            return;
        }
        if let Err(e) = self.st.store.delete_transcript(id) {
            tracing::warn!("could not delete transcript {id}: {e}");
        }
        self.open = None;
        self.view = View::List;
        self.delete_armed = false;
        self.reload_list();
        cx.notify();
    }

    fn copy_open(&self, cx: &mut App) -> bool {
        let Some(open) = &self.open else { return false };
        let lang = self.st.cfg.get().general.lang();
        let text = open
            .paragraphs
            .iter()
            .map(|v| {
                let body =
                    v.editor.as_ref().map(|e| e.read(cx).value().to_string()).unwrap_or_else(|| v.data.text.clone());
                if open.summary.sources.both() {
                    format!("{}: {body}", lang.tr(speaker_label(v.data.speaker)))
                } else {
                    body
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        true
    }

    /// One paragraph's words, as they read on screen.
    fn copy_paragraph(&self, id: i64, cx: &mut App) -> bool {
        let Some(open) = &self.open else { return false };
        let Some(view) = open.paragraphs.iter().find(|v| v.data.id == id) else { return false };
        let text =
            view.editor.as_ref().map(|e| e.read(cx).value().to_string()).unwrap_or_else(|| view.data.text.clone());
        if text.trim().is_empty() {
            return false;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        true
    }

    /// Milliseconds since the running session started, for the panel's clock.
    fn session_elapsed_ms(&self) -> Option<u64> {
        let started = match self.status.state {
            SessionState::Recording { started_ts, .. } => started_ts,
            SessionState::Finishing { id } => self
                .open
                .as_ref()
                .filter(|open| open.summary.id == id)
                .map(|open| open.summary.started_ts)
                .or_else(|| self.list.iter().find(|row| row.id == id).map(|row| row.started_ts))?,
            _ => return None,
        };
        Some((now() - started).max(0) as u64 * 1000)
    }

    fn mic_ready(&self) -> bool {
        self.perms.microphone == Status::Granted
    }

    fn system_audio_ready(&self) -> bool {
        self.perms.system_audio == Status::Granted
    }

    /// Under a build without the local runtimes the detector is not part of the path, so it never blocks a start.
    fn vad_ready(&self) -> bool {
        !cfg!(feature = "runtimes") || self.st.models.is_installed(SILERO_VAD)
    }

    /// A running session is never hidden behind the gate, even if a permission is revoked mid-session.
    fn gate_visible(&self) -> bool {
        #[cfg(feature = "screenshots")]
        if self.assume_ready {
            return false;
        }
        matches!(self.status.state, SessionState::Idle)
            && (!self.mic_ready() || !self.system_audio_ready() || !self.vad_ready())
    }

    /// Starts or stops the timer that redraws the sweep.
    fn keep_sweeping(&mut self, animating: bool, cx: &mut Context<Self>) {
        if !animating {
            self.sweep = None;
            return;
        }
        if self.sweep.is_some() {
            return;
        }
        self.sweep = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(SWEEP_FRAME).await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        }));
    }
}

fn speaker_label(speaker: Speaker) -> Tr {
    match speaker {
        Speaker::Me => Tr::TranscriptionMe,
        Speaker::Others => Tr::TranscriptionOthers,
    }
}

fn clock(ms: u64) -> String {
    let secs = ms / 1000;
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
    } else {
        format!("{:02}:{:02}", secs / 60, secs % 60)
    }
}

/// `2026/11/13 14:02` in the user's time zone.
fn date_time(ts: i64) -> String {
    let off = dito_platform::local_utc_offset_secs();
    format!("{} {}", ymd(local_day(ts, off)).replace('-', "/"), hhmm(ts, off))
}

/// Opens a transcript from a list row or the folded panel.
fn open_row(cx: &mut Context<TranscriptionPage>, id: i64) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    cx.listener(move |page, _, window, cx| page.open_transcript(id, window, cx))
}

impl Render for TranscriptionPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        ui::with_motion_scope(self.motion.handle(), || self.view(window, cx))
    }
}

impl TranscriptionPage {
    fn view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let c = Ctx::new(&self.st, cx);
        // A reader at the end of a live entry stays there as rows arrive and polished text replaces raw text; one
        // who scrolled up is left where they are.
        if self.view == View::Detail && self.is_recording() {
            let live =
                self.open.as_ref().is_some_and(|open| self.status.state.transcript_id() == Some(open.summary.id));
            if live && self.scroll.offset().y <= -self.scroll.max_offset().y + px(BOTTOM_SLACK) {
                self.scroll.scroll_to_bottom();
            }
        }
        let mut page = div()
            .size_full()
            .flex()
            .flex_col()
            .px(px(PAGE_PAD_X))
            .pt(px(PAGE_PAD_TOP))
            .pb(px(PAGE_PAD_BOTTOM))
            .gap(px(PAGE_GAP));
        if self.gate_visible() {
            return page.child(div().flex_1().min_h(px(0.)).child(readiness::view(self, &c, cx)));
        }
        let banner = self.banner(&c, cx);
        if self.view == View::Detail && self.open.is_some() {
            page = page.children(banner).child(div().flex_1().min_h(px(0.)).child(detail::view(self, &c, window, cx)));
        } else {
            page = page
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(ui::page_title(&c.p, c.t(Tr::NavTranscription))),
                )
                .children(banner)
                .child(div().flex_1().min_h(px(0.)).child(list::view(self, &c, cx)));
        }
        // Reading a finished entry with no session running leaves the page to the document itself.
        if self.view == View::Detail && matches!(self.status.state, SessionState::Idle) {
            self.dock.reset();
            self.dock_lanes = false;
            page
        } else {
            page.child(dock::view(self, &c, window, cx))
        }
    }

    /// A runtime failure is one dismissible banner; closing it holds until the next distinct error.
    fn banner(&self, c: &Ctx, cx: &mut Context<Self>) -> Option<Div> {
        let error = self.status.error.as_deref()?;
        if self.dismissed.as_deref() == Some(error) {
            return None;
        }
        Some(ui::dismissible_banner(
            &c.p,
            "transcript-error-close",
            ui::error_line(c.lang, error),
            c.t(Tr::HudClose),
            ui::cbk(cx, |page, _, cx| {
                page.dismissed = page.status.error.clone();
                cx.notify();
            }),
        ))
    }
}
