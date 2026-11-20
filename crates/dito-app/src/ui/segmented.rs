use gpui_kit::prelude::*;
use gpui_kit::*;

use super::button::CONTROL_H;
use super::callbacks::IndexCb;
use super::hover::{HOVER_MS, hover_fill_if, hover_tracker, is_hovered};
use super::plate::{coverage, plate_state};
use super::primitives::{icon_path, mix, mix_rgb};
use super::scope::{ScopeHandle, motion_scope};
use super::tooltip::{tip_above, tip_label};
use super::tween::tweened_at;
use super::typography::{self, text_width};
use crate::theme::Palette;

const SEG_PAD: f32 = 3.;
const SEG_GAP: f32 = 2.;
const SEG_CELL_PAD: f32 = 16.;

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

pub(super) fn segment_hover_colors(p: &Palette) -> (Hsla, Hsla) {
    (p.segment_hover, p.segment_hover_label)
}

struct SegLayout {
    widths: Vec<f32>,
    h: f32,
    inset: f32,
    gap: f32,
}

impl SegLayout {
    fn new(options: &[SegOption], style: &SegStyle) -> Self {
        let widths = options
            .iter()
            .map(|o| {
                o.width.unwrap_or_else(|| match &o.content {
                    SegContent::Text(label) => {
                        let width = |weight| text_width(label, style.label, weight);
                        width(FontWeight::MEDIUM).max(width(FontWeight::NORMAL)).ceil() + 2. * style.cell_pad
                    }
                    SegContent::Icon(_) => style.label + 2. * style.cell_pad,
                })
            })
            .collect();
        Self { widths, h: style.h, inset: style.inset, gap: style.gap }
    }

    fn left(&self, i: usize) -> f32 {
        self.inset + self.widths[..i].iter().sum::<f32>() + self.gap * i as f32
    }

    fn total(&self) -> f32 {
        2. * self.inset + self.widths.iter().sum::<f32>() + self.gap * self.widths.len().saturating_sub(1) as f32
    }

    fn cell_h(&self) -> f32 {
        self.h - 2. * self.inset
    }

    /// With no selection the plate rests off-screen.
    fn plate_target(&self, selected: usize) -> (f32, f32) {
        if selected < self.widths.len() {
            (self.left(selected), self.left(selected) + self.widths[selected])
        } else {
            (-1000., -999.)
        }
    }

    fn slot(&self, i: usize) -> Div {
        div().absolute().top(px(self.inset)).left(px(self.left(i))).w(px(self.widths[i])).h(px(self.cell_h()))
    }
}

