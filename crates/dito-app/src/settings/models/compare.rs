//! The model comparison dialog (`design:settings/compare-models`). The user picks up to five speech models,
//! local or cloud, records a sentence each round, rates the blind results, and ends on a ranking of their own.

use std::cell::Cell;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use dito_core::asr_compare::{
    self, Contender, MAX_CONTENDERS, MIN_CONTENDERS, Outcome, Rating, Readiness, Scoreboard, Tally,
};
use dito_core::audio::AudioClip;
use dito_core::controller::HudState;
use dito_core::i18n::{Tr, fmt};
use dito_local::ModelState;
use dito_local::catalog::ModelKind;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::page::{ModalChanged, ModelsPage};
use crate::assets::Ic;
use crate::services::{self, Heard};
use crate::settings::Ctx;
use crate::settings::edge_fade::{Edge, edge_fade};
use crate::state::AppState;
use crate::ui::{self, BtnKind, DialogAnswer, cbk, text};

const DIALOG: &str = "models-compare";
const OPEN: &str = "models-compare-open";
const LEVEL_BARS: usize = 24;
const LEVEL_EVERY: Duration = Duration::from_millis(50);
const CHIP_GAP: f32 = 6.;
const SCROLL: &str = "models-compare-scroll";
const FADE: f32 = 24.;
/// A model that can take part, with the name the dialog shows for it.
#[derive(Clone)]
struct Choice {
    contender: Contender,
    name: String,
    /// Why a cloud configuration cannot take part yet; `None` when it can.
    unavailable: Option<Tr>,
}

enum Answer {
    Waiting,
    Running,
    Text { text: String, millis: u64 },
    Failed,
}

/// One result of the round, in the blind order it is shown in.
struct Shown {
    contender: Contender,
    answer: Answer,
    rating: Option<Rating>,
}

/// The microphone and the paused dictation shortcut of one round. Letting it go releases both, whether the round moves
/// on to rating, the dialog closes or the settings window goes away mid-recording.
struct MicHold {
    st: Arc<AppState>,
    recording: bool,
}

impl MicHold {
    fn start(st: &Arc<AppState>) -> Result<Self, dito_core::controller::RecorderError> {
        st.hotkeys.pause_starts(true);
        // Built before the start so that a failed start still unpauses the shortcut when it drops.
        let mut hold = Self { st: st.clone(), recording: false };
        st.recorder.start()?;
        hold.recording = true;
        Ok(hold)
    }

    /// The shortcut stays paused until the hold is dropped, so no dictation starts while the round is recognized.
    fn stop(&mut self) -> AudioClip {
        self.recording = false;
        self.st.recorder.stop()
    }
}

impl Drop for MicHold {
    fn drop(&mut self) {
        if self.recording {
            self.st.recorder.stop();
        }
        self.st.hotkeys.pause_starts(false);
    }
}

enum Stage {
    Closed,
    Choose,
    Ready,
    Recording {
        started: Instant,
        levels: VecDeque<f32>,
        mic: MicHold,
        _tick: Task<()>,
    },
    /// Dropping the task stops the round.
    Recognizing {
        _mic: MicHold,
        _task: Task<()>,
    },
    Rate,
    Ranking(Vec<(Contender, Tally)>),
}

pub(super) struct Compare {
    stage: Stage,
    focus: FocusHandle,
    choices: Vec<Choice>,
    chosen: Vec<Contender>,
    board: Scoreboard,
    round: u32,
    shown: Vec<Shown>,
    error: Option<Tr>,
    scroll: ScrollHandle,
    /// The stage and round the scroll position belongs to; a new one starts from the top.
    scrolled_for: Cell<(&'static str, u32)>,
}

impl Compare {
    pub(super) fn new(cx: &mut Context<ModelsPage>) -> Self {
        Self {
            stage: Stage::Closed,
            focus: cx.focus_handle(),
            choices: Vec::new(),
            chosen: Vec::new(),
            board: Scoreboard::default(),
            round: 1,
            shown: Vec::new(),
            error: None,
            scroll: ScrollHandle::new(),
            scrolled_for: Cell::new(("closed", 0)),
        }
    }
}

impl ModelsPage {
    fn compare_choices(&self) -> Vec<Choice> {
        self.read_compare_choices()
    }

