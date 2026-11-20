//! The bottom Dock Panel of `design:tools/recording`: one continuous opaque surface. At rest it is a small tray
//! around the New meeting button, widened on a resumable transcript to hold Continue recording beside it; a session
//! unfolds it upward into the timer bar and the live speaker lanes, each tray button folding into its counterpart
//! (New meeting into stop, Continue recording into pause), and the session's end folds it into the status capsule. The shell's size runs on the two-axis `morph` spring. Every control is placed
//! against the size being drawn, so the stop circle rides the panel's right edge and the other controls spread out
//! from the center without any layout pass of their own.

use std::rc::Rc;
use std::time::{Duration, Instant};

use dito_core::i18n::{Lang, Tr};
use dito_core::store::{Sources, Speaker};
use dito_core::transcription::SessionState;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::morph::{Ease, Fade, Morph, MorphKind, Timing};
use super::{TranscriptionPage, View, open_row, speaker_label};
use crate::assets::Ic;
use crate::services;
use crate::settings::Ctx;
use crate::theme::Palette;
use crate::ui::{self, Cb, CircleAppearance, CircleSize, click_cb, lerp, text};

/// `design:tools/recording` measurements: the 50 pt tray holding its 36 pt buttons 7 pt inside (the inset of the
/// recording controls), the 44 pt status capsule, the 62 pt one-line session bar, and the 197 pt expanded dock
/// (15 lanes pad + 121 lane card + 12 gap + 36 controls + 12 pad, border inside).
const TRAY_H: f32 = 50.;
const TRAY_INSET: f32 = 7.;
const CHIP_H: f32 = 36.;
/// The two tray buttons of a resumable transcript stand this far apart.
const CHIP_GAP: f32 = 8.;
const STATUS_H: f32 = 44.;
const COMPACT_H: f32 = 62.;
const EXPANDED_H: f32 = 197.;
const EXPANDED_W: f32 = 776.;
const PANEL_RADIUS: f32 = 18.;
const LANE_GAP: f32 = 12.;
const LANE_INSET: f32 = 17.;
const LANE_TOP: f32 = 15.;
/// The expanded dock below one lane card: the gap, the controls row, the bottom pad and the border.
const LANE_BELOW: f32 = 61.;
const CONTROL_H: f32 = 36.;
/// The controls keep this gap at every size, and the title field ends this far before them.
const CONTROL_GAP: f32 = 8.;
/// Every circle carries the 20 pt glyph of the 36 pt Icon Button contract; the labelled confirm pill takes the
/// Regular Button's 16 pt icon.
const CIRCLE_GLYPH: f32 = 20.;
const PILL_GLYPH: f32 = 16.;
/// The identity and the controls sit this far inside the panel's border, left and right.
const ROW_INSET: f32 = 16.;
const SHELL_BORDER: f32 = 2.;
/// A tray button is the Regular Button: 16 pt sides, a 16 pt leading icon and an 8 pt gap before the label.
const CHIP_PAD: f32 = 16.;
const CHIP_ICON: f32 = 16.;
const CHIP_ICON_GAP: f32 = 8.;
/// Releasing New meeting on the lone tray opens it this much wider around the button before the button folds.
const TRAY_OPEN: f32 = 31.;
/// The status capsule's label sides.
const STATUS_PAD: f32 = 20.;
const LABEL_SIZE: f32 = 13.;
/// The clock has no border beside it, so it runs a step larger than the title, in tabular figures.
const CLOCK_SIZE: f32 = 16.;
const CLOCK_LINE: f32 = 22.;
/// The clock column holds the widest clock it can show plus this slack.
const CLOCK_PAD: f32 = 4.;
/// The held-start error line above the capsule: one small line with air around it.
const ERROR_H: f32 = 24.;
/// Pressing flexes the lone tray, or the pressed one of two tray buttons, down to this share of its size.
const PRESS_SCALE: f32 = 0.97;
/// Above this drawn height the dock is a session shape, so crossing it expands or collapses.
const TALL_H: f32 = TRAY_H + 1.;
/// The lanes and the identity materialize once the morph is this far through.
const SHOW_AT: f32 = 0.4;
/// A start the session answers within this long folds the button straight into the controls; a slower one shows
/// the `TranscriptionStarting` status capsule first, so a quick start never flashes it.
const PREPARE_GRACE: Duration = Duration::from_millis(250);
/// Continue recording materializes beside New meeting once the tray has begun to widen.
const RESUME_IN: Timing = Timing { duration_ms: 180., delay_ms: 60., curve: Ease::Out };
/// A tray button narrows into its circle while its label is gone almost at once, before the width could squeeze
/// it.
const FOLD: Timing = Timing { duration_ms: 240., delay_ms: 0., curve: Ease::Out };
const LABEL_OUT: Timing = Timing { duration_ms: 45., delay_ms: 0., curve: Ease::Out };
/// Each control grows out of the center on its own ease; stop leads and the others follow 30 ms apart.
const SPREAD_MS: f32 = 300.;
const SPREAD_STAGGER_MS: f32 = 30.;
const SPREAD_FROM: f32 = 0.35;
/// The armed stop pill widens and folds back on these.
const ARM_IN: Timing = Timing { duration_ms: 240., delay_ms: 0., curve: Ease::Out };
const ARM_OUT: Timing = Timing { duration_ms: 200., delay_ms: 0., curve: Ease::InOut };
const HALO_IN: Timing = Timing { duration_ms: 300., delay_ms: 0., curve: Ease::Out };
const HALO_W: f32 = 2.;
/// The armed pill folds back after this long without a second press.
const ARM_WAIT: Duration = Duration::from_millis(4000);
/// While armed, one light band crosses the pill in 420 ms, once every 1600 ms.
const BAND_MS: f32 = 420.;
const BAND_EVERY_MS: f32 = 1600.;
/// The live entry fills in and the review strip returns on the morph spring's own duration.
const MOVE_MS: f32 = 460.;
const ENTRY_TIMING: Timing = Timing { duration_ms: MOVE_MS, delay_ms: 0., curve: Ease::Out };
const REVIEW_TIMING: Timing = Timing { duration_ms: MOVE_MS, delay_ms: 0., curve: Ease::InOut };

/// The recording controls in their left-to-right order.
const AUDIO: usize = 0;
const MIC: usize = 1;
const PAUSE: usize = 2;
const STOP: usize = 3;

/// What the detail view samples: how far the live entry has filled in, and how far the review strip has returned.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct Pose {
    pub(super) entry: f32,
    pub(super) review: f32,
}

/// What the dock is showing, which decides its shape and its content.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Look {
    /// The New meeting button, which is the whole capsule.
    Button,
    /// The status capsule: a label alone on the seg surface.
    Status(StatusKind),
    /// The session bar or panel with its controls.
    Recording,
}

/// The tray's two buttons.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Chip {
    /// New meeting, which folds into the stop circle.
    Start,
    /// Continue recording, which folds into the pause circle.
    Resume,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StatusKind {
    Preparing,
    Finishing,
}

/// Label measurements, taken once per language instead of on every frame.
#[derive(Clone, Copy)]
struct Widths {
    start: f32,
    resume: f32,
    preparing: f32,
    finishing: f32,
    confirm: f32,
    clock: f32,
}

impl Widths {
    fn measure(c: &Ctx) -> Self {
        let label = |tr: Tr| ui::text_width(&c.t(tr), LABEL_SIZE, FontWeight::MEDIUM).ceil();
        let clock = ui::text_width("88:88:88", CLOCK_SIZE, FontWeight::MEDIUM)
            .max(ui::text_width("88:88", CLOCK_SIZE, FontWeight::MEDIUM))
            .ceil()
            + CLOCK_PAD;
        Self {
            start: chip_width(label(Tr::TranscriptionStart)),
            resume: chip_width(label(Tr::TranscriptionContinue)),
            preparing: 2. * STATUS_PAD + label(Tr::TranscriptionStarting) + SHELL_BORDER,
            finishing: 2. * STATUS_PAD + label(Tr::TranscriptionFinishing) + SHELL_BORDER,
            // The Danger button's 16 pt sides around the square, its 8 pt gap and the label.
            confirm: 2. * 16. + PILL_GLYPH + 8. + label(Tr::TranscriptionStopConfirm),
            clock,
        }
    }

