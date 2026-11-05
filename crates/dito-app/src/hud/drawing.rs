//! Capsule drawing: geometry, fills, buttons and labels — stateless functions on [`Scale`].

use dito_core::config::RecordMode;
use dito_core::controller::{ErrorAction, HudState};
use dito_core::i18n::{Lang, Tr};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::assets::HudIc;
use crate::theme::{PLAIN_BLACK, Palette, hex};
use crate::ui::{hex_alpha, hud_icon, text};

use super::Scale;
use super::waveform::REST;

pub(crate) const CAPSULE_W: f32 = 105.0;
/// Hold mode has no buttons, so while listening the capsule is only as wide as its waveform needs; it widens to
/// [`CAPSULE_W`] when the recording ends.
pub(crate) const NARROW_W: f32 = 72.0;
/// The narrow → full widening, in seconds (ease-out).
pub(crate) const WIDEN_S: f32 = 0.3;
pub(crate) const CAPSULE_H: f32 = 33.0;
pub(super) const CAPSULE_PAD: f32 = 5.0;
/// Round button inside the capsule.
pub(super) const BTN: f32 = 23.0;
/// Default recovery-action glyph box in design points (rendered via `tx`).
const BTN_ICON: f32 = 13.;
/// Centered state glyph (inserted/copied) in design points: between the capsule-height fill and the
/// earlier, smaller proportion (≈19 pt rendered).
const CHECK_ICON: f32 = 19.;
const COPY_ICON: f32 = 18.;
/// Hairline border used by the capsule.
const HAIRLINE: f32 = 1.0;
/// Outline of the capsule's inside (the part within its hairline border): height, and width for a capsule of width `w`.
const INNER_H: f32 = CAPSULE_H - 2. * HAIRLINE;

fn inner_w(w: f32) -> f32 {
    w - 2. * HAIRLINE
}

/// Polygon of the capsule (width `w`, height `h`) cut to the vertical band `x0..x1`: both cut edges are straight
/// vertical lines; the top and bottom follow the capsule's rounded ends. GPUI clips to rectangles only, so the
/// progress fill is drawn as this shape rather than as a clipped box.
fn capsule_slice(w: f32, h: f32, x0: f32, x1: f32) -> Vec<(f32, f32)> {
    let r = h / 2.;
    let half = |x| capsule_half_height(w, h, x);
    let (x0, x1) = (x0.clamp(0., w), x1.clamp(0., w));
    let mut xs = vec![x0];
    let mut x = (x0 / 0.5).floor() * 0.5 + 0.5;
    while x < x1 {
        xs.push(x);
        x += 0.5;
    }
    xs.push(x1);
    let mut pts: Vec<(f32, f32)> = xs.iter().map(|&x| (x, r - half(x))).collect();
    pts.extend(xs.iter().rev().map(|&x| (x, r + half(x))));
    pts
}

fn capsule_half_height(w: f32, h: f32, x: f32) -> f32 {
    let r = h / 2.;
    let x = x.clamp(0., w);
    let d = if x < r {
        r - x
    } else if x > w - r {
        x - (w - r)
    } else {
        0.
    };
    (r * r - d * d).max(0.).sqrt()
}

/// The slice `x0..x1` of a capsule of width `w` (design points), painted in `fill`.
fn slice_canvas(w: f32, x0: f32, x1: f32, fill: impl Into<Background> + 'static, scale: Scale) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let pts = capsule_slice(inner_w(w), INNER_H, x0, x1);
            let mut b = PathBuilder::fill();
            for (i, (x, y)) in pts.iter().enumerate() {
                let p =
                    point(bounds.origin.x + gpui_kit::px(*x * scale.0), bounds.origin.y + gpui_kit::px(*y * scale.0));
                if i == 0 { b.move_to(p) } else { b.line_to(p) }
            }
            b.close();
            if let Ok(path) = b.build() {
                window.paint_path(path, fill);
            }
        },
    )
    .absolute()
    .inset_0()
}

/// Theme-specific HUD colors stay independent from the settings window's palette.
pub(super) fn hud_color(p: &Palette, light: &str, dark: &str) -> Hsla {
    hex(if p.dark { dark } else { light })
}

/// One subtle elevation layer, kept inside the transparent window's reserved padding.
pub(super) fn surface_shadow(p: &Palette, scale: Scale, button: bool) -> Vec<BoxShadow> {
    let (y, blur, opacity) =
        if button { (1., 2., if p.dark { 0.14 } else { 0.08 }) } else { (1., 4., if p.dark { 0.24 } else { 0.12 }) };
    vec![BoxShadow {
        color: hex_alpha(hex("#000000"), opacity),
        offset: point(scale.px(0.), scale.px(y)),
        blur_radius: scale.px(blur),
        spread_radius: scale.px(0.),
        inset: false,
    }]
}