    /// Downloaded local models from small to large, then the saved cloud configurations in their tile order.
    fn read_compare_choices(&self) -> Vec<Choice> {
        let cfg = self.st.cfg.get();
        let snapshot = self.snapshot();
        let local = self
            .st
            .models
            .catalog()
            .iter()
            .filter(|spec| spec.kind == ModelKind::Asr && snapshot.get(&spec.id) == Some(&ModelState::Installed))
            .map(|spec| Choice {
                contender: Contender::Local(spec.id.clone()),
                name: spec.name.clone(),
                unavailable: None,
            });
        let cloud = cfg.asr.cloud_asr.iter().flat_map(|list| &list.items).map(|configuration| {
            let has_key = self.keyed_accounts.contains(&configuration.account());
            Choice {
                contender: Contender::Cloud(configuration.id.clone()),
                name: configuration.name.clone(),
                unavailable: match asr_compare::cloud_readiness(&configuration.config, has_key) {
                    Readiness::Ready => None,
                    Readiness::MissingKey => Some(Tr::CompareMissingKey),
                    Readiness::Incomplete => Some(Tr::CompareIncomplete),
                },
            }
        });
        local.chain(cloud).collect()
    }

    fn compare_name(&self, contender: &Contender) -> String {
        self.compare.choices.iter().find(|choice| choice.contender == *contender).map_or_else(
            || match contender {
                Contender::Local(id) | Contender::Cloud(id) => id.clone(),
            },
            |choice| choice.name.clone(),
        )
    }

    fn open_compare(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let choices = self.read_compare_choices();
        let available: Vec<Contender> = choices
            .iter()
            .filter(|choice| choice.unavailable.is_none())
            .map(|choice| choice.contender.clone())
            .collect();
        let in_use = Contender::in_use(&self.st.cfg.get().asr);
        self.compare.chosen = asr_compare::initial_choice(&available, in_use.as_ref());
        self.compare.choices = choices;
        self.compare.error = None;
        self.compare.stage = Stage::Choose;
        self.compare.focus.focus(window, cx);
        ui::dismiss_tips(&self.motion.handle());
        cx.emit(ModalChanged);
        cx.notify();
    }

    /// Every round goes with the dialog; a recording in progress is dropped.
    fn close_compare(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.compare.stage, Stage::Closed) {
            return;
        }
        self.compare.stage = Stage::Closed;
        self.compare.shown.clear();
        window.blur(cx);
        cx.emit(ModalChanged);
        cx.notify();
    }

    fn toggle_compare_choice(&mut self, contender: Contender, cx: &mut Context<Self>) {
        let chosen = &mut self.compare.chosen;
        if let Some(index) = chosen.iter().position(|c| *c == contender) {
            chosen.remove(index);
        } else if chosen.len() < MAX_CONTENDERS {
            chosen.push(contender);
        }
        cx.notify();
    }

    fn begin_compare(&mut self, cx: &mut Context<Self>) {
        if self.compare.chosen.len() < MIN_CONTENDERS {
            return;
        }
        self.compare.board = Scoreboard::new(&self.compare.chosen);
        self.compare.round = 1;
        self.compare.shown.clear();
        self.compare.error = None;
        self.compare.stage = Stage::Ready;
        cx.notify();
    }