    /// The tray around New meeting alone, or around both buttons.
    fn tray(self, pair: bool) -> f32 {
        2. * TRAY_INSET + self.start + if pair { CHIP_GAP + self.resume } else { 0. }
    }

    fn status(self, kind: StatusKind) -> f32 {
        match kind {
            StatusKind::Preparing => self.preparing,
            StatusKind::Finishing => self.finishing,
        }
    }
}

fn chip_width(label: f32) -> f32 {
    2. * CHIP_PAD + CHIP_ICON + CHIP_ICON_GAP + label
}

/// The dock's motion: the shell's two-axis morph plus the fades that bring each piece of content in and out.
pub(super) struct DockAnim {
    morph: Morph,
    look: Look,
    /// The status label being shown, kept while it fades out.
    status_kind: StatusKind,
    /// How far the surface has turned from the panel's sub to the status capsule's seg.
    statusness: Fade,
    /// New meeting in the tray when it fades rather than folds.
    start: Fade,
    /// Continue recording beside it while the open transcript is resumable.
    resume: Fade,
    /// The tray buttons narrowing into their circles, with their labels leaving first.
    fold: Fade,
    label: Fade,
    folding: bool,
    /// The fold began from the two-button tray, so Continue recording folds into pause beside stop.
    fold_pair: bool,
    spread: [Fade; 4],
    controls: Fade,
    status: Fade,
    lanes: Fade,
    identity: Fade,
    entry: Fade,
    review: Fade,
    /// The button's hover shadow: its shape is fixed, so hover and press deepen only this.
    deepen: Fade,
    /// The stop circle widening into the confirm pill, and the halo around it.
    arm: Fade,
    halo: Fade,
    transcript: Option<i64>,
    /// The tray button held down: on the lone tray the whole tray flexes, beside its neighbour only the button.
    pressed: Option<Chip>,
    /// When New meeting was pressed: the `TranscriptionStarting` grace runs from here.
    start_at: Option<Instant>,
    /// The redraw at the moment the open transcript stops being resumable, so the tray narrows without input.
    resume_wake: Option<(i64, Task<()>)>,
    /// When the current Finishing began: the shimmer clock of the status label.
    finishing_since: Option<Instant>,
    stop_armed: bool,
    armed_at: Instant,
    /// The redraw waiting for the armed pill's next band, so the pauses between bands cost no frames.
    band_wake: Option<(Instant, Task<()>)>,
    widths: Option<(Lang, Widths)>,
    /// Whether the identity shows the browsing line rather than the title field; held while it fades out, so the
    /// collapse never swaps one for the other.
    identity_compact: bool,
}

impl DockAnim {
    pub(super) fn new() -> Self {
        Self {
            morph: Morph::new(140., TRAY_H),
            look: Look::Button,
            status_kind: StatusKind::Preparing,
            statusness: Fade::new(0.),
            start: Fade::new(1.),
            resume: Fade::new(0.),
            fold: Fade::new(0.),
            label: Fade::new(1.),
            folding: false,
            fold_pair: false,
            spread: std::array::from_fn(|_| Fade::new(1.)),
            controls: Fade::new(0.),
            status: Fade::new(0.),
            lanes: Fade::new(0.),
            identity: Fade::new(0.),
            entry: Fade::new(1.),
            review: Fade::new(1.),
            deepen: Fade::new(0.),
            arm: Fade::new(0.),
            halo: Fade::new(0.),
            transcript: None,
            pressed: None,
            start_at: None,
            resume_wake: None,
            finishing_since: None,
            stop_armed: false,
            armed_at: Instant::now(),
            band_wake: None,
            widths: None,
            identity_compact: true,
        }
    }

    /// Jumps every motion to its target, for a screenshot taken right after a state was posed.
    pub(super) fn settle(&mut self) {
        self.morph.settle();
        for fade in [
            &mut self.statusness,
            &mut self.start,
            &mut self.resume,
            &mut self.fold,
            &mut self.label,
            &mut self.controls,
            &mut self.status,
            &mut self.lanes,
            &mut self.identity,
            &mut self.entry,
            &mut self.review,
            &mut self.deepen,
            &mut self.arm,
            &mut self.halo,
        ] {
            fade.settle();
        }
        for fade in &mut self.spread {
            fade.settle();
        }
        self.folding = false;
    }

    fn widths(&mut self, c: &Ctx) -> Widths {
        match self.widths {
            Some((lang, widths)) if lang == c.lang => widths,
            _ => {
                let widths = Widths::measure(c);
                self.widths = Some((c.lang, widths));
                widths
            }
        }
    }

    fn settled(&self, now: Instant) -> bool {
        self.morph.settled(now)
            && [
                &self.statusness,
                &self.start,
                &self.resume,
                &self.fold,
                &self.label,
                &self.controls,
                &self.status,
                &self.lanes,
                &self.identity,
                &self.entry,
                &self.review,
                &self.deepen,
                &self.arm,
                &self.halo,
            ]
            .iter()
            .all(|fade| fade.settled(now))
            && self.spread.iter().all(|fade| fade.settled(now))
    }

    /// Starts the content changes of a new look; the shell's size follows from the look's target.
    fn enter(&mut self, look: Look, drawn_h: f32, motion: ui::Motion, now: Instant) {
        let from = self.look;
        self.look = look;
        let reduced = motion.is_reduced();
        match look {
            Look::Recording => {
                self.folding = from == Look::Button && !reduced;
                // The fold takes over Continue recording, whatever its fade was doing.
                self.fold_pair = self.folding && self.resume.target() == 1.;
                if self.fold_pair {
                    self.resume = Fade::new(0.);
                } else {
                    self.resume.retarget(0., Timing::materialize_out(motion), now);
                }
                if self.folding {
                    self.fold = Fade::new(0.);
                    self.fold.retarget(1., FOLD, now);
                    self.label = Fade::new(1.);
                    self.label.retarget(0., LABEL_OUT, now);
                    self.start = Fade::new(0.);
                } else {
                    self.fold = Fade::new(1.);
                    self.start.retarget(0., Timing::materialize_out(motion), now);
                }
                let pair = self.fold_pair;
                for (index, fade) in self.spread.iter_mut().enumerate() {
                    *fade = Fade::new(0.);
                    let timing = if reduced {
                        Timing::materialize_in(motion, 0.)
                    } else {
                        Timing { duration_ms: SPREAD_MS, delay_ms: spread_delay(index, pair), curve: Ease::Out }
                    };
                    fade.retarget(1., timing, now);
                }
                self.controls = Fade::new(1.);
            }
            Look::Status(kind) => {
                self.status_kind = kind;
                self.folding = false;
                self.start.retarget(0., Timing::materialize_out(motion), now);
                self.resume.retarget(0., Timing::materialize_out(motion), now);
                self.controls.retarget(0., Timing::materialize_out(motion), now);
                // From a session shape the label waits for the collapse to be underway.
                let delay = if drawn_h > TALL_H { 240. } else { 0. };
                self.status = Fade::new(self.status.sample(now));
                self.status.retarget(1., Timing::materialize_in(motion, delay), now);
            }
            Look::Button => {
                self.folding = false;
                self.fold = Fade::new(0.);
                self.label = Fade::new(1.);
                self.start.retarget(1., Timing::materialize_in(motion, 0.), now);
                self.controls.retarget(0., Timing::materialize_out(motion), now);
            }
        }
        if !matches!(look, Look::Status(_)) {
            self.status.retarget(0., Timing::materialize_out(motion), now);
        }
        if look != Look::Recording {
            self.disarm(motion, now);
        }
    }

    fn arm(&mut self, motion: ui::Motion, now: Instant) {
        self.stop_armed = true;
        self.armed_at = now;
        self.arm.retarget(1., if motion.is_reduced() { ARM_OUT } else { ARM_IN }, now);
        self.halo.retarget(1., if motion.is_reduced() { Timing::materialize_in(motion, 0.) } else { HALO_IN }, now);
    }

