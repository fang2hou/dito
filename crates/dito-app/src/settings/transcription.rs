//! The Transcription page: start and stop a session, and read or edit each transcript as a memo entry.

use std::sync::Arc;
use std::time::Duration;

use dito_core::config::TranscriptionSettings;
use dito_core::i18n::Tr;
use dito_core::store::{ParagraphState, Sources, Speaker, Transcript, TranscriptParagraph, TranscriptSummary};
use dito_core::transcription::{SessionState, TranscriptionStatus};
use dito_local::{ModelState, SILERO_VAD};
use dito_platform::{Permission, Status, permissions};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{Ctx, page_scroll};
use crate::assets::Ic;
use crate::services;
use crate::state::AppState;
use crate::theme::Palette;
use crate::timefmt::{hhmm, local_day, now, ymd};
use crate::ui::{self, BtnKind, BtnSize, InputEvent, InputState, Textarea, TextareaState, cbk, text};

/// The time, speaker and state column left of each paragraph.
const META_COL: f32 = 64.;
const ROW_PAD_X: f32 = 24.;
const BODY_SIZE: f32 = 14.;
const BODY_LINE: f32 = 22.;
/// The textarea's own inset around its text (its medium input padding plus the 1 pt border it keeps); plain text
/// paragraphs use the same inset so editing never moves the words.
const EDITOR_PAD_X: f32 = 12.;
const EDITOR_PAD_Y: f32 = 9.;
/// The textarea wraps this far short of its text area (gpui-base's `RIGHT_MARGIN`).
const EDITOR_WRAP_MARGIN: f32 = 10.;
/// Lines of each speaker's live text the floating box shows; older lines scroll out at the top.
const FLOAT_LINES: usize = 4;
/// The floating box's distance from the bottom of the page and its vertical padding.
const FLOAT_BOTTOM: f32 = 24.;
const FLOAT_PAD_Y: f32 = 16.;
const FLOAT_GAP: f32 = 12.;

/// Room kept under the rows while recording, so the floating box at its tallest covers none of them: one entry per
/// speaker, each a meta line and `FLOAT_LINES` of text.
fn float_reserve(sources: Sources) -> f32 {
    let entries = if sources.both() { 2. } else { 1. };
    let entry = 18. + 6. + FLOAT_LINES as f32 * BODY_LINE;
    FLOAT_BOTTOM + 2. * FLOAT_PAD_Y + entries * entry + (entries - 1.) * FLOAT_GAP
}
/// The sweep over unsettled words: its half width in characters and its speed in characters a second.
const SWEEP_BAND: f32 = 6.;
const SWEEP_SPEED: f32 = 18.;
/// The sweep is redrawn this often instead of every display frame: it is a slow color change, and each redraw
/// renders the whole page.
const SWEEP_FRAME: Duration = Duration::from_millis(33);
/// How close to the end of the page still counts as reading the newest paragraphs.
const BOTTOM_SLACK: f32 = 4.;

#[derive(Clone, Copy, PartialEq, Eq)]
enum VadPrompt {
    Hidden,
    /// The model is missing; the user is asked before it is downloaded.
    Asking,
    Downloading,
    Failed,
}

struct ParagraphView {
    data: TranscriptParagraph,
    /// Only a finished paragraph is editable.
    editor: Option<Entity<TextareaState>>,
}

struct OpenTranscript {
    summary: TranscriptSummary,
    title: Entity<InputState>,
    paragraphs: Vec<ParagraphView>,
    /// The room under the rows for the floating box stays until the entry is left, even after recording ends, so the
    /// page never gets shorter under a reader at its end.
    reserve: Option<Sources>,
}

