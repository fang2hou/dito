use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::callbacks::{Cb, IndexCb, ToggleCb, click_cb};
use super::motion::{
    HOVER_MS, count_flips, coverage, hover_tracker, is_hovered, motion_scope, plate_state, spring, tweened, tweened_at,
};
use super::primitives::{hex_black, icon, mix};
use super::tooltip::{tip_above, tip_label};
use super::typography::{text, text_width};
use crate::assets::Ic;
use crate::theme::Palette;

pub const CONTROL_H: f32 = 36.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BtnKind {
    Primary,
    Secondary,
    Danger,
}

/// Empty background that drags the window when pressed: for custom-titlebar zones outside AppKit's
/// native titlebar strip. Only attach to regions that contain no children — a bubble-phase mouse
/// listener on a container would also fire for clicks on everything inside it.
pub fn drag_area(id: impl Into<ElementId>) -> Stateful<Div> {
    div().id(id).on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
}

/// A rounded fill laid over an element whose opacity follows hover: `color` fades in when `id` is hovered.
pub fn hover_fill(id: &ElementId, radius: Option<f32>, color: Hsla) -> AnyElement {
    hover_fill_if(id, radius, color, true)
}

/// Like [`hover_fill`], but only while `enabled`; turning it off (e.g. the item became the selected one) fades the
/// fill out instead of dropping it, so nothing blinks.
pub fn hover_fill_if(id: &ElementId, radius: Option<f32>, color: Hsla, enabled: bool) -> AnyElement {
    let target = if enabled && is_hovered(id) { 1. } else { 0. };
    tweened(ElementId::from((id.clone(), "hover-fill")), target, HOVER_MS, move |v| {
        let d = div().absolute().inset_0().bg(Hsla { a: color.a * v, ..color });
        match radius {
            Some(r) => d.rounded(px(r)),
            None => d.rounded_full(),
        }
    })
}

/// Label of a choice card: medium weight always, brightness eased between idle, hovered and chosen.
pub fn choice_label(id: &ElementId, p: &Palette, label: SharedString, size: f32, on: bool) -> AnyElement {
    let level = if on {
        1.
    } else if is_hovered(id) {
        0.6
    } else {
        0.
    };
    let (text_c, muted_c) = (p.text, p.muted);
    tweened(ElementId::from((id.clone(), "label")), level, HOVER_MS, move |v| {
        text(label.clone(), size, FontWeight::MEDIUM, mix(muted_c, text_c, v)).line_height(relative(1.))
    })
}

/// Pill button, 36px high (40 when `large`).
pub fn button(
    id: impl Into<ElementId>,
    p: &Palette,
    label: &str,
    kind: BtnKind,
    ic: Option<Ic>,
    on_click: Cb,
) -> Stateful<Div> {
    // Secondary sits raised on its card (card fill + hairline), Danger carries the red:
    // a soft red plate in light, the solid danger fill with white text in dark.
    let (bg, fg, border) = match kind {
        BtnKind::Primary => (p.primary, p.on_primary, None),
        BtnKind::Secondary => (p.card, p.text, Some(p.border_strong)),
        BtnKind::Danger => {
            if p.dark {
                (p.danger, p.danger_text, None)
            } else {
                (p.danger_soft, p.danger_text, Some(p.danger))
            }
        }
    };
    // Hover tint: lighten a dark button, darken a light one, strongly enough to read at a glance.
    let tint = if kind == BtnKind::Primary {
        if bg.l < 0.5 { hsla(0., 0., 1., 0.24) } else { hsla(0., 0., 0., 0.16) }
    } else if kind == BtnKind::Danger {
        Hsla { a: if p.dark { 0.22 } else { 0.14 }, ..p.danger }
    } else {
        p.seg
    };
    let id: ElementId = id.into();
    let mut el = div()
        .id(id.clone())
        .h(px(CONTROL_H))
        .px(px(16.))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .relative()
        .rounded_full()
        .bg(bg)
        .cursor_pointer()
        .on_click(click_cb(on_click))
        .on_hover(hover_tracker(id.clone()))
        .child(hover_fill(&id, None, tint));
    if let Some(b) = border {
        el = el.border_1().border_color(b);
    }
    if let Some(ic) = ic {
        el = el.child(icon(ic, 14., fg));
    }
    el.child(text(label.to_string(), 13., FontWeight::MEDIUM, fg).line_height(relative(1.)))
}

