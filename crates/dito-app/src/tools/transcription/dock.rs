//! The bottom Dock Panel: one continuous object that holds the source toggles, the session's action, the timer and
//! the meeting title, and unfolds upward into the live speaker lanes once a session records.

use std::rc::Rc;
use std::time::{Duration, Instant};

use dito_core::i18n::Tr;
use dito_core::store::{Sources, Speaker};
use dito_core::transcription::SessionState;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{TranscriptionPage, View, clock, open_row, speaker_label};
use crate::assets::Ic;
use crate::services;
use crate::settings::Ctx;
use crate::theme::Palette;
use crate::ui::{self, BtnKind, BtnSize, CircleAppearance, CircleSize, click_cb, text};

/// `design:tools/dock` measurements.
const CAPSULE_W: f32 = 188.;
const CAPSULE_H: f32 = 48.;
const COMPACT_H: f32 = 56.;
const EXPANDED_H: f32 = 186.;
const EXPANDED_W: f32 = 776.;
const DOCK_MS: f32 = 280.;
const TITLE_W: f32 = 482.;

/// The panel's size move, retargeted whenever a session starts, stops or the user browses away and back.
#[derive(Clone, Copy)]
pub(super) struct DockAnim {
    from: (f32, f32),
    to: (f32, f32),
    started: Instant,
    epoch: usize,
}

impl DockAnim {
    pub(super) fn new() -> Self {
        Self { from: (CAPSULE_W, CAPSULE_H), to: (CAPSULE_W, CAPSULE_H), started: Instant::now(), epoch: 0 }
    }

    /// The current interpolation, 0 to 1; the spring adds its small overshoot on top.
    fn progress(&self) -> f32 {
        (self.started.elapsed().as_secs_f32() * 1000. / DOCK_MS).clamp(0., 1.)
    }

    fn at(&self, t: f32, motion: ui::Motion) -> (f32, f32) {
        let k = ui::spring(t, motion);
        (self.from.0 + (self.to.0 - self.from.0) * k, self.from.1 + (self.to.1 - self.from.1) * k)
    }

    /// Continues from wherever the panel is now, so pressing stop mid-unfold reverses smoothly.
    pub(super) fn retarget(&mut self, to: (f32, f32), motion: ui::Motion) {
        if to != self.to {
            self.from = self.at(self.progress(), motion);
            self.to = to;
            self.started = Instant::now();
            self.epoch += 1;
        }
    }

    /// 0 right after a move starts, 1 once it has settled.
    fn ramp(&self) -> f32 {
        (self.progress() * 2.).min(1.)
    }

    fn settled(&self) -> bool {
        self.progress() >= 1.
    }

    /// Jumps to where the move was heading, for a screenshot taken right after a state was posed.
    pub(super) fn settle(&mut self) {
        self.from = self.to;
        self.started = Instant::now() - Duration::from_secs(1);
    }

    /// Parks the hidden panel at rest, so it does not unfold from the size it had when it left.
    pub(super) fn reset(&mut self) {
        self.to = (CAPSULE_W, CAPSULE_H);
        self.settle();
    }

    fn size(&self, motion: ui::Motion) -> (f32, f32) {
        self.at(self.progress(), motion)
    }
}