    /// Folds the confirm pill back into the stop circle; true when it was armed.
    pub(super) fn disarm(&mut self, motion: ui::Motion, now: Instant) -> bool {
        if !self.stop_armed {
            return false;
        }
        self.stop_armed = false;
        self.arm.retarget(0., ARM_OUT, now);
        self.halo.retarget(0., Timing::materialize_out(motion), now);
        self.band_wake = None;
        true
    }
}

/// Stop leads; pause, mic and audio follow it out of the center 30 ms apart. Folding from the two-button tray, pause
/// rides beside stop from the start and only mic and audio follow.
fn spread_delay(index: usize, pair: bool) -> f32 {
    let order: f32 = match index {
        STOP => 0.,
        PAUSE => 1.,
        MIC => 2.,
        _ => 3.,
    };
    SPREAD_STAGGER_MS * if pair { (order - 1.).max(0.) } else { order }
}

fn faded(color: Hsla, opacity: f32) -> Hsla {
    Hsla { a: color.a * opacity, ..color }
}

/// Where the controls row stands inside a drawn shell: its top, and the center of each control. The row is
/// right-aligned with fixed gaps, so a wider stop pushes the others left and the title field gives up the width.
struct Row {
    top: f32,
    centers: [f32; 4],
    stop_w: f32,
}

impl Row {
    fn at(w: f32, h: f32, stop_w: f32) -> Self {
        let bottom = ((h - CONTROL_H - SHELL_BORDER) / 2.).min(12.);
        let right = w - SHELL_BORDER / 2. - ROW_INSET;
        let stop = right - stop_w / 2.;
        let pause = right - stop_w - CONTROL_GAP - CONTROL_H / 2.;
        let mic = pause - CONTROL_H - CONTROL_GAP;
        let audio = mic - CONTROL_H - CONTROL_GAP;
        Self { top: h - SHELL_BORDER / 2. - bottom - CONTROL_H, centers: [audio, mic, pause, stop], stop_w }
    }

    fn left(&self) -> f32 {
        self.centers[AUDIO] - CONTROL_H / 2.
    }
}