    fn start_compare_recording(&mut self, cx: &mut Context<Self>) {
        let dictation = &self.st.dictation;
        if dictation.recording_started().is_some() || *dictation.hud().borrow() != HudState::Hidden {
            self.compare.error = Some(Tr::CompareDictationBusy);
            self.compare.stage = Stage::Ready;
            cx.notify();
            return;
        }
        let mic = match MicHold::start(&self.st) {
            Ok(mic) => mic,
            Err(error) => {
                tracing::warn!("model comparison: the microphone could not start: {error}");
                self.compare.error = Some(Tr::CompareMicrophoneFailed);
                self.compare.stage = Stage::Ready;
                cx.notify();
                return;
            }
        };
        let tick = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(LEVEL_EVERY).await;
                let recording = this.update(cx, |this, cx| {
                    let level = this.st.recorder.level();
                    let Stage::Recording { levels, .. } = &mut this.compare.stage else { return false };
                    levels.pop_front();
                    levels.push_back(level);
                    cx.notify();
                    true
                });
                if !matches!(recording, Ok(true)) {
                    break;
                }
            }
        });
        self.compare.error = None;
        self.compare.shown.clear();
        self.compare.stage = Stage::Recording {
            started: Instant::now(),
            levels: VecDeque::from(vec![0.; LEVEL_BARS]),
            mic,
            _tick: tick,
        };
        cx.notify();
    }

    fn stop_compare_recording(&mut self, cx: &mut Context<Self>) {
        let Stage::Recording { mut mic, .. } = std::mem::replace(&mut self.compare.stage, Stage::Ready) else {
            return;
        };
        let clip = mic.stop();
        if clip.is_effectively_silent() {
            self.compare.error = Some(Tr::CompareNoSpeech);
            self.compare.stage = Stage::Ready;
            cx.notify();
            return;
        }
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_nanos() as u64);
        let chosen = &self.compare.chosen;
        self.compare.shown = asr_compare::blind_order(chosen.len(), seed)
            .into_iter()
            .map(|index| Shown { contender: chosen[index].clone(), answer: Answer::Waiting, rating: None })
            .collect();
        let task = self.recognize_round(Arc::new(clip), cx);
        self.compare.stage = Stage::Recognizing { _mic: mic, _task: task };
        cx.notify();
    }

    /// One contender after another, in the order shown, so at most one extra local model is loaded at a time.
    fn recognize_round(&self, clip: Arc<AudioClip>, cx: &mut Context<Self>) -> Task<()> {
        let st = self.st.clone();
        let contenders: Vec<Contender> = self.compare.shown.iter().map(|shown| shown.contender.clone()).collect();
        cx.spawn(async move |this, cx| {
            for (index, contender) in contenders.into_iter().enumerate() {
                let started = this.update(cx, |this, cx| {
                    let Some(shown) = this.compare.shown.get_mut(index) else { return false };
                    shown.answer = Answer::Running;
                    cx.notify();
                    true
                });
                if !matches!(started, Ok(true)) {
                    return;
                }
                let Heard { text, millis } = services::compare_transcribe(st.clone(), contender, clip.clone()).await;
                let shown = this.update(cx, |this, cx| {
                    let Some(shown) = this.compare.shown.get_mut(index) else { return false };
                    shown.answer = match text {
                        Ok(text) => Answer::Text { text, millis },
                        Err(error) => {
                            tracing::warn!("model comparison: a result failed: {error}");
                            Answer::Failed
                        }
                    };
                    cx.notify();
                    true
                });
                if !matches!(shown, Ok(true)) {
                    return;
                }
            }
            let _ = this.update(cx, |this, cx| {
                if matches!(this.compare.stage, Stage::Recognizing { .. }) {
                    this.compare.stage = Stage::Rate;
                    cx.notify();
                }
            });
        })
    }

    fn rate_compare_result(&mut self, index: usize, rating: Rating, cx: &mut Context<Self>) {
        if let Some(shown) = self.compare.shown.get_mut(index) {
            shown.rating = if shown.rating == Some(rating) { None } else { Some(rating) };
            cx.notify();
        }
    }

    fn close_compare_round(&mut self) {
        let outcomes: Vec<Outcome> = self
            .compare
            .shown
            .drain(..)
            .map(|shown| {
                let millis = match shown.answer {
                    Answer::Text { millis, .. } => Some(millis),
                    _ => None,
                };
                Outcome { contender: shown.contender, millis, rating: shown.rating }
            })
            .collect();
        self.compare.board.add_round(&outcomes);
    }

    fn compare_has_ratings(&self) -> bool {
        self.compare.board.has_ratings()
            || self
                .compare
                .shown
                .iter()
                .any(|shown| shown.rating.is_some() && matches!(shown.answer, Answer::Text { .. }))
    }

    fn next_compare_round(&mut self, cx: &mut Context<Self>) {
        self.close_compare_round();
        self.compare.round += 1;
        self.start_compare_recording(cx);
    }

    fn finish_compare(&mut self, cx: &mut Context<Self>) {
        self.close_compare_round();
        self.compare.stage = Stage::Ranking(self.compare.board.ranking());
        cx.notify();
    }

    fn compare_again(&mut self, cx: &mut Context<Self>) {
        self.compare.error = None;
        self.compare.stage = Stage::Choose;
        cx.notify();
    }

    fn use_compared(&mut self, contender: Contender, cx: &mut Context<Self>) {
        let saved = services::try_save_config(&self.st, |cfg| cfg.asr = contender.engine(&cfg.asr));
        self.note_save(saved.is_ok());
        cx.notify();
    }
}

