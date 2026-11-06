use gpui_kit::prelude::*;
use gpui_kit::*;

use super::callbacks::IndexCb;
use super::hover::{HOVER_MS, hover_fill_if, hover_tracker, is_hovered};
use super::plate::{coverage, plate_state};
use super::primitives::mix;
use super::scope::{ScopeHandle, motion_scope};
use super::segmented::segment_hover_colors;
use super::tooltip::{tip_above, tip_label};
use super::tween::tweened_at;
use crate::theme::Palette;

const GLYPHS: [&str; 3] = ["system", "light", "dark"];
const CELL_W: f32 = 42.;
const CELL_H: f32 = 30.;
const INSET: f32 = 3.;
const PITCH: f32 = 45.;

struct IconPaint {
    id: ElementId,
    scope: ScopeHandle,
    muted: Hsla,
    text: Hsla,
    hover: Hsla,
}

fn left(i: usize) -> f32 {
    INSET + PITCH * i as f32
}

fn cell_slot(x: f32) -> Div {
    div().absolute().left(px(x)).top(px(INSET)).w(px(CELL_W)).h(px(CELL_H))
}

/// Icon-only appearance control.
pub fn appearance_seg(
    id: impl Into<ElementId>,
    p: &Palette,
    labels: &[SharedString],
    selected: usize,
    on_select: IndexCb,
) -> Stateful<Div> {
    let id: ElementId = id.into();
    let motion = plate_state(ElementId::from((id.clone(), "plate")), (left(selected), left(selected) + CELL_W));
    let plate = p.seg_on;
    let mut control =
        div().id(id.clone()).w(px(138.)).h(px(36.)).flex_none().relative().rounded_full().bg(p.seg).child(
            motion.animate("plate", move |a, b, energy| {
                cell_slot(a).w(px(b - a)).rounded_full().bg(mix(plate, hsla(0., 0., 1., 1.), 0.18 * energy))
            }),
        );
    let (hover_color, hover_icon) = segment_hover_colors(p);
    let mut hovered = [false; GLYPHS.len()];
    for (i, glyph) in GLYPHS.into_iter().enumerate() {
        let action = on_select.clone();
        let cell_id = ElementId::from((id.clone(), SharedString::from(glyph)));
        tip_label(cell_id.clone(), labels[i].clone());
        control = control.child(
            cell_slot(left(i))
                .id(cell_id.clone())
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
        hovered[i] = is_hovered(&cell_id) && i != selected;
    }
    let paint = IconPaint { id, scope: motion_scope(), muted: p.muted, text: p.text, hover: hover_icon };
    control.child(motion.animate("icons", move |a, b, _| icons_layer(&paint, &hovered, a, b)))
}

fn icons_layer(paint: &IconPaint, hovered: &[bool], a: f32, b: f32) -> Div {
    let mut layer = div().absolute().inset_0();
    for (i, glyph) in GLYPHS.into_iter().enumerate() {
        let x = left(i);
        let cover = coverage(a, b, x, x + CELL_W);
        let path = SharedString::from(format!("icons/appearance/{glyph}.svg"));
        let hover_id = ElementId::from((paint.id.clone(), SharedString::from(format!("icon-hover-{glyph}"))));
        let (muted, text_color, hover_icon) = (paint.muted, paint.text, paint.hover);
        let icon = tweened_at(paint.scope.clone(), hover_id, if hovered[i] { 1. } else { 0. }, HOVER_MS, move |hv| {
            svg().path(path.clone()).size(px(18.)).text_color(mix(mix(muted, hover_icon, hv), text_color, cover))
        });
        layer = layer.child(cell_slot(x).flex().items_center().justify_center().child(icon));
    }
    layer
}