pub(super) fn view(
    page: &mut TranscriptionPage,
    c: &Ctx,
    window: &Window,
    cx: &mut Context<TranscriptionPage>,
) -> (Div, Pose, f32) {
    let p = &c.p;
    let motion = ui::Motion::system();
    let reduced = motion.is_reduced();
    let now = Instant::now();
    let first_frame = page.dock.widths.is_none();
    let widths = page.dock.widths(c);
    let transcript_id = page.status.state.transcript_id();
    let session = transcript_id.is_some();
    let recording = matches!(page.status.state, SessionState::Recording { .. });
    if page.dock.stop_armed && (!recording || now.duration_since(page.dock.armed_at) >= ARM_WAIT) {
        page.dock.disarm(motion, now);
    }
    let look = match page.status.state {
        SessionState::Idle => Look::Button,
        SessionState::Starting if page.dock.start_at.is_some_and(|at| now.duration_since(at) < PREPARE_GRACE) => {
            Look::Button
        }
        SessionState::Starting => Look::Status(StatusKind::Preparing),
        SessionState::Recording { .. } => Look::Recording,
        SessionState::Finishing { .. } => Look::Status(StatusKind::Finishing),
    };
    if look != Look::Button {
        page.dock.start_at = None;
        page.dock.pressed = None;
    }
    let (_, drawn_h) = page.dock.morph.sample(now);
    if look != page.dock.look {
        page.dock.enter(look, drawn_h, motion, now);
    }
    let expanded_sources = match page.status.state {
        SessionState::Recording { id, sources, .. }
            if page.view == View::Detail && page.open.as_ref().is_some_and(|open| open.summary.id == id) =>
        {
            Some(sources)
        }
        _ => None,
    };
    let expanded = expanded_sources.is_some();
    if look == Look::Recording {
        page.dock.identity_compact = !expanded;
    }
    let avail = page.dock_width.get().map_or_else(|| content_width(window), |width| width.min(EXPANDED_W));
    // A resumable transcript widens the tray to hold Continue recording beside New meeting.
    let pair = look == Look::Button && page.open_resumable();
    page.dock.resume.retarget(
        f32::from(pair),
        match (pair, reduced) {
            (true, false) => RESUME_IN,
            (true, true) => Timing::materialize_in(motion, 0.),
            (false, _) => Timing::materialize_out(motion),
        },
        now,
    );
    schedule_resume_wake(page, pair, cx);
    let hover_id = ElementId::from("dock-capsule");
    if look != Look::Button || pair {
        ui::reset_hover(&hover_id);
    }
    let chip_ids = [Chip::Start, Chip::Resume].map(chip_id);
    if !pair {
        for id in &chip_ids {
            ui::reset_hover(id);
        }
    }
    let hovered =
        look == Look::Button && if pair { chip_ids.iter().any(ui::is_hovered) } else { ui::is_hovered(&hover_id) };
    let tray_pressed = page.dock.pressed.is_some() && !pair && !reduced;
    if transcript_id.is_some() && transcript_id != page.dock.transcript {
        page.dock.transcript = transcript_id;
        // A new session's entry starts from empty, whatever it was animating toward; a resumed one already holds
        // its earlier paragraphs.
        let resumed = page
            .open
            .as_ref()
            .is_some_and(|open| Some(open.summary.id) == transcript_id && !open.paragraphs.is_empty());
        if !resumed {
            page.dock.entry.retarget(0., Timing::materialize_out(motion), now);
            page.dock.entry.settle();
        }
    }
    let follows_session = page.view == View::Detail
        && page.open.as_ref().is_some_and(|open| Some(open.summary.id) == page.dock.transcript);
    page.dock.entry.retarget(f32::from(!session || follows_session), ENTRY_TIMING, now);
    if recording {
        // Stop takes the review strip away at once.
        page.dock.review.retarget(0., Timing::materialize_out(motion), now);
        page.dock.review.settle();
    } else {
        page.dock.review.retarget(1., REVIEW_TIMING, now);
    }
    let tray_w = widths.tray(pair);
    // Released on the lone tray, the tray opens around New meeting before the button folds; the two-button tray is
    // already open.
    let opening = page.dock.start_at.is_some() && !pair && !reduced;
    let (target_w, target_h) = match look {
        Look::Button if tray_pressed => (tray_w * PRESS_SCALE, TRAY_H * PRESS_SCALE),
        Look::Button if opening => (tray_w + TRAY_OPEN, TRAY_H),
        Look::Button => (tray_w, TRAY_H),
        Look::Status(kind) => (widths.status(kind), STATUS_H),
        Look::Recording if expanded => (avail, EXPANDED_H),
        Look::Recording => (avail, COMPACT_H),
    };
    // A morph that is about to start dismisses whatever tip is showing; tips return once everything settles.
    if (target_w, target_h) != page.dock.morph.target() && page.dock.morph.settled(now) {
        ui::dismiss_tips(&page.motion.handle());
    }
    let kind = if tray_pressed && look == Look::Button {
        MorphKind::Press
    } else if drawn_h > TALL_H && target_h < drawn_h {
        MorphKind::Collapse
    } else if target_h > drawn_h && target_h > TALL_H {
        MorphKind::Expand
    } else {
        MorphKind::Capsule
    };
    page.dock.morph.retarget(target_w, target_h, kind, motion, now);
    if first_frame {
        // The capsule opens at its measured size instead of springing there from a guess.
        page.dock.morph.settle();
    }
    let deep = hovered || page.dock.pressed.is_some();
    page.dock.deepen.retarget(f32::from(deep), fade_timing(deep, motion), now);
    let status_look = matches!(look, Look::Status(_));
    // Collapsing from a session shape, the surface turns to the status gray once the content has dissolved.
    let status_timing = if status_look {
        Timing::materialize_in(motion, if drawn_h > TALL_H { 120. } else { 0. })
    } else {
        Timing::materialize_out(motion)
    };
    page.dock.statusness.retarget(f32::from(status_look), status_timing, now);
    let (w, h) = page.dock.morph.sample(now);
    let rest_w = widths.tray(false);
    let travel = ((w - rest_w) / (avail - rest_w).max(1.)).clamp(0., 1.);
    // The lanes materialize once the panel has risen 40 percent of the way from the capsule.
    let lanes_shown = expanded && (h >= TRAY_H + SHOW_AT * (EXPANDED_H - TRAY_H) || page.dock.lanes.target() == 1.);
    page.dock.lanes.retarget(f32::from(lanes_shown), fade_timing(lanes_shown, motion), now);
    // The identity materializes once the panel has widened 40 percent of the way to the bar.
    let identity_shown = look == Look::Recording && (travel >= SHOW_AT || page.dock.identity.target() == 1.);
    page.dock.identity.retarget(f32::from(identity_shown), fade_timing(identity_shown, motion), now);
    if look == Look::Status(StatusKind::Finishing) {
        page.dock.finishing_since.get_or_insert(now);
    } else {
        page.dock.finishing_since = None;
    }
    if let Some(sources) = expanded_sources {
        page.dock_lanes = Some(sources);
    } else if page.dock.morph.settled(now) {
        page.dock_lanes = None;
    }
    let lanes_fade = page.dock.lanes.sample(now);
    let identity_fade = page.dock.identity.sample(now);
    let controls_fade = page.dock.controls.sample(now);
    let status_fade = page.dock.status.sample(now);
    let entry = page.dock.entry.sample(now);
    let review = page.dock.review.sample(now);
    let cards = page
        .dock_lanes
        .filter(|_| lanes_fade > 0.01)
        .map(|sources| lane_cards(sources, w, h, avail, lanes_fade, motion));
    // The sweep timer runs while the lanes show unsettled words or the status label sweeps, and nothing else.
    let lanes_sweep = !reduced
        && cards.as_ref().is_some_and(|g| g.body_opacity > 0.)
        && page.status.live.iter().any(|l| !l.unsettled.trim().is_empty());
    let finishing_shown = page.dock.status_kind == StatusKind::Finishing && status_fade > 0.;
    let status_sweep = !reduced && finishing_shown;
    // The lanes' listening label breathes while a session records; the same timer drives it.
    let pulse = !reduced && recording && !page.is_paused() && cards.as_ref().is_some_and(|g| g.show_status);
    page.keep_sweeping(lanes_sweep || status_sweep || pulse, cx);
    let sweep_t = lanes_sweep.then(|| now.duration_since(page.born).as_secs_f32());
    if page.dock.folding && page.dock.fold.settled(now) && page.dock.spread[STOP].settled(now) {
        page.dock.folding = false;
    }
    let band = band_phase(&page.dock, reduced, now);
    if band.is_none() && page.dock.stop_armed && !reduced {
        schedule_band(&mut page.dock, now, cx);
    }
    if !page.dock.settled(now) || band.is_some() {
        window.request_animation_frame();
    }

    let radius = lerp(h.min(TRAY_H) / 2., PANEL_RADIUS, ((h - TRAY_H) / (COMPACT_H - TRAY_H)).clamp(0., 1.));
    let deepen = page.dock.deepen.sample(now);
    // Gold is the strongest color on the page, so a shape coming back to the tray shows its buttons only in the
    // last stretch of the move, when it is nearly tray-sized.
    let near = nearness((w, h), page.dock.morph.target());
    let start_fade = page.dock.start.sample(now) * near;
    let resume_fade = page.dock.resume.sample(now) * near;
    let surface_color = ui::mix_rgb(p.sub, p.seg, page.dock.statusness.sample(now));
    let mut shell = div().relative().w(px(w)).h(px(h)).flex_none().child(
        div()
            .absolute()
            .inset_0()
            .rounded(px(radius))
            .bg(surface_color)
            .border_1()
            .border_color(p.border_strong)
            .shadow(vec![shadow_for(h, deepen, p)]),
    );
    if let Some((sources, cards)) = page.dock_lanes.zip(cards) {
        shell = shell.child(lanes_area(page, c, sources, cards, sweep_t));
    }
    if start_fade > 0.01 || resume_fade > 0.01 {
        // The buttons keep their size: New meeting stays centered while the lone tray opens or flexes, and slides
        // right as the tray widens for Continue recording, which stands 8 pt to its left.
        let pair_w = widths.tray(true);
        let pair_t = if look == Look::Button && (pair || page.dock.start_at.is_none()) {
            ((w - rest_w) / (pair_w - rest_w).max(1.)).clamp(0., 1.)
        } else {
            0.
        };
        let start_center = w / 2. + pair_t * (widths.resume + CHIP_GAP) / 2.;
        let top = (h - CHIP_H) / 2.;
        if resume_fade > 0.01 {
            let center = start_center - (widths.start + widths.resume) / 2. - CHIP_GAP;
            shell = shell.child(
                tray_chip(page, c, cx, Chip::Resume, widths.resume, pair)
                    .absolute()
                    .left(px(center - widths.resume / 2.))
                    .top(px(top + rise(resume_fade, motion)))
                    .opacity(resume_fade),
            );
        }
        if start_fade > 0.01 {
            let chip = if pair {
                tray_chip(page, c, cx, Chip::Start, widths.start, true).into_any_element()
            } else {
                start_face(c).into_any_element()
            };
            shell = shell.child(
                div()
                    .absolute()
                    .left(px(start_center - widths.start / 2.))
                    .top(px(top))
                    .opacity(start_fade)
                    .child(chip),
            );
        }
    }
    if status_fade > 0.01 {
        shell = shell.child(
            div()
                .absolute()
                .inset_0()
                .top(px(rise(status_fade, motion)))
                .flex()
                .items_center()
                .justify_center()
                .opacity(status_fade)
                .child(status_label(page, c, now, motion)),
        );
    }
    let arm = page.dock.arm.sample(now);
    let row = Row::at(w, h, lerp(CONTROL_H, widths.confirm, arm));
    if identity_fade > 0.01 {
        // The field ends before the audio circle wherever it is drawn, so it widens as the controls move right
        // instead of sliding under them.
        let identity_w =
            spread_left(&page.dock, &row, w, now).min(row.left()) - CONTROL_GAP - SHELL_BORDER / 2. - ROW_INSET;
        if identity_w > 0. {
            shell = shell.child(
                div()
                    .absolute()
                    .top(px(row.top + rise(identity_fade, motion)))
                    .left(px(SHELL_BORDER / 2. + ROW_INSET))
                    .w(px(identity_w))
                    .h(px(CONTROL_H))
                    .flex()
                    .items_center()
                    .opacity(identity_fade)
                    .child(session_identity(page, c, cx, page.dock.identity_compact, widths.clock)),
            );
        }
    }
    if controls_fade > 0.01 {
        shell = shell.child(controls(page, c, cx, &row, w, controls_fade, band, now));
    }
    if page.dock.folding {
        let offset = if page.dock.fold_pair { (widths.resume + CHIP_GAP) / 2. } else { 0. };
        shell = shell.child(fold_chip(page, c, &row, w, Chip::Start, widths.start, offset, now));
        if page.dock.fold_pair {
            shell = shell.child(fold_chip(
                page,
                c,
                &row,
                w,
                Chip::Resume,
                widths.resume,
                -(widths.start + CHIP_GAP) / 2.,
                now,
            ));
        }
    }
    let shell = if look == Look::Button && !pair && page.dock.start.target() == 1. {
        start_hit_area(shell, &hover_id, c, cx)
    } else {
        shell.into_any_element()
    };
    // A start that was held leaves its reason where someone watching only the capsule looks: on a line above it.
    let held = !session && page.status.error.as_deref().is_some_and(|error| page.dismissed.as_deref() != Some(error));
    let widths_cell = page.dock_width.clone();
    let slot = div()
        .relative()
        .w_full()
        .h(px(h + f32::from(held) * ERROR_H))
        .flex_none()
        .children(held.then(|| {
            div()
                .absolute()
                .left_0()
                .right_0()
                .top_0()
                .h(px(ERROR_H))
                .flex()
                .items_center()
                .gap(px(6.))
                .px(px(12.))
                .child(
                    div().flex_1().min_w(px(0.)).flex().items_center().justify_center().child(
                        text(
                            ui::error_line(c.lang, page.status.error.as_deref().unwrap_or_default()),
                            13.,
                            FontWeight::NORMAL,
                            p.muted,
                        )
                        .truncate(),
                    ),
                )
                .child(ui::close_button(
                    "dock-error-close",
                    p,
                    ui::cbk(cx, |page, _, cx| {
                        page.dismissed = page.status.error.clone();
                        cx.notify();
                    }),
                ))
        }))
        .child(
            div().absolute().left_0().right_0().bottom_0().h(px(h)).flex().items_start().justify_center().child(shell),
        )
        .child(
            canvas(
                move |bounds, window, _| {
                    let width = f32::from(bounds.size.width);
                    if widths_cell.replace(Some(width)) != Some(width) {
                        window.refresh();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        );
    let pose = Pose { entry, review };
    let detail = if follows_session { pose } else { Pose { entry: 1., review: 1. } };
    // The slot height tells a list beneath the floating panel how much room to leave at its end.
    (slot, detail, h + f32::from(held) * ERROR_H)
}

/// New meeting on the lone tray: the Primary Regular button drawn as a plain face, because the whole tray is its hit
/// area and hover deepens only the tray's shadow.
fn start_face(c: &Ctx) -> Div {
    let p = &c.p;
    div()
        .h(px(CHIP_H))
        .px(px(CHIP_PAD))
        .flex()
        .items_center()
        .gap(px(CHIP_ICON_GAP))
        .rounded_full()
        .bg(p.primary)
        .child(ui::icon(Ic::Plus, CHIP_ICON, p.on_primary))
        .child(text(c.t(Tr::TranscriptionStart), LABEL_SIZE, FontWeight::MEDIUM, p.on_primary).whitespace_nowrap())
}

fn chip_id(chip: Chip) -> ElementId {
    ElementId::from(match chip {
        Chip::Start => "dock-chip-start",
        Chip::Resume => "dock-chip-resume",
    })
}

/// One button of the two-button tray: the shared Regular Button, its own target with its own hover tint. Pressing
/// flexes only this button, about its center, so its neighbour never shifts.
fn tray_chip(
    page: &TranscriptionPage,
    c: &Ctx,
    cx: &mut Context<TranscriptionPage>,
    chip: Chip,
    width: f32,
    live: bool,
) -> Div {
    let (label, kind, icon) = match chip {
        Chip::Start => (Tr::TranscriptionStart, ui::BtnKind::Primary, Ic::Plus),
        Chip::Resume => (Tr::TranscriptionContinue, ui::BtnKind::Secondary, Ic::Play),
    };
    let on_click: Cb = if live { ui::cbk(cx, move |page, _, cx| launch(page, chip, cx)) } else { Rc::new(|_, _| ()) };
    let scale = if page.dock.pressed == Some(chip) && !ui::Motion::system().is_reduced() { PRESS_SCALE } else { 1. };
    let button = ui::button(chip_id(chip), &c.p, &c.t(label), kind, Some(icon), on_click)
        .w(px(width * scale))
        .h(px(CHIP_H * scale))
        .role(Role::Button)
        .when(live, |button| {
            button
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |page, _: &MouseDownEvent, _, cx| {
                        if matches!(page.status.state, SessionState::Idle) {
                            page.dock.pressed = Some(chip);
                            cx.notify();
                        }
                    }),
                )
                .on_mouse_up_out(MouseButton::Left, cx.listener(release))
        });
    div().w(px(width)).h(px(CHIP_H)).flex().items_center().justify_center().child(button)
}

fn release(page: &mut TranscriptionPage, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<TranscriptionPage>) {
    if page.dock.pressed.take().is_some() {
        cx.notify();
    }
}

/// Starts the session a tray button asks for, from the lone tray or the two-button one.
fn launch(page: &mut TranscriptionPage, chip: Chip, cx: &mut Context<TranscriptionPage>) {
    page.dock.pressed = None;
    if matches!(page.status.state, SessionState::Idle) {
        page.dock.start_at = Some(Instant::now());
        match chip {
            Chip::Start => page.start(cx),
            Chip::Resume => page.resume_open(cx),
        }
        // The grace ends with no event of its own, so a slow start still gets its `TranscriptionStarting` frame.
        cx.spawn(async move |page, cx| {
            cx.background_executor().timer(PREPARE_GRACE).await;
            let _ = page.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }
    cx.notify();
}

/// The lone tray is the button at rest: its frame is the hit area, and hover deepens only the shadow.
fn start_hit_area(shell: Div, id: &ElementId, c: &Ctx, cx: &mut Context<TranscriptionPage>) -> AnyElement {
    shell
        .id(id.clone())
        .cursor_pointer()
        .role(Role::Button)
        .aria_label(c.t(Tr::TranscriptionStart))
        .on_hover(ui::hover_tracker(id.clone()))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|page, _: &MouseDownEvent, _, cx| {
                if matches!(page.status.state, SessionState::Idle) && !ui::Motion::system().is_reduced() {
                    page.dock.pressed = Some(Chip::Start);
                    cx.notify();
                }
            }),
        )
        .on_mouse_up_out(MouseButton::Left, cx.listener(release))
        .on_click(cx.listener(|page, _: &ClickEvent, _, cx| launch(page, Chip::Start, cx)))
        .into_any_element()
}

/// Wakes the page when the open transcript stops being resumable, so the tray narrows back on its own.
fn schedule_resume_wake(page: &mut TranscriptionPage, pair: bool, cx: &mut Context<TranscriptionPage>) {
    let until = pair.then(|| page.open.as_ref().and_then(|open| open.summary.resumable_until())).flatten();
    let Some(until) = until else {
        page.dock.resume_wake = None;
        return;
    };
    if page.dock.resume_wake.as_ref().is_some_and(|(at, _)| *at == until) {
        return;
    }
    let wait = Duration::from_secs((until - crate::timefmt::now()).max(0) as u64 + 1);
    let task = cx.spawn(async move |page, cx| {
        cx.background_executor().timer(wait).await;
        let _ = page.update(cx, |_, cx| cx.notify());
    });
    page.dock.resume_wake = Some((until, task));
}

/// The status capsule's label: `TranscriptionStarting` plain, `TranscriptionFinishing` with the HUD text sweep.
fn status_label(page: &TranscriptionPage, c: &Ctx, now: Instant, motion: ui::Motion) -> AnyElement {
    let p = &c.p;
    match page.dock.status_kind {
        StatusKind::Preparing => text(c.t(Tr::TranscriptionStarting), LABEL_SIZE, FontWeight::MEDIUM, p.muted)
            .line_height(px(20.))
            .whitespace_nowrap()
            .into_any_element(),
        StatusKind::Finishing => {
            let t = page.dock.finishing_since.map_or(0., |since| now.duration_since(since).as_secs_f32());
            ui::shimmer_text(
                &c.t(Tr::TranscriptionFinishing),
                t,
                LABEL_SIZE,
                FontWeight::MEDIUM,
                px(20.),
                ui::ShimmerInk::processing(p),
                motion,
            )
            .into_any_element()
        }
    }
}

/// A tray button folding into its circle on its way to the right end: the label leaves first and the width narrows.
/// New meeting cross-fades gold into the Danger surface and the plus into the square; Continue recording keeps its
/// quiet surface and turns play into pause. `offset` is where the button stood from the tray's center; both buttons
/// ride on stop's spread, so a pair travels together.
#[allow(clippy::too_many_arguments)]
fn fold_chip(
    page: &TranscriptionPage,
    c: &Ctx,
    row: &Row,
    w: f32,
    chip: Chip,
    chip_w: f32,
    offset: f32,
    now: Instant,
) -> Div {
    let p = &c.p;
    let f = page.dock.fold.sample(now);
    let e = page.dock.spread[STOP].sample(now);
    let (target, label, from_bg, from_border, from_ink, from_icon, to_bg, to_border, to_ink, to_icon) = match chip {
        Chip::Start => (
            STOP,
            Tr::TranscriptionStart,
            p.primary,
            p.primary,
            p.on_primary,
            Ic::Plus,
            p.button_danger_bg,
            p.button_danger_border,
            p.button_danger_text,
            Ic::Stop,
        ),
        Chip::Resume => (
            PAUSE,
            Tr::TranscriptionContinue,
            p.input_bg,
            p.border_strong,
            p.text,
            Ic::Play,
            p.input_bg,
            p.border_strong,
            p.text,
            Ic::Pause,
        ),
    };
    let pill_w = lerp(chip_w, CONTROL_H, f);
    let center_x = lerp(w / 2. + offset, row.centers[target], e);
    let glyph_top = (CHIP_H - CIRCLE_GLYPH) / 2.;
    let icon_left = lerp(CHIP_PAD, (pill_w - CHIP_ICON) / 2., f);
    div()
        .absolute()
        .left(px(center_x - pill_w / 2.))
        .top(px(row.top))
        .w(px(pill_w))
        .h(px(CHIP_H))
        .rounded_full()
        .overflow_hidden()
        .child(
            div().absolute().inset_0().rounded_full().bg(from_bg).border_1().border_color(from_border).opacity(1. - f),
        )
        .child(div().absolute().inset_0().rounded_full().bg(to_bg).border_1().border_color(to_border).opacity(f))
        .child(
            div()
                .absolute()
                .top(px((CHIP_H - 20.) / 2.))
                .left(px(CHIP_PAD + CHIP_ICON + CHIP_ICON_GAP))
                .h(px(20.))
                .flex()
                .items_center()
                .opacity(page.dock.label.sample(now))
                .child(text(c.t(label), LABEL_SIZE, FontWeight::MEDIUM, from_ink).whitespace_nowrap()),
        )
        .child(
            div()
                .absolute()
                .top(px((CHIP_H - CHIP_ICON) / 2.))
                .left(px(icon_left))
                .opacity(1. - f)
                .child(ui::icon(from_icon, CHIP_ICON, from_ink)),
        )
        .child(div().absolute().top(px(glyph_top)).left(px((pill_w - CIRCLE_GLYPH) / 2.)).opacity(f).child(ui::icon(
            to_icon,
            CIRCLE_GLYPH,
            to_ink,
        )))
}

/// The recording controls at their spread positions: a control still growing out of the center is drawn as a
/// plain scaled circle, and becomes the live control once it has arrived.
#[allow(clippy::too_many_arguments)]
fn controls(
    page: &TranscriptionPage,
    c: &Ctx,
    cx: &mut Context<TranscriptionPage>,
    row: &Row,
    w: f32,
    fade: f32,
    band: Option<f32>,
    now: Instant,
) -> Div {
    let p = &c.p;
    let motion = ui::Motion::system();
    let reduced = motion.is_reduced();
    let live = page.dock.controls.target() == 1. && page.dock.look == Look::Recording;
    let tips = live && page.dock.settled(now);
    let session_sources = match page.status.state {
        SessionState::Recording { sources, .. } => Some(sources),
        _ => None,
    };
    let sources = session_sources.unwrap_or_else(|| page.st.cfg.get().transcription.sources());
    let paused = matches!(page.status.state, SessionState::Recording { segment_start: None, .. });
    let mut layer = div().absolute().inset_0().top(px(rise(fade, motion))).opacity(fade);
    for index in [AUDIO, MIC, PAUSE, STOP] {
        if page.dock.folding && (index == STOP || (index == PAUSE && page.dock.fold_pair)) {
            continue;
        }
        let e = page.dock.spread[index].sample(now);
        if e <= 0.01 {
            continue;
        }
        let target = row.centers[index];
        let (center, scale, opacity) =
            if reduced { (target, 1., e) } else { (lerp(w / 2., target, e), lerp(SPREAD_FROM, 1., e), e) };
        let arrived = e >= 0.999;
        let width = if index == STOP { row.stop_w } else { CONTROL_H };
        if arrived {
            let control = match index {
                AUDIO => source_toggle(
                    page,
                    p,
                    c.t(Tr::TranscriptionSourceSystemTip),
                    Source::System,
                    sources.system,
                    live,
                    tips,
                )
                .into_any_element(),
                MIC => source_toggle(
                    page,
                    p,
                    c.t(Tr::TranscriptionSourceMicTip),
                    Source::Mic,
                    sources.microphone,
                    live,
                    tips,
                )
                .into_any_element(),
                PAUSE => pause_control(c, cx, paused, tips, live).into_any_element(),
                _ => stop_control(page, c, cx, row.stop_w, band, tips, live, now).into_any_element(),
            };
            layer = layer
                .child(div().absolute().left(px(center - width / 2.)).top(px(row.top)).opacity(opacity).child(control));
        } else {
            let look = ghost_look(p, index, sources);
            let size = width * scale;
            layer = layer.child(
                ghost_circle(look, size, scale)
                    .absolute()
                    .left(px(center - size / 2.))
                    .top(px(row.top + (CONTROL_H - size) / 2.))
                    .opacity(opacity),
            );
        }
    }
    layer
}

/// What a control looks like while it grows: its surface, ring, glyph and ink.
struct GhostLook {
    background: Hsla,
    border: Hsla,
    glyph: Ic,
    glyph_size: f32,
    ink: Hsla,
}

fn ghost_look(p: &Palette, index: usize, sources: Sources) -> GhostLook {
    match index {
        AUDIO => {
            let (background, glyph, ink) = source_look(p, Source::System, sources.system);
            GhostLook { background, border: p.border_strong, glyph, glyph_size: CIRCLE_GLYPH, ink }
        }
        MIC => {
            let (background, glyph, ink) = source_look(p, Source::Mic, sources.microphone);
            GhostLook { background, border: p.border_strong, glyph, glyph_size: CIRCLE_GLYPH, ink }
        }
        PAUSE => GhostLook {
            background: p.input_bg,
            border: p.border_strong,
            glyph: Ic::Pause,
            glyph_size: CIRCLE_GLYPH,
            ink: p.text,
        },
        _ => GhostLook {
            background: p.button_danger_bg,
            border: p.button_danger_border,
            glyph: Ic::Stop,
            glyph_size: CIRCLE_GLYPH,
            ink: p.button_danger_text,
        },
    }
}

/// A non-interactive circle at a fraction of its size, glyph scaled with it.
fn ghost_circle(look: GhostLook, size: f32, scale: f32) -> Div {
    div()
        .size(px(size))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(look.background)
        .border_1()
        .border_color(look.border)
        .child(ui::icon(look.glyph, look.glyph_size * scale, look.ink))
}

/// Where the armed pill's band stands, 0 to 1 across, while it is crossing; `None` between bands.
fn band_phase(dock: &DockAnim, reduced: bool, now: Instant) -> Option<f32> {
    if !dock.stop_armed || reduced {
        return None;
    }
    let elapsed = now.duration_since(dock.armed_at).as_secs_f32() * 1000.;
    let phase = elapsed % BAND_EVERY_MS;
    (phase < BAND_MS).then(|| phase / BAND_MS)
}

/// Wakes the page for the next band instead of drawing the idle stretch between bands frame by frame.
fn schedule_band(dock: &mut DockAnim, now: Instant, cx: &mut Context<TranscriptionPage>) {
    let elapsed = now.duration_since(dock.armed_at).as_secs_f32() * 1000.;
    let next =
        dock.armed_at + Duration::from_secs_f32(((elapsed / BAND_EVERY_MS).floor() + 1.) * BAND_EVERY_MS / 1000.);
    if dock.band_wake.as_ref().is_some_and(|(at, _)| *at == next) {
        return;
    }
    let wait = next.saturating_duration_since(now);
    let task = cx.spawn(async move |page, cx| {
        cx.background_executor().timer(wait).await;
        let _ = page.update(cx, |_, cx| cx.notify());
    });
    dock.band_wake = Some((next, task));
}

/// Materialized content rises 4 pt into place; Reduce Motion fades it without moving.
fn rise(fade: f32, motion: ui::Motion) -> f32 {
    if motion.is_reduced() { 0. } else { (1. - fade) * 4. }
}

/// The materialize timing for a fade whose target is `shown`.
fn fade_timing(shown: bool, motion: ui::Motion) -> Timing {
    if shown { Timing::materialize_in(motion, 0.) } else { Timing::materialize_out(motion) }
}

/// A shape casts the deeper shadow of `design:tools/recording`: the capsule rests shallow and deepens with hover
/// alone, taller shapes interpolate by drawn height from wherever the capsule's shadow stands; the color is always
/// the palette's shadow with its alpha scaled.
fn shadow_for(h: f32, deepen: f32, p: &Palette) -> BoxShadow {
    let (offset, blur, strength) = if h <= TRAY_H {
        (lerp(4., 10., deepen), lerp(14., 28., deepen), lerp(1., 1.5, deepen))
    } else {
        let t = ((h - TRAY_H) / (EXPANDED_H - TRAY_H)).clamp(0., 1.);
        (lerp(lerp(4., 10., deepen), 16., t), lerp(lerp(14., 28., deepen), 40., t), lerp(lerp(1., 1.5, deepen), 1.5, t))
    };
    BoxShadow {
        color: Hsla { a: (p.shadow.a * strength).min(1.), ..p.shadow },
        offset: point(px(0.), px(offset)),
        blur_radius: px(blur),
        spread_radius: px(0.),
        inset: false,
    }
}

/// The page's content width guessed from the window, until the row has painted once: the window minus the sidebar
/// and the two 56 pt gutters.
fn content_width(window: &Window) -> f32 {
    let window_w = f32::from(window.viewport_size().width);
    // The shell collapses its sidebar into the 76 pt rail below 1024 pt.
    let side = if window_w < 1024. { 76. } else { 232. };
    (window_w - side - 112.).clamp(240., EXPANDED_W)
}

/// The stop circle, which widens into the confirm pill on its first press: the square stays, the confirm label is
/// revealed by the growing width, and a halo and a passing band say it is waiting for the second press.
#[allow(clippy::too_many_arguments)]
fn stop_control(
    page: &TranscriptionPage,
    c: &Ctx,
    cx: &mut Context<TranscriptionPage>,
    width: f32,
    band: Option<f32>,
    tips: bool,
    live: bool,
    now: Instant,
) -> Div {
    let p = &c.p;
    let id = ElementId::from("dock-stop");
    let armed = page.dock.stop_armed;
    let arm = page.dock.arm.sample(now);
    let label = c.t(if armed { Tr::TranscriptionStopConfirm } else { Tr::TranscriptionStop });
    let tips = tips && !armed;
    if tips {
        ui::tip_label(id.clone(), label.clone());
    }
    let on_click: Cb = if live {
        ui::cbk(cx, |page, _, cx| {
            let motion = ui::Motion::system();
            if !matches!(page.status.state, SessionState::Recording { .. }) {
                return;
            }
            if page.dock.stop_armed {
                page.dock.disarm(motion, Instant::now());
                page.st.transcription.stop();
            } else {
                ui::dismiss_tips(&page.motion.handle());
                page.dock.arm(motion, Instant::now());
                // The fold-back needs a redraw at its deadline even when nothing else is moving.
                cx.spawn(async move |page, cx| {
                    cx.background_executor().timer(ARM_WAIT).await;
                    let _ = page.update(cx, |_, cx| cx.notify());
                })
                .detach();
            }
            cx.notify();
        })
    } else {
        Rc::new(|_, _| ())
    };
    let mut pill = div()
        .id(id.clone())
        .relative()
        .w(px(width))
        .h(px(CONTROL_H))
        .flex_none()
        .rounded_full()
        .bg(p.button_danger_bg)
        .border_1()
        .border_color(p.button_danger_border)
        .cursor_pointer()
        .role(Role::Button)
        .aria_label(label)
        .on_hover(ui::hover_tracker(id.clone()))
        .on_click(click_cb(on_click))
        .child(ui::hover_fill(&id, None, p.button_danger_hover));
    if let Some(k) = band {
        pill = pill.child(band_layer(p, width, k));
    }
    // The square eases from the circle's 20 pt glyph to the pill's 16 pt icon as the width opens.
    let glyph = lerp(CIRCLE_GLYPH, PILL_GLYPH, arm);
    pill = pill
        .child(
            div()
                .absolute()
                .top(px((CONTROL_H - glyph) / 2. - 1.))
                .left(px(lerp((CONTROL_H - CIRCLE_GLYPH) / 2., 16., arm) - 1.))
                .child(ui::icon(Ic::Stop, glyph, p.button_danger_text)),
        )
        .when(arm > 0.01, |pill| {
            // The label sits where the pill will put it and is revealed by the width, so it never rewraps.
            pill.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(16. + PILL_GLYPH + 8. - 1.))
                    .right_0()
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .opacity(arm)
                    .child(
                        text(c.t(Tr::TranscriptionStopConfirm), LABEL_SIZE, FontWeight::MEDIUM, p.button_danger_text)
                            .whitespace_nowrap(),
                    ),
            )
        });
    let pill = if tips { pill.child(ui::tip_above(p, &id)) } else { pill };
    let halo = page.dock.halo.sample(now);
    div()
        .relative()
        .w(px(width))
        .h(px(CONTROL_H))
        .when(halo > 0.01, |frame| {
            frame.child(
                div()
                    .absolute()
                    .top(px(-HALO_W))
                    .left(px(-HALO_W))
                    .w(px(width + 2. * HALO_W))
                    .h(px(CONTROL_H + 2. * HALO_W))
                    .rounded_full()
                    .bg(faded(p.button_danger_halo, halo)),
            )
        })
        .child(pill)
}

