//! The setup wizard's "how to use" demo: a key, an arrow, the real HUD capsule and the phase of the recording
//! that is happening, looping on its own.
//!
//! [`usage_demo`] is a plain element. The loop's clock is the element's own state (kept by gpui across frames
//! under the element's id), so the caller holds nothing: it renders the demo every frame like any other element and
//! passes the mode. The clock stops while the window is inactive and resumes from the same phase, restarts at 0
//! when the mode changes (the old picture cross-fades into the new one), and under Reduce Motion the loop is
//! replaced by one still: keyframe 2, the recording.
//!
//! The sequence follows the storyboard (design 22.09 Usage Demo). Times are seconds
//! into the loop; the key always leads the HUD by 80 ms:
//!
//! | time        | toggle                                       | hold                                          |
//! |-------------|----------------------------------------------|-----------------------------------------------|
//! | 0.6         | tap the key; HUD pops in listening (83 pt)   | key goes down and stays; HUD pops in (56 pt)  |
//! | 1.2 – 2.8   | key up, waveform loops                       | key down, waveform loops                      |
//! | 3.0         | tap again; HUD turns to recognizing          | key up; HUD turns to recognizing, 56 → 83 pt  |
//! | 3.2 – 4.7   | progress fill runs its 1.5 s ease-out        | the same                                      |
//! | 4.8         | capsule cross-fades to green, inserted       | the same                                      |
//! | 5.6         | after a 0.8 s rest, 200 ms back to the start | the same                                      |

use std::time::Instant;

use dito_core::config::RecordMode;
use dito_core::controller::HudState;
use dito_core::i18n::{Lang, Tr};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::assets::Ic;
use crate::hud::{self, Btn, Capsule, Scale, Wave};
use crate::theme::Palette;
use crate::ui::{self, text};

/// First key press, in both modes.
const T_PRESS: f32 = 0.6;
/// Toggle: the second press. Hold: the release.
const T_SECOND: f32 = 3.0;
/// The capsule turns green.
const T_INSERT: f32 = 4.8;
/// The end of the rest on the result; the picture then cross-fades back to the start.
const T_REST_END: f32 = 5.6;
const RESTART_S: f32 = 0.2;
/// One loop, in seconds.
const PERIOD: f32 = T_REST_END + RESTART_S;
/// The key leads the HUD by this much.
const LEAD: f32 = 0.08;
/// Key press: ease-out to the bottom; release: spring back.
const KEY_DOWN_S: f32 = 0.12;
const KEY_UP_S: f32 = 0.16;
/// A tap stays down this long once it has bottomed out.
const TAP_HOLD_S: f32 = 0.10;
/// The key shrinks to this scale when pressed.
const KEY_SCALE: f32 = 0.94;
/// The recognizing capsule cross-fades to the inserted one over this.
const INSERT_FADE_S: f32 = 0.16;
/// A phase chip fades its plate in and out over this.
const CHIP_FADE_S: f32 = 0.16;
/// Switching the mode cross-fades the old picture into the new one over this.
const SWITCH_S: f32 = 0.2;
/// Keyframe 2 (recording): the picture Reduce Motion holds.
const STILL_AT: f32 = 2.0;
/// Where each phase chip's highlight begins; it ends where the next begins (the last, at the end of the rest).
const CHIP_STARTS: [f32; 4] = [T_PRESS, 1.2, T_SECOND, T_INSERT];

fn smooth(u: f32) -> f32 {
    let u = u.clamp(0., 1.);
    u * u * (3. - 2. * u)
}

fn ease_out(u: f32) -> f32 {
    1. - (1. - u.clamp(0., 1.)).powi(3)
}

/// How far down the key is, 0–1: it eases down over [`KEY_DOWN_S`] from `down`, springs back over [`KEY_UP_S`] from `up`.
fn press(at: f32, down: f32, up: f32) -> f32 {
    if at < down {
        0.
    } else if at < up {
        ease_out((at - down) / KEY_DOWN_S)
    } else {
        (1. - ui::spring((at - up) / KEY_UP_S, false)).clamp(0., 1.)
    }
}

/// The capsule as it is at one instant.
struct HudAt {
    state: HudState,
    /// Seconds since `state` began.
    since: f32,
    /// Outer width in design points.
    w: f32,
    /// 0–1 through the pop-in while it is arriving.
    pop: Option<f32>,
    /// 0–1 cross-fade into the green inserted capsule.
    inserted: f32,
    /// 1 shown … 0 gone: the way back to the start fades it out.
    shown: f32,
}

