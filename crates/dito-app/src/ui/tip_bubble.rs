use gpui_kit::prelude::*;
use gpui_kit::*;

use super::primitives::transparent;
use super::typography::{family_for, text};
use crate::theme::Palette;

/// The bubble's fixed height: the label's line box is taller than its font size, so the height is stated,
/// not measured. The tooltip's anchor offset depends on it exactly.
pub const TIP_BUBBLE_H: f32 = 30.;

const MIN_SCALE: f32 = 0.2;

#[derive(Clone)]
struct BubblePaint {
    label: SharedString,
    scale: f32,
    alpha: f32,
    bg: Hsla,
    fg: Hsla,
    shadow: Hsla,
    border: Hsla,
}

impl BubblePaint {
    fn paint(&self, region: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let k = self.scale.clamp(MIN_SCALE, 1.);
        let w = region.size.width * k;
        let h = region.size.height * k;
        let center_x = region.origin.x + region.size.width * 0.5;
        let bottom = region.origin.y + region.size.height;
        let bounds = Bounds::new(point(center_x - w * 0.5, bottom - h), size(w, h));
        self.paint_chrome(bounds, k, window);
        self.paint_label(center_x, h, bottom, k, window, cx);
    }

    fn paint_chrome(&self, bounds: Bounds<Pixels>, k: f32, window: &mut Window) {
        let radii = Corners::all(px(8. * k));
        window.paint_drop_shadows(
            bounds,
            radii,
            &[BoxShadow {
                color: self.shadow.opacity(self.alpha),
                offset: point(px(0.), px(4. * k)),
                blur_radius: px(12. * k),
                spread_radius: px(0.),
                inset: false,
            }],
        );
        window.paint_quad(quad(
            bounds,
            radii,
            self.bg.opacity(self.alpha),
            Edges::all(px(1.)),
            self.border.opacity(self.alpha),
            BorderStyle::Solid,
        ));
    }

    fn paint_label(&self, center_x: Pixels, h: Pixels, bottom: Pixels, k: f32, window: &mut Window, cx: &mut App) {
        let font_size = px(12. * k);
        let run = TextRun {
            len: self.label.len(),
            font: Font { family: family_for(&self.label).into(), weight: FontWeight::MEDIUM, ..Default::default() },
            color: self.fg.opacity(self.alpha),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line = window.text_system().shape_line(self.label.clone(), font_size, &[run], None);
        let text_top = origin_y_center(h, font_size, bottom);
        line.paint(point(center_x - line.width() * 0.5, text_top), font_size, TextAlign::Left, None, window, cx).ok();
    }
}

#[derive(Clone, Copy)]
pub(super) struct TipColors {
    bg: Hsla,
    fg: Hsla,
    shadow: Hsla,
    border: Hsla,
}

impl TipColors {
    pub(super) fn new(p: &Palette, bg: Hsla, fg: Hsla) -> Self {
        Self { bg, fg, shadow: p.shadow, border: p.hud_border }
    }
}

/// Only overflowing text needs edge strips; settled text uses one paint. The strips reuse the glyph atlas and the
/// line-layout cache.
pub(super) fn paint_shared<'a>(
    colors: TipColors,
    labels: impl Iterator<Item = (&'a SharedString, f32)>,
    region: Bounds<Pixels>,
    scale: f32,
    alpha: f32,
    window: &mut Window,
    cx: &mut App,
) {
    let TipColors { bg, fg, shadow, border } = colors;
    let k = scale.clamp(MIN_SCALE, 1.);
    let center = region.center().x;
    let bottom = region.bottom_left().y;
    let bounds = Bounds::new(
        point(center - region.size.width * k * 0.5, bottom - region.size.height * k),
        size(region.size.width * k, region.size.height * k),
    );
    BubblePaint { label: SharedString::default(), scale: k, alpha, bg, fg, shadow, border }
        .paint_chrome(bounds, k, window);
    let inset = px(10.5 * k);
    let viewport = Bounds::new(
        point(bounds.origin.x + inset, bounds.origin.y),
        size((bounds.size.width - inset * 2.).max(px(1.)), bounds.size.height),
    );
    for (label, weight) in labels.filter(|(_, weight)| *weight > 0.005) {
        let paint = BubblePaint { label: label.clone(), scale: k, alpha: alpha * weight, bg, fg, shadow, border };
        let line_width = super::text_width(label, 12. * k, FontWeight::MEDIUM);
        let overflow = (line_width - f32::from(viewport.size.width)).max(0.);
        if overflow <= 0.1 {
            window.with_content_mask(Some(ContentMask { bounds: viewport }), |window| {
                paint.paint_label(center, bounds.size.height, bottom, k, window, cx);
            });
            continue;
        }
        let fade = px((6. * k).min(f32::from(viewport.size.width) * 0.5));
        let strength = (overflow / (12. * k)).min(1.);
        // Non-overlapping half-point strips implement a text-only alpha mask without offscreen textures.
        for step in 0..12 {
            let part = fade / 12.;
            let opacity = 1. - strength * (1. - (step as f32 + 0.5) / 12.);
            for x in [viewport.origin.x + part * step as f32, viewport.right() - part * (step + 1) as f32] {
                let clip = Bounds::new(point(x, viewport.origin.y), size(part, viewport.size.height));
                let strip = BubblePaint { alpha: paint.alpha * opacity, ..paint.clone() };
                window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
                    strip.paint_label(center, bounds.size.height, bottom, k, window, cx);
                });
            }
        }
        let center_clip = Bounds::new(
            point(viewport.origin.x + fade, viewport.origin.y),
            size((viewport.size.width - fade * 2.).max(px(0.)), viewport.size.height),
        );
        window.with_content_mask(Some(ContentMask { bounds: center_clip }), |window| {
            paint.paint_label(center, bounds.size.height, bottom, k, window, cx);
        });
    }
}

/// Layout always reserves the final size (an invisible line of the label at full size), and paint scales the whole
/// bubble (chrome, shadow and text) about its bottom-centre by `k`. Scaling through layout (growing font sizes)
/// re-flows the text every frame and makes it visibly slide sideways; this way nothing re-flows and the pop-in reads
/// as one object growing out of the trigger. `a` is the fade alpha.
pub(super) fn tip_bubble(p: &Palette, label: &SharedString, k: f32, a: f32, bg: Hsla, fg: Hsla) -> Div {
    let paint =
        BubblePaint { label: label.clone(), scale: k, alpha: a, bg, fg, shadow: p.shadow, border: p.hud_border };
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
            canvas(move |b, _, _| b, move |b, _, window, cx| paint.paint(b, window, cx))
                .absolute()
                .left_0()
                .top_0()
                .right_0()
                .bottom_0(),
        )
}

fn origin_y_center(h: Pixels, font_size: Pixels, bottom: Pixels) -> Pixels {
    bottom - h * 0.5 - font_size * 0.5
}