impl ModelsPage {
    pub(super) fn compare_stage_name(&self) -> &'static str {
        match self.compare.stage {
            Stage::Closed => "closed",
            Stage::Choose => "choose",
            Stage::Ready => "ready",
            Stage::Recording { .. } => "recording",
            Stage::Recognizing { .. } => "recognizing",
            Stage::Rate => "rate",
            Stage::Ranking(_) => "ranking",
        }
    }

    /// The action in the speech recognition card header, beside the source switch on either source; it needs two models
    /// that can take part.
    pub(super) fn compare_entry(&self, c: &Ctx, cx: &mut Context<Self>) -> AnyElement {
        let p = &c.p;
        let label = c.t(Tr::CompareOpen);
        let ready = self.compare_choices().iter().filter(|choice| choice.unavailable.is_none()).count();
        if ready >= MIN_CONTENDERS {
            return ui::button(OPEN, p, &label, BtnKind::Secondary, Some(Ic::AudioLines), cbk(cx, Self::open_compare))
                .into_any_element();
        }
        let id = ElementId::from(OPEN);
        ui::tip_label(id.clone(), c.t(Tr::CompareOpenUnavailable));
        ui::disabled_button(id.clone(), p, &label, BtnKind::Secondary, ui::BtnSize::Regular, Some(Ic::AudioLines), None)
            .on_hover(ui::hover_tracker(id.clone()))
            .child(ui::tip_above(p, &id))
            .into_any_element()
    }

    pub(super) fn compare_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if matches!(self.compare.stage, Stage::Closed) {
            return None;
        }
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        let lang = c.lang;
        let answer = |label: Tr, on_click: Option<ui::Cb>| DialogAnswer { label: c.t(label), on_click };
        let action = |name: &'static str, kind: BtnKind, lead: Option<Ic>, answer: DialogAnswer| {
            ui::dialog_button(&p, ElementId::from((ElementId::from(DIALOG), name)), kind, lead, answer)
                .into_any_element()
        };
        let close = cbk(cx, Self::close_compare);
        let close_action = |label: Tr| action("cancel", BtnKind::Secondary, None, answer(label, Some(close.clone())));
        let count = self.compare.chosen.len().to_string();
        let round = fmt::compare_round(lang, &self.compare.round.to_string());
        let (title, aside, content, actions) = match &self.compare.stage {
            Stage::Closed => return None,
            Stage::Choose => {
                let start = (self.compare.chosen.len() >= MIN_CONTENDERS)
                    .then(|| cbk(cx, |this: &mut Self, _, cx| this.begin_compare(cx)));
                (
                    c.t(Tr::CompareChooseTitle),
                    format!("{count} / {MAX_CONTENDERS}"),
                    vec![
                        description(&c, c.t(Tr::CompareChooseDesc)),
                        self.compare_scroll(&c, self.compare_picker(&c, window, cx), cx),
                    ],
                    vec![
                        close_action(Tr::AsrEditorCancel),
                        action("start", BtnKind::Primary, None, answer(Tr::CompareStart, start)),
                    ],
                )
            }
            Stage::Ready | Stage::Recording { .. } | Stage::Recognizing { .. } => {
                let mut content = vec![description(&c, fmt::compare_desc(lang, &count).into())];
                content.extend(self.compare.error.map(|error| ui::banner(&p, c.t(error), None).into_any_element()));
                let main = if self.compare.shown.is_empty() {
                    self.record_panel(&c, cx)
                } else {
                    self.compare_results(&c, false, cx)
                };
                content.push(self.compare_scroll(&c, main, cx));
                (c.t(Tr::CompareTitle), round, content, vec![close_action(Tr::HudClose)])
            }
            Stage::Rate => {
                let finish =
                    self.compare_has_ratings().then(|| cbk(cx, |this: &mut Self, _, cx| this.finish_compare(cx)));
                let next = cbk(cx, |this: &mut Self, _, cx| this.next_compare_round(cx));
                (
                    c.t(Tr::CompareTitle),
                    round,
                    vec![
                        description(&c, c.t(Tr::CompareRateDesc)),
                        self.compare_scroll(&c, self.compare_results(&c, true, cx), cx),
                    ],
                    vec![
                        action("finish", BtnKind::Secondary, None, answer(Tr::CompareFinish, finish)),
                        action("next", BtnKind::Primary, Some(Ic::Mic), answer(Tr::CompareNextRound, Some(next))),
                    ],
                )
            }
            Stage::Ranking(ranking) => {
                let again = cbk(cx, |this: &mut Self, _, cx| this.compare_again(cx));
                (
                    c.t(Tr::CompareRankingTitle),
                    fmt::compare_rounds(lang, &self.compare.board.rounds().to_string()),
                    vec![
                        description(&c, c.t(Tr::CompareRankingDesc)),
                        self.compare_scroll(&c, self.compare_ranking(&c, ranking, cx), cx),
                    ],
                    vec![
                        action("again", BtnKind::Secondary, None, answer(Tr::CompareAgain, Some(again))),
                        action("done", BtnKind::Primary, None, answer(Tr::CompareDone, Some(close.clone()))),
                    ],
                )
            }
        };
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.))
            .child(text(title, 20., FontWeight::BOLD, p.text).flex_1().min_w(px(0.)).truncate())
            .child(text(aside, 13., FontWeight::NORMAL, p.muted).line_height(px(20.)).flex_none());
        Some(ui::render_with_scope(self.motion.handle(), || {
            ui::dialog_with_actions(
                window,
                &p,
                DIALOG,
                &self.compare.focus,
                header.into_any_element(),
                content,
                actions,
                Some(close),
            )
        }))
    }

    /// The part of a stage that may run past a short window (`design:settings/compare-models/short-window`): it scrolls
    /// on its own while the title, description and buttons stay put.
    fn compare_scroll(&self, c: &Ctx, content: AnyElement, cx: &mut Context<Self>) -> AnyElement {
        let p = c.p;
        let scroll = &self.compare.scroll;
        let now = (self.compare_stage_name(), self.compare.round);
        if self.compare.scrolled_for.replace(now) != now {
            scroll.set_offset(point(px(0.), px(0.)));
        }
        let offset = -f32::from(scroll.offset().y);
        let max = f32::from(scroll.max_offset().y);
        let hover = ElementId::from((ElementId::from(SCROLL), "hover"));
        let extent = ui::ScrollExtent { viewport: f32::from(scroll.bounds().size.height), max, scrolled: offset };
        div()
            .id(hover.clone())
            .relative()
            .w_full()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .on_hover(ui::hover_tracker(hover.clone()))
            .child(
                div()
                    .id(SCROLL)
                    .w_full()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .track_scroll(scroll)
                    .on_scroll(cx.listener(|_, _: &ScrollEvent, _, cx| cx.notify()))
                    .child(content),
            )
            .when(offset > 1., |d| {
                d.child(edge_fade(Edge::Top, p.card, FADE, 0.))
                    .child(div().absolute().top_0().left_0().right_0().h(px(1.)).bg(p.border))
            })
            .when(max - offset > 1., |d| d.child(edge_fade(Edge::Bottom, p.card, FADE, 0.)))
            .child(ui::overlay_thumb((ElementId::from(SCROLL), "thumb"), &p, ui::is_hovered(&hover), extent, 0.))
            .into_any_element()
    }

    fn compare_picker(&self, c: &Ctx, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = c.p;
        let full = self.compare.chosen.len() >= MAX_CONTENDERS;
        let group = |title: Tr, local: bool, cx: &mut Context<Self>| {
            let chips: Vec<AnyElement> = self
                .compare
                .choices
                .iter()
                .filter(|choice| choice.contender.is_local() == local)
                .enumerate()
                .map(|(index, choice)| {
                    let chip_id =
                        ElementId::from((if local { "models-compare-local" } else { "models-compare-cloud" }, index));
                    let ring_id = ElementId::from((chip_id.clone(), "ring"));
                    let on = self.compare.chosen.contains(&choice.contender);
                    let hover = if ui::is_hovered(&chip_id) && !on { 1. } else { 0. };
                    let (chosen, fading) =
                        ui::fade_value(&ElementId::from((chip_id.clone(), "chosen")), f32::from(on), ui::HOVER_MS);
                    if fading {
                        window.request_animation_frame();
                    }
                    let ring = div()
                        .size(px(24.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(ui::selection_ring(&p, ring_id, hover, chosen));
                    let surface = ui::chip_surface(&p, chip_id, choice.name.clone().into(), chosen);
                    if let Some(reason) = choice.unavailable {
                        let reason =
                            text(c.t(reason), 12., FontWeight::MEDIUM, p.muted).line_height(px(18.)).flex_none();
                        return surface.child(reason).opacity(0.45).into_any_element();
                    }
                    let chip = surface.child(ring);
                    if !on && full {
                        return chip.opacity(0.45).into_any_element();
                    }
                    let contender = choice.contender.clone();
                    chip.cursor_pointer()
                        .on_click(ui::click_cb(cbk(cx, move |this: &mut Self, _, cx| {
                            this.toggle_compare_choice(contender.clone(), cx)
                        })))
                        .into_any_element()
                })
                .collect();
            (!chips.is_empty()).then(|| {
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(text(c.t(title), 12., FontWeight::MEDIUM, p.muted).line_height(px(18.)))
                    .child(div().w_full().grid().grid_cols(2).gap(px(CHIP_GAP)).children(chips))
            })
        };
        let local = group(Tr::ModelsLocal, true, cx);
        let cloud = group(Tr::ModelsProvider, false, cx)
            .map(|group| group.child(ui::muted12(&p, c.t(Tr::CompareCloudNote)).line_height(px(18.))));
        div().w_full().flex().flex_col().gap(px(16.)).children(local).children(cloud).into_any_element()
    }

    /// One height while idle and while recording, so the dialog does not jump when the recording starts.
    fn record_panel(&self, c: &Ctx, cx: &mut Context<Self>) -> AnyElement {
        let p = c.p;
        let (levels, recording, caption) = match &self.compare.stage {
            Stage::Recording { started, levels, .. } => {
                let seconds = started.elapsed().as_secs();
                (levels.iter().copied().collect(), true, format!("{}:{:02}", seconds / 60, seconds % 60))
            }
            _ => (vec![0.; LEVEL_BARS], false, c.t(Tr::CompareRecordHint).to_string()),
        };
        let bars = levels.into_iter().map(|level: f32| {
            let height = 4. + (level * 1.6).clamp(0., 1.) * 28.;
            div().w(px(3.)).h(px(height)).rounded(px(1.5)).bg(if recording { p.primary } else { p.border_strong })
        });
        let button = if recording {
            ui::button_with(
                "models-compare-stop",
                &p,
                &c.t(Tr::CompareStopRecording),
                BtnKind::Primary,
                ui::BtnSize::Large,
                Some(Ic::Stop),
                None,
                cbk(cx, |this: &mut Self, _, cx| this.stop_compare_recording(cx)),
            )
        } else {
            ui::button_with(
                "models-compare-record",
                &p,
                &c.t(Tr::CompareRecord),
                BtnKind::Primary,
                ui::BtnSize::Large,
                Some(Ic::Mic),
                None,
                cbk(cx, |this: &mut Self, _, cx| this.start_compare_recording(cx)),
            )
        };
        let caption = if recording {
            text(caption, 13., FontWeight::MEDIUM, p.muted)
        } else {
            text(caption, 12., FontWeight::NORMAL, p.muted)
        };
        div()
            .w_full()
            .p(px(28.))
            .rounded(px(12.))
            .border_1()
            .border_color(p.border)
            .bg(p.input_bg)
            .flex()
            .flex_col()
            .items_center()
            .gap(px(12.))
            .child(div().h(px(32.)).flex().items_center().gap(px(3.)).children(bars))
            .child(button)
            .child(caption.line_height(px(20.)))
            .into_any_element()
    }

    fn compare_results(&self, c: &Ctx, rating: bool, cx: &mut Context<Self>) -> AnyElement {
        let p = c.p;
        let cards = self.compare.shown.iter().enumerate().map(|(index, shown)| {
            let label = fmt::compare_result(c.lang, &asr_compare::label(index).to_string());
            let time = match &shown.answer {
                Answer::Text { millis, .. } => fmt::compare_seconds(c.lang, &seconds(*millis)),
                _ => String::new(),
            };
            let (body, body_color) = match &shown.answer {
                Answer::Text { text, .. } => (text.clone(), p.text),
                Answer::Running => (c.t(Tr::CompareRecognizing).to_string(), p.muted),
                Answer::Waiting => (c.t(Tr::CompareWaiting).to_string(), p.muted),
                Answer::Failed => (c.t(Tr::CompareFailed).to_string(), p.muted),
            };
            let toggles = (rating && matches!(shown.answer, Answer::Text { .. })).then(|| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(rating_toggle(c, index, Rating::Good, shown.rating, cx))
                    .child(rating_toggle(c, index, Rating::Poor, shown.rating, cx))
            });
            div()
                .w_full()
                .px(px(16.))
                .py(px(14.))
                .rounded(px(12.))
                .border_1()
                .border_color(p.border)
                .bg(p.input_bg)
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .w_full()
                        .min_h(px(28.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(text(label, 13., FontWeight::MEDIUM, p.text).line_height(px(20.)))
                        .child(text(time, 12., FontWeight::NORMAL, p.muted).line_height(px(18.)).flex_1())
                        .children(toggles),
                )
                .child(text(body, 14., FontWeight::NORMAL, body_color).line_height(px(22.)))
        });
        div().w_full().flex().flex_col().gap(px(10.)).children(cards).into_any_element()
    }

    fn compare_ranking(&self, c: &Ctx, ranking: &[(Contender, Tally)], cx: &mut Context<Self>) -> AnyElement {
        let p = c.p;
        let in_use = Contender::in_use(&self.st.cfg.get().asr);
        let rows = ranking.iter().enumerate().map(|(index, (contender, tally))| {
            let first = index == 0;
            let average = tally.average_ms().map_or_else(|| "—".to_owned(), seconds);
            let score = fmt::compare_score(c.lang, &tally.good.to_string(), &tally.poor.to_string(), &average);
            let source = c.t(if contender.is_local() { Tr::ModelsLocal } else { Tr::ModelsProvider });
            let action = if in_use.as_ref() == Some(contender) {
                Some(text(c.t(Tr::ModelsInUse), 12., FontWeight::MEDIUM, p.success_text).into_any_element())
            } else if first {
                let contender = contender.clone();
                Some(
                    ui::button(
                        "models-compare-use",
                        &p,
                        &c.t(Tr::ModelsUse),
                        BtnKind::Secondary,
                        None,
                        cbk(cx, move |this: &mut Self, _, cx| this.use_compared(contender.clone(), cx)),
                    )
                    .into_any_element(),
                )
            } else {
                None
            };
            div()
                .w_full()
                .px(px(16.))
                .py(px(12.))
                .rounded(px(12.))
                .border_1()
                .border_color(if first { p.accent } else { p.border })
                .bg(if first { p.accent_soft } else { p.input_bg })
                .flex()
                .items_center()
                .gap(px(14.))
                .child(
                    div().w(px(20.)).flex_none().flex().justify_center().child(
                        text(
                            (index + 1).to_string(),
                            20.,
                            FontWeight::SEMIBOLD,
                            if first { p.accent_text } else { p.muted },
                        )
                        .line_height(px(28.)),
                    ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10.))
                                .child(text(self.compare_name(contender), 14., FontWeight::MEDIUM, p.text).truncate())
                                .child(ui::source_label(&p, contender.is_local(), source)),
                        )
                        .child(ui::muted12(&p, score).line_height(px(18.))),
                )
                .children(action)
        });
        div().w_full().flex().flex_col().gap(px(10.)).children(rows).into_any_element()
    }
}