/// What the stage shows at one instant, as continuous numbers: the loop's cross-fade back to the start is just these
/// running to their values at 0.
struct Scene {
    /// 0 up … 1 down.
    key: f32,
    hud: Option<HudAt>,
    /// How lit each phase chip is, 0–1.
    chips: [f32; 4],
}

impl Scene {
    fn at(mode: RecordMode, at: f32) -> Scene {
        let tap = KEY_DOWN_S + TAP_HOLD_S;
        let key = match mode {
            RecordMode::Hold => press(at, T_PRESS, T_SECOND),
            RecordMode::Toggle => press(at, T_PRESS, T_PRESS + tap).max(press(at, T_SECOND, T_SECOND + tap)),
        };
        let listen_at = T_PRESS + LEAD;
        let process_at = T_SECOND + LEAD;
        let listening = HudState::Listening { mode };
        let hud = (at >= listen_at).then(|| {
            let narrow = hud::rest_width(&listening);
            let (state, began, w) = if at < process_at {
                (listening, listen_at, narrow)
            } else {
                (HudState::Recognizing, process_at, hud::eased_width(narrow, hud::CAPSULE_W, at - process_at))
            };
            let pop = (at - listen_at) / (hud::POP_MS as f32 / 1000.);
            HudAt {
                state,
                since: at - began,
                w,
                pop: (pop < 1.).then_some(pop),
                inserted: ((at - T_INSERT) / INSERT_FADE_S).clamp(0., 1.),
                shown: 1. - smooth((at - T_REST_END) / RESTART_S),
            }
        });
        let mut chips = [0.; 4];
        for (i, chip) in chips.iter_mut().enumerate() {
            let fall = match CHIP_STARTS.get(i + 1) {
                Some(next) => smooth((at - next) / CHIP_FADE_S),
                None => smooth((at - T_REST_END) / RESTART_S),
            };
            *chip = smooth((at - CHIP_STARTS[i]) / CHIP_FADE_S) * (1. - fall);
        }
        Scene { key, hud, chips }
    }
}

/// One picture of the stage: `mode`'s loop `at` seconds in, drawn with `opacity`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Layer {
    mode: RecordMode,
    at: f32,
    opacity: f32,
}

/// What to draw this frame: the pictures to stack (the old one first, while a mode switch cross-fades), whether this
/// is the still of Reduce Motion (the waveform rests and a held key keeps its size, like the design's static frame),
/// and whether another frame is due.
struct Tick {
    layers: Vec<Layer>,
    still: bool,
    running: bool,
}

impl Tick {
    /// Keyframe 2, nothing moving.
    fn still(mode: RecordMode) -> Self {
        Self { layers: vec![Layer { mode, at: STILL_AT, opacity: 1. }], still: true, running: false }
    }
}

/// An outgoing mode's last picture, fading out.
struct Switch {
    mode: RecordMode,
    at: f32,
    since: Instant,
}

/// The loop's clock. gpui keeps it with the element across frames.
struct Clock {
    mode: RecordMode,
    /// Seconds into the loop. It only advances while the window is active, so the loop resumes where it paused.
    at: f32,
    /// When `at` last advanced; `None` while paused.
    last: Option<Instant>,
    switch: Option<Switch>,
}

impl Clock {
    fn new(mode: RecordMode) -> Self {
        Self { mode, at: 0., last: None, switch: None }
    }

    fn tick(&mut self, mode: RecordMode, now: Instant, active: bool, reduced: bool) -> Tick {
        if reduced {
            *self = Self::new(mode);
            return Tick::still(mode);
        }
        if mode != self.mode {
            // The old picture stays as it was and fades out, unless nobody is looking.
            let switch = active.then_some(Switch { mode: self.mode, at: self.at, since: now });
            *self = Self { switch, ..Self::new(mode) };
        }
        if active {
            if let Some(last) = self.last {
                self.at = (self.at + now.saturating_duration_since(last).as_secs_f32()) % PERIOD;
            }
            self.last = Some(now);
        } else {
            self.last = None;
        }
        let mut layers = vec![Layer { mode: self.mode, at: self.at, opacity: 1. }];
        if let Some(sw) = &self.switch {
            let u = now.saturating_duration_since(sw.since).as_secs_f32() / SWITCH_S;
            if u < 1. {
                let u = smooth(u);
                layers[0].opacity = u;
                layers.insert(0, Layer { mode: sw.mode, at: sw.at, opacity: 1. - u });
            } else {
                self.switch = None;
            }
        }
        Tick { layers, still: false, running: active }
    }
}

