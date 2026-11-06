use std::path::Path;
use std::time::{Duration, Instant};

use dito_core::config::Appearance;
use gpui_kit::*;
use image::{Rgba, RgbaImage};

use crate::hud::{HUD_SCALE, HudView, WIN_H, WIN_PAD, WIN_W};
use crate::settings;

/// Frame rate of the frame-by-frame HUD captures.
const FRAME_RATE: f32 = 30.;

pub const BACKDROP_LIGHT: Rgba<u8> = Rgba([0xF7, 0xF5, 0xF0, 255]);
pub const BACKDROP_DARK: Rgba<u8> = Rgba([0x38, 0x35, 0x2F, 255]);
pub const BACKDROP_WHITE: Rgba<u8> = Rgba([255, 255, 255, 255]);

pub fn save(dir: &Path, name: &str, img: RgbaImage) {
    let path = dir.join(name);
    match img.save(&path) {
        Ok(()) => println!("wrote {}", path.display()),
        Err(e) => eprintln!("save {}: {e}", path.display()),
    }
}

pub fn theme_tag(appearance: Appearance) -> &'static str {
    if appearance == Appearance::Dark { "dark" } else { "light" }
}

pub fn render_settings_window(cx: &mut AsyncApp) -> Option<Result<RgbaImage, String>> {
    cx.update(|cx| {
        let (window, _) = settings::handle(cx)?;
        window.update(cx, |_, w, _| w.render_to_image().map_err(|e| e.to_string())).ok()
    })
}

pub async fn page_shot(cx: &mut AsyncApp, dir: &Path, name: &str) {
    cx.background_executor().timer(Duration::from_millis(700)).await;
    if let Some(Ok(img)) = render_settings_window(cx) {
        save(dir, name, img);
    }
}

pub fn render(cx: &mut AsyncApp, window: impl Into<AnyWindowHandle>) -> Option<RgbaImage> {
    let window = window.into();
    cx.update(|cx| window.update(cx, |_, w, _| w.render_to_image().ok()).ok().flatten())
}

/// Redraw from the current state before reading the image: occluded native windows may not commit frames.
pub fn render_fresh(cx: &mut AsyncApp, window: impl Into<AnyWindowHandle>) -> Option<RgbaImage> {
    let window = window.into();
    cx.update(|cx| {
        window
            .update(cx, |_, w, cx| {
                w.refresh();
                w.draw(cx).clear(cx);
                w.render_to_image().ok()
            })
            .ok()
            .flatten()
    })
}

pub fn close_window(cx: &mut AsyncApp, window: impl Into<AnyWindowHandle>) {
    let window = window.into();
    let _ = cx.update(|cx| window.update(cx, |_, w, _| w.remove_window()));
}

/// Paint a transparent window shot over an opaque backdrop.
pub fn over_backdrop(img: &RgbaImage, backdrop: Rgba<u8>) -> RgbaImage {
    let mut canvas = RgbaImage::from_pixel(img.width(), img.height(), backdrop);
    image::imageops::overlay(&mut canvas, img, 0, 0);
    canvas
}

/// Sleep until frame `frame` (zero-based) of a capture that started at `start` is due.
pub async fn wait_for_frame(cx: &mut AsyncApp, start: Instant, frame: u32) {
    let due = Duration::from_secs_f32((frame + 1) as f32 / FRAME_RATE);
    cx.background_executor().timer(due.saturating_sub(start.elapsed()).max(Duration::from_millis(2))).await;
}

pub fn open_hud_window(
    cx: &mut AsyncApp,
    build: impl FnOnce(&mut Context<HudView>) -> HudView + 'static,
) -> Option<WindowHandle<HudView>> {
    cx.update(|cx| {
        let window_size = size(px((WIN_W + 2. * WIN_PAD) * HUD_SCALE), px((WIN_H + 2. * WIN_PAD) * HUD_SCALE));
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, window_size, cx))),
            titlebar: None,
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        };
        cx.open_window(options, |_, cx| cx.new(build)).ok()
    })
}

pub fn resize_settings_window(cx: &mut App, width: f32, height: f32) {
    if let Some((window, _)) = settings::handle(cx) {
        let _ = window.update(cx, |_, w, _| w.resize(size(px(width), px(height))));
    }
}

pub fn refresh_settings_window(cx: &mut App) {
    if let Some((window, _)) = settings::handle(cx) {
        let _ = window.update(cx, |_, w, _| w.refresh());
    }
}