/// One light band crossing the pill's straight middle, where its edges are flat, so the band never shows outside
/// the rounded ends. `k` is how far across it is.
fn band_layer(p: &Palette, width: f32, k: f32) -> Div {
    let inset = CONTROL_H / 2.;
    let span = (width - 2. * inset).max(0.);
    let band_w = (span * 0.45).max(24.);
    let left = lerp(-band_w, span, ui::ease(k));
    let light = if p.dark { 0.2 } else { 0.9 };
    let peak = faded(ui::white(), light);
    let clear = faded(ui::white(), 0.);
    div().absolute().top_0().bottom_0().left(px(inset)).w(px(span)).overflow_hidden().child(
        div()
            .absolute()
            .top_0()
            .bottom_0()
            .left(px(left))
            .w(px(band_w))
            .flex()
            .child(div().flex_1().h_full().bg(linear_gradient(
                90.,
                linear_color_stop(clear, 0.),
                linear_color_stop(peak, 1.),
            )))
            .child(div().flex_1().h_full().bg(linear_gradient(
                90.,
                linear_color_stop(peak, 0.),
                linear_color_stop(clear, 1.),
            ))),
    )
}

/// The pause circle: quiet surface chrome whose glyph swaps to play while paused. Toggling it also disarms the
/// armed stop.
fn pause_control(c: &Ctx, cx: &mut Context<TranscriptionPage>, paused: bool, tips: bool, live: bool) -> Stateful<Div> {
    let p = &c.p;
    let id = ElementId::from("dock-pause");
    let label = c.t(if paused { Tr::TranscriptionResume } else { Tr::TranscriptionPause });
    if tips {
        ui::tip_label(id.clone(), label.clone());
    }
    let on_click: Cb = if live {
        ui::cbk(cx, |page, _, cx| {
            page.st.transcription.toggle_pause();
            page.dock.disarm(ui::Motion::system(), Instant::now());
            cx.notify();
        })
    } else {
        Rc::new(|_, _| ())
    };
    with_tip(
        ui::circle_control_with_glyph(
            id.clone(),
            if paused { Ic::Play } else { Ic::Pause },
            CircleSize::Regular,
            CIRCLE_GLYPH,
            CircleAppearance {
                background: Some(p.input_bg),
                foreground: p.text,
                border: Some(p.border_strong),
                hover: Some(p.seg),
            },
        )
        .cursor_pointer()
        .role(Role::Button)
        .aria_label(label)
        .on_click(click_cb(on_click)),
        p,
        &id,
        tips,
    )
}