fn description(c: &Ctx, label: SharedString) -> AnyElement {
    text(label, 13., FontWeight::NORMAL, c.p.muted).line_height(px(20.)).into_any_element()
}

fn seconds(millis: u64) -> String {
    format!("{:.1}", millis as f64 / 1000.)
}

/// Good and Poor share one selected look: neither answer should read as an alarm.
fn rating_toggle(
    c: &Ctx,
    index: usize,
    rating: Rating,
    current: Option<Rating>,
    cx: &mut Context<ModelsPage>,
) -> impl IntoElement {
    let p = c.p;
    let (name, label) = match rating {
        Rating::Good => ("good", Tr::CompareGood),
        Rating::Poor => ("poor", Tr::ComparePoor),
    };
    let id = ElementId::from((ElementId::from(("models-compare-rate", index)), name));
    let on = current == Some(rating);
    div()
        .id(id.clone())
        .relative()
        .h(px(28.))
        .px(px(12.))
        .flex()
        .items_center()
        .rounded_full()
        .border_1()
        .border_color(if on { p.accent } else { p.border })
        .bg(if on { p.accent_soft } else { p.card })
        .cursor_pointer()
        .on_hover(ui::hover_tracker(id.clone()))
        .on_click(ui::click_cb(cbk(cx, move |this: &mut ModelsPage, _, cx| {
            this.rate_compare_result(index, rating, cx)
        })))
        .child(ui::hover_fill_if(&id, None, p.seg, !on))
        .child(text(c.t(label), 12., FontWeight::MEDIUM, if on { p.accent_text } else { p.text }).line_height(px(18.)))
}