/// A flat fill, a crisp 1 pt front and a narrow symmetric halo (4 pt each side).
fn progress_layer(x: f32, w: f32, scale: Scale, p: &Palette) -> impl IntoElement {
    let edge = hud_color(p, "#AA8C40", "#E0C47D");
    let halo = hex_alpha(edge, if p.dark { 0.20 } else { 0.14 });
    let half_height =
        capsule_half_height(inner_w(w), INNER_H, x - 0.5).min(capsule_half_height(inner_w(w), INNER_H, x + 0.5));
    div()
        .absolute()
        .inset_0()
        .child(slice_canvas(w, 0., x, hud_color(p, "#F6DA97", "#827554"), scale))
        .child(slice_canvas(
            w,
            x - 4.,
            x,
            linear_gradient(90., linear_color_stop(hex_alpha(halo, 0.), 0.), linear_color_stop(halo, 1.)),
            scale,
        ))
        .child(slice_canvas(
            w,
            x,
            x + 4.,
            linear_gradient(90., linear_color_stop(halo, 0.), linear_color_stop(hex_alpha(halo, 0.), 1.)),
            scale,
        ))
        // Paint the crisp front as a quad: path antialiasing spreads subpixel-width paths
        // into a faint blur, which swallowed the front inside its own halo.
        .child(
            div()
                .absolute()
                .left(gpui_kit::px((x - 0.5) * scale.0))
                .top(gpui_kit::px((INNER_H / 2. - half_height) * scale.0))
                .w(gpui_kit::px(1. * scale.0))
                .h(gpui_kit::px(2. * half_height * scale.0))
                .bg(edge),
        )
}

const SWEEP_MS: f32 = 2500.;
const SWEEP_TRAVEL: f32 = 1100.;
pub(super) const SETTLE_S: f32 = 0.18;
/// A narrow crest crosses the label, then rests outside it for 1400 ms.
fn sweep_at(t: f32, total: f32) -> f32 {
    let half = total * 0.25;
    let travel = (t.max(0.) * 1000. % SWEEP_MS) / SWEEP_TRAVEL;
    if travel < 1. { -half + travel * (total + 2. * half) } else { total + half }
}

/// Look of one round HUD button. `size` is in design points and rendered via [`Scale::tx`].
#[derive(Clone, Copy)]
pub(super) struct Circle {
    pub(super) id: &'static str,
    pub(super) bg: Hsla,
    /// Background while hovered. Hover never lightens: the gold button deepens a step, the danger
    /// buttons swap to their inverted scheme.
    pub(super) hover_bg: Hsla,
    glyph: HudIc,
    size: f32,
    color: Hsla,
    /// Glyph color while hovered (the danger buttons invert their glyph with the fill).
    pub(super) hover_color: Hsla,
    /// Resting hairline: `(color, opacity)`, painted inside the button and fading out on hover.
    pub(super) ring: Option<(Hsla, f32)>,
    dark: bool,
}

/// The round buttons a capsule can carry.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Btn {
    Cancel,
    Finish,
    Dismiss,
    Retry,
    Settings,
}

impl Btn {
    pub(super) fn look(self, p: &Palette) -> Circle {
        let (glass, glass_hover) = (hud_color(p, "#EBE8E1", "#5A564B"), hud_color(p, "#D4D1C9", "#6A6455"));
        let red = hex("#FF4949");
        // Danger buttons, scheme A: light rests white with a Red icon and a 25% Red hairline,
        // dark rests solid Red with a cream icon; hover inverts both themes.
        let danger = || {
            (
                hud_color(p, "#FFFFFF", "#FF4949"),
                hud_color(p, "#FF4949", "#FFFFFF"),
                hud_color(p, "#FF4949", "#FFEFEF"),
                hud_color(p, "#FFFFFF", "#FF4949"),
                (!p.dark).then_some((red, 0.25)),
            )
        };
        let (id, bg, hover_bg, glyph, size, color, hover_color, ring) = match self {
            Self::Cancel => ("hud-cancel", glass, glass_hover, HudIc::X, 15., p.text, p.text, None),
            Self::Finish => (
                "hud-finish",
                hud_color(p, "#F6DA97", "#E0C47D"),
                hud_color(p, "#E0C47D", "#AA8C40"),
                HudIc::Finish,
                14.,
                hex(PLAIN_BLACK),
                hex(PLAIN_BLACK),
                None,
            ),
            Self::Dismiss => {
                let (bg, hbg, c, hc, ring) = danger();
                ("hud-dismiss", bg, hbg, HudIc::X, 15., c, hc, ring)
            }
            Self::Retry => {
                let (bg, hbg, c, hc, ring) = danger();
                ("hud-retry", bg, hbg, HudIc::Retry, BTN_ICON, c, hc, ring)
            }
            Self::Settings => {
                let (bg, hbg, c, hc, ring) = danger();
                ("hud-settings", bg, hbg, HudIc::Sliders, BTN_ICON, c, hc, ring)
            }
        };
        Circle { id, bg, hover_bg, glyph, size, color, hover_color, ring, dark: p.dark }
    }
}