/// 36px circular icon button (copy, ⋮ ...).
pub fn icon_button(id: impl Into<ElementId>, p: &Palette, ic: Ic, active: bool, on_click: Cb) -> Stateful<Div> {
    let id: ElementId = id.into();
    div()
        .id(id.clone())
        .size(px(CONTROL_H))
        .flex()
        .items_center()
        .justify_center()
        .relative()
        .rounded_full()
        .border_1()
        .border_color(p.border_strong)
        .bg(if active { p.seg } else { p.input_bg })
        .cursor_pointer()
        .on_click(click_cb(on_click))
        .on_hover(hover_tracker(id.clone()))
        .child(hover_fill(&id, None, p.seg))
        .child(icon(ic, 16., p.text))
}

/// Plain (borderless) icon button, e.g. the × beside a sample row.
pub fn bare_icon_button(id: impl Into<ElementId>, p: &Palette, ic: Ic, on_click: Cb) -> Stateful<Div> {
    let id: ElementId = id.into();
    div()
        .id(id.clone())
        .size(px(CONTROL_H))
        .flex()
        .items_center()
        .justify_center()
        .relative()
        .rounded_full()
        .cursor_pointer()
        .on_click(click_cb(on_click))
        .on_hover(hover_tracker(id.clone()))
        .child(hover_fill(&id, None, p.seg))
        .child(icon(ic, 16., p.muted))
}

const SEG_PAD: f32 = 3.;
const SEG_GAP: f32 = 2.;
const SEG_CELL_PAD: f32 = 16.;

/// Segmented control. `on_select` receives the chosen index. The selected plate slides from the previous
/// option to the new one, changing width along the way; every cell has a fixed width (its label at the heavier
/// weight) so choosing never resizes the control.
/// Metrics and hover treatment for [`seg_with`].
pub struct SegStyle {
    pub h: f32,
    pub label: f32,
    pub cell_pad: f32,
    pub inset: f32,
    pub gap: f32,
    /// Hover tint over an unselected cell and the label color while hovered. Dark plate surfaces
    /// need a light pair (the default mixes the label toward `p.text`, which lands mid-gray on gold).
    pub hover: Option<(Hsla, Hsla)>,
}

/// Shared default feedback for text and icon segmented controls.
fn segment_hover_colors(p: &Palette) -> (Hsla, Hsla) {
    (Hsla { a: 0.55, ..p.seg_on }, mix(p.muted, p.text, 0.6))
}

pub fn seg(
    id: impl Into<ElementId>,
    p: &Palette,
    options: &[SharedString],
    selected: usize,
    on_select: IndexCb,
) -> Stateful<Div> {
    seg_with(
        id,
        p,
        options,
        selected,
        on_select,
        SegStyle { h: CONTROL_H, label: 13., cell_pad: SEG_CELL_PAD, inset: SEG_PAD, gap: SEG_GAP, hover: None },
    )
}