/// Screenshots: how a frozen demo is held.
#[cfg(feature = "screenshots")]
#[derive(Clone, Copy)]
pub enum Frozen {
    /// This many seconds into the loop.
    At(f32),
    /// The still Reduce Motion shows.
    Still,
}

#[cfg(feature = "screenshots")]
thread_local! {
    static FROZEN: std::cell::Cell<Option<Frozen>> = const { std::cell::Cell::new(None) };
}

/// Screenshots: hold every demo as `frozen` says (`None` lets them run again).
#[cfg(feature = "screenshots")]
pub fn freeze(frozen: Option<Frozen>) {
    FROZEN.with(|f| f.set(frozen));
}

#[cfg(feature = "screenshots")]
fn frozen(mode: RecordMode) -> Option<Tick> {
    FROZEN.with(|f| f.get()).map(|frozen| match frozen {
        Frozen::At(at) => Tick { layers: vec![Layer { mode, at, opacity: 1. }], still: false, running: false },
        Frozen::Still => Tick::still(mode),
    })
}

#[cfg(not(feature = "screenshots"))]
fn frozen(_mode: RecordMode) -> Option<Tick> {
    None
}

/// RGB blend of two colors (the palette's neutrals and golds sit too far apart in hue for a per-channel HSL blend).
fn lerp(a: Hsla, b: Hsla, u: f32) -> Hsla {
    let (a, b): (Rgba, Rgba) = (a.into(), b.into());
    Rgba { r: a.r + (b.r - a.r) * u, g: a.g + (b.g - a.g) * u, b: a.b + (b.b - a.b) * u, a: a.a + (b.a - a.a) * u }
        .into()
}

/// `s` glyph by glyph, `gap` px apart. Every glyph box is exactly as wide as its advance: gpui rounds the width of a
/// run of text up to whole pixels (an 11 px glyph measures 11.000001 and takes 12), which `ui::text_tracked` lets
/// pile up into ~9% extra spacing on top of the tracking — the design's widths are the plain advance plus the gap.
fn glyphs(s: &str, size: f32, weight: FontWeight, color: Hsla, gap: f32) -> Div {
    let mut row = div().flex().items_center().gap(px(gap));
    for ch in s.chars() {
        let advance = ui::text_width(&ch.to_string(), size, weight);
        row = row.child(text(ch.to_string(), size, weight, color).line_height(relative(1.)).w(px(advance)).flex_none());
    }
    row
}

/// `s` with the design's CJK letter-spacing `cjk_em`, applied where [`ui::tracking_for`] says so (like
/// [`ui::text_tracked`], but see [`glyphs`] for the widths).
fn spaced(s: &str, size: f32, weight: FontWeight, color: Hsla, cjk_em: f32) -> Div {
    let em = ui::tracking_for(s, cjk_em);
    if em == 0. {
        text(s.to_string(), size, weight, color).line_height(relative(1.))
    } else {
        glyphs(s, size, weight, color, size * em)
    }
}

/// Width of `s` set by [`spaced`].
fn spaced_width(s: &str, size: f32, weight: FontWeight, cjk_em: f32) -> f32 {
    let em = ui::tracking_for(s, cjk_em);
    if em == 0. {
        return ui::text_width(s, size, weight);
    }
    let advances: f32 = s.chars().map(|c| ui::text_width(&c.to_string(), size, weight)).sum();
    advances + size * em * s.chars().count().saturating_sub(1) as f32
}

/// A line of text centred in a box of fixed height, so every column lines up the way the design does.
fn line_box(content: Div, h: f32) -> Div {
    div().h(px(h)).flex().items_center().whitespace_nowrap().child(content)
}

