//! Frosts the settings' wordmark backdrop behind cards without blurring foreground controls.
//!
//! GPUI has no backdrop-filter primitive, so the blurred raster is cached and composited inside each card. Scrolling
//! only changes the texture crop; it never re-rasterizes text or blurs a whole frame.
use std::{cell::RefCell, fmt::Write, sync::Arc};

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::Palette;

const PAD: f32 = 32.;
const FONT_SIZE: f32 = 150.;
const TOP: f32 = -34.;
const RIGHT: f32 = -18.;
const WEIGHT: FontWeight = FontWeight::SEMIBOLD;

/// Shared by the sharp wordmark and its frosted copy, so the two line up.
pub fn wordmark(title: &'static str, p: Palette) -> Div {
    div().absolute().top(px(TOP)).right(px(RIGHT)).child(
        super::typography::text(title, FONT_SIZE, WEIGHT, p.accent)
            .line_height(relative(1.))
            .whitespace_nowrap()
            .opacity(if p.dark { 0.10 } else { 0.09 }),
    )
}

struct Backdrop {
    window: WindowId,
    title: &'static str,
    dark: bool,
    image: Arc<RenderImage>,
    bounds: Bounds<Pixels>,
}

thread_local! {
    // Only the currently painted settings surface is retained, not every visited page/window.
    static BACKDROP: RefCell<Option<Backdrop>> = const { RefCell::new(None) };
}

fn color(c: Hsla) -> String {
    let c = c.to_rgb();
    format!("#{:02x}{:02x}{:02x}", (c.r * 255.).round() as u8, (c.g * 255.).round() as u8, (c.b * 255.).round() as u8)
}

/// SVG path geometry uses the same bundled face and native-shaped glyph IDs/positions as the
/// sharp wordmark. SVG's own text/font resolver must never independently pick a substitute face.
#[derive(Default)]
struct Outline(String);

impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        let _ = write!(self.0, "M{x} {y}");
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let _ = write!(self.0, "L{x} {y}");
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let _ = write!(self.0, "Q{x1} {y1} {x} {y}");
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let _ = write!(self.0, "C{x1} {y1} {x2} {y2} {x} {y}");
    }
    fn close(&mut self) {
        self.0.push('Z');
    }
}

/// Mount before the wordmark and page, so card paint sees the current window's backdrop.
pub fn prepare(title: &'static str, p: Palette) -> AnyElement {
    canvas(
        move |bounds, window, cx| {
            let id = window.window_handle().window_id();
            let mut face = font("Outfit");
            face.weight = WEIGHT;
            let run = TextRun { len: title.len(), font: face, color: p.accent,
                background_color: None, underline: None, strikethrough: None };
            let line = window.text_system().shape_line(title.into(), px(FONT_SIZE), &[run], None);
            let width = f32::from(line.width) + PAD * 2.;
            let height = FONT_SIZE + PAD * 2.;
            let image_bounds = Bounds::new(
                point(bounds.origin.x + bounds.size.width + px(-RIGHT - width + PAD), bounds.origin.y + px(TOP - PAD)),
                size(px(width), px(height)),
            );
            BACKDROP.with(|cache| {
                let mut cache = cache.borrow_mut();
                if let Some(old) = cache.as_mut()
                    && old.title == title && old.dark == p.dark
                {
                    old.window = id;
                    old.bounds = image_bounds;
                    return;
                }
                let baseline = PAD + (FONT_SIZE - f32::from(line.ascent) - f32::from(line.descent)) / 2.
                    + f32::from(line.ascent);
                let face = ttf_parser::Face::parse(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fonts/Outfit-SemiBold.ttf")), 0)
                    .expect("bundled Outfit SemiBold font");
                let scale = FONT_SIZE / face.units_per_em() as f32;
                let mut outlines = String::new();
                for run in &line.runs {
                    for glyph in &run.glyphs {
                        let mut path = Outline::default();
                        if let Ok(id) = u16::try_from(glyph.id.0)
                            && face.outline_glyph(ttf_parser::GlyphId(id), &mut path).is_some()
                        {
                            let x = PAD + f32::from(glyph.position.x);
                            let y = baseline + f32::from(glyph.position.y);
                            let _ = write!(outlines, r#"<path transform="translate({x} {y}) scale({scale} {})" d="{}"/>"#, -scale, path.0);
                        }
                    }
                }
                let opacity = if p.dark { 0.09 } else { 0.08 };
                let svg = format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}">
<defs><filter id="frost" x="-30%" y="-50%" width="160%" height="200%" color-interpolation-filters="sRGB"><feGaussianBlur stdDeviation="8"/></filter></defs>
<g fill="{}" opacity="{opacity}" filter="url(#frost)">{outlines}</g></svg>"##,
                    color(p.accent));
                // Blur is low frequency: a 1× cached texture remains smooth on Retina displays.
                match cx.svg_renderer().render_single_frame(svg.as_bytes(), 0.5) {
                    Ok(image) => *cache = Some(Backdrop { window: id, title, dark: p.dark, image, bounds: image_bounds }),
                    Err(error) => { *cache = None; tracing::warn!(%error, "could not render frosted wordmark"); }
                }
            });
        },
        |_, _, _, _| (),
    ).absolute().top_0().left_0().size_full().into_any_element()
}

pub fn surface() -> AnyElement {
    canvas(
        |_, _, _| (),
        |bounds, _, window, _| {
            BACKDROP.with(|cache| {
                let cache = cache.borrow();
                if let Some(backdrop) = cache.as_ref()
                    && backdrop.window == window.window_handle().window_id()
                {
                    // The card hides the sharp backdrop; composite only its blurred sample here.
                    // The transparent texture edges avoid seams as the sampling region scrolls.
                    window
                        .paint_image(bounds, backdrop.bounds, Corners::all(px(15.)), backdrop.image.clone(), 0, false)
                        .ok();
                }
            })
        },
    )
    .absolute()
    .inset(px(1.))
    .into_any_element()
}
