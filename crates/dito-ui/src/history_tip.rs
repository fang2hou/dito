use gpui_kit::prelude::*;
use gpui_kit::*;

use super::primitives::{icon, white};
use super::scope::motion_scope;
use super::tip_bubble::TIP_BUBBLE_H;
use super::tween::tweened_at;
use super::typography::{text, text_width};
use crate::Ic;
use crate::Palette;

/// Passive, so it never steals a row click.
pub fn history_tip(
    p: &Palette,
    id: ElementId,
    show: SharedString,
    hide: SharedString,
    open: bool,
    on: bool,
) -> AnyElement {
    let p = *p;
    let reduced = super::motion::reduce_motion();
    // Keep the disclosure clock alive even while its hint is hidden.
    let toggle_id = ElementId::from((id.clone(), "history-toggle"));
    let pop_id = ElementId::from((id, "history-tip"));
    tweened_at(motion_scope(), toggle_id, if open { 1. } else { 0. }, if reduced { 1 } else { 200 }, move |v| {
        let (show, hide) = (show.clone(), hide.clone());
        tweened_at(motion_scope(), pop_id.clone(), if on { 1. } else { 0. }, if reduced { 60 } else { 150 }, move |a| {
            if a <= 0.001 {
                return div().into_any_element();
            }
            hint_bubble(&p, &show, &hide, v, a)
        })
    })
}

/// `v` is the disclosure progress (0 shows `show`, 1 shows `hide`), `a` the fade alpha.
fn hint_bubble(p: &Palette, show: &SharedString, hide: &SharedString, v: f32, a: f32) -> AnyElement {
    let fg = if p.dark { white() } else { p.text };
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
            icon(Ic::ChevronDown, 12., fg)
                .with_transformation(Transformation::rotate(radians(std::f32::consts::PI * v))),
        )
        .child(mouse_glyph(p, fg))
        .child(crossfading_labels(show, hide, v, fg))
        .into_any_element()
}

fn mouse_glyph(p: &Palette, fg: Hsla) -> Div {
    div()
        .relative()
        .size(px(16.))
        .flex_none()
        .child(icon(Ic::MouseLeftButton, 16., p.accent).absolute().top_0().left_0())
        .child(icon(Ic::Mouse, 16., fg).absolute().top_0().left_0())
}

/// Both labels share one measured slot and one explicit line box. Hidden
/// text in a 12px flex column inherited the page's taller line-height,
/// which pushed its visible copy below the chevron.
fn crossfading_labels(show: &SharedString, hide: &SharedString, v: f32, fg: Hsla) -> Div {
    let width = text_width(show, 12., FontWeight::MEDIUM).max(text_width(hide, 12., FontWeight::MEDIUM));
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
    div()
        .relative()
        .flex_none()
        .w(px(width))
        .h(px(12.))
        .child(label(show.clone(), 1. - v))
        .child(label(hide.clone(), v))
}