fn key_column(p: &Palette, lang: Lang, key: &str, mode: RecordMode, scene: &Scene, still: bool) -> Div {
    let down = scene.key.clamp(0., 1.);
    // The press shrinks the key; a still has no motion, so a held key there only changes color.
    let k = if still { 1. } else { 1. - (1. - KEY_SCALE) * scene.key };
    let w = (spaced_width(key, 18., FontWeight::BOLD, 0.) + 28.).max(84.);
    let face = div()
        .w(px(w * k))
        .h(px(52. * k))
        .flex_none()
        .rounded(px(14. * k))
        .bg(lerp(p.card, p.text, down))
        .border(px(2.))
        .border_color(lerp(p.border_strong, p.text, down))
        .flex()
        .items_center()
        .justify_center()
        .child(text(key.to_string(), 18. * k, FontWeight::BOLD, lerp(p.text, p.bg, down)).line_height(relative(1.)));
    // Only a held key reads "held": a tap is over before the caption could say so.
    let (state, color) = if mode == RecordMode::Hold && down > 0.5 {
        (lang.tr(Tr::DemoHeldDown), lerp(p.muted, p.text, down))
    } else {
        (lang.tr(Tr::DemoReleased), lerp(p.muted, p.text, if mode == RecordMode::Hold { down } else { 0. }))
    };
    div()
        .min_w(px(150.))
        .h(px(104.))
        .flex_none()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(10.))
        .child(line_box(spaced(lang.tr(Tr::DemoKey), 11., FontWeight::MEDIUM, p.muted, 0.08), 16.))
        .child(div().w(px(w)).h(px(52.)).flex_none().flex().items_center().justify_center().child(face))
        .child(line_box(spaced(state, 11., FontWeight::NORMAL, color, 0.05), 16.))
}

/// The 105 × 52 slot the capsule lives in — as tall as the key column's keycap row, so the rows
/// underneath line up. Bottom-anchored like the HUD window, so the pop-in rises the same way.
fn hud_slot(p: &Palette, lang: Lang, hud: Option<&HudAt>, still: bool) -> Div {
    const SLOT_W: f32 = 105.;
    let slot =
        div().relative().w(px(SLOT_W)).h(px(52.)).flex_none().flex().flex_col().justify_end().items_center().pb(px(5.));
    let Some(at) = hud else { return slot };
    let build = |state: HudState, w: f32, since: f32| {
        let fill = hud::fill_at(&state, since, None);
        let wave = if still { Wave::Rest } else { Wave::Bars(crate::hud::waveform::Waveform::demo(since)) };
        Capsule {
            state,
            lang,
            scale: Scale::DESIGN,
            palette: *p,
            reduced: still,
            w,
            t: since,
            wave,
            fill,
            outcome_t: None,
        }
        .element(&mut |b| hud::static_button(b, Scale::DESIGN, p).into_any_element())
    };
    let mut capsule = build(at.state.clone(), at.w, at.since);
    if let Some(u) = at.pop {
        capsule = hud::pop_shape(capsule, Scale::DESIGN, at.w, u, false);
    }
    let mut slot = slot.child(capsule);
    if at.inserted > 0. {
        // On top of the recognizing capsule, at the spot where it sits: the cross-fade to green.
        let left = (SLOT_W - hud::CAPSULE_W) / 2.;
        slot = slot.child(
            build(HudState::Inserted, hud::CAPSULE_W, 0.).absolute().left(px(left)).bottom(px(5.)).opacity(at.inserted),
        );
    }
    slot.opacity(at.shown)
}

/// Under the capsule: toggle names its two buttons, hold says it has none.
fn hud_footer(p: &Palette, hud_palette: &Palette, lang: Lang, mode: RecordMode) -> Div {
    let label = |s: &str| line_box(spaced(s, 11., FontWeight::NORMAL, p.muted, 0.05), 16.);
    let row = div().h(px(23.)).flex().items_center().gap(px(6.));
    match mode {
        RecordMode::Toggle => row
            .child(hud::legend_button(Btn::Cancel, hud_palette))
            .child(label(lang.tr(Tr::DemoToCancel)))
            .child(div().w(px(6.)))
            .child(hud::legend_button(Btn::Finish, hud_palette))
            .child(label(lang.tr(Tr::DemoToConfirm))),
        RecordMode::Hold => row.child(label(lang.tr(Tr::DemoNoButtons))),
    }
}

fn hud_column(p: &Palette, hud_palette: &Palette, lang: Lang, mode: RecordMode, scene: &Scene, still: bool) -> Div {
    div()
        .min_w(px(150.))
        .h(px(104.))
        .flex_none()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(10.))
        .child(line_box(glyphs("HUD", 11., FontWeight::MEDIUM, p.muted, 11. * 0.08), 14.))
        .child(hud_slot(hud_palette, lang, scene.hud.as_ref(), still))
        .child(hud_footer(p, hud_palette, lang, mode))
}

/// The four phases of a recording, as the user lives it.
fn chip_labels(mode: RecordMode) -> [Tr; 4] {
    match mode {
        RecordMode::Toggle => [Tr::DemoPress, Tr::DemoRecording, Tr::DemoPressAgain, Tr::DemoTextAppears],
        RecordMode::Hold => [Tr::DemoHold, Tr::DemoRecording, Tr::DemoRelease, Tr::DemoTextAppears],
    }
}

