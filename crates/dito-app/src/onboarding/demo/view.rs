use std::time::Instant;

use dito_core::config::RecordMode;
use dito_core::controller::HudState;
use dito_core::i18n::{Lang, Tr};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::assets::Ic;
use crate::hud::{self, Btn, Capsule, Scale, Wave};
use crate::theme::Palette;
use crate::ui::{self, Motion, text};

use super::clock::{Clock, Layer, Tick, frozen};
use super::timeline::{HudAt, KEY_SCALE, Scene};

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
        .bg(ui::mix_rgb(p.card, p.text, down))
        .border(px(2.))
        .border_color(ui::mix_rgb(p.border_strong, p.text, down))
        .flex()
        .items_center()
        .justify_center()
        .child(
            text(key.to_string(), 18. * k, FontWeight::BOLD, ui::mix_rgb(p.text, p.bg, down)).line_height(relative(1.)),
        );
    // Only a held key reads "held": a tap is over before the caption could say so.
    let (state, color) = if mode == RecordMode::Hold && down > 0.5 {
        (lang.tr(Tr::DemoHeldDown), ui::mix_rgb(p.muted, p.text, down))
    } else {
        (lang.tr(Tr::DemoReleased), ui::mix_rgb(p.muted, p.text, if mode == RecordMode::Hold { down } else { 0. }))
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

/// The slot the capsule lives in, as tall as the key column's keycap row, so the rows
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
            motion: if still { Motion::Reduced } else { Motion::Full },
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
        capsule = hud::pop_shape(capsule, Scale::DESIGN, at.w, u, Motion::Full);
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
    let ink = ui::mix_rgb(p.muted, p.bg, lit);
    div()
        .w(px(w))
        .h(px(28.))
        .flex_none()
        .rounded_full()
        .bg(Hsla { a: lit, ..p.text })
        .border_1()
        .border_color(ui::mix_rgb(p.border_strong, p.text, lit))
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
                .bg(ui::mix_rgb(idle_disc, lit_disc, lit))
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
            let tick = frozen(self.mode)
                .unwrap_or_else(|| clock.tick(self.mode, Instant::now(), window.is_window_active(), Motion::system()));
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
