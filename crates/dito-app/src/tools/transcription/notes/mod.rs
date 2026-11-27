//! The live meeting notes beside the open transcript (`design:tools/recording/live-notes`). This module is the
//! recording page's side of them: whether the panel is open and how far its slide has come, the notes runner that
//! lives while it is open, and the questions asked from it. The notes themselves are written by
//! `dito_core::meeting_notes`; `panel` draws them.

mod ask;
mod panel;
mod reveal;

use std::time::Instant;

use dito_core::meeting_notes::{Cadence, Notes, NotesDeps, NotesHandle, NotesPhase, NotesView, Unavailable};
use gpui_kit::prelude::*;
use gpui_kit::*;

pub(super) use self::ask::Ask;
pub(super) use self::panel::{PANEL_W, view as panel_view};
use self::reveal::{Reveal, compact_for};
use super::TranscriptionPage;
use crate::services;
use crate::tools::chat::Handoff;
use crate::ui;

/// The panel and the transcript trade 340 pt of width plus the 16 pt gap between them.
pub(super) const GAP: f32 = 16.;

/// What the recording page tells the window around it.
#[derive(Clone)]
pub(crate) enum TranscriptionEvent {
    /// A question asked in the panel goes on in Chat.
    OpenInChat(Handoff),
}

pub(super) struct NotesPane {
    /// The user's choice. It ends with the transcript on screen, not with a switch to another page.
    pub open: bool,
    reveal: Reveal,
    /// Transcript and notes take turns, because the page is too narrow for both.
    pub compact: bool,
    meeting: Option<i64>,
    handle: Option<NotesHandle>,
    pub view: NotesView,
    /// The last notes of a meeting, so a panel opened again does not write the folded points again.
    seed: Option<(i64, Notes)>,
    watching: Option<Task<()>>,
    pub scroll: ScrollHandle,
    pub ask: Ask,
    _config: Option<Task<()>>,
}

impl NotesPane {
    pub fn new(ask: Ask) -> Self {
        Self {
            open: false,
            reveal: Reveal::new(),
            compact: false,
            meeting: None,
            handle: None,
            view: NotesView { phase: NotesPhase::Waiting, notes: None, stale: false, seq: 0 },
            seed: None,
            watching: None,
            scroll: ScrollHandle::new(),
            ask,
            _config: None,
        }
    }

    /// How far the panel has come, for the width, and how far it has faded in.
    pub fn sample(&self, now: Instant) -> (f32, f32) {
        self.reveal.sample(now)
    }

    pub fn settled(&self, now: Instant) -> bool {
        self.reveal.settled(now)
    }

    /// Aims the motion at the open or closed panel.
    pub fn retarget(&mut self, motion: ui::Motion, now: Instant) {
        self.reveal.retarget(self.open, motion, now);
    }

    /// Moves to the closed pose without motion, for a panel that closes because its transcript went away.
    fn settle_closed(&mut self, now: Instant) {
        self.reveal.close_now(now);
    }

    /// Follows the width the page has: side by side while 776 pt fit, taking turns below that.
    pub fn follow_width(&mut self, width: Option<f32>) {
        let Some(width) = width else { return };
        self.compact = compact_for(width, self.compact);
    }

    /// Whether anything of the panel is drawn.
    pub fn visible(&self, now: Instant) -> bool {
        self.open || self.sample(now).0 > 0.001 || self.sample(now).1 > 0.001
    }

    /// Whether the panel is out of the way of the keyboard and the pointer.
    pub fn unavailable(&self) -> Option<&Unavailable> {
        match &self.view.phase {
            NotesPhase::Unavailable(why) => Some(why),
            _ => None,
        }
    }
}

impl TranscriptionPage {
    /// The book button: opens or closes the panel. Only the open transcript has notes, and only while it is the one
    /// on screen.
    pub(super) fn toggle_notes(&mut self, cx: &mut Context<Self>) {
        if !self.detail_shown() {
            return;
        }
        self.notes.open = !self.notes.open;
        self.notes.retarget(ui::Motion::system(), Instant::now());
        self.sync_notes(cx);
        cx.notify();
    }

