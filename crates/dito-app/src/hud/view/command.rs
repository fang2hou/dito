//! The windows a command opens above the capsule (`design:hud/command`): the choice window, the instruction field, the
//! text window and the answer and confirm windows, with the round actions floating over them.
//!
//! Only the windows with text take the keyboard: the panel becomes key while one is open (ADR-0004) and gives key
//! status back before the core writes into the source app.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use dito_core::controller::{Choice, Command, HudState, ResultWrite};
use dito_core::i18n::{Lang, Tr};
use dito_core::intent::Intent;
use dito_platform::system::HudKeyChange;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::assets::Ic;
use crate::hud::capsule::{self, WIDEN_S};
use crate::hud::motion::Spring;
use crate::hud::{px, tx};
use crate::theme::{self, Palette, hex};
use crate::ui::{self, InputEvent, TextAreaStyle, TextareaState, hex_alpha, mix_rgb, text};

use super::HudView;
use super::layout::{COMMAND_BOTTOM, COMMAND_GAP, COMMAND_MAX_H};

const WINDOW_RADIUS: f32 = 16.;
const WINDOW_PAD: f32 = 14.;
const WINDOW_GAP: f32 = 12.;
const WINDOW_W: f32 = 300.;
const RESULT_W: f32 = 320.;
const INNER_W: f32 = WINDOW_W - 2. * WINDOW_PAD;
const ACTION: f32 = 30.;
const ACTION_GAP: f32 = 10.;

const TEXT_SIZE: f32 = 13.;
const LINE_H: f32 = TEXT_SIZE * 1.6;
const FIELD_PAD_X: f32 = 12.;
const FIELD_RADIUS: f32 = 10.;
const FIELD_EDGE: f32 = 1.;
const ONE_LINE_H: f32 = 36.;
const SEND_GAP: f32 = 8.;
/// The one-line field leaves room for Send to its right.
const ONE_LINE_W: f32 = INNER_W - SEND_GAP - ACTION;
/// A field of two or more lines keeps Send in its corner, on a row of its own under the text.
const SEND_INSET: f32 = 8.;
const SEND_ROW: f32 = 4. + ACTION + SEND_INSET;
const MAX_FIELD_LINES: usize = 3;
const RESULT_ROWS: usize = 9;

const CHOICE_H: f32 = 72.;
const CHOICE_GAP: f32 = 8.;
const CHOICE_MIN_W: f32 = 68.;

/// Where a window asked for keyboard focus.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Instruction,
    Text,
    Result,
}

/// What the HUD view keeps for command windows; created with the first one, because its fields need a window.
pub(super) struct CommandUi {
    instruction: Entity<TextareaState>,
    text: Entity<TextareaState>,
    result: Entity<TextareaState>,
    /// The window and action bounds of the last frame, which the pointer poll reads to know where clicks land.
    regions: Rc<RefCell<Vec<Bounds<Pixels>>>>,
    /// The state the fields were last prepared for.
    prepared: Option<HudState>,
    wants_focus: Option<Field>,
    /// The panel was last asked to be key.
    key_held: bool,
    /// A command that waits for the panel to give key status back.
    deferred: Option<Command>,
    /// The field's pose: 0 one line, 1 two lines, 2 three lines or more.
    pose_from: Spring,
    pose_target: f32,
    pose_since: Instant,
}

/// What the placement loop does after its update: the key change first, then the command that waited for it.
pub(in crate::hud) struct KeyStep {
    pub(in crate::hud) change: Option<HudKeyChange>,
    pub(in crate::hud) then: Option<Command>,
}

fn takes_keyboard(state: &HudState) -> bool {
    matches!(
        state,
        HudState::TypingInstruction | HudState::NeedsText | HudState::Answer { .. } | HudState::Confirm { .. }
    )
}

fn is_command_window(state: &HudState) -> bool {
    takes_keyboard(state) || matches!(state, HudState::Choosing { .. })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tone {
    Outlined,
    Primary,
}

fn glow(p: &Palette) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: p.command_glow,
        offset: point(gpui_kit::px(0.), gpui_kit::px(0.)),
        blur_radius: gpui_kit::px(14.),
        spread_radius: gpui_kit::px(0.),
        inset: false,
    }]
}

