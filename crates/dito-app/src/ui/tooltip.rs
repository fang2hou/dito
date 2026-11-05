use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::motion::{is_hovered, motion_scope, tween_to_at, tweened_at};
use super::primitives::{hex_white, transparent};
use super::typography::{family_for, text};
use crate::theme::Palette;

/// A tooltip bubble drawn as one rigid unit. Layout always reserves the final size (an invisible
/// line of the label at full size), and paint scales the whole thing — chrome, shadow and text
/// together — about its bottom-centre by `k`. Scaling through layout (growing font sizes)
/// re-flows the text every frame and makes it visibly slide sideways; this way nothing re-flows
/// and the pop-in reads as one object growing out of the trigger. `a` is the fade alpha.
fn tip_bubble(p: &Palette, label: &SharedString, k: f32, a: f32, bg: Hsla, fg: Hsla) -> Div {
    let p = *p;
    let label = label.clone();
    div()
        .relative()
        .flex_none()
        .h(px(TIP_BUBBLE_H))
        .flex()
        .items_center()
        .child(
            text(label.clone(), 12., FontWeight::NORMAL, transparent())
                .px(px(10.5))
                .line_height(relative(1.))
                .opacity(0.),
        )
        .child(
            canvas(
                move |b, _, _| b,
                move |b, _, window, cx| {
                    let k = k.clamp(0.2, 1.);
                    let w = b.size.width * k;
                    let h = b.size.height * k;
                    let center_x = b.origin.x + b.size.width * 0.5;
                    let bottom = b.origin.y + b.size.height;
                    let bounds = Bounds::new(point(center_x - w * 0.5, bottom - h), size(w, h));
                    let radii = Corners::all(px(8. * k));
                    window.paint_drop_shadows(
                        bounds,
                        radii,
                        &[BoxShadow {
                            color: p.shadow.opacity(a),
                            offset: point(px(0.), px(4. * k)),
                            blur_radius: px(12. * k),
                            spread_radius: px(0.),
                            inset: false,
                        }],
                    );
                    window.paint_quad(quad(
                        bounds,
                        radii,
                        bg.opacity(a),
                        Edges::all(px(1.)),
                        p.hud_border.opacity(a),
                        BorderStyle::Solid,
                    ));
                    let font_size = px(12. * k);
                    let run = TextRun {
                        len: label.len(),
                        font: Font {
                            family: family_for(&label).into(),
                            weight: FontWeight::MEDIUM,
                            ..Default::default()
                        },
                        color: fg.opacity(a),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let line = window.text_system().shape_line(label.clone(), font_size, &[run], None);
                    let text_top = origin_y_center(h, font_size, bottom);
                    line.paint(
                        point(center_x - line.width() * 0.5, text_top),
                        font_size,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    )
                    .ok();
                },
            )
            .absolute()
            .left_0()
            .top_0()
            .right_0()
            .bottom_0(),
        )
}

/// Top of a vertically centred `font_size`-tall line box inside a bubble of height `h` whose
/// bottom sits at `bottom`.
fn origin_y_center(h: Pixels, font_size: Pixels, bottom: Pixels) -> Pixels {
    bottom - h * 0.5 - font_size * 0.5
}

/// Gap between a tooltip and its anchor, how far it still has to rise when it starts appearing,
/// how long that takes, the size it pops out of the trigger at, and the bubble's fixed height:
/// the label's line box is taller than its font size, so the height is stated, not measured —
/// the anchor offset below depends on it exactly.
pub const TIP_BUBBLE_H: f32 = 30.;
const TIP_GAP: f32 = 6.;
const TIP_RISE: f32 = 6.;
const TIP_MS: u64 = 150;
const TIP_FROM: f32 = 0.8;

/// The tooltip for a trigger: put it next to the trigger's content and pass the trigger's
/// height. While hovered, one bubble pops out of the trigger — growing from 0.8 to full size
/// as it rises the last few pixels and fades in (150 ms, ease-out) — and vanishes the instant
/// the hover leaves. The positioned box is a plain static element (an animated one re-lays out
/// against the wrong ancestor); only its inner child tweens. Centring does not measure the
/// text: the box is symmetric about the trigger's centre (non-zero insets — a zero `left` lays
/// out as if unset), so a longer translation cannot drift it off-centre.
pub fn tip_above(p: &Palette, id: &ElementId) -> AnyElement {
    let on = is_hovered(id);
    let label =
        super::motion::ambient_registry().and_then(|reg| reg.borrow().tips.get(id).cloned()).unwrap_or_default();
    // Priority 2: above the dropdown panels' floating layer (priority 1), so a tip anchored
    // inside an open menu is never painted underneath the menu itself.
    deferred(
        div()
            .absolute()
            .top(px(-(TIP_BUBBLE_H + TIP_GAP)))
            .left(px(0.5))
            .right(px(0.5))
            .flex()
            .justify_center()
            .child(tip_pop(p, ElementId::from((id.clone(), "tip")), label, on)),
    )
    .with_priority(2)
    .into_any_element()
}

/// The tweened inner of a tooltip, shared by the hover tips ([`tip_above`]) and the
/// shortcut-recording hint. `on` decides whether it shows: a tween only plays when its target
/// *changes*, so while off the value is held at 0 (an element that first appears already at
/// its target would render statically) and the 0→1 flip when it turns on is the pop-in —
/// growing from [`TIP_FROM`] to full size while rising and fading in, ease-out. Turning off
/// hides it at once; the value settles back to 0 unseen.
pub fn tip_pop(p: &Palette, tween_id: ElementId, label: SharedString, on: bool) -> AnyElement {
    let (bg, fg) = if p.dark { (p.side, hex_white()) } else { (p.hud_bg, p.text) };
    tip_pop_with(p, tween_id, label, on, bg, fg)
}

/// [`tip_pop`] with the bubble chrome stated by the caller — the HUD passes its own surface
/// colors, which differ from the settings surface in dark mode.
pub fn tip_pop_with(p: &Palette, tween_id: ElementId, label: SharedString, on: bool, bg: Hsla, fg: Hsla) -> AnyElement {
    let scope = motion_scope();
    if !on {
        tween_to_at(&scope, &tween_id, 0., Duration::from_millis(TIP_MS));
        return div().into_any_element();
    }
    let p = *p;
    tweened_at(scope, tween_id, 1., TIP_MS, move |v| {
        if v <= 0.01 {
            return div().into_any_element();
        }
        let v = 1. - (1. - v).powi(3);
        let k = TIP_FROM + (1. - TIP_FROM) * v;
        div().mt(px((1. - v) * TIP_RISE)).child(tip_bubble(&p, &label, k, v, bg, fg)).into_any_element()
    })
}

/// Remember `label` as the tooltip text for the control `id` (see [`tip_above`]). Called during
/// the control's build, so the label always matches the rendered language.
pub fn tip_label(id: ElementId, label: SharedString) {
    if let Some(reg) = super::motion::ambient_registry() {
        reg.borrow_mut().tips.insert(id, label);
    }
}

/// History's divider-centred hint: the same tooltip surface, with a reversible
/// chevron and cross-fading labels. It is passive so it never steals a row click.
pub fn history_tip(
    p: &Palette,
    id: ElementId,
    show: SharedString,
    hide: SharedString,
    open: bool,
    on: bool,
) -> AnyElement {
    let p = *p;
    let reduced = dito_platform::system::reduce_motion();
    // Keep the disclosure clock alive even while its hint is hidden.
    let toggle_id = ElementId::from((id.clone(), "history-toggle"));
    let pop_id = ElementId::from((id, "history-tip"));
    tweened_at(motion_scope(), toggle_id, if open { 1. } else { 0. }, if reduced { 1 } else { 200 }, move |v| {
        let show = show.clone();
        let hide = hide.clone();
        tweened_at(motion_scope(), pop_id.clone(), if on { 1. } else { 0. }, if reduced { 60 } else { 150 }, move |a| {
            if !on || a <= 0.001 {
                return div().into_any_element();
            }
            let fg = if p.dark { hex_white() } else { p.text };
            // Both labels share one measured slot and one explicit line box. Hidden
            // text in a 12px flex column inherited the page's taller line-height,
            // which pushed its visible copy below the chevron.
            let width = super::typography::text_width(&show, 12., FontWeight::MEDIUM)
                .max(super::typography::text_width(&hide, 12., FontWeight::MEDIUM));
            let label = |value: SharedString, alpha: f32| {
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .w_full()
                    .h_full()
                    .flex()
                    .items_center()
                    .opacity(alpha)
                    .child(text(value, 12., FontWeight::MEDIUM, fg).line_height(relative(1.)))
            };
            let labels = div()
                .relative()
                .flex_none()
                .w(px(width))
                .h(px(12.))
                .child(label(show.clone(), 1. - v))
                .child(label(hide.clone(), v));
            // Keep the centre fixed on the divider throughout the fade. A
            // vertical entrance offset makes the hint look detached from its row.
            div()
                .h(px(TIP_BUBBLE_H))
                .px(px(12.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(10.))
                .border_1()
                .border_color(p.hud_border)
                .bg(if p.dark { p.side } else { p.hud_bg })
                .opacity(a)
                .shadow(vec![BoxShadow {
                    color: p.shadow,
                    offset: point(px(0.), px(4.)),
                    blur_radius: px(12.),
                    spread_radius: px(0.),
                    inset: false,
                }])
                .child(
                    super::icon(crate::assets::Ic::ChevronDown, 12., fg)
                        .with_transformation(Transformation::rotate(radians(std::f32::consts::PI * v))),
                )
                .child(
                    div()
                        .relative()
                        .size(px(16.))
                        .flex_none()
                        .child(
                            super::icon(crate::assets::Ic::MouseLeftButton, 16., p.accent).absolute().top_0().left_0(),
                        )
                        .child(super::icon(crate::assets::Ic::Mouse, 16., fg).absolute().top_0().left_0()),
                )
                .child(labels)
                .into_any_element()
        })
    })
}