pub(super) fn view(page: &mut TranscriptionPage, c: &Ctx, window: &Window, cx: &mut Context<TranscriptionPage>) -> Div {
    let p = &c.p;
    let motion = ui::Motion::system();
    let transcript_id = page.status.state.transcript_id();
    let recording = matches!(page.status.state, SessionState::Recording { .. });
    let idle = matches!(page.status.state, SessionState::Idle);
    let session = transcript_id.is_some();
    let expanded = recording
        && page.view == View::Detail
        && page.open.as_ref().is_some_and(|open| Some(open.summary.id) == transcript_id);
    let avail = page.dock_width.get().map_or_else(|| content_width(window), |width| width.min(EXPANDED_W));
    let target = (
        if session { avail } else { CAPSULE_W },
        if expanded {
            EXPANDED_H
        } else if session {
            COMPACT_H
        } else {
            CAPSULE_H
        },
    );
    page.dock.retarget(target, motion);
    let (w, h) = page.dock.size(motion);
    let ramp = page.dock.ramp();
    // The lanes fold into the bar while the panel shrinks, so they stay drawn until the move settles.
    if expanded {
        page.dock_lanes = true;
    } else if page.dock_lanes && page.dock.settled() {
        page.dock_lanes = false;
    }
    let inner = (w - 32.).max(0.);
    let mut panel = div().w(px(w)).h(px(h)).flex().flex_col().justify_end().overflow_hidden();
    if page.dock_lanes && session {
        panel = panel.child(lanes_area(page, c, cx, inner));
    }
    panel = if session {
        panel.child(session_bar(page, c, cx, inner, !expanded, ramp))
    } else {
        panel.child(capsule(page, c, cx, idle))
    };
    let radius = 24. - 6. * ((h - CAPSULE_H) / (EXPANDED_H - CAPSULE_H)).clamp(0., 1.);
    let panel = panel.rounded(px(radius)).bg(p.sub).border_1().border_color(p.border_strong).shadow(vec![BoxShadow {
        color: p.shadow,
        offset: point(px(0.), px(8.)),
        blur_radius: px(24.),
        spread_radius: px(0.),
        inset: false,
    }]);
    let widths = page.dock_width.clone();
    div().relative().w_full().flex_none().flex().justify_center().child(panel).child(
        canvas(
            move |bounds, window, _| {
                let width = f32::from(bounds.size.width);
                if widths.replace(Some(width)) != Some(width) {
                    window.refresh();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0(),
    )
}

/// The page's content width guessed from the window, until the row has painted once: the window minus the sidebar
/// and the two 56 pt gutters.
fn content_width(window: &Window) -> f32 {
    let window_w = f32::from(window.viewport_size().width);
    // The shell collapses its sidebar into the 76 pt rail below 1024 pt.
    let side = if window_w < 1024. { 76. } else { 232. };
    (window_w - side - 112.).clamp(240., EXPANDED_W)
}

/// At rest: the two source toggles and the action, in a capsule that widens around them when a session starts.
fn capsule(page: &TranscriptionPage, c: &Ctx, cx: &mut Context<TranscriptionPage>, idle: bool) -> Div {
    let p = &c.p;
    let toggles = toggles(page, c, idle);
    let action = if idle {
        ui::button(
            "dock-start",
            p,
            &c.t(Tr::TranscriptionStart),
            BtnKind::Primary,
            Some(Ic::Play),
            ui::cbk(cx, |page, _, cx| page.start(cx)),
        )
        .into_any_element()
    } else {
        ui::disabled_button(
            "dock-starting",
            p,
            &c.t(Tr::TranscriptionStarting),
            BtnKind::Primary,
            BtnSize::Regular,
            Some(Ic::Play),
            None,
        )
        .into_any_element()
    };
    div().w_full().flex().items_center().justify_end().gap(px(8.)).px(px(8.)).py(px(6.)).child(toggles).child(action)
}

/// While a session runs: the timer, the meeting title and the actions, with the lanes above them.
fn session_bar(
    page: &TranscriptionPage,
    c: &Ctx,
    cx: &mut Context<TranscriptionPage>,
    inner: f32,
    compact: bool,
    ramp: f32,
) -> Div {
    let p = &c.p;
    let recording = matches!(page.status.state, SessionState::Recording { .. });
    let elapsed = page.session_elapsed_ms().unwrap_or(0);
    let identity = div()
        .id("dock-identity")
        .flex_1()
        .min_w(px(0.))
        .flex()
        .items_center()
        .gap(px(12.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(div().size(px(8.)).rounded_full().bg(p.danger))
                .child(text(clock(elapsed), 13., FontWeight::MEDIUM, p.text).line_height(px(20.))),
        )
        .child(div().w(px(1.)).h(px(16.)).flex_none().bg(p.border_strong));
    let identity =
        if compact { identity.child(compact_identity(page, c)) } else { identity.child(title_field(page, c, inner)) };
    // The bar stays a place to come back to: pressing its identity reopens the live workspace.
    let identity = match (compact, page.status.state.transcript_id()) {
        (true, Some(id)) if recording => identity.cursor_pointer().on_click(open_row(cx, id)),
        _ => identity,
    };
    let action = if recording {
        ui::button(
            "dock-stop",
            p,
            &c.t(Tr::TranscriptionStop),
            BtnKind::Danger,
            Some(Ic::Stop),
            ui::cbk(cx, |page, _, _| page.st.transcription.stop()),
        )
        .into_any_element()
    } else {
        ui::disabled_button(
            "dock-finishing",
            p,
            &c.t(Tr::TranscriptionFinishing),
            BtnKind::Danger,
            BtnSize::Regular,
            Some(Ic::Stop),
            None,
        )
        .into_any_element()
    };
    let mut bar =
        div().w_full().flex().items_center().gap(px(16.)).child(identity.opacity(ramp)).child(
            div().flex_none().flex().items_center().gap(px(8.)).child(toggles(page, c, recording)).child(action),
        );
    bar = if compact { bar.px(px(14.)).py(px(10.)) } else { bar.px(px(16.)).pb(px(12.)) };
    bar
}

/// Browsing the list while a session runs: the meeting title and the last words heard, in one line.
fn compact_identity(page: &TranscriptionPage, c: &Ctx) -> Div {
    let p = &c.p;
    let title: SharedString = page
        .list
        .iter()
        .find(|row| Some(row.id) == page.status.state.transcript_id())
        .map(|row| row.title.clone())
        .filter(|title| !title.trim().is_empty())
        .map(SharedString::from)
        .unwrap_or_else(|| c.t(Tr::TranscriptionUntitled));
    let snippet = last_words(page, c);
    div()
        .flex_1()
        .min_w(px(0.))
        .flex()
        .items_center()
        .gap(px(12.))
        .child(text(title, 14., FontWeight::SEMIBOLD, p.text).truncate())
        .child(div().w(px(1.)).h(px(14.)).flex_none().bg(p.border_strong))
        .child(text(snippet, 13., FontWeight::NORMAL, p.muted).flex_1().min_w(px(0.)).truncate())
}

/// The newest words still in the lanes, prefixed with the speaker, for the single-line bar.
fn last_words(page: &TranscriptionPage, c: &Ctx) -> String {
    let Some(live) = page.status.live.iter().rev().find(|l| !l.settled.is_empty() || !l.unsettled.is_empty()) else {
        return String::new();
    };
    let words = format!("{}{}", live.settled, live.unsettled);
    let words = words.trim();
    let tail: String = {
        let mut chars: Vec<char> = words.chars().collect();
        if chars.len() > 34 {
            chars = chars.split_off(chars.len() - 34);
            format!("…{}", chars.into_iter().collect::<String>())
        } else {
            chars.into_iter().collect()
        }
    };
    format!("{}: {}", c.t(speaker_label(live.speaker)), tail)
}

fn title_field(page: &TranscriptionPage, c: &Ctx, inner: f32) -> AnyElement {
    let Some(open) = &page.open else { return div().into_any_element() };
    let width = if inner >= EXPANDED_W - 36. { TITLE_W } else { (inner - 260.).max(160.) };
    div().w(px(width)).flex_none().child(ui::text_input(&c.p, &open.title, None)).into_any_element()
}

/// The speaker lanes, one per source that records, side by side.
fn lanes_area(page: &mut TranscriptionPage, c: &Ctx, cx: &mut Context<TranscriptionPage>, inner: f32) -> Div {
    let sources = match page.status.state {
        SessionState::Recording { sources, .. } => sources,
        _ => page.open.as_ref().map(|open| open.summary.sources).unwrap_or(Sources { system: true, microphone: false }),
    };
    let mut speakers = Vec::new();
    if sources.system {
        speakers.push(Speaker::Others);
    }
    if sources.microphone {
        speakers.push(Speaker::Me);
    }
    let both = speakers.len() > 1;
    let width = if both { (inner - 12.) / 2. } else { inner };
    let animating =
        page.status.live.iter().any(|l| !l.unsettled.trim().is_empty()) && !ui::Motion::system().is_reduced();
    page.keep_sweeping(animating, cx);
    let t = animating.then(|| page.born.elapsed().as_secs_f32());
    let row = div()
        .w_full()
        .flex()
        .items_start()
        .gap(px(12.))
        .children(speakers.into_iter().map(|speaker| super::lanes::lane(page, c, speaker, width, t)));
    div().w_full().flex_none().pt(px(14.)).px(px(16.)).child(row)
}

fn toggles(page: &TranscriptionPage, c: &Ctx, enabled: bool) -> Div {
    let p = &c.p;
    // A running session records what it was told to, which the user can change while it runs.
    let session = match page.status.state {
        SessionState::Recording { sources, .. } => Some(sources),
        _ => None,
    };
    let sources = session.unwrap_or_else(|| page.st.cfg.get().transcription.sources());
    let live = session.is_some();
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .child(source_toggle(
            page,
            p,
            c.t(Tr::TranscriptionSourceSystemTip),
            Source::System,
            sources.system,
            enabled,
            live,
        ))
        .child(source_toggle(
            page,
            p,
            c.t(Tr::TranscriptionSourceMicTip),
            Source::Mic,
            sources.microphone,
            enabled,
            live,
        ))
}

#[derive(Clone, Copy)]
enum Source {
    System,
    Mic,
}

/// 36 pt circle in the macOS indicator colors: solid green for the Mac's audio, macOS orange for the microphone,
/// a quiet tile with the slashed glyph when a source is off.
fn source_toggle(
    page: &TranscriptionPage,
    p: &Palette,
    label: SharedString,
    source: Source,
    on: bool,
    enabled: bool,
    live: bool,
) -> Stateful<Div> {
    let id = ElementId::from(match source {
        Source::System => "dock-source-system",
        Source::Mic => "dock-source-mic",
    });
    ui::tip_label(id.clone(), label.clone());
    let (bg, border, ink, glyph) = match (source, on) {
        (Source::System, true) => (p.success, None, ui::white(), Ic::Headphones),
        (Source::Mic, true) => (p.orange, None, ui::white(), Ic::Mic),
        (Source::System, false) => (p.tile_idle, Some(p.border_strong), p.muted, Ic::HeadphoneOff),
        (Source::Mic, false) => (p.tile_idle, Some(p.border_strong), p.muted, Ic::MicOff),
    };
    let hover = if on { ui::hex_alpha(ui::white(), 0.16) } else { p.seg };
    let control = ui::circle_control(
        id.clone(),
        glyph,
        CircleSize::Regular,
        CircleAppearance { background: Some(bg), foreground: ink, border, hover: Some(hover) },
    );
    let control = if enabled {
        let st = page.st.clone();
        let toggle = move |_window: &mut Window, _cx: &mut App| {
            let mut next = st.cfg.get().transcription.clone();
            match source {
                Source::System => next.system = !on,
                Source::Mic => next.microphone = !on,
            }
            // A session records at least one source: turning off the last one is refused.
            if !(next.system || next.microphone) {
                return;
            }
            let sources = next.sources();
            services::save_config(&st, move |cfg| cfg.transcription = next);
            if live {
                st.transcription.set_sources(sources);
            }
        };
        control.cursor_pointer().role(Role::Button).aria_label(label).on_click(click_cb(Rc::new(toggle)))
    } else {
        control.opacity(0.45)
    };
    control.child(ui::tip_above(p, &id))
}