fn window_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: hex_alpha(hex("#000000"), 0.14),
        offset: point(gpui_kit::px(0.), gpui_kit::px(8.)),
        blur_radius: gpui_kit::px(24.),
        spread_radius: gpui_kit::px(0.),
        inset: false,
    }]
}

/// Surface, edge and glyph of a round action at hover position `t` (0 resting, 1 hovered).
fn action_face(p: &Palette, glyph: Ic, tone: Tone, t: f32) -> Div {
    let (bg, edge, ink) = match tone {
        Tone::Outlined => (
            mix_rgb(p.menu_bg, p.command_soft, t),
            Some(mix_rgb(p.border_strong, p.command_primary, t)),
            mix_rgb(p.text, p.command_primary, t),
        ),
        Tone::Primary => (mix_rgb(p.command_primary, p.command_primary_hover, t), None, p.on_primary),
    };
    let mut face = div().absolute().inset_0().rounded_full().bg(bg).flex().items_center().justify_center();
    if let Some(edge) = edge {
        face = face.border(gpui_kit::px(0.75)).border_color(edge);
    }
    face.child(ui::icon(glyph, 14., ink))
}

/// Widths of the choice buttons: shares of the row by probability, none under the minimum.
fn choice_widths(choices: &[Choice], row: f32) -> Vec<f32> {
    let n = choices.len();
    let avail = row - CHOICE_GAP * n.saturating_sub(1) as f32;
    let mut fixed = vec![false; n];
    let mut widths = vec![0.; n];
    loop {
        let free = avail - fixed.iter().filter(|f| **f).count() as f32 * CHOICE_MIN_W;
        let weight: f32 =
            choices.iter().zip(&fixed).filter(|(_, f)| !**f).map(|(c, _)| f32::from(c.percent).max(1.)).sum();
        let mut clamped = false;
        for (i, choice) in choices.iter().enumerate() {
            if fixed[i] {
                widths[i] = CHOICE_MIN_W;
                continue;
            }
            widths[i] = free * f32::from(choice.percent).max(1.) / weight;
            if widths[i] < CHOICE_MIN_W {
                fixed[i] = true;
                clamped = true;
            }
        }
        if !clamped {
            return widths;
        }
    }
}

fn field_lines(text: &str, width: f32) -> usize {
    ui::text_lines(text, ui::family_for(text), TEXT_SIZE, FontWeight::NORMAL, width)
}

/// 0 for one line; otherwise 1 for two and 2 for three or more. Whether the text needs a second line is decided at
/// the one-line width, then the rows are counted at the widened one, so growing never flips it back.
fn field_target(text: &str) -> f32 {
    if field_lines(text, ONE_LINE_W - 2. * FIELD_PAD_X - 2.) <= 1 {
        return 0.;
    }
    (field_lines(text, INNER_W - 2. * FIELD_PAD_X - 2.).clamp(2, MAX_FIELD_LINES) - 1) as f32
}