/// A round button's shell: `bg` is its background layer, `glyph` sits on top (already colored —
/// see [`hover_glyph`] for the hover cross-fade version).
pub(super) fn circle_shell(c: &Circle, scale: Scale, bg: AnyElement, glyph: AnyElement) -> Div {
    div()
        .size(scale.px(BTN))
        .flex_none()
        .relative()
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .shadow(surface_shadow(&if c.dark { Palette::dark() } else { Palette::light() }, scale, true))
        .child(bg)
        .child(glyph)
}

/// The background layer at cross-fade position `t` (0 resting, 1 hovered): the fill blends and a
/// resting hairline, if any, fades out with it.
pub(super) fn hover_layer(c: &Circle, scale: Scale, t: f32) -> AnyElement {
    let mut layer = div().absolute().inset_0().rounded_full().bg(mix(c.bg, c.hover_bg, t));
    if let Some((color, alpha)) = c.ring {
        layer = layer.border(scale.px(0.75)).border_color(hex_alpha(color, alpha * (1. - t)));
    }
    layer.into_any_element()
}

/// The glyph at cross-fade position `t`: danger buttons invert their glyph with the fill, so both
/// copies are stacked and blended by opacity.
pub(super) fn hover_glyph(c: &Circle, scale: Scale, t: f32) -> AnyElement {
    let base = hud_icon(c.glyph, scale.tx(c.size), c.color);
    if c.color == c.hover_color {
        return base.into_any_element();
    }
    div()
        .relative()
        .size(gpui_kit::px(scale.tx(c.size)))
        .child(base.opacity(1. - t))
        .child(hud_icon(c.glyph, scale.tx(c.size), c.hover_color).opacity(t).absolute().inset_0())
        .into_any_element()
}

/// A button drawn as the HUD draws it, without the behaviour: for capsules that are only shown.
pub(crate) fn static_button(b: Btn, scale: Scale, p: &Palette) -> Div {
    let look = b.look(p);
    let Circle { id: _, bg, hover_bg: _, glyph: _, size: _, color: _, hover_color: _, ring, dark: _ } = &look;
    let mut layer = div().absolute().inset_0().rounded_full().bg(*bg);
    if let Some((color, alpha)) = ring {
        layer = layer.border(scale.px(0.75)).border_color(hex_alpha(*color, *alpha));
    }
    circle_shell(
        &look,
        scale,
        layer.into_any_element(),
        hud_icon(look.glyph, scale.tx(look.size), look.color).into_any_element(),
    )
}

/// The legend uses exactly the same button colors as its HUD preview.
pub(crate) fn legend_button(b: Btn, p: &Palette) -> Div {
    static_button(b, Scale::DESIGN, p)
}

/// HUD text: 8% tracking for Japanese and Chinese, none for English (see `ui::tracking_for`).
fn spaced(s: &str, size: f32, weight: FontWeight, color: Hsla, scale: Scale) -> Div {
    crate::ui::text_tracked(s, scale.tx(size), weight, color, 0.08)
}

/// Linear channel blend for the hover cross-fade (the HUD palette sits close in hue, so per-channel is fine).
pub(super) fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let (a, b): (Rgba, Rgba) = (a.into(), b.into());
    Rgba { r: a.r + (b.r - a.r) * t, g: a.g + (b.g - a.g) * t, b: a.b + (b.b - a.b) * t, a: a.a + (b.a - a.a) * t }
        .into()
}

/// Capsule width in design points once its state has settled: only hold-mode listening is narrow.
pub(crate) fn rest_width(state: &HudState) -> f32 {
    if matches!(state, HudState::Listening { mode: RecordMode::Hold }) { NARROW_W } else { CAPSULE_W }
}

/// The width `since` seconds into a spring from `from` to `to` (hold mode's capsule widening on release).
pub(crate) fn eased_width(from: f32, to: f32, since: f32) -> f32 {
    if since >= WIDEN_S - 0.000_01 {
        return to;
    }
    let u = (since / WIDEN_S).clamp(0., 1.);
    from + (to - from) * crate::ui::spring(u, false)
}

/// Ease toward the phase boundary, stopping 1.5 percentage points short until a real response.
/// A carried recognition fill settles to 50% before the polishing phase begins.
pub(crate) fn fill_at(state: &HudState, t: f32, carry: Option<f32>) -> Option<f32> {
    let (a, b) = match state {
        HudState::Recognizing => (0., 0.5),
        HudState::Polishing => (0.5, 1.),
        _ => return None,
    };
    let mut elapsed = t.max(0.);
    if let Some(from) = carry {
        if elapsed < SETTLE_S {
            return Some(settle(from, a, elapsed));
        }
        elapsed -= SETTLE_S;
    }
    let u = (elapsed / 1.5).clamp(0., 1.);
    Some(a + (b - a - 0.015) * (1. - (1. - u).powi(3)))
}