/// A numbered phase chip. The lit one is solid ink (the text color, so it inverts in dark) with a translucent number
/// disc; the rest are hairline outlines. Neutral on purpose: gold is the selection color.
fn chip(p: &Palette, n: usize, label: &str, lit: f32) -> Div {
    // Every chip is as wide as its label at the heavy weight, so lighting one never moves the row.
    let w = (5. + 18. + 6. + spaced_width(label, 12., FontWeight::BOLD, 0.04) + 12. + 2.).ceil();
    let weight = if lit > 0.5 { FontWeight::BOLD } else { FontWeight::MEDIUM };
    let idle_disc = Hsla { a: 0.1, ..p.text };
    let lit_disc = Hsla { a: 0.2, ..p.bg };
    let ink = lerp(p.muted, p.bg, lit);
    div()
        .w(px(w))
        .h(px(28.))
        .flex_none()
        .rounded_full()
        .bg(Hsla { a: lit, ..p.text })
        .border_1()
        .border_color(lerp(p.border_strong, p.text, lit))
        .flex()
        .items_center()
        .gap(px(6.))
        .pl(px(4.))
        .pr(px(12.))
        .whitespace_nowrap()
        .child(
            div()
                .size(px(18.))
                .flex_none()
                .rounded_full()
                .bg(lerp(idle_disc, lit_disc, lit))
                .flex()
                .items_center()
                .justify_center()
                .child(text(n.to_string(), 10., FontWeight::BOLD, ink).line_height(relative(1.))),
        )
        .child(spaced(label, 12., weight, ink, 0.04))
}

/// Long, thin and quiet: the order of the chips, not an action.
fn chip_arrow(p: &Palette) -> Svg {
    svg().path(Ic::ArrowThin.path()).w(px(22.)).h(px(9.)).flex_none().text_color(Hsla { a: 0.55, ..p.muted })
}

fn chips_row(p: &Palette, lang: Lang, mode: RecordMode, scene: &Scene) -> Div {
    let mut row = div().h(px(28.)).flex().items_center().gap(px(12.));
    for (i, (label, lit)) in chip_labels(mode).into_iter().zip(scene.chips).enumerate() {
        if i > 0 {
            row = row.child(chip_arrow(p));
        }
        row = row.child(chip(p, i + 1, lang.tr(label), lit));
    }
    row
}

/// The scene and the chips for one layer.
fn picture(p: &Palette, hud_palette: &Palette, lang: Lang, key: &str, layer: Layer, still: bool) -> Div {
    let scene = Scene::at(layer.mode, layer.at);
    div()
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(16.))
        .opacity(layer.opacity)
        .child(
            div()
                .h(px(104.))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(72.))
                .child(key_column(p, lang, key, layer.mode, &scene, still))
                .child(hud_column(p, hud_palette, lang, layer.mode, &scene, still)),
        )
        .child(chips_row(p, lang, layer.mode, &scene))
}

/// The grey stage card around the pictures.
fn stage(p: &Palette, hud_palette: &Palette, lang: Lang, key: &str, tick: &Tick) -> Div {
    let mut pictures = div().relative().w_full();
    let top = tick.layers.len().saturating_sub(1);
    for (i, layer) in tick.layers.iter().enumerate() {
        let picture = picture(p, hud_palette, lang, key, *layer, tick.still);
        // The newest picture sizes the stack; older ones lie over it while they fade out.
        pictures = pictures.child(if i == top { picture } else { picture.absolute().top_0().left_0() });
    }
    div()
        .w_full()
        .flex_none()
        .rounded(px(16.))
        .bg(p.sub)
        .border_1()
        .border_color(p.border)
        .pt(px(20.))
        .pr(px(24.))
        .pb(px(18.))
        .pl(px(24.))
        .child(pictures)
}

/// The demo element: owns the clock and draws the stage for it.
pub struct UsageDemo {
    p: Palette,
    lang: Lang,
    mode: RecordMode,
    key: SharedString,
}

/// The wizard's animated "how to use" demo for `mode`, in `lang`'s words, with the user's shortcut `key_label`
/// (e.g. `右 ⌥`) on the keycap. It is as wide as its container and 186 pt high at design size; it animates by itself.
pub fn usage_demo(p: &Palette, lang: Lang, mode: RecordMode, key_label: impl Into<SharedString>) -> impl IntoElement {
    // The typography helpers (tracking, CJK typeface) read the UI language: keep it the demo's.
    ui::set_ui_language(lang);
    UsageDemo { p: *p, lang, mode, key: key_label.into() }
}