/// Attaches the circle's above-tip unless the dock's motion keeps tips away.
fn with_tip(control: Stateful<Div>, p: &Palette, id: &ElementId, tips: bool) -> Stateful<Div> {
    if tips { control.child(ui::tip_above(p, id)) } else { control }
}

fn session_identity(
    page: &TranscriptionPage,
    c: &Ctx,
    cx: &mut Context<TranscriptionPage>,
    compact: bool,
    clock_w: f32,
) -> Stateful<Div> {
    let p = &c.p;
    let recording = matches!(page.status.state, SessionState::Recording { .. });
    let elapsed = page.session_elapsed_ms().unwrap_or(0);
    let identity = div().id("dock-identity").flex_1().min_w(px(0.)).flex().items_center().gap(px(12.)).child(
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(div().size(px(8.)).rounded_full().bg(if page.is_paused() { p.muted } else { p.danger }))
            .child(
                // The clock column keeps the widest clock's width, so the title never moves as it ticks.
                div().w(px(clock_w)).flex_none().child(
                    text(crate::timefmt::clock(elapsed), CLOCK_SIZE, FontWeight::MEDIUM, p.text)
                        .line_height(px(CLOCK_LINE))
                        .font_features(FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)]))),
                ),
            ),
    );
    // The title field has its own border, so only the borderless browsing line keeps a divider after the clock.
    let identity = if compact {
        identity.child(div().w(px(1.)).h(px(16.)).flex_none().bg(p.border_strong)).child(compact_identity(page, c))
    } else {
        identity.child(title_field(page, c))
    };
    // The bar stays a place to come back to: pressing its identity reopens the live workspace.
    match (compact, page.status.state.transcript_id()) {
        (true, Some(id)) if recording => identity.cursor_pointer().on_click(open_row(cx, id)),
        _ => identity,
    }
}