#[derive(Clone)]
pub enum SegContent {
    Text(SharedString),
    Icon(&'static str),
}

pub struct SegOption {
    pub content: SegContent,
    pub width: Option<f32>,
    pub tip: Option<SharedString>,
}

struct SegLabel {
    content: SegContent,
    hovered: bool,
}

struct LabelPaint {
    size: f32,
    muted: Hsla,
    text: Hsla,
    hover_label: Hsla,
    id: ElementId,
    scope: ScopeHandle,
}

/// Every cell has a fixed width (its label at the heavier weight) so choosing never resizes the control.
pub fn seg(
    id: impl Into<ElementId>,
    p: &Palette,
    options: &[SharedString],
    selected: usize,
    on_select: IndexCb,
) -> Stateful<Div> {
    let options: Vec<_> = options
        .iter()
        .cloned()
        .map(|text| SegOption { content: SegContent::Text(text), width: None, tip: None })
        .collect();
    seg_with(
        id,
        p,
        &options,
        selected,
        on_select,
        SegStyle { h: CONTROL_H, label: 13., cell_pad: SEG_CELL_PAD, inset: SEG_PAD, gap: SEG_GAP, hover: None },
    )
}

pub fn seg_with(
    id: impl Into<ElementId>,
    p: &Palette,
    options: &[SegOption],
    selected: usize,
    on_select: IndexCb,
    style: SegStyle,
) -> Stateful<Div> {
    let id: ElementId = id.into();
    let layout = SegLayout::new(options, &style);
    let (hover_fill, hover_label) = style.hover.unwrap_or_else(|| segment_hover_colors(p));
    let el = div().id(id.clone()).h(px(layout.h)).w(px(layout.total())).flex_none().relative().rounded_full().bg(p.seg);

    let motion = plate_state(ElementId::from((id.clone(), "plate")), layout.plate_target(selected));
    let (seg_on, inset, h) = (p.seg_on, layout.inset, layout.h);
    let mut el = el.child(motion.animate("plate", move |a, b, energy| {
        // RGB, because the HSL blend of a blue plate with white passes through green.
        let lit = mix_rgb(seg_on, hsla(0., 0., 1., 1.), 0.18 * energy);
        div().absolute().top(px(inset)).left(px(a)).w(px(b - a)).h(px(h - 2. * inset)).rounded_full().bg(lit)
    }));

    // Interactive cells: transparent, they only take the pointer and show the hover tint.
    let mut labels = Vec::with_capacity(options.len());
    for (i, option) in options.iter().enumerate() {
        let cell_id = ElementId::from((id.clone(), SharedString::from(format!("i{i}"))));
        let on_select = on_select.clone();
        let mut cell = layout
            .slot(i)
            .id(cell_id.clone())
            .rounded_full()
            .cursor_pointer()
            .on_click(move |_, w, cx| on_select(i, w, cx))
            .on_hover(hover_tracker(cell_id.clone()))
            .child(hover_fill_if(&cell_id, None, hover_fill, i != selected));
        if let Some(tip) = &option.tip {
            tip_label(cell_id.clone(), tip.clone());
            cell = cell
                .role(Role::RadioButton)
                .aria_label(tip.clone())
                .aria_selected(i == selected)
                .child(tip_above(p, &cell_id));
        }
        el = el.child(cell);
        labels.push(SegLabel { content: option.content.clone(), hovered: is_hovered(&cell_id) && i != selected });
    }

    // Labels sit above the cells. Their brightness is how much of the label the plate covers (so they light up
    // as the plate arrives, not when the click lands), or the hover level, whichever is higher.
    let paint = LabelPaint { size: style.label, muted: p.muted, text: p.text, hover_label, id, scope: motion_scope() };
    el.child(motion.animate("labels", move |a, b, _| labels_layer(&layout, &labels, &paint, a, b)))
}

fn labels_layer(layout: &SegLayout, labels: &[SegLabel], paint: &LabelPaint, a: f32, b: f32) -> Div {
    let mut layer = div().absolute().top(px(0.)).left(px(0.)).size_full();
    for (i, label) in labels.iter().enumerate() {
        let (x, w) = (layout.left(i), layout.widths[i]);
        let cover = coverage(a, b, x, x + w);
        let weight = if cover >= 0.5 { FontWeight::MEDIUM } else { FontWeight::NORMAL };
        let content = label.content.clone();
        let hover_id = ElementId::from((paint.id.clone(), SharedString::from(format!("hover-{i}"))));
        let (size, muted, text_c, hover_label) = (paint.size, paint.muted, paint.text, paint.hover_label);
        let hover_level =
            tweened_at(paint.scope.clone(), hover_id, if label.hovered { 1. } else { 0. }, HOVER_MS, move |hv| {
                let ink = mix(mix(muted, hover_label, hv), text_c, cover);
                match &content {
                    SegContent::Text(label) => typography::label(label, size, weight, ink).into_any_element(),
                    SegContent::Icon(path) => icon_path(path, size, ink).into_any_element(),
                }
            });
        layer = layer.child(
            div()
                .absolute()
                .top(px(layout.inset))
                .left(px(x))
                .w(px(w))
                .h(px(layout.cell_h()))
                .flex()
                .items_center()
                .justify_center()
                .child(hover_level),
        );
    }
    layer
}