impl HudView {
    /// Creates the fields with the first command window; later states reuse them.
    fn ensure_command_ui(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.command.is_some() {
            return;
        }
        let lang = self.st.cfg.get().general.lang();
        // The fields send on Enter and scroll inside the height the view gives them; the result grows with its text.
        let field = |window: &mut Window, cx: &mut Context<Self>, placeholder: &'static str| {
            let state = cx.new(|cx| TextareaState::new(window, cx).placeholder(placeholder).submit_on_enter(true));
            cx.subscribe_in(&state, window, |this: &mut Self, _, ev: &InputEvent, _, cx| match ev {
                InputEvent::Change => cx.notify(),
                InputEvent::PressEnter { shift: false, .. } => this.submit_field(cx),
                _ => {}
            })
            .detach();
            state
        };
        let instruction = field(window, cx, lang.tr(Tr::HudInstructionPlaceholder));
        let text = field(window, cx, lang.tr(Tr::HudTextPlaceholder));
        let result = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, RESULT_ROWS));
        self.command = Some(CommandUi {
            instruction,
            text,
            result,
            regions: Rc::default(),
            prepared: None,
            wants_focus: None,
            key_held: false,
            deferred: None,
            pose_from: Spring::default(),
            pose_target: 0.,
            pose_since: Instant::now(),
        });
    }

    /// Puts the right text in the fields when a window opens. Runs in render, which has the window.
    fn prepare_command_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ui) = self.command.as_mut() else { return };
        if ui.prepared.as_ref() == Some(&self.state) {
            return;
        }
        ui.prepared = Some(self.state.clone());
        let (instruction, text, result) = (ui.instruction.clone(), ui.text.clone(), ui.result.clone());
        match &self.state {
            HudState::TypingInstruction => {
                instruction.update(cx, |s, cx| s.set_value("", window, cx));
                ui.wants_focus = Some(Field::Instruction);
                ui.reset_pose();
            }
            HudState::NeedsText => {
                text.update(cx, |s, cx| s.set_value("", window, cx));
                ui.wants_focus = Some(Field::Text);
                ui.reset_pose();
            }
            HudState::Answer { text: shown } | HudState::Confirm { text: shown, .. } => {
                let shown = shown.clone();
                result.update(cx, |s, cx| s.set_value(shown, window, cx));
                ui.wants_focus = Some(Field::Result);
            }
            _ => {}
        }
    }

    #[cfg(feature = "screenshots")]
    pub(super) fn type_into_field(&mut self, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_command_ui(window, cx);
        self.prepare_command_fields(window, cx);
        let Some(ui) = &self.command else { return };
        let field = if self.state == HudState::NeedsText { &ui.text } else { &ui.instruction };
        field.update(cx, |s, cx| s.set_value(value, window, cx));
    }

    /// Enter in the instruction field or the text window.
    fn submit_field(&mut self, cx: &mut Context<Self>) {
        let command = match &self.state {
            HudState::TypingInstruction => {
                let value = self.command.as_ref().map(|c| c.instruction.read(cx).value().to_string());
                value.map(|v| Command::SubmitInstruction(v.trim().to_string()))
            }
            HudState::NeedsText => {
                let value = self.command.as_ref().map(|c| c.text.read(cx).value().to_string());
                value.map(|v| Command::SubmitText(v.trim().to_string()))
            }
            _ => None,
        };
        match command {
            Some(Command::SubmitInstruction(v) | Command::SubmitText(v)) if v.is_empty() => {}
            Some(command) => self.send_after_key(command, cx),
            None => {}
        }
    }

    /// Sends a command that ends in a write to the source app: the panel gives key status back first, so the core's
    /// wait for the source app's focus does not outlast it.
    fn send_after_key(&mut self, command: Command, cx: &mut Context<Self>) {
        match self.command.as_mut() {
            Some(ui) if ui.key_held => ui.deferred = Some(command),
            _ => self.send(command),
        }
        cx.notify();
    }

    /// Called by the placement loop on every tick, where the native window is at hand. The change it returns runs
    /// after the app borrow is released.
    pub(in crate::hud) fn sync_key_status(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Option<bool>, Option<Command>) {
        let wants = takes_keyboard(&self.state);
        let Some(ui) = self.command.as_mut() else { return (None, None) };
        let then = ui.deferred.take();
        let wants = wants && then.is_none();
        let mut change = None;
        if wants != ui.key_held {
            ui.key_held = wants;
            change = Some(wants);
        }
        if wants && let Some(field) = ui.wants_focus.take() {
            let state = match field {
                Field::Instruction => &ui.instruction,
                Field::Text => &ui.text,
                Field::Result => &ui.result,
            };
            state.read(cx).focus_handle(cx).focus(window, cx);
        }
        (change, then)
    }

    /// Whether a field that has the keyboard holds text an input method is composing. Esc then cancels that text
    /// instead of closing the window, so the hotkey tap leaves Esc alone.
    pub(in crate::hud) fn composing(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(ui) = self.command.as_ref().filter(|ui| ui.key_held) else { return false };
        [&ui.instruction, &ui.text, &ui.result]
            .into_iter()
            .any(|field| field.update(cx, |field, cx| field.marked_text_range(window, cx).is_some()))
    }

    /// Whether the pointer is over a command window or one of its actions.
    pub(super) fn over_command_region(&self, x: f32, y: f32) -> bool {
        self.command.as_ref().is_some_and(|ui| {
            ui.regions.borrow().iter().any(|b| {
                x >= f32::from(b.origin.x)
                    && x <= f32::from(b.origin.x + b.size.width)
                    && y >= f32::from(b.origin.y)
                    && y <= f32::from(b.origin.y + b.size.height)
            })
        })
    }

    /// The field's pose eases like the capsule's width; retargets start from the pose it is in.
    fn field_pose(&mut self, target: f32) -> f32 {
        let Some(ui) = self.command.as_mut() else { return target };
        if ui.pose_target != target {
            let now = ui.pose_since.elapsed().as_secs_f32();
            ui.pose_from = capsule::width_at(ui.pose_from, ui.pose_target, now);
            ui.pose_target = target;
            ui.pose_since = Instant::now();
        }
        if self.freeze.is_some() || ui::Motion::system().is_reduced() {
            ui.pose_from = Spring { position: target, velocity: 0. };
            return target;
        }
        capsule::width_at(ui.pose_from, ui.pose_target, ui.pose_since.elapsed().as_secs_f32()).position
    }

    pub(super) fn field_moving(&self) -> bool {
        self.freeze.is_none() && self.command.as_ref().is_some_and(|ui| ui.pose_since.elapsed().as_secs_f32() < WIDEN_S)
    }

    /// The stack of actions and window above the capsule, `None` when the state has no window.
    pub(super) fn command_layer(
        &mut self,
        p: &Palette,
        lang: Lang,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        if !is_command_window(&self.state) {
            if let Some(ui) = &self.command {
                ui.regions.borrow_mut().clear();
            }
            return None;
        }
        // The editors read their colors from the shared theme, which a settings window sets from its own appearance.
        theme::apply(p, cx);
        let accent = ui::Theme::global_mut(cx);
        accent.colors.caret = p.command_primary;
        accent.colors.selection = hex_alpha(p.command_primary, 0.3);
        self.ensure_command_ui(window, cx);
        self.prepare_command_fields(window, cx);
        let regions = self.command.as_ref()?.regions.clone();
        regions.borrow_mut().clear();
        let state = self.state.clone();
        let (actions, body) = match &state {
            HudState::Choosing { choices } => {
                (vec![self.close_action(p, lang, cx)], self.choice_window(p, lang, choices, &regions, cx))
            }
            HudState::TypingInstruction => {
                (vec![self.close_action(p, lang, cx)], self.field_window(p, lang, false, &regions, cx))
            }
            HudState::NeedsText => {
                (vec![self.close_action(p, lang, cx)], self.field_window(p, lang, true, &regions, cx))
            }
            HudState::Answer { .. } => (
                vec![
                    self.copy_action(p, lang, Tone::Outlined, cx),
                    self.primary_action(p, lang, "hud-ok", Ic::Check, Tr::HudOk, cx, |v, _, _| {
                        v.send(Command::Dismiss)
                    }),
                ],
                self.result_window(p, false, &regions, cx),
            ),
            HudState::Confirm { write: ResultWrite::CopyOnly, .. } => (
                vec![self.close_action(p, lang, cx), self.copy_action(p, lang, Tone::Primary, cx)],
                self.result_window(p, true, &regions, cx),
            ),
            HudState::Confirm { .. } => (
                vec![
                    self.close_action(p, lang, cx),
                    self.copy_action(p, lang, Tone::Outlined, cx),
                    self.primary_action(p, lang, "hud-replace", Ic::ArrowLeftRight, Tr::HudReplace, cx, |v, _, cx| {
                        let text = v.result_text(cx);
                        v.send_after_key(Command::Replace(text), cx);
                    }),
                ],
                self.result_window(p, true, &regions, cx),
            ),
            _ => return None,
        };
        let actions =
            div().relative().flex().items_center().gap(px(ACTION_GAP)).children(actions).child(probe(&regions));
        Some(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(COMMAND_BOTTOM))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(COMMAND_GAP))
                .child(actions)
                .child(body),
        )
    }

    fn result_text(&self, cx: &App) -> String {
        self.command.as_ref().map(|c| c.result.read(cx).value().to_string()).unwrap_or_default()
    }

    fn close_action(&self, p: &Palette, lang: Lang, cx: &mut Context<Self>) -> AnyElement {
        self.action(p, "hud-close", Ic::X, lang.tr(Tr::HudClose), Tone::Outlined, cx, |v, _, _| {
            v.send(Command::Dismiss)
        })
    }

    /// Primary when nothing else can be done with the result (a reply is only copied).
    fn copy_action(&self, p: &Palette, lang: Lang, tone: Tone, cx: &mut Context<Self>) -> AnyElement {
        self.action(p, "hud-copy", Ic::Copy, lang.tr(Tr::HudCopy), tone, cx, |v, _, cx| {
            let text = v.result_text(cx);
            v.send(Command::CopyResult(text));
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn primary_action(
        &self,
        p: &Palette,
        lang: Lang,
        id: &'static str,
        glyph: Ic,
        label: Tr,
        cx: &mut Context<Self>,
        on: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        self.action(p, id, glyph, lang.tr(label), Tone::Primary, cx, on)
    }

    /// A 30 pt round action with the glow, which names itself in the standard tooltip on hover.
    #[allow(clippy::too_many_arguments)]
    fn action(
        &self,
        p: &Palette,
        id: &'static str,
        glyph: Ic,
        label: &str,
        tone: Tone,
        cx: &mut Context<Self>,
        on: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let key = ElementId::from(id);
        ui::tip_label(key.clone(), SharedString::from(label.to_string()));
        let target = f32::from(ui::is_hovered(&key));
        let face_palette = *p;
        let face = ui::tweened(ElementId::from((key.clone(), "face")), target, ui::HOVER_MS, move |t| {
            action_face(&face_palette, glyph, tone, t)
        });
        let weak = cx.entity().downgrade();
        div()
            .id(id)
            .size(px(ACTION))
            .flex_none()
            .relative()
            .rounded_full()
            .cursor_pointer()
            .shadow(glow(p))
            .on_hover(ui::hover_tracker(key.clone()))
            .on_click(move |_, window, app| {
                weak.update(app, |v, cx| on(v, window, cx)).ok();
            })
            .child(face)
            .child(ui::tip_above(p, &key))
            .into_any_element()
    }

    /// The surface every command window shares, with the probe that reports its bounds to the hit test.
    fn window_surface(p: &Palette, width: f32, regions: &Rc<RefCell<Vec<Bounds<Pixels>>>>) -> Div {
        div()
            .relative()
            .w(px(width))
            .max_h(px(COMMAND_MAX_H))
            .flex_none()
            .rounded(px(WINDOW_RADIUS))
            .bg(p.menu_bg)
            .border(px(0.75))
            .border_color(p.border_strong)
            .shadow(window_shadow())
            .p(px(WINDOW_PAD))
            .flex()
            .flex_col()
            .gap(px(WINDOW_GAP))
            .child(probe(regions))
    }

    fn choice_window(
        &self,
        p: &Palette,
        lang: Lang,
        choices: &[Choice],
        regions: &Rc<RefCell<Vec<Bounds<Pixels>>>>,
        cx: &mut Context<Self>,
    ) -> Div {
        let widths = choice_widths(choices, INNER_W);
        let buttons = choices
            .iter()
            .zip(widths)
            .enumerate()
            .map(|(rank, (choice, width))| self.choice_button(p, lang, rank, *choice, width, cx));
        Self::window_surface(p, WINDOW_W, regions)
            .child(div().w_full().flex().justify_center().child(text(
                lang.tr(Tr::HudChoiceQuestion),
                tx(13.),
                FontWeight::MEDIUM,
                p.text,
            )))
            .child(div().flex().gap(px(CHOICE_GAP)).children(buttons))
    }

    fn choice_button(
        &self,
        p: &Palette,
        lang: Lang,
        rank: usize,
        choice: Choice,
        width: f32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let likeliest = rank == 0;
        let id = ["hud-choice-0", "hud-choice-1", "hud-choice-2"][rank.min(2)];
        let key = ElementId::from(id);
        let target = f32::from(ui::is_hovered(&key));
        let palette = *p;
        let face = ui::tweened(ElementId::from((key.clone(), "face")), target, ui::HOVER_MS, move |t| {
            let (bg, edge) = if likeliest {
                (mix_rgb(palette.command_soft, palette.command_soft_hover, t), palette.command_primary)
            } else {
                (
                    mix_rgb(palette.card, palette.command_soft, t),
                    mix_rgb(palette.border_strong, palette.command_primary, t),
                )
            };
            div().absolute().inset_0().rounded(gpui_kit::px(12.)).bg(bg).border(gpui_kit::px(1.)).border_color(edge)
        });
        let (ink, quiet) = if likeliest { (p.command_primary, p.command_primary) } else { (p.text, p.muted) };
        let label_size = [17., 14., 12.][rank.min(2)];
        let label = match choice.intent {
            Intent::Polish => Tr::HudIntentPolish,
            Intent::Rewrite => Tr::HudIntentRewrite,
            Intent::Translate => Tr::HudIntentTranslate,
            Intent::Summarize => Tr::HudIntentSummarize,
            Intent::Reply => Tr::HudIntentReply,
            // The core never offers cancel or none.
            Intent::Ask | Intent::Cancel | Intent::None => Tr::HudIntentAsk,
        };
        let percent = text(
            format!("{}%", choice.percent),
            tx(if likeliest { 12. } else { 11. }),
            if likeliest { FontWeight::MEDIUM } else { FontWeight::NORMAL },
            quiet,
        )
        .font_features(FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)])));
        let weak = cx.entity().downgrade();
        let intent = choice.intent;
        div()
            .id(id)
            .relative()
            .w(px(width))
            .h(px(CHOICE_H))
            .flex_none()
            .cursor_pointer()
            .on_hover(ui::hover_tracker(key))
            .on_click(move |_, _, app| {
                weak.update(app, |v, cx| v.send_after_key(Command::Choose(intent), cx)).ok();
            })
            .child(face)
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .pt(px(10.))
                    .pl(px(10.))
                    .pr(px(10.))
                    .pb(px(8.))
                    .flex()
                    .flex_col()
                    .justify_between()
                    .child(text(lang.tr(label), tx(label_size), FontWeight::MEDIUM, ink))
                    .child(div().flex().justify_end().child(percent)),
            )
    }

    /// The instruction field, or the text window (the same field under a prompt).
    fn field_window(
        &mut self,
        p: &Palette,
        lang: Lang,
        prompted: bool,
        regions: &Rc<RefCell<Vec<Bounds<Pixels>>>>,
        cx: &mut Context<Self>,
    ) -> Div {
        let Some(ui) = self.command.as_ref() else { return div() };
        let state = if prompted { ui.text.clone() } else { ui.instruction.clone() };
        let value = state.read(cx).value().to_string();
        let pose = self.field_pose(field_target(&value)).clamp(0., 2.05);
        let e = pose.min(1.);
        let rows_h = (1. + pose) * LINE_H;
        // The pads are inside the 1 pt edge; one line sits centered in the field's outer height.
        let one_line_pad = 0.5 * (ONE_LINE_H - 2. * FIELD_EDGE - LINE_H);
        let pad_top = lerp(one_line_pad, 12., e);
        let bottom = lerp(one_line_pad, SEND_ROW, e);
        let height = lerp(2. * FIELD_EDGE, 0., e) + pad_top + rows_h + bottom;
        let width = lerp(ONE_LINE_W, INNER_W, e);
        let send_left = lerp(ONE_LINE_W + SEND_GAP, INNER_W - SEND_INSET - ACTION, e);
        let send_top = lerp(0.5 * (ONE_LINE_H - ACTION), height - SEND_INSET - ACTION, e);
        let area = ui::text_area(
            &state,
            TextAreaStyle {
                bg: p.input_bg,
                edge: Some(p.command_primary),
                radius: FIELD_RADIUS,
                text_size: TEXT_SIZE,
                line_height: LINE_H,
                pad: [FIELD_PAD_X, pad_top, FIELD_PAD_X, 0.],
                fill: true,
                reserve_bottom: bottom,
            },
        );
        let weak = cx.entity().downgrade();
        let send = self.send_button(p, lang, move |app| {
            weak.update(app, |v, cx| v.submit_field(cx)).ok();
        });
        let row = div()
            .relative()
            .w(px(INNER_W))
            .h(px(height))
            .flex_none()
            .child(div().absolute().top_0().left_0().w(px(width)).h(px(height)).child(area))
            .child(div().absolute().left(px(send_left)).top(px(send_top)).child(send));
        let mut window = Self::window_surface(p, WINDOW_W, regions);
        if prompted {
            window = window.child(div().w_full().flex().justify_center().child(text(
                lang.tr(Tr::HudTextPrompt),
                tx(13.),
                FontWeight::MEDIUM,
                p.text,
            )));
        }
        window.child(row)
    }

    fn send_button(&self, p: &Palette, lang: Lang, on: impl Fn(&mut App) + 'static) -> AnyElement {
        let key = ElementId::from("hud-send");
        ui::tip_label(key.clone(), SharedString::from(lang.tr(Tr::HudSend).to_string()));
        let target = f32::from(ui::is_hovered(&key));
        let palette = *p;
        let face = ui::tweened(ElementId::from((key.clone(), "face")), target, ui::HOVER_MS, move |t| {
            action_face(&palette, Ic::Send, Tone::Primary, t)
        });
        div()
            .id("hud-send")
            .size(px(ACTION))
            .flex_none()
            .relative()
            .rounded_full()
            .cursor_pointer()
            .on_hover(ui::hover_tracker(key.clone()))
            .on_click(move |_, _, app| on(app))
            .child(face)
            .child(ui::tip_above(p, &key))
            .into_any_element()
    }

    fn result_window(
        &self,
        p: &Palette,
        confirm: bool,
        regions: &Rc<RefCell<Vec<Bounds<Pixels>>>>,
        cx: &mut Context<Self>,
    ) -> Div {
        let _ = cx;
        let Some(ui) = self.command.as_ref() else { return div() };
        let area = ui::text_area(
            &ui.result,
            TextAreaStyle {
                bg: if confirm { p.command_soft } else { p.seg },
                edge: confirm.then_some(p.command_primary),
                radius: FIELD_RADIUS,
                text_size: TEXT_SIZE,
                line_height: LINE_H,
                pad: [12., 12., 12., 12.],
                fill: false,
                reserve_bottom: 0.,
            },
        );
        Self::window_surface(p, RESULT_W, regions).child(area)
    }
}

