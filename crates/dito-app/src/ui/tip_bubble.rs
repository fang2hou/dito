use gpui_kit::prelude::*;
use gpui_kit::*;

use super::primitives::transparent;
use super::typography::{family_for, text};
use crate::theme::Palette;

/// The bubble's fixed height: the label's line box is taller than its font size, so the height is stated,
/// not measured. The tooltip's anchor offset depends on it exactly.
pub const TIP_BUBBLE_H: f32 = 30.;

const MIN_SCALE: f32 = 0.2;

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

/// A tooltip bubble drawn as one rigid unit. Layout always reserves the final size (an invisible
/// line of the label at full size), and paint scales the whole thing — chrome, shadow and text
/// together — about its bottom-centre by `k`. Scaling through layout (growing font sizes)
/// re-flows the text every frame and makes it visibly slide sideways; this way nothing re-flows
/// and the pop-in reads as one object growing out of the trigger. `a` is the fade alpha.
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

/// Top of a vertically centred `font_size`-tall line box inside a bubble of height `h` whose
/// bottom sits at `bottom`.
fn origin_y_center(h: Pixels, font_size: Pixels, bottom: Pixels) -> Pixels {
    bottom - h * 0.5 - font_size * 0.5
}