impl IntoElement for UsageDemo {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for UsageDemo {
    type RequestLayoutState = AnyElement;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(ElementId::from("usage-demo"))
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, AnyElement) {
        window.with_element_state(id.expect("the demo has an element id"), |clock: Option<Clock>, window| {
            let mut clock = clock.unwrap_or_else(|| Clock::new(self.mode));
            let tick = frozen(self.mode).unwrap_or_else(|| {
                clock.tick(self.mode, Instant::now(), window.is_window_active(), dito_platform::system::reduce_motion())
            });
            if tick.running {
                window.request_animation_frame();
            }
            let hud_palette = cx
                .try_global::<crate::state::AppGlobal>()
                .map(|st| crate::theme::palette(st.0.cfg.get().general.hud_appearance, cx))
                .unwrap_or(self.p);
            let mut stage = stage(&self.p, &hud_palette, self.lang, &self.key, &tick).into_any_element();
            ((stage.request_layout(window, cx), stage), clock)
        })
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        stage: &mut AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) {
        stage.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        stage: &mut AnyElement,
        _prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        stage.paint(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    const MODES: [RecordMode; 2] = [RecordMode::Toggle, RecordMode::Hold];

    fn hud_state(mode: RecordMode, at: f32) -> Option<HudState> {
        Scene::at(mode, at).hud.map(|h| h.state)
    }

    #[::core::prelude::v1::test]
    fn the_key_follows_the_storyboard() {
        // Toggle: tapped at 0.6 and again at 3.0, up in between and after.
        let key = |at| Scene::at(RecordMode::Toggle, at).key;
        assert_eq!(key(0.3), 0.);
        assert!(key(0.66) > 0.5 && key(0.66) < 1., "mid-press at 0.66");
        assert!(key(0.73) > 0.999, "bottomed out after the 120 ms ease-out");
        assert_eq!(key(1.2), 0., "released again by keyframe 2");
        assert_eq!(key(2.0), 0.);
        assert!(key(3.05) > 0.2, "pressed again at 3.0");
        assert_eq!(key(3.6), 0.);
        // Hold: down at 0.6 and stays down until 3.0, springs back within 160 ms.
        let key = |at| Scene::at(RecordMode::Hold, at).key;
        assert_eq!(key(0.5), 0.);
        assert_eq!(key(0.8), 1.);
        assert_eq!(key(2.99), 1.);
        assert!(key(3.05) < 1. && key(3.05) > 0.);
        assert_eq!(key(3.2), 0., "released 160 ms after 3.0");
    }

    #[::core::prelude::v1::test]
    fn the_hud_follows_the_key_by_80_ms() {
        for mode in MODES {
            assert_eq!(hud_state(mode, 0.67), None, "{mode:?}: nothing before 0.68");
            assert_eq!(hud_state(mode, 0.69), Some(HudState::Listening { mode }));
            assert_eq!(hud_state(mode, 3.07), Some(HudState::Listening { mode }));
            assert_eq!(hud_state(mode, 3.09), Some(HudState::Recognizing));
            assert_eq!(
                hud_state(mode, 5.0),
                Some(HudState::Recognizing),
                "inserted fades in over the recognizing capsule"
            );
        }
    }

    #[::core::prelude::v1::test]
    fn hold_listens_narrow_and_widens_on_release_toggle_never_does() {
        let width = |mode, at| Scene::at(mode, at).hud.unwrap().w;
        assert_eq!(width(RecordMode::Toggle, 1.0), hud::CAPSULE_W);
        assert_eq!(width(RecordMode::Toggle, 3.2), hud::CAPSULE_W);
        assert_eq!(width(RecordMode::Hold, 1.0), hud::NARROW_W);
        assert_eq!(width(RecordMode::Hold, 3.07), hud::NARROW_W, "still narrow until the HUD turns");
        let mid = width(RecordMode::Hold, 3.08 + hud::WIDEN_S / 2.);
        assert!(
            mid > hud::NARROW_W + 0.7 * (hud::CAPSULE_W - hud::NARROW_W),
            "ease-out: past 70% at half time, got {mid}"
        );
        assert_eq!(width(RecordMode::Hold, 3.08 + hud::WIDEN_S), hud::CAPSULE_W);
        assert_eq!(width(RecordMode::Hold, 4.0), hud::CAPSULE_W);
    }

    #[::core::prelude::v1::test]
    fn the_pop_in_runs_420_ms_from_the_first_frame_of_the_capsule() {
        let pop = |at| Scene::at(RecordMode::Toggle, at).hud.and_then(|h| h.pop);
        assert_eq!(pop(0.6), None);
        assert!(pop(0.681).unwrap() < 0.01);
        assert!((pop(0.68 + 0.21).unwrap() - 0.5).abs() < 0.01);
        assert_eq!(pop(0.68 + 0.43), None, "arrived");
    }

    #[::core::prelude::v1::test]
    fn recognizing_fills_to_half_and_the_inserted_capsule_fades_in_at_4_8() {
        let at = |at| Scene::at(RecordMode::Toggle, at).hud.unwrap();
        let (a, b) = (at(3.5), at(4.7));
        let fill = |h: &HudAt| hud::fill_at(&h.state, h.since, None).unwrap();
        assert!(fill(&a) < fill(&b) && fill(&b) < 0.5, "{} {}", fill(&a), fill(&b));
        assert!(fill(&b) > 0.45, "1.5 s ease-out: nearly at 50% by 4.7");
        assert_eq!(at(4.79).inserted, 0.);
        assert!((at(4.88).inserted - 0.5).abs() < 1e-4);
        assert_eq!(at(4.97).inserted, 1., "done 160 ms after 4.8");
        assert_eq!(at(5.5).inserted, 1.);
    }

    #[::core::prelude::v1::test]
    fn the_phase_chips_follow_the_sequence() {
        let lit = |mode, at| {
            let chips = Scene::at(mode, at).chips;
            let on: Vec<usize> = (0..4).filter(|i| chips[*i] > 0.99).collect();
            (on, chips)
        };
        for mode in MODES {
            assert_eq!(lit(mode, 0.3).0, vec![] as Vec<usize>, "{mode:?}: nothing before the first press");
            assert_eq!(lit(mode, 1.0).0, vec![0]);
            assert_eq!(lit(mode, 2.0).0, vec![1]);
            assert_eq!(lit(mode, 3.5).0, vec![2], "the user's last action stays lit through recognizing");
            assert_eq!(lit(mode, 5.0).0, vec![3]);
        }
        // The plates cross-fade: halfway between two phases both are partly lit.
        let (_, chips) = lit(RecordMode::Toggle, 1.2 + CHIP_FADE_S / 2.);
        assert!((chips[0] - 0.5).abs() < 1e-4 && (chips[1] - 0.5).abs() < 1e-4, "{chips:?}");
    }

    #[::core::prelude::v1::test]
    fn the_end_of_the_loop_runs_into_its_start() {
        for mode in MODES {
            let end = Scene::at(mode, PERIOD - 1e-4);
            let start = Scene::at(mode, 0.);
            assert_eq!(end.key, start.key);
            assert!(end.chips.iter().all(|c| *c < 1e-3), "{mode:?}: chips {:?}", end.chips);
            assert!(end.hud.as_ref().is_none_or(|h| h.shown < 1e-3), "{mode:?}: the capsule has faded out");
            assert!(start.hud.is_none() && start.chips == [0.; 4]);
            // And the result holds for the whole rest.
            let rest = Scene::at(mode, 5.5);
            assert_eq!(rest.hud.unwrap().shown, 1.);
            assert!((rest.chips[3] - 1.).abs() < 1e-6);
        }
    }

    fn after(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    #[::core::prelude::v1::test]
    fn the_clock_runs_loops_and_pauses_while_inactive() {
        let base = Instant::now();
        let mut clock = Clock::new(RecordMode::Toggle);
        let first = clock.tick(RecordMode::Toggle, base, true, false);
        assert_eq!(first.layers, vec![Layer { mode: RecordMode::Toggle, at: 0., opacity: 1. }]);
        assert!(first.running && !first.still);
        let t = clock.tick(RecordMode::Toggle, after(base, 1500), true, false);
        assert!((t.layers[0].at - 1.5).abs() < 1e-3);
        // The window deactivates: the loop stops, and no more frames are asked for.
        let paused = clock.tick(RecordMode::Toggle, after(base, 2000), false, false);
        assert!(!paused.running);
        let still = clock.tick(RecordMode::Toggle, after(base, 60_000), false, false);
        assert!((still.layers[0].at - 1.5).abs() < 1e-3, "frozen while inactive: {}", still.layers[0].at);
        // It comes back a minute later and carries on from the same phase.
        let back = clock.tick(RecordMode::Toggle, after(base, 60_100), true, false);
        assert!((back.layers[0].at - 1.5).abs() < 1e-3, "resumes where it paused: {}", back.layers[0].at);
        let go = clock.tick(RecordMode::Toggle, after(base, 60_600), true, false);
        assert!((go.layers[0].at - 2.0).abs() < 1e-3);
        // And it wraps at the end of the loop.
        let wrapped = clock.tick(RecordMode::Toggle, after(base, 60_600 + 4_000), true, false);
        assert!((wrapped.layers[0].at - (2.0 + 4.0 - PERIOD)).abs() < 1e-2, "{}", wrapped.layers[0].at);
    }

    #[::core::prelude::v1::test]
    fn switching_modes_cross_fades_the_old_picture_into_a_new_loop_from_zero() {
        let base = Instant::now();
        let mut clock = Clock::new(RecordMode::Toggle);
        clock.tick(RecordMode::Toggle, base, true, false);
        clock.tick(RecordMode::Toggle, after(base, 2000), true, false);
        let start = clock.tick(RecordMode::Hold, after(base, 2000), true, false);
        assert_eq!(start.layers.len(), 2, "old picture lies under the new one");
        let (old, new) = (start.layers[0], start.layers[1]);
        assert_eq!((old.mode, new.mode), (RecordMode::Toggle, RecordMode::Hold));
        assert!((old.at - 2.0).abs() < 1e-3, "the old picture holds the moment it was switched away from");
        assert_eq!((old.opacity, new.opacity, new.at), (1., 0., 0.));
        let mid = clock.tick(RecordMode::Hold, after(base, 2100), true, false);
        assert!((mid.layers[0].opacity - 0.5).abs() < 1e-3 && (mid.layers[1].opacity - 0.5).abs() < 1e-3);
        assert!((mid.layers[1].at - 0.1).abs() < 1e-3, "the new loop already runs");
        let done = clock.tick(RecordMode::Hold, after(base, 2250), true, false);
        assert_eq!(done.layers.len(), 1);
        assert_eq!(done.layers[0].opacity, 1.);
        assert!((done.layers[0].at - 0.25).abs() < 1e-3);
    }

    #[::core::prelude::v1::test]
    fn reduce_motion_holds_the_recording_keyframe_without_a_loop() {
        let base = Instant::now();
        let mut clock = Clock::new(RecordMode::Hold);
        for ms in [0, 700, 5_000] {
            let t = clock.tick(RecordMode::Hold, after(base, ms), true, true);
            assert_eq!(t.layers, vec![Layer { mode: RecordMode::Hold, at: STILL_AT, opacity: 1. }]);
            assert!(!t.running && t.still);
        }
        // A mode switch is just a new still, not a fade.
        let t = clock.tick(RecordMode::Toggle, after(base, 6_000), true, true);
        assert_eq!(t.layers.len(), 1);
        // The keyframe is the recording: key per mode, capsule listening, chip 2 lit.
        let hold = Scene::at(RecordMode::Hold, STILL_AT);
        assert_eq!(hold.key, 1.);
        assert_eq!(hold.hud.as_ref().map(|h| h.w), Some(hud::NARROW_W));
        let toggle = Scene::at(RecordMode::Toggle, STILL_AT);
        assert_eq!(toggle.key, 0.);
        assert_eq!(toggle.hud.as_ref().map(|h| h.w), Some(hud::CAPSULE_W));
        for scene in [hold, toggle] {
            assert!(matches!(scene.hud.as_ref().map(|h| &h.state), Some(HudState::Listening { .. })));
            assert!((scene.chips[1] - 1.).abs() < 1e-6 && scene.chips[0] < 1e-6 && scene.chips[2] < 1e-6);
        }
    }

    #[::core::prelude::v1::test]
    fn every_frame_builds_in_every_language() {
        // (Only builds the element trees; the UI-language global is left to the tests that own it.)
        for lang in Lang::ALL {
            let key = dito_platform::Shortcut::parse("RightOption").map(|s| dito_platform::keymap::label(&s, lang));
            let key = key.unwrap();
            for p in [Palette::light(), Palette::dark()] {
                for mode in MODES {
                    let mut at = 0.;
                    while at < PERIOD {
                        let tick = Tick { layers: vec![Layer { mode, at, opacity: 1. }], still: false, running: false };
                        let _ = stage(&p, &p, lang, &key, &tick);
                        at += 0.05;
                    }
                }
            }
        }
    }
}