pub(super) fn settle(from: f32, to: f32, t: f32) -> f32 {
    let u = (t / SETTLE_S).clamp(0., 1.);
    from + (to - from) * crate::ui::spring(u, false)
}

pub(super) fn motion_fill_at(state: &HudState, t: f32, carry: Option<f32>, reduced: bool) -> Option<f32> {
    if !reduced {
        return fill_at(state, t, carry);
    }
    let (start, target) = match state {
        HudState::Recognizing => (0., 0.485),
        HudState::Polishing => (0.5, 0.985),
        _ => return None,
    };
    let u = (t / 0.12).clamp(0., 1.);
    Some(carry.unwrap_or(start) + (target - carry.unwrap_or(start)) * (1. - (1. - u).powi(3)))
}

/// Smoothed microphone-driven heights, or the static design under Reduce Motion.
#[derive(Clone, Copy)]
pub(crate) enum Wave {
    Bars([f32; 8]),
    Rest,
}

fn waveform(wave: Wave, scale: Scale, p: &Palette) -> Div {
    let px = |v: f32| scale.px(v);
    let heights = match wave {
        Wave::Bars(heights) => heights,
        Wave::Rest => REST,
    };
    let mut row = div().flex().items_center().gap(px(2.)).h(px(19.));
    for h in heights {
        row = row.child(div().flex_none().w(px(2.)).h(px(h)).rounded_full().bg(hud_color(p, "#6A6455", "#D4D1C9")));
    }
    row
}

/// One stable label with a narrow crest; no shadow, font-weight change or baseline movement.
/// The row hugs its content (a single text run when there is no tracking, per-glyph items with a
/// gap otherwise) and the capsule centers it — nothing is measured or given a computed width, so
/// every language centers exactly on every display scale. The crest is a moving clip window over
/// stacked copies of that same row; window edges snap to whole points and stay well above a point
/// wide, so GPUI's paint culling cannot drop them either.
fn shimmer_label(s: &str, t: f32, scale: Scale, p: &Palette, reduced: bool) -> Div {
    let px = |v: f32| scale.px(v);
    let size = scale.tx(12.);
    let track = size * crate::ui::tracking_for(s, 0.08);
    let base = hud_color(p, "#6A6455", "#C2BDB2");
    let hot = hud_color(p, "#24221D", "#FFFFFF");
    let shoulder = hud_color(p, "#444139", "#FFECBD");
    let chars: Vec<char> = s.chars().collect();
    // Only the crest's travel math needs a width; layout never uses it.
    // text_width already returns scaled pixels: do not scale the measurements a second time.
    let ws: Vec<f32> = chars.iter().map(|c| crate::ui::text_width(&c.to_string(), size, FontWeight::NORMAL)).collect();
    let total = ws.iter().sum::<f32>() + track * chars.len().saturating_sub(1) as f32;
    let row = |color: Hsla| {
        let mut r = div().flex().items_center().gap(gpui_kit::px(track)).h(px(19.));
        if track == 0. {
            // No tracking: one plain text run — its width is the run itself, never a measurement.
            r = r.child(text(s.to_string(), size, FontWeight::NORMAL, color).line_height(relative(1.6)));
        } else {
            for c in &chars {
                r = r
                    .child(text(c.to_string(), size, FontWeight::NORMAL, color).line_height(relative(1.6)).flex_none());
            }
        }
        r
    };
    // The crest: a soft light band crossing the label. `from`/`to` are clamped to the label so a
    // partially arrived or leaving band shows exactly the part that overlaps the glyphs.
    let band = |from: f32, to: f32, color: Hsla| {
        let from = from.round().clamp(-2., total);
        let to = to.round().clamp(from + 2., total + 2.);
        div()
            .absolute()
            .top_0()
            .left(gpui_kit::px(from))
            .w(gpui_kit::px(to - from))
            .h(px(19.))
            .overflow_hidden()
            .child(div().ml(gpui_kit::px(-from)).child(row(color)))
    };
    let mut label = div().relative().h(px(19.)).flex_none().child(row(base));
    if !reduced && (t.max(0.) * 1000. % SWEEP_MS) < SWEEP_TRAVEL {
        let sweep = sweep_at(t, total);
        let half = total * 0.25;
        label = label
            .child(band(sweep - half, sweep + half, shoulder))
            .child(band(sweep - half * 0.62, sweep + half * 0.62, mix(shoulder, hot, 0.5)))
            .child(band(sweep - half * 0.3, sweep + half * 0.3, hot));
    }
    label
}

/// Success starts after the fill settles; the background is already becoming green/gold.
pub(crate) const OUTCOME_ICON_DELAY: f32 = SETTLE_S + 0.08;
const OUTCOME_ICON_S: f32 = 0.32;
/// After the icon finishes drawing, the whole capsule (icon and background together) holds
/// briefly, then fades as one — neither outlives the other.
pub(super) const OUTCOME_HOLD_S: f32 = 0.24;
pub(super) const OUTCOME_HOLD_END: f32 = OUTCOME_ICON_DELAY + OUTCOME_ICON_S + OUTCOME_HOLD_S;
pub(crate) const OUTCOME_FADE_S: f32 = 0.26;
pub(crate) const OUTCOME_END: f32 = OUTCOME_HOLD_END + OUTCOME_FADE_S;