#[cfg(feature = "screenshots")]
impl ModelsPage {
    /// Steps of the screenshot harness. A shown round can be filled with stand-in results, one line of `text` each,
    /// since the mock build has no local model on disk to transcribe with.
    pub(super) fn probe_compare(
        &mut self,
        case: &str,
        arg: &str,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match case {
            "compare-installed" => {
                let count: usize = arg.parse().expect("a model count");
                let specs = self.st.models.catalog().iter().filter(|spec| spec.kind == ModelKind::Asr);
                self.preview_snapshot = Some(Arc::new(
                    specs
                        .enumerate()
                        .map(|(index, spec)| {
                            let state = if index < count { ModelState::Installed } else { ModelState::NotInstalled };
                            (spec.id.clone(), state)
                        })
                        .collect(),
                ));
                cx.notify();
            }
            "compare-open" => self.open_compare(window, cx),
            "compare-toggle" => {
                let index: usize = arg.parse().expect("a choice index");
                let contender = self.compare.choices[index].contender.clone();
                self.toggle_compare_choice(contender, cx);
            }
            "compare-begin" => self.begin_compare(cx),
            "compare-record" => self.start_compare_recording(cx),
            "compare-stop" => self.stop_compare_recording(cx),
            "compare-stand-in" => {
                let texts: Vec<&str> = text.lines().collect();
                let millis = [800, 1100, 400, 600, 900];
                let recognizing = arg == "recognizing";
                self.compare.shown = self
                    .compare
                    .chosen
                    .iter()
                    .enumerate()
                    .map(|(index, contender)| Shown {
                        contender: contender.clone(),
                        answer: match (recognizing, index) {
                            (true, 1) => Answer::Running,
                            (true, 2..) => Answer::Waiting,
                            _ => {
                                Answer::Text { text: texts[index % texts.len()].to_owned(), millis: millis[index % 5] }
                            }
                        },
                        rating: None,
                    })
                    .collect();
                self.compare.stage = if recognizing {
                    Stage::Recognizing {
                        _mic: MicHold { st: self.st.clone(), recording: false },
                        _task: Task::ready(()),
                    }
                } else {
                    Stage::Rate
                };
                cx.notify();
            }
            "compare-rate" => {
                let (index, rating) = arg.split_once(':').expect("index:good or index:poor");
                let rating = if rating == "good" { Rating::Good } else { Rating::Poor };
                self.rate_compare_result(index.parse().expect("a result index"), rating, cx);
            }
            "compare-scroll" => {
                let fraction: f32 = arg.parse().expect("a scroll fraction");
                let max = self.compare.scroll.max_offset().y;
                self.compare.scroll.set_offset(point(px(0.), -max * fraction));
                cx.notify();
            }
            "compare-next" => self.next_compare_round(cx),
            "compare-finish" => self.finish_compare(cx),
            "compare-close" => self.close_compare(window, cx),
            other => panic!("unknown comparison probe step {other:?}"),
        }
    }
}