impl CommandUi {
    fn reset_pose(&mut self) {
        self.pose_from = Spring::default();
        self.pose_target = 0.;
        self.pose_since = Instant::now();
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Reports its parent's bounds to the hit test while the parent is on screen.
fn probe(regions: &Rc<RefCell<Vec<Bounds<Pixels>>>>) -> impl IntoElement {
    let regions = regions.clone();
    canvas(move |bounds, _, _| regions.borrow_mut().push(bounds), |_, _, _, _| {}).absolute().inset_0()
}

#[cfg(test)]
mod tests {
    use dito_core::controller::Choice;
    use dito_core::intent::Intent;

    use super::{INNER_W, choice_widths};

    #[test]
    fn choice_buttons_share_the_row_by_probability_with_a_minimum() {
        let choice = |intent, percent| Choice { intent, percent };
        let widths = choice_widths(
            &[choice(Intent::Summarize, 62), choice(Intent::Translate, 27), choice(Intent::Rewrite, 11)],
            INNER_W,
        );
        assert_eq!(widths.iter().map(|w| w.round()).collect::<Vec<_>>(), [120., 68., 68.]);
        let even = choice_widths(&[choice(Intent::Polish, 50), choice(Intent::Rewrite, 50)], INNER_W);
        assert!((even[0] - even[1]).abs() < 0.01 && (even[0] * 2. + 8. - INNER_W).abs() < 0.01);
    }
}