/// [`seg`] with explicit [`SegSize`] metrics.
pub fn seg_with(
    id: impl Into<ElementId>,
    p: &Palette,
    options: &[SharedString],
    selected: usize,
    on_select: IndexCb,
    m: SegStyle,
) -> Stateful<Div> {
    let SegStyle { h, label: size, cell_pad, inset, gap, hover } = m;
    let id: ElementId = id.into();
    let widths: Vec<f32> = options
        .iter()
        .map(|o| {
            let w = text_width(o, size, FontWeight::MEDIUM).max(text_width(o, size, FontWeight::NORMAL));
            w.ceil() + 2. * cell_pad
        })
        .collect();
    let left = |i: usize| inset + widths[..i].iter().sum::<f32>() + gap * i as f32;
    let total = 2. * inset + widths.iter().sum::<f32>() + gap * widths.len().saturating_sub(1) as f32;

    let seg_on = p.seg_on;
    let (text_c, muted_c) = (p.text, p.muted);
    let (hover_fill, hover_label) = hover.unwrap_or_else(|| segment_hover_colors(p));
    let mut el = div().id(id.clone()).h(px(h)).w(px(total)).flex_none().relative().rounded_full().bg(p.seg);
    // One motion state drives the plate and the labels; with no selection the plate rests off-screen.
    let target =
        if selected < widths.len() { (left(selected), left(selected) + widths[selected]) } else { (-1000., -999.) };
    let motion = plate_state(ElementId::from((id.clone(), "plate")), target);
    el = el.child(motion.animate("plate", move |a, b, energy| {
        // Lit from within while it travels (a touch brighter).
        let lit = mix(seg_on, hsla(0., 0., 1., 1.), 0.18 * energy);
        div().absolute().top(px(inset)).left(px(a)).w(px(b - a)).h(px(h - 2. * inset)).rounded_full().bg(lit)
    }));

    // Interactive cells: transparent, they only take the pointer and show the hover tint.
    let mut cell_ids = Vec::new();
    for (i, w) in widths.iter().enumerate() {
        let f = on_select.clone();
        let cell_id = ElementId::from((id.clone(), SharedString::from(format!("i{i}"))));
        el = el.child(
            div()
                .id(cell_id.clone())
                .absolute()
                .top(px(inset))
                .left(px(left(i)))
                .w(px(*w))
                .h(px(h - 2. * inset))
                .rounded_full()
                .cursor_pointer()
                .on_click(move |_, w, cx| f(i, w, cx))
                .on_hover(hover_tracker(cell_id.clone()))
                .child(hover_fill_if(&cell_id, None, hover_fill, i != selected)),
        );
        cell_ids.push(is_hovered(&cell_id));
    }

    // Labels sit above the cells. Their brightness is how much of the label the plate covers (so they light up
    // as the plate arrives, not when the click lands), or the hover level, whichever is higher.
    let labels: Vec<(SharedString, f32, f32, bool)> = options
        .iter()
        .enumerate()
        .map(|(i, o)| (o.clone(), left(i), widths[i], cell_ids[i] && i != selected))
        .collect();
    let id_for_labels = id.clone();
    let seg_scope = motion_scope();
    el.child(motion.animate("labels", move |a, b, _| {
        let mut layer = div().absolute().top(px(0.)).left(px(0.)).size_full();
        for (i, (label, x, w, hovered)) in labels.iter().enumerate() {
            let cover = coverage(a, b, *x, *x + *w);
            let weight = if cover >= 0.5 { FontWeight::MEDIUM } else { FontWeight::NORMAL };
            let label = label.clone();
            let hover_id = ElementId::from((id_for_labels.clone(), SharedString::from(format!("hover-{i}"))));
            let hover_level =
                tweened_at(seg_scope.clone(), hover_id, if *hovered { 1. } else { 0. }, HOVER_MS, move |hv| {
                    text(label.clone(), size, weight, mix(mix(muted_c, hover_label, hv), text_c, cover))
                });
            layer = layer.child(
                div()
                    .absolute()
                    .top(px(inset))
                    .left(px(*x))
                    .w(px(*w))
                    .h(px(h - 2. * inset))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(hover_level),
            );
        }
        layer
    }))
}