/// Copy times are relative to the icon clock, which begins after the progress fill settles.
pub(crate) const COPY_ENTER_AT: f32 = 0.35;
const COPY_STAGGER_S: f32 = 0.035;
const COPY_GLYPH_FADE_S: f32 = 0.22;
const COPY_SETTLE_AT: f32 = 0.9;
pub(crate) const COPY_HOLD_END: f32 = OUTCOME_ICON_DELAY + 1.45;
pub(crate) const COPY_FADE_S: f32 = 0.20;
pub(crate) const COPY_END: f32 = COPY_HOLD_END + COPY_FADE_S;

fn copied_progress(t: Option<f32>, reduced: bool) -> f32 {
    if reduced || t.is_none() {
        return 1.;
    }
    let t = t.unwrap();
    if t >= COPY_SETTLE_AT {
        return 1.;
    }
    super::motion::spring_progress(t - COPY_ENTER_AT, 0.55, 18.)
}

/// Premeasure the entire label so the shell never expands in character-sized steps.
fn copied_label_width(lang: Lang) -> f32 {
    crate::ui::text_width(lang.tr(Tr::HudCopied), 12., FontWeight::NORMAL)
}

pub(crate) fn copied_growth(t: Option<f32>, reduced: bool, lang: Lang) -> f32 {
    let target = (COPY_ICON + 4. + copied_label_width(lang) + 38.).max(CAPSULE_W);
    (target - CAPSULE_W) * copied_progress(t, reduced)
}

fn copied_glyph(t: Option<f32>, index: usize, reduced: bool) -> (f32, f32) {
    let Some(t) = t else {
        return (1., 0.);
    };
    if reduced {
        return ((t / 0.1).clamp(0., 1.), 0.);
    }
    let local = t - COPY_ENTER_AT - index as f32 * COPY_STAGGER_S;
    let fade = (local / COPY_GLYPH_FADE_S).clamp(0., 1.);
    let rise = if local >= 0.4 { 1. } else { super::motion::spring_progress(local, 0.55, 24.) };
    (1. - (1. - fade).powi(3), 6. * (1. - rise))
}

/// A constant-speed tip travels along the two segments, following the check's actual centerline.
fn check_trace(progress: f32) -> Vec<(f32, f32)> {
    let [a, b, c] = [(2.499_f32, 10.108_f32), (6.760, 14.374), (16.502, 4.627)];
    let ab = (b.0 - a.0).hypot(b.1 - a.1);
    let bc = (c.0 - b.0).hypot(c.1 - b.1);
    let length = progress.clamp(0., 1.) * (ab + bc);
    if length <= ab {
        let t = length / ab;
        vec![a, (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)]
    } else {
        let t = (length - ab) / bc;
        vec![a, b, (b.0 + (c.0 - b.0) * t, b.1 + (c.1 - b.1) * t)]
    }
}

fn drawn_check(progress: f32, scale: Scale) -> AnyElement {
    if progress >= 1. {
        return hud_icon(HudIc::Check, scale.tx(CHECK_ICON), hex("#FFFFFF")).into_any_element();
    }
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            if progress <= 0. {
                return;
            }
            let points = check_trace(progress);
            for pair in points.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let angle = (b.1 - a.1).atan2(b.0 - a.0);
                let mut path = PathBuilder::fill();
                // A round-ended stroke for each segment, including the bend. Its thickness
                // matches the canonical SVG, so the settled icon never changes visual weight.
                for end in 0..2 {
                    let center = if end == 0 { b } else { a };
                    for i in 0..=16 {
                        let theta =
                            angle - std::f32::consts::FRAC_PI_2 + (end as f32 + i as f32 / 16.) * std::f32::consts::PI;
                        let point = point(
                            bounds.origin.x + scale.px(center.0 + theta.cos() * 0.916),
                            bounds.origin.y + scale.px(center.1 + theta.sin() * 0.916),
                        );
                        if end == 0 && i == 0 {
                            path.move_to(point);
                        } else {
                            path.line_to(point);
                        }
                    }
                }
                path.close();
                if let Ok(path) = path.build() {
                    window.paint_path(path, hex("#FFFFFF"));
                }
            }
        },
    )
    .size(scale.px(CHECK_ICON))
    .flex_none()
    .into_any_element()
}