pub struct TranscriptionPage {
    st: Arc<AppState>,
    motion: ui::MotionScope,
    status: TranscriptionStatus,
    list: Vec<TranscriptSummary>,
    open: Option<OpenTranscript>,
    vad: VadPrompt,
    /// A problem found before the session could start (microphone access).
    notice: Option<Tr>,
    delete_armed: bool,
    /// The clock of the sweep over unsettled words.
    born: std::time::Instant,
    /// Redraws the sweep while unsettled words are shown.
    sweep: Option<Task<()>>,
    scroll: ScrollHandle,
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
        // The elapsed time moves once a second while recording.
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
        let status = st.transcription.status();
        let mut page = Self {
            st,
            motion: ui::MotionScope::new(),
            status,
            list: Vec::new(),
            open: None,
            vad: VadPrompt::Hidden,
            notice: None,
            delete_armed: false,
            born: std::time::Instant::now(),
            sweep: None,
            scroll: ScrollHandle::new(),
        };
        page.reload_list();
        page
    }

    /// The open transcript follows the session's status even while the page is hidden; only the list is re-read.
    pub fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if active {
            self.reload_list();
            cx.notify();
        }
    }

    pub(super) fn reset_motion(&mut self) {
        self.motion.reset();
    }

    #[cfg(feature = "screenshots")]
    pub(super) fn motion_scope(&self) -> ui::ScopeHandle {
        self.motion.handle()
    }

    #[cfg(feature = "screenshots")]
    pub(super) fn open_newest(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.list.first().map(|row| row.id) {
            self.open_transcript(id, window, cx);
        }
    }

    /// The newest transcript as if it were still being recorded: a row waiting for polishing, then the open
    /// paragraph's settled words and the words still being heard in the floating box.
    #[cfg(feature = "screenshots")]
    pub(super) fn preview_live(&mut self, tail: [&str; 3], window: &mut Window, cx: &mut Context<Self>) {
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
        let (id, started_ts, sources) = (open.summary.id, open.summary.started_ts, open.summary.sources);
        open.reserve = Some(sources);
        self.status.state = SessionState::Recording { id, started_ts, sources };
        self.status.closed = open.paragraphs.len();
        self.status.live = vec![dito_core::transcription::LiveText {
            speaker: Speaker::Others,
            start_ms: 60_000,
            settled: settled.into(),
            unsettled: unsettled.into(),
        }];
        cx.notify();
    }

    /// Follows another session's status, as the app's own is followed.
    #[cfg(feature = "screenshots")]
    pub(super) fn follow(
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
        let reserve = (self.status.state.transcript_id() == Some(id)).then_some(t.summary.sources);
        self.open = Some(OpenTranscript { summary: t.summary, title, paragraphs, reserve });
        // A live entry opens at its end, where the words arrive.
        if reserve.is_some() {
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

    fn start(&mut self, cx: &mut Context<Self>) {
        self.notice = None;
        let sources = self.st.cfg.get().transcription.sources();
        if sources.microphone {
            match permissions::status(Permission::Microphone) {
                Status::Denied => {
                    self.notice = Some(Tr::TranscriptionMicDenied);
                    cx.notify();
                    return;
                }
                Status::NotDetermined => permissions::request(Permission::Microphone),
                Status::Granted => {}
            }
        }
        if cfg!(feature = "runtimes") && !self.st.models.is_installed(SILERO_VAD) {
            self.vad = VadPrompt::Asking;
            cx.notify();
            return;
        }
        self.st.transcription.start(sources);
    }

    /// Downloads the detector the user just agreed to, then starts the session it was asked for.
    fn download_vad(&mut self, cx: &mut Context<Self>) {
        if let Err(e) = self.st.models.start_download(SILERO_VAD) {
            tracing::warn!("could not start the voice activity model download: {e}");
            self.vad = VadPrompt::Failed;
            cx.notify();
            return;
        }
        self.vad = VadPrompt::Downloading;
        cx.notify();
        let mut rx = self.st.models.subscribe();
        cx.spawn(async move |this, cx| {
            loop {
                let state = rx.borrow_and_update().get(SILERO_VAD).cloned();
                let outcome = match state {
                    Some(ModelState::Installed) => Some(true),
                    Some(ModelState::Failed { .. }) => Some(false),
                    _ => None,
                };
                if let Some(installed) = outcome {
                    let _ = this.update(cx, |page, cx| {
                        page.vad = if installed { VadPrompt::Hidden } else { VadPrompt::Failed };
                        if installed {
                            page.st.transcription.start(page.st.cfg.get().transcription.sources());
                        }
                        cx.notify();
                    });
                    break;
                }
                if rx.changed().await.is_err() {
                    break;
                }
            }
        })
        .detach();
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

fn started_at(ts: i64) -> String {
    let off = dito_platform::local_utc_offset_secs();
    format!("{} {}", ymd(local_day(ts, off)), hhmm(ts, off))
}

impl Render for TranscriptionPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        ui::with_motion_scope(self.motion.handle(), || self.view(window, cx))
    }
}

impl TranscriptionPage {
    fn view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        let actions = self.session_controls(&c, cx).into_any_element();
        let body =
            div().w_full().flex().flex_col().gap(px(28.)).children(self.problem(&c)).children(self.vad_card(&c, cx));
        let body = if self.open.is_some() {
            body.child(self.detail(&c, window, cx))
        } else {
            body.child(self.recording_group(&c)).child(self.transcript_list(&c, cx))
        };
        let floating = self.floating(&c, cx);
        // A reader at the end of a live entry stays there as rows arrive and polished text replaces raw text; one
        // who scrolled up is left where they are.
        let at_end = self.scroll.offset().y <= -self.scroll.max_offset().y + px(BOTTOM_SLACK);
        if at_end && self.open.as_ref().is_some_and(|open| self.is_live(open)) {
            self.scroll.scroll_to_bottom();
        }
        // The page scrolls under the floating box, which stays at the bottom of the window.
        div()
            .relative()
            .size_full()
            .child(page_scroll(
                p,
                "transcription-page",
                c.t(Tr::NavTranscription),
                Some(actions),
                body,
                Some(&self.scroll),
            ))
            .children(floating)
    }

    fn session_controls(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = &c.p;
        let row = div().flex().items_center().gap(px(14.));
        match &self.status.state {
            SessionState::Idle => row.child(ui::button(
                "transcription-start",
                p,
                &c.t(Tr::TranscriptionStart),
                BtnKind::Primary,
                Some(Ic::Mic),
                cbk(cx, |page, _, cx| page.start(cx)),
            )),
            SessionState::Starting => row.child(ui::disabled_button(
                "transcription-starting",
                p,
                &c.t(Tr::TranscriptionStarting),
                BtnKind::Primary,
                BtnSize::Regular,
                Some(Ic::Mic),
                None,
            )),
            SessionState::Recording { started_ts, .. } => {
                let elapsed = (now() - started_ts).max(0) as u64 * 1000;
                row.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(div().size(px(8.)).rounded_full().bg(p.danger))
                        .child(text(c.t(Tr::TranscriptionRecording), 13., FontWeight::MEDIUM, p.text))
                        .child(text(clock(elapsed), 13., FontWeight::MEDIUM, p.muted)),
                )
                .child(ui::button(
                    "transcription-stop",
                    p,
                    &c.t(Tr::TranscriptionStop),
                    BtnKind::Secondary,
                    Some(Ic::Stop),
                    cbk(cx, |page, _, _| page.st.transcription.stop()),
                ))
            }
            SessionState::Finishing { .. } => row.child(ui::disabled_button(
                "transcription-finishing",
                p,
                &c.t(Tr::TranscriptionFinishing),
                BtnKind::Secondary,
                BtnSize::Regular,
                None,
                None,
            )),
        }
    }

    fn problem(&self, c: &Ctx) -> Option<Div> {
        let message: SharedString = match (&self.notice, &self.status.error) {
            (Some(tr), _) => c.t(*tr),
            (None, Some(error)) => ui::error_line(c.lang, error).into(),
            (None, None) => return None,
        };
        Some(ui::banner(&c.p, message, None))
    }

    fn vad_card(&self, c: &Ctx, cx: &mut Context<Self>) -> Option<Div> {
        if self.vad == VadPrompt::Hidden {
            return None;
        }
        let p = &c.p;
        let buttons = div().flex().items_center().gap(px(10.));
        let buttons = match self.vad {
            VadPrompt::Downloading => buttons.child(ui::disabled_button(
                "transcription-vad-downloading",
                p,
                &c.t(Tr::TranscriptionVadDownloading),
                BtnKind::Primary,
                BtnSize::Regular,
                Some(Ic::Download),
                None,
            )),
            _ => buttons
                .child(ui::button(
                    "transcription-vad-download",
                    p,
                    &c.t(Tr::TranscriptionVadDownload),
                    BtnKind::Primary,
                    Some(Ic::Download),
                    cbk(cx, |page, _, cx| page.download_vad(cx)),
                ))
                .child(ui::button(
                    "transcription-vad-cancel",
                    p,
                    &c.t(Tr::TranscriptionCancel),
                    BtnKind::Secondary,
                    None,
                    cbk(cx, |page, _, cx| {
                        page.vad = VadPrompt::Hidden;
                        cx.notify();
                    }),
                )),
        };
        let mut card = ui::card(p)
            .p(px(24.))
            .gap(px(8.))
            .child(text(c.t(Tr::TranscriptionVadTitle), 14., FontWeight::MEDIUM, p.text))
            .child(text(c.t(Tr::TranscriptionVadBody), 13., FontWeight::NORMAL, p.muted));
        if self.vad == VadPrompt::Failed {
            card = card.child(text(c.t(Tr::TranscriptionVadFailed), 13., FontWeight::MEDIUM, p.danger_text));
        }
        Some(card.child(div().pt(px(8.)).child(buttons)))
    }

    fn recording_group(&self, c: &Ctx) -> Div {
        let p = &c.p;
        let sources = self.st.cfg.get().transcription.sources();
        // Turning off the only source left does nothing: a session records at least one.
        let toggle = |id: &'static str, on: bool, set: fn(&mut TranscriptionSettings, bool)| {
            let st = self.st.clone();
            ui::switch(
                id,
                p,
                on,
                std::rc::Rc::new(move |on, _, _| {
                    services::save_config(&st, move |cfg| {
                        let mut next = cfg.transcription.clone();
                        set(&mut next, on);
                        if next.system || next.microphone {
                            cfg.transcription = next;
                        }
                    })
                }),
            )
        };
        let row = |label: Tr, hint: Tr, switch: Stateful<Div>| {
            div()
                .w_full()
                .px(px(ROW_PAD_X))
                .py(px(16.))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(24.))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(text(c.t(label), 14., FontWeight::MEDIUM, p.text))
                        .child(ui::muted12(p, c.t(hint))),
                )
                .child(div().flex_none().child(switch))
        };
        let rows = vec![
            row(
                Tr::TranscriptionSourceSystem,
                Tr::TranscriptionSourceSystemHint,
                toggle("transcription-source-system", sources.system, |t, on| t.system = on),
            ),
            row(
                Tr::TranscriptionSourceMic,
                Tr::TranscriptionSourceMicHint,
                toggle("transcription-source-mic", sources.microphone, |t, on| t.microphone = on),
            ),
        ];
        let mut notes =
            div().px(px(4.)).flex().flex_col().gap(px(4.)).child(ui::muted12(p, c.t(Tr::TranscriptionSourcesRule)));
        if sources.system {
            notes = notes.child(ui::muted12(p, c.t(Tr::TranscriptionPermissionNote)));
        }
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(ui::group(p, c.t(Tr::TranscriptionRecordingGroup), rows))
            .child(notes)
    }

    fn transcript_list(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = &c.p;
        let heading = text(c.t(Tr::TranscriptionTranscripts), 13., FontWeight::BOLD, p.muted);
        let card = if self.list.is_empty() {
            ui::card(p).p(px(24.)).child(text(c.t(Tr::TranscriptionEmpty), 13., FontWeight::NORMAL, p.muted))
        } else {
            let last = self.list.len() - 1;
            ui::card(p).children(self.list.iter().enumerate().map(|(i, row)| self.list_row(c, cx, row, i == last)))
        };
        div().w_full().flex().flex_col().gap(px(10.)).child(heading).child(card)
    }

    fn list_row(&self, c: &Ctx, cx: &mut Context<Self>, row: &TranscriptSummary, last: bool) -> Stateful<Div> {
        let p = &c.p;
        let id = row.id;
        let title = if row.title.trim().is_empty() { c.t(Tr::TranscriptionUntitled) } else { row.title.clone().into() };
        let when = match row.ended_ts {
            Some(end) => text(clock((end - row.started_ts).max(0) as u64 * 1000), 12., FontWeight::NORMAL, p.muted),
            None => text(c.t(Tr::TranscriptionRecording), 12., FontWeight::MEDIUM, p.danger_text),
        };
        let mut el = div()
            .id(("transcript-row", id as usize))
            .w_full()
            .px(px(ROW_PAD_X))
            .py(px(14.))
            .flex()
            .items_center()
            .gap(px(24.))
            .cursor_pointer()
            .hover(|s| s.bg(p.row_idle))
            .on_click(cx.listener(move |page, _, window, cx| page.open_transcript(id, window, cx)))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(text(title, 14., FontWeight::MEDIUM, p.text).truncate())
                    .when(!row.preview.is_empty(), |d| {
                        d.child(text(row.preview.clone(), 13., FontWeight::NORMAL, p.muted).truncate())
                    }),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap(px(4.))
                    .child(ui::muted12(p, started_at(row.started_ts)))
                    .child(when),
            );
        if !last {
            el = el.border_b_1().border_color(p.border);
        }
        el
    }

    fn detail(&self, c: &Ctx, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let p = &c.p;
        let open = self.open.as_ref().expect("detail is shown for an open transcript");
        let recording = open.summary.ended_ts.is_none();
        let delete_label =
            c.t(if self.delete_armed { Tr::TranscriptionDeleteConfirm } else { Tr::TranscriptionDelete });
        let delete = if recording {
            ui::disabled_button(
                "transcription-delete",
                p,
                &delete_label,
                BtnKind::Danger,
                BtnSize::Regular,
                Some(Ic::Trash),
                None,
            )
        } else {
            ui::button(
                "transcription-delete",
                p,
                &delete_label,
                BtnKind::Danger,
                Some(Ic::Trash),
                cbk(cx, |page, _, cx| page.delete_open(cx)),
            )
        };
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.))
            .child(ui::button(
                "transcription-back",
                p,
                &c.t(Tr::TranscriptionBack),
                BtnKind::Subtle,
                Some(Ic::ArrowLeft),
                cbk(cx, |page, _, cx| {
                    page.open = None;
                    page.delete_armed = false;
                    page.scroll.set_offset(point(px(0.), px(0.)));
                    page.reload_list();
                    cx.notify();
                }),
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(ui::copy_button("transcription-copy", p, ui::cbk_copy(cx, |page, _, cx| page.copy_open(cx))))
                    .child(delete),
            );
        let length = open.summary.ended_ts.map(|end| clock((end - open.summary.started_ts).max(0) as u64 * 1000));
        let meta = div()
            .flex()
            .items_center()
            .gap(px(16.))
            .child(ui::muted12(p, started_at(open.summary.started_ts)))
            .children(length.map(|l| ui::muted12(p, l)));
        // The open paragraph lives in the floating box until it closes; only closed paragraphs are rows. The store
        // can be ahead of the status: a paragraph closed after it is still in the floating box, so it waits.
        let live = self.is_live(open);
        let rows: Vec<&ParagraphView> = open
            .paragraphs
            .iter()
            .filter(|v| v.data.state != ParagraphState::Listening)
            .take(if live { self.status.closed } else { usize::MAX })
            .collect();
        let card = if rows.is_empty() {
            let empty = if live { Tr::TranscriptionListening } else { Tr::TranscriptionNothingYet };
            ui::card(p).p(px(24.)).child(text(c.t(empty), 13., FontWeight::NORMAL, p.muted))
        } else {
            let last = rows.len() - 1;
            ui::card(p).children(
                rows.iter()
                    .enumerate()
                    .map(|(i, v)| paragraph_row(c, v, open.summary.sources.both(), i == last, window, cx)),
            )
        };
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(header)
            .child(ui::text_input(p, &open.title, None))
            .child(meta)
            .child(div().pt(px(8.)).child(card))
            // The floating box at its tallest never covers the last row.
            .children(open.reserve.map(|sources| div().flex_none().h(px(float_reserve(sources)))))
    }

    /// The open transcript is the one being recorded.
    fn is_live(&self, open: &OpenTranscript) -> bool {
        self.status.state.transcript_id() == Some(open.summary.id)
    }

    /// The paragraph still being heard, per speaker, pinned to the bottom of the page for the whole session so it
    /// never pops in and out between paragraphs. Everything in it comes from the session's live text, never from
    /// the store.
    fn floating(&mut self, c: &Ctx, cx: &mut Context<Self>) -> Option<Div> {
        if !self.open.as_ref().is_some_and(|open| self.is_live(open)) {
            self.sweep = None;
            return None;
        }
        let p = &c.p;
        let animating =
            self.status.live.iter().any(|l| !l.unsettled.trim().is_empty()) && !ui::Motion::system().is_reduced();
        self.keep_sweeping(animating, cx);
        let labelled = self.open.as_ref()?.summary.sources.both();
        let live = &self.status.live;
        let t = animating.then(|| self.born.elapsed().as_secs_f32());
        let meta = |speaker: Option<Speaker>, start_ms: Option<u64>| {
            let mut meta = div().flex().items_center().gap(px(10.));
            meta = meta.children(start_ms.map(|ms| ui::muted12(p, clock(ms))));
            if let Some(speaker) = speaker.filter(|_| labelled) {
                let color = if speaker == Speaker::Me { p.accent_text } else { p.text };
                meta = meta.child(text(c.t(speaker_label(speaker)), 12., FontWeight::MEDIUM, color));
            }
            meta.child(ui::muted12(p, c.t(Tr::TranscriptionListening)))
        };
        let body = div().w_full().flex().flex_col().gap(px(FLOAT_GAP));
        let body = if live.is_empty() {
            body.child(meta(None, None))
        } else {
            body.children(live.iter().map(|l| {
                // The newest words stay in view: a long paragraph overflows at the top.
                let words = div()
                    .w_full()
                    .max_h(px(FLOAT_LINES as f32 * BODY_LINE))
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .justify_end()
                    .child(unsettled_text(p, &l.settled, &l.unsettled, t));
                div().w_full().flex().flex_col().gap(px(6.)).child(meta(Some(l.speaker), Some(l.start_ms))).child(words)
            }))
        };
        Some(
            div()
                .absolute()
                .left(px(56.))
                .right(px(56.))
                .bottom(px(FLOAT_BOTTOM))
                .px(px(ROW_PAD_X))
                .py(px(FLOAT_PAD_Y))
                .rounded(px(16.))
                .bg(p.card)
                .border_1()
                .border_color(p.border)
                .shadow(vec![BoxShadow {
                    color: p.shadow,
                    offset: point(px(0.), px(10.)),
                    blur_radius: px(28.),
                    spread_radius: px(0.),
                    inset: false,
                }])
                .child(body),
        )
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

/// Settled words, then the words still being heard as a sweeping gradient that ends in an ellipsis. Trailing
/// punctuation of the unsettled words goes before the ellipsis. `t` is the animation clock in seconds; `None` holds
/// the gradient still.
fn unsettled_text(p: &Palette, settled: &str, unsettled: &str, t: Option<f32>) -> Div {
    let mut s = settled.to_string();
    let mut highlights = Vec::new();
    let unsettled = unsettled.trim_end_matches(|ch: char| {
        ch.is_whitespace() || matches!(ch, '。' | '、' | '．' | '，' | '.' | ',' | '！' | '？' | '!' | '?' | '…')
    });
    if !unsettled.trim().is_empty() {
        let from = s.len();
        s.push_str(unsettled);
        s.push('…');
        let faint = ui::mix_rgb(p.muted, p.card, 0.55);
        let count = s[from..].chars().count() as f32;
        // The crest runs past both ends so the sweep fades in and out.
        let crest = t.map_or(-SWEEP_BAND, |t| (t * SWEEP_SPEED) % (count + 2. * SWEEP_BAND) - SWEEP_BAND);
        for (n, (i, ch)) in s[from..].char_indices().enumerate() {
            let lift = (1. - (n as f32 - crest).abs() / SWEEP_BAND).max(0.);
            let color = ui::mix_rgb(faint, p.muted, lift);
            highlights.push((
                from + i..from + i + ch.len_utf8(),
                HighlightStyle { color: Some(color), ..Default::default() },
            ));
        }
    }
    div()
        .w_full()
        .font_family(ui::cjk_family())
        .text_size(px(BODY_SIZE))
        .line_height(px(BODY_LINE))
        .text_color(p.text)
        .child(StyledText::new(s).with_highlights(highlights))
}

/// `labelled` shows who spoke, for a session that recorded both sides.
fn paragraph_row(c: &Ctx, v: &ParagraphView, labelled: bool, last: bool, window: &Window, cx: &App) -> Div {
    let p = &c.p;
    let mut meta = div()
        .w(px(META_COL))
        .flex_none()
        .pt(px(EDITOR_PAD_Y + 2.))
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(ui::muted12(p, clock(v.data.start_ms)));
    if labelled {
        let color = if v.data.speaker == Speaker::Me { p.accent_text } else { p.text };
        meta = meta.child(text(c.t(speaker_label(v.data.speaker)), 12., FontWeight::MEDIUM, color));
    }
    // The state sits in this column, so the text keeps its place when polishing ends. Its line is kept, without
    // ink, once the paragraph is done, so a short row never gets shorter.
    let polishing = v.editor.is_none();
    meta = meta.child(text(
        c.t(Tr::TranscriptionPolishing),
        12.,
        FontWeight::NORMAL,
        if polishing { p.muted } else { ui::transparent() },
    ));
    let body = match &v.editor {
        Some(editor) => {
            let mut column = div().flex().flex_col().gap(px(4.)).child(paragraph_editor(p, editor, window, cx));
            if v.data.error.is_some() {
                column =
                    column.child(div().px(px(EDITOR_PAD_X)).child(ui::muted12(p, c.t(Tr::TranscriptionNotPolished))));
            }
            column
        }
        None => div().child(paragraph_text(v.data.text.clone(), p.muted)),
    };
    let mut row = div()
        .w_full()
        .px(px(ROW_PAD_X))
        .py(px(12.))
        .flex()
        .items_start()
        .gap(px(16.))
        .child(meta)
        .child(div().flex_1().min_w(px(0.)).child(body));
    if !last {
        row = row.border_b_1().border_color(p.border);
    }
    row
}

/// A paragraph's words laid out exactly as the textarea lays them out: the same font, inset and wrap width.
fn paragraph_text(s: impl Into<SharedString>, color: Hsla) -> Div {
    div()
        .w_full()
        .pl(px(EDITOR_PAD_X))
        .pr(px(EDITOR_PAD_X + EDITOR_WRAP_MARGIN))
        .py(px(EDITOR_PAD_Y))
        .font_family(ui::cjk_family())
        .text_size(px(BODY_SIZE))
        .font_weight(FontWeight::NORMAL)
        .line_height(px(BODY_LINE))
        .text_color(color)
        .child(s.into())
}

/// Borderless until focused, so a finished transcript reads as text and is still edited in place. The height comes
/// from the same words laid out as plain text underneath, so it is right on the first frame: the textarea only knows
/// its wrapped rows after it has painted once. The plain text shows while the textarea's own text is hidden, and the
/// other way round while it has focus.
fn paragraph_editor(p: &Palette, editor: &Entity<TextareaState>, window: &Window, cx: &App) -> Div {
    let value = editor.read(cx).value();
    let focused = editor.read(cx).focus_handle(cx).contains_focused(window, cx);
    let hidden = ui::transparent();
    let body = div()
        .relative()
        .w_full()
        .rounded(px(10.))
        .child(paragraph_text(value, if focused { hidden } else { p.text }))
        .child(
            div().absolute().inset_0().child(
                Textarea::new(editor)
                    .appearance(false)
                    .bordered(false)
                    .h_full()
                    .text_size(px(BODY_SIZE))
                    .font_weight(FontWeight::NORMAL)
                    .line_height(px(BODY_LINE))
                    .when(!focused, |t| t.text_color(hidden)),
            ),
        );
    ui::with_ring(
        body,
        ("transcript-paragraph", editor.entity_id().as_u64() as usize),
        focused,
        10.,
        ui::transparent(),
        p.accent,
    )
}