/// Icon-only appearance control, matching the Settings design (138 × 36 pt).
pub fn appearance_seg(
    id: impl Into<ElementId>,
    p: &Palette,
    labels: &[SharedString],
    selected: usize,
    on_select: IndexCb,
) -> Stateful<Div> {
    let id: ElementId = id.into();
    let left = |i: usize| 3. + 45. * i as f32;
    let motion = plate_state(ElementId::from((id.clone(), "plate")), (left(selected), left(selected) + 42.));
    let plate = p.seg_on;
    let mut control =
        div().id(id.clone()).w(px(138.)).h(px(36.)).flex_none().relative().rounded_full().bg(p.seg).child(
            motion.animate("plate", move |a, b, energy| {
                div().absolute().left(px(a)).top(px(3.)).w(px(b - a)).h(px(30.)).rounded_full().bg(mix(
                    plate,
                    hsla(0., 0., 1., 1.),
                    0.18 * energy,
                ))
            }),
        );
    let (hover_color, hover_icon) = segment_hover_colors(p);
    let mut icons = Vec::new();
    for (i, glyph) in ["system", "light", "dark"].into_iter().enumerate() {
        let action = on_select.clone();
        let cell_id = ElementId::from((id.clone(), SharedString::from(glyph)));
        tip_label(cell_id.clone(), labels[i].clone());
        control = control.child(
            div()
                .id(cell_id.clone())
                .absolute()
                .left(px(left(i)))
                .top(px(3.))
                .w(px(42.))
                .h(px(30.))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .on_click(move |_, window, cx| action(i, window, cx))
                .role(Role::RadioButton)
                .aria_label(labels[i].clone())
                .aria_selected(i == selected)
                .on_hover(hover_tracker(cell_id.clone()))
                .child(tip_above(p, &cell_id))
                .child(hover_fill_if(&cell_id, None, hover_color, i != selected)),
        );
        icons.push((glyph, left(i), is_hovered(&cell_id) && i != selected));
    }
    let scope = motion_scope();
    let (muted, text_color) = (p.muted, p.text);
    control.child(motion.animate("icons", move |a, b, _| {
        let mut layer = div().absolute().inset_0();
        for (glyph, x, hovered) in &icons {
            let cover = coverage(a, b, *x, *x + 42.);
            let path = SharedString::from(format!("icons/appearance/{glyph}.svg"));
            let hover_id = ElementId::from((id.clone(), SharedString::from(format!("icon-hover-{glyph}"))));
            let icon = tweened_at(scope.clone(), hover_id, if *hovered { 1. } else { 0. }, HOVER_MS, move |hv| {
                svg().path(path.clone()).size(px(18.)).text_color(mix(mix(muted, hover_icon, hv), text_color, cover))
            });
            layer = layer.child(
                div()
                    .absolute()
                    .left(px(*x))
                    .top(px(3.))
                    .w(px(42.))
                    .h(px(30.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon),
            );
        }
        layer
    }))
}

const SWITCH_W: f32 = 44.;
const SWITCH_H: f32 = 26.;
const KNOB: f32 = 20.;
const KNOB_INSET: f32 = 3.;
const SWITCH_TRAVEL: f32 = SWITCH_W - KNOB - 2. * KNOB_INSET;
const SWITCH_MS: u64 = 380;

/// Switch: the knob slides and the track colour cross-fades, both on the same eased clock.
pub fn switch(id: impl Into<ElementId>, p: &Palette, on: bool, on_toggle: ToggleCb) -> Stateful<Div> {
    let id: ElementId = id.into();
    let flips = count_flips(&id, on, Duration::from_millis(SWITCH_MS));
    let (track_off, track_on, knob_off, knob_on) = (p.toggle_off, p.primary, p.knob_off, p.knob_on);
    let track_at = move |f: f32| {
        div().absolute().top(px(0.)).left(px(0.)).size_full().rounded_full().bg(mix(
            track_off,
            track_on,
            f.clamp(0., 1.),
        ))
    };
    let knob_at = move |f: f32| {
        div()
            .absolute()
            .top(px(KNOB_INSET))
            .left(px(KNOB_INSET + SWITCH_TRAVEL * f))
            .size(px(KNOB))
            .rounded_full()
            .bg(mix(knob_off, knob_on, f.clamp(0., 1.)))
            .shadow(vec![BoxShadow {
                color: hex_black(0.18),
                offset: point(px(0.), px(1.)),
                blur_radius: px(2.),
                spread_radius: px(0.),
                inset: false,
            }])
    };
    let settled = if on { 1. } else { 0. };
    let root = div()
        .id(id.clone())
        .w(px(SWITCH_W))
        .h(px(SWITCH_H))
        .flex_none()
        .relative()
        .cursor_pointer()
        .on_click(move |_, w, cx| on_toggle(!on, w, cx));
    if flips == 0 {
        return root.child(track_at(settled)).child(knob_at(settled));
    }
    let animation = || Animation::new(Duration::from_millis(SWITCH_MS));
    let reduced = dito_platform::system::reduce_motion();
    // The knob rides the spring (a hair of overshoot); the colour just follows the clamped position.
    let value = move |t: f32| {
        let q = spring(t, reduced);
        if on { q } else { 1. - q }
    };
    root.child(track_at(settled).with_animation(
        (id.clone(), SharedString::from(format!("track-{flips}"))),
        animation(),
        move |_, t| track_at(value(t)),
    ))
    .child(knob_at(settled).with_animation(
        (id, SharedString::from(format!("knob-{flips}"))),
        animation(),
        move |_, t| knob_at(value(t)),
    ))
}

pub fn progress_bar(p: &Palette, fraction: f32) -> Div {
    div()
        .w_full()
        .h(px(8.))
        .rounded(px(4.))
        .bg(p.seg)
        .child(div().h_full().w(relative(fraction.clamp(0., 1.))).rounded(px(4.)).bg(p.accent))
}

pub fn radio(p: &Palette, on: bool) -> Div {
    let d = div().size(px(16.)).flex_none().rounded_full().flex().items_center().justify_center();
    if on {
        d.bg(p.primary).child(div().size(px(6.)).rounded_full().bg(p.on_primary))
    } else {
        d.border_1().border_color(p.border_strong)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TestState {
    Idle,
    Running,
    Ok(u64),
    Failed(String),
}