    /// Makes the runner match the panel: one while it is open on a transcript, none otherwise.
    pub(super) fn sync_notes(&mut self, cx: &mut Context<Self>) {
        let wanted = self
            .open
            .as_ref()
            .map(|open| open.summary.id)
            .filter(|_| self.notes.open && self.detail_shown() && self.active);
        if self.notes.meeting == wanted && (wanted.is_none() || self.notes.handle.is_some()) {
            return;
        }
        self.stop_notes();
        if let Some(id) = wanted {
            self.start_notes(id, cx);
        }
    }

    /// The transcript on screen changed or the page left it: a panel for another meeting would be wrong.
    pub(super) fn close_notes_for_other_meeting(&mut self, cx: &mut Context<Self>) {
        let on_screen = self.open.as_ref().map(|open| open.summary.id).filter(|_| self.detail_shown());
        if self.notes.open && on_screen != self.notes.meeting {
            self.notes.open = false;
            self.notes.settle_closed(Instant::now());
        }
        if self.notes.ask.meeting.is_some() && self.notes.ask.meeting != on_screen {
            self.ask_reset();
        }
        self.sync_notes(cx);
    }

    fn start_notes(&mut self, id: i64, cx: &mut Context<Self>) {
        let lang = self.st.cfg.get().general.lang();
        let deps = NotesDeps {
            store: self.st.store.clone(),
            status: self.status_source(),
            source: services::notes_model_source(&self.st),
            me: lang.tr(dito_core::i18n::Tr::TranscriptionMe).to_owned(),
            others: lang.tr(dito_core::i18n::Tr::TranscriptionOthers).to_owned(),
            offset_secs: dito_platform::local_utc_offset_secs(),
            cadence: Cadence::default(),
        };
        let seed = self.notes.seed.take().filter(|(seeded, _)| *seeded == id).map(|(_, notes)| notes);
        let handle = {
            let _runtime = self.st.rt.enter();
            dito_core::meeting_notes::spawn(deps, id, seed)
        };
        self.notes.view = handle.view();
        let mut rx = handle.subscribe();
        self.notes.watching = Some(cx.spawn(async move |this, cx| {
            while rx.changed().await.is_ok() {
                let view = rx.borrow_and_update().clone();
                if this
                    .update(cx, |page, cx| {
                        page.notes.view = view;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
        self.notes.handle = Some(handle);
        self.notes.meeting = Some(id);
        services::meeting_notes_started(&self.st);
        self.watch_chat_model(cx);
    }

    fn stop_notes(&mut self) {
        if self.notes.handle.is_some() {
            services::meeting_notes_stopped(&self.st);
        }
        if let (Some(id), Some(notes)) = (self.notes.meeting, self.notes.view.notes.clone()) {
            self.notes.seed = Some((id, notes));
        }
        self.notes.handle = None;
        self.notes.watching = None;
        self.notes._config = None;
        self.notes.meeting = None;
    }

    /// A changed chat model brings the notes of an open panel that could not start back to life.
    fn watch_chat_model(&mut self, cx: &mut Context<Self>) {
        let mut cfg = self.st.cfg.subscribe();
        self.notes._config = Some(cx.spawn(async move |this, cx| {
            while cfg.changed().await.is_ok() {
                let restart = this.update(cx, |page, cx| {
                    if page.notes.unavailable().is_some() {
                        let id = page.notes.meeting;
                        page.stop_notes();
                        if let Some(id) = id {
                            page.start_notes(id, cx);
                        }
                    }
                    cx.notify();
                });
                if restart.is_err() {
                    break;
                }
            }
        }));
    }

    pub(super) fn open_chat_model_settings(&self, cx: &mut Context<Self>) {
        crate::settings::open_text_model(self.st.clone(), cx);
    }

    /// Keeps the frames coming while the panel moves.
    pub(super) fn keep_notes_moving(&self, window: &mut Window) {
        if !self.notes.settled(Instant::now()) {
            window.request_animation_frame();
        }
    }
}

#[cfg(feature = "screenshots")]
impl TranscriptionPage {
    /// Asks `question` about the meeting on screen, as typing it and pressing Enter does.
    pub fn pose_ask(&mut self, question: &str, window: &mut Window, cx: &mut Context<Self>) {
        let composer = self.notes.ask.composer.clone();
        composer.update(cx, |state, cx| state.set_value(question, window, cx));
        self.ask_send(window, cx);
    }
}