fn outcome_icon(copy: bool, time: Option<f32>, reduced: bool, scale: Scale) -> AnyElement {
    let progress = if reduced { 1. } else { time.map_or(1., |t| (t / OUTCOME_ICON_S).clamp(0., 1.)) };
    if !copy {
        return drawn_check(progress, scale);
    }
    let ink = hex("#FFF9EB");
    if progress >= 1. {
        return hud_icon(HudIc::Copy, scale.tx(COPY_ICON), ink).into_any_element();
    }
    let front = (progress / 0.375).clamp(0., 1.);
    let back = ((progress - 0.3125) / 0.6875).clamp(0., 1.);
    let ease = |u: f32| 1. - (1. - u).powi(3);
    div()
        .relative()
        .size(scale.px(COPY_ICON))
        .flex_none()
        .child(
            div()
                .absolute()
                .left(scale.px(-0.9 * (1. - ease(back))))
                .top(scale.px(0.9 * (1. - ease(back))))
                .size(scale.px(COPY_ICON))
                .opacity(ease(back))
                .child(svg().path("icons/hud/copy-back.svg").size(scale.px(COPY_ICON)).text_color(ink)),
        )
        .child(
            div()
                .absolute()
                .left_0()
                .top(scale.px(0.65 * (1. - ease(front))))
                .size(scale.px(COPY_ICON))
                .opacity(ease(front))
                .child(svg().path("icons/hud/copy-front.svg").size(scale.px(COPY_ICON)).text_color(ink)),
        )
        .into_any_element()
}

/// Everything one capsule shows, so the HUD window and the setup wizard's usage demo draw the very same capsule.
pub(crate) struct Capsule {
    pub state: HudState,
    pub lang: Lang,
    pub scale: Scale,
    pub palette: Palette,
    pub reduced: bool,
    /// Outer width in design points: [`rest_width`], or on its way there while widening.
    pub w: f32,
    /// Shared shimmer clock in seconds; stage changes do not restart the sweep.
    pub t: f32,
    pub wave: Wave,
    /// Progress fill, 0–1 (see [`fill_at`]).
    pub fill: Option<f32>,
    /// None renders the settled icon for static previews.
    pub outcome_t: Option<f32>,
}

impl Capsule {
    /// The capsule. `buttons` supplies every round button it carries: the HUD window passes live ones,
    /// a still picture passes [`static_button`]s.
    pub(crate) fn element(self, buttons: &mut dyn FnMut(Btn) -> AnyElement) -> Div {
        let Capsule { state, lang, scale, palette: p, reduced, w, t: since, wave, fill, outcome_t } = self;
        let px = |v: f32| scale.px(v);
        // Outcome capsules take their state color; the error one is the inverse danger style —
        // Red 80% (light) / dark Red tint (dark) with a Red border and Red ink, never solid Red.
        let bg = match &state {
            HudState::Inserted => hex("#28A788"),
            HudState::Copied => hex("#AA8C40"),
            HudState::Error { .. } => hud_color(&p, "#FFEFEF", "#62423C"),
            _ => hud_color(&p, "#FFF9EB", "#444139"),
        };
        let (border_w, border_c) = match &state {
            HudState::Error { .. } => (HAIRLINE, hex("#FF4949")),
            HudState::Inserted | HudState::Copied => (0., p.hud_border),
            _ => (HAIRLINE, p.hud_border),
        };
        let copy_overshoot =
            if matches!(state, HudState::Copied) { (copied_progress(outcome_t, reduced) - 1.).max(0.) } else { 0. };
        let height = CAPSULE_H - (copy_overshoot * 3.2).min(0.4);
        let cap = div()
            .w(px(w))
            .h(px(height))
            .rounded(px(height / 2.))
            .bg(bg)
            .border(px(border_w))
            .border_color(border_c)
            .shadow(surface_shadow(&p, scale, false));
        let mut cap =
            cap.overflow_hidden().relative().px(px(CAPSULE_PAD)).flex().items_center().justify_between().gap(px(3.));

        if let Some(frac) = fill {
            cap = cap.child(progress_layer((w * frac).clamp(0., w), w, scale, &p));
        }

        match state {
            HudState::Hidden => {}
            HudState::Listening { mode: RecordMode::Toggle } => {
                cap = cap
                    .child(buttons(Btn::Cancel))
                    .child(div().flex_1().flex().justify_center().child(waveform(wave, scale, &p)))
                    .child(buttons(Btn::Finish));
            }
            HudState::Listening { mode: RecordMode::Hold } => {
                cap = cap.justify_center().child(waveform(wave, scale, &p))
            }
            HudState::Recognizing => {
                cap = cap.justify_center().child(shimmer_label(lang.tr(Tr::HudTranscribing), since, scale, &p, reduced))
            }
            HudState::Polishing => {
                cap = cap.justify_center().child(shimmer_label(lang.tr(Tr::HudPolishing), since, scale, &p, reduced))
            }
            HudState::Inserted => cap = cap.justify_center().child(outcome_icon(false, outcome_t, reduced, scale)),
            HudState::Copied => {
                // Keep full-size horizontal slots throughout; only opacity and Y move per glyph.
                let label = lang.tr(Tr::HudCopied);
                let size = scale.tx(12.);
                let full_width = scale.tx(copied_label_width(lang));
                let progress = copied_progress(outcome_t, reduced);
                let icon_left = (scale.tx(w - COPY_ICON) / 2.) - (full_width + scale.tx(4.)) / 2. * progress;
                let mut row = div().absolute().left_0().top(px((height - 18.) / 2.)).w(px(w)).h(px(18.));
                row = row.child(
                    div()
                        .absolute()
                        .left(gpui_kit::px(icon_left))
                        .top_0()
                        .child(outcome_icon(true, outcome_t, reduced, scale)),
                );
                let mut prefix = String::new();
                for (i, ch) in label.chars().enumerate() {
                    let x = crate::ui::text_width(&prefix, size, FontWeight::NORMAL);
                    prefix.push(ch);
                    let (opacity, rise) = copied_glyph(outcome_t, i, reduced);
                    row = row.child(
                        div()
                            .absolute()
                            .left(gpui_kit::px(icon_left + scale.tx(COPY_ICON + 4.) + x))
                            .top(px(rise))
                            .opacity(opacity)
                            .child(text(ch.to_string(), size, FontWeight::NORMAL, hex("#FFF9EB")).line_height(px(18.))),
                    );
                }
                cap = cap.child(row);
            }
            HudState::Error { action, .. } => {
                let right = match action {
                    ErrorAction::Retry => buttons(Btn::Retry),
                    ErrorAction::OpenSettings => buttons(Btn::Settings),
                };
                cap = cap
                    .child(buttons(Btn::Dismiss))
                    .child(spaced(
                        lang.tr(Tr::HudError),
                        12.,
                        FontWeight::NORMAL,
                        hud_color(&p, "#FF4949", "#FF7972"),
                        scale,
                    ))
                    .child(right);
            }
        }
        cap
    }
}