#[derive(Clone, Copy)]
enum Source {
    System,
    Mic,
}

/// An active source is a neutral circle whose glyph alone carries the macOS indicator color: green for the Mac's
/// audio, orange for the microphone. An inactive one is the quiet tile with the slashed glyph.
fn source_look(p: &Palette, source: Source, on: bool) -> (Hsla, Ic, Hsla) {
    match (source, on) {
        (Source::System, true) => (p.input_bg, Ic::Headphones, p.success),
        (Source::Mic, true) => (p.input_bg, Ic::Mic, p.orange),
        (Source::System, false) => (p.tile_idle, Ic::HeadphoneOff, p.muted),
        (Source::Mic, false) => (p.tile_idle, Ic::MicOff, p.muted),
    }
}

fn source_toggle(
    page: &TranscriptionPage,
    p: &Palette,
    label: SharedString,
    source: Source,
    on: bool,
    live: bool,
    tips: bool,
) -> Stateful<Div> {
    let id = ElementId::from(match source {
        Source::System => "dock-source-system",
        Source::Mic => "dock-source-mic",
    });
    if tips {
        ui::tip_label(id.clone(), label.clone());
    }
    let (background, glyph, ink) = source_look(p, source, on);
    let control = ui::circle_control_with_glyph(
        id.clone(),
        glyph,
        CircleSize::Regular,
        CIRCLE_GLYPH,
        CircleAppearance {
            background: Some(background),
            foreground: ink,
            border: Some(p.border_strong),
            hover: Some(p.seg),
        },
    );
    let control = if live {
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
            st.transcription.set_sources(sources);
        };
        control.cursor_pointer().role(Role::Button).aria_label(label).on_click(click_cb(Rc::new(toggle)))
    } else {
        control
    };
    with_tip(control, p, &id, tips)
}