/// The capsule's entry: it arrives like a drop of liquid — narrower and taller at first, rising from below —
/// and springs into its exact shape (a plain ease under Reduce Motion). `t` runs 0–1 over [`POP_MS`]; `w` is the
/// capsule's width in design points once arrived.
pub(crate) fn pop_shape<E: Styled>(c: E, scale: Scale, w: f32, t: f32, reduced: bool) -> E {
    let s = crate::ui::spring(t, reduced);
    let h = CAPSULE_H * (1.18 - 0.18 * s);
    let fade = (t * 2.2).min(1.);
    c.w(scale.px(w * (0.68 + 0.32 * s)))
        .h(scale.px(h))
        .rounded(scale.px(h / 2.))
        .mb(scale.px(-18. * (1. - s)))
        .opacity(fade * fade)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn copied_letters_overlap_and_the_shell_overshoots_then_settles() {
        let (first, first_y) = copied_glyph(Some(0.47), 0, false);
        let (next, next_y) = copied_glyph(Some(0.47), 1, false);
        assert!(first > next && next > 0. && first < 1.);
        assert!(first_y < next_y && next_y < 6.);
        assert!(copied_progress(Some(0.56), false) > 1.1);
        assert_eq!(copied_progress(Some(0.9), false), 1.);
        assert!(copied_glyph(Some(0.35 + 0.16), 0, false).1 < 0.);
        assert_eq!(copied_glyph(Some(0.2), 4, true), (1., 0.));
        assert_eq!(copied_progress(Some(0.), true), 1.);
        const {
            assert!(COPY_END > COPY_HOLD_END);
        }
    }

    #[::core::prelude::v1::test]
    fn the_progress_edge_is_a_vertical_line_even_inside_the_rounded_ends() {
        let inner = inner_w(CAPSULE_W);
        for x in [2., 6., 13., 41., 70., 78., inner - 0.5] {
            let pts = capsule_slice(inner, INNER_H, 0., x);
            let right: Vec<_> = pts.iter().filter(|p| (p.0 - x).abs() < 1e-4).collect();
            assert_eq!(right.len(), 2, "exactly the top and bottom point on the edge at x={x}");
            let (top, bottom) = (right[0].1.min(right[1].1), right[0].1.max(right[1].1));
            assert!(top >= 0. && bottom <= INNER_H, "inside the capsule at x={x}");
        }
        // Past the end cap the cut is shorter than the full height: the capsule's curve, not a box.
        let pts = capsule_slice(inner, INNER_H, 0., inner - 1.);
        let h = pts
            .iter()
            .filter(|p| (p.0 - (inner - 1.)).abs() < 1e-4)
            .map(|p| p.1)
            .fold((f32::MAX, 0f32), |a, y| (a.0.min(y), a.1.max(y)));
        assert!(h.1 - h.0 < INNER_H * 0.8);
    }

    #[::core::prelude::v1::test]
    fn the_narrow_capsule_cuts_its_rounded_ends_at_its_own_width() {
        // Hold mode's 72 pt capsule: the cut near its right end is the shorter curve of *that* capsule, not of a 105 pt one.
        let narrow = inner_w(NARROW_W);
        let near_end = narrow - 1.;
        let height_at = |w: f32, x: f32| {
            let pts = capsule_slice(w, INNER_H, 0., x);
            let ys: Vec<f32> = pts.iter().filter(|p| (p.0 - x).abs() < 1e-4).map(|p| p.1).collect();
            ys.iter().cloned().fold(f32::MIN, f32::max) - ys.iter().cloned().fold(f32::MAX, f32::min)
        };
        assert!(height_at(narrow, near_end) < INNER_H * 0.8, "short near the narrow capsule's end");
        assert!(
            (height_at(inner_w(CAPSULE_W), near_end) - INNER_H).abs() < 1e-3,
            "full height mid-way along the wide one"
        );
    }

    #[::core::prelude::v1::test]
    fn only_hold_mode_listening_is_narrow() {
        let listening = |mode| HudState::Listening { mode };
        assert_eq!(rest_width(&listening(RecordMode::Hold)), NARROW_W);
        assert_eq!(rest_width(&listening(RecordMode::Toggle)), CAPSULE_W);
        for state in [HudState::Recognizing, HudState::Polishing, HudState::Inserted, HudState::Copied] {
            assert_eq!(rest_width(&state), CAPSULE_W, "{state:?}");
        }
        assert_eq!(CAPSULE_W, 105.);
    }

    #[::core::prelude::v1::test]
    fn the_capsule_widens_with_a_small_spring() {
        let at = |t| eased_width(NARROW_W, CAPSULE_W, t);
        assert_eq!(at(0.), NARROW_W);
        assert_eq!(at(WIDEN_S), CAPSULE_W);
        let peak = (0..=100).map(|i| at(i as f32 * WIDEN_S / 100.)).fold(0., f32::max);
        assert!(peak > CAPSULE_W && peak < CAPSULE_W * 1.03);
        assert_eq!(eased_width(70., 70., 0.05), 70.);
    }

    #[::core::prelude::v1::test]
    fn the_fill_runs_half_then_half_and_lands_an_early_finish() {
        assert_eq!(fill_at(&HudState::Listening { mode: RecordMode::Hold }, 1., None), None);
        assert_eq!(fill_at(&HudState::Inserted, 1., None), None);
        let recognizing = |t| fill_at(&HudState::Recognizing, t, None).unwrap();
        assert_eq!(recognizing(0.), 0.);
        assert!(recognizing(0.75) > 0.25);
        assert!((recognizing(1.5) - 0.485).abs() < 1e-6);
        assert_eq!(recognizing(1.5), recognizing(60.), "hold without creeping or claiming completion");
        let polishing = |t, carry| fill_at(&HudState::Polishing, t, carry).unwrap();
        assert_eq!(polishing(0., None), 0.5);
        assert!((polishing(1.5, None) - 0.985).abs() < 1e-6);
        assert_eq!(polishing(60., None), polishing(1.5, None));
        assert!((polishing(0., Some(0.3)) - 0.3).abs() < 1e-6);
        assert!((polishing(SETTLE_S, Some(0.3)) - 0.5).abs() < 1e-6);
        assert!((settle(0.62, 1., SETTLE_S) - 1.).abs() < 1e-6);
    }

    #[::core::prelude::v1::test]
    fn the_narrow_sweep_takes_1100_ms_then_rests_for_1400_ms() {
        let total = 40.;
        let half = total * 0.25;
        assert!((sweep_at(0., total) + half).abs() < 1e-4);
        assert!((sweep_at(0.55, total) - total / 2.).abs() < 1e-3);
        for t in [1.1, 1.5, 2.49] {
            assert!((sweep_at(t, total) - (total + half)).abs() < 1e-4);
        }
        assert!((sweep_at(2.5, total) + half).abs() < 1e-4);
    }

    #[::core::prelude::v1::test]
    fn reduced_motion_settles_once_instead_of_continuously_advancing() {
        for (state, from, target) in [(HudState::Recognizing, 0., 0.485), (HudState::Polishing, 0.5, 0.985)] {
            assert_eq!(motion_fill_at(&state, 0., None, true), Some(from));
            assert!((motion_fill_at(&state, 0.12, None, true).unwrap() - target).abs() < 1e-6);
            assert_eq!(motion_fill_at(&state, 0.12, None, true), motion_fill_at(&state, 10., None, true));
        }
    }

    #[::core::prelude::v1::test]
    fn english_text_keeps_natural_letter_spacing() {
        use dito_core::i18n::Lang;
        crate::ui::set_ui_language(Lang::En);
        assert_eq!(crate::ui::tracking_for("Recognizing", 0.08), 0.);
        assert_eq!(crate::ui::tracking_for("認識中", 0.08), 0.);
        crate::ui::set_ui_language(Lang::Ja);
        assert_eq!(crate::ui::tracking_for("認識中", 0.08), 0.08);
        assert_eq!(crate::ui::tracking_for("Slack", 0.08), 0., "no CJK glyphs, no tracking");
        crate::ui::set_ui_language(Lang::Zh);
        assert_eq!(crate::ui::tracking_for("识别中", 0.08), 0.08);
    }
}