impl TranscriptionPage {
    /// Arms the two-step stop for the screenshot film, as a first press does.
    #[cfg(feature = "screenshots")]
    pub fn pose_stop_armed(&mut self, cx: &mut Context<Self>) {
        self.dock.arm(ui::Motion::system(), Instant::now());
        cx.notify();
    }

    /// Marks New meeting as just pressed for the screenshot film, so the next session folds the button.
    #[cfg(feature = "screenshots")]
    pub fn pose_start_pressed(&mut self) {
        self.dock.start_at = Some(Instant::now());
    }
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

fn title_field(page: &TranscriptionPage, c: &Ctx) -> AnyElement {
    let Some(open) = &page.open else { return div().into_any_element() };
    div().flex_1().min_w(px(0.)).child(ui::text_input(&c.p, &open.title, None)).into_any_element()
}

/// The lane cards for the panel size being drawn: they grow with the panel and share its overshoot, the text keeps
/// its native metrics, and the materialize fade sets their opacity, a 4 pt rise and a frame scale about the capsule
/// center.
struct LaneCards {
    fade: f32,
    scale: f32,
    center_x: f32,
    card_w: f32,
    width: f32,
    height: f32,
    top: f32,
    text_width: f32,
    body_opacity: f32,
    show_status: bool,
}

fn lane_cards(sources: Sources, w: f32, h: f32, avail: f32, fade: f32, motion: ui::Motion) -> LaneCards {
    let both = sources.system && sources.microphone;
    let settled = avail - 2. * LANE_INSET;
    let text_width = if both { (settled - LANE_GAP) / 2. } else { settled };
    let card_w = if both { (w - 2. * LANE_INSET - LANE_GAP).max(0.) / 2. } else { (w - 2. * LANE_INSET).max(0.) };
    let card_h = (h - LANE_TOP - LANE_BELOW).max(0.);
    let scale = if motion.is_reduced() { 1. } else { lerp(0.96, 1., fade) };
    let center_y = h - TRAY_H / 2.;
    LaneCards {
        fade,
        scale,
        center_x: w / 2.,
        card_w,
        width: card_w * scale,
        height: card_h * scale,
        top: center_y + (LANE_TOP - center_y) * scale + rise(fade, motion),
        text_width,
        body_opacity: ((card_h - 92.) / 29.).clamp(0., 1.) * ((card_w - text_width + 14.) / 14.).clamp(0., 1.),
        show_status: card_w >= 196. && (!both || card_w + 0.5 >= text_width),
    }
}

/// Both cards keep their heading and body at native text metrics while their frames follow the drawn panel.
fn lanes_area(page: &TranscriptionPage, c: &Ctx, sources: Sources, cards: LaneCards, t: Option<f32>) -> Div {
    let place = |x: f32| cards.center_x + (x - cards.center_x) * cards.scale;
    // The lanes fill the slots in order: the user's own lane takes the second slot only beside the other side's.
    let me_slot = if sources.system { cards.card_w + LANE_GAP } else { 0. };
    let speakers = [
        (Speaker::Others, sources.system, place(LANE_INSET)),
        (Speaker::Me, sources.microphone, place(LANE_INSET + me_slot)),
    ];
    div().absolute().inset_0().children(speakers.into_iter().filter(|(_, on, _)| *on).map(|(speaker, _, left)| {
        let geometry = super::lanes::Geometry {
            width: cards.width,
            height: cards.height,
            text_width: cards.text_width,
            body_opacity: cards.body_opacity,
            show_status: cards.show_status,
        };
        super::lanes::lane(page, c, speaker, geometry, t)
            .absolute()
            .top(px(cards.top - SHELL_BORDER / 2.))
            .left(px(left - SHELL_BORDER / 2.))
            .opacity(cards.fade)
    }))
}

/// The left edge of the audio circle as it is drawn while the controls spread out of the center.
fn spread_left(dock: &DockAnim, row: &Row, w: f32, now: Instant) -> f32 {
    if ui::Motion::system().is_reduced() {
        return row.left();
    }
    let e = dock.spread[AUDIO].sample(now);
    lerp(w / 2., row.centers[AUDIO], e) - CONTROL_H * lerp(SPREAD_FROM, 1., e) / 2.
}

/// How close a drawn shape is to the size it is moving to, 0 far away to 1 there. The tray buttons ride on it while
/// the dock heads for the tray, pressed or not.
fn nearness((w, h): (f32, f32), (target_w, target_h): (f32, f32)) -> f32 {
    /// The buttons begin this far from the tray's size, counting height twice since it changes less.
    const GOLD_FROM: f32 = 48.;
    let distance = (w - target_w).abs() + 2. * (h - target_h).abs();
    (1. - distance / GOLD_FROM).clamp(0., 1.)
}
