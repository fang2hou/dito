use std::path::Path;
use std::time::{Duration, Instant};

use dito_core::config::Appearance;
use gpui_kit::*;
use image::{Rgba, RgbaImage};

use super::input::redraw;
use crate::hud::{HUD_SCALE, HudView, WIN_H, WIN_PAD, WIN_W};
use crate::settings;
use crate::ui::{self, Root};

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

/// Shots run in windows that are never shown, and a window that is not on screen does not commit frames by itself.
///
/// Goes through `update_window` on an untyped handle: a typed `WindowHandle::update` leases the window's root view,
/// and drawing needs it.
pub fn paint_to_image(window: AnyWindowHandle, cx: &mut App) -> Result<RgbaImage, String> {
    cx.update_window(window, |_, w, cx| {
        redraw(w, cx);
        w.render_to_image().map_err(|e| e.to_string())
    })
    .map_err(|e| e.to_string())?
}

pub fn render_settings_window(cx: &mut AsyncApp) -> Option<Result<RgbaImage, String>> {
    cx.update(|cx| {
        let (window, _) = settings::handle(cx)?;
        Some(paint_to_image(window.into(), cx))
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
    cx.update(|cx| paint_to_image(window, cx).ok())
}

pub fn close_window(cx: &mut AsyncApp, window: impl Into<AnyWindowHandle>) {
    let window = window.into();
    let _ = cx.update(|cx| window.update(cx, |_, w, _| w.remove_window()));
}

pub fn over_backdrop(img: &RgbaImage, backdrop: Rgba<u8>) -> RgbaImage {
    let mut canvas = RgbaImage::from_pixel(img.width(), img.height(), backdrop);
    image::imageops::overlay(&mut canvas, img, 0, 0);
    canvas
}

/// `frame` is zero-based.
pub async fn wait_for_frame(cx: &mut AsyncApp, start: Instant, frame: u32) {
    let due = Duration::from_secs_f32((frame + 1) as f32 / FRAME_RATE);
    cx.background_executor().timer(due.saturating_sub(start.elapsed()).max(Duration::from_millis(2))).await;
}

pub fn open_hud_window(
    cx: &mut AsyncApp,
    build: impl FnOnce(&mut Context<HudView>) -> HudView + 'static,
) -> Option<WindowHandle<Root>> {
    cx.update(|cx| {
        let window_size = size(px((WIN_W + 2. * WIN_PAD) * HUD_SCALE), px((WIN_H + 2. * WIN_PAD) * HUD_SCALE));
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, window_size, cx))),
            titlebar: None,
            window_background: WindowBackgroundAppearance::Transparent,
            focus: false,
            show: false,
            ..Default::default()
        };
        cx.open_window(options, |window, cx| {
            let view = cx.new(build);
            cx.new(|cx| Root::new(view, window, cx).bg(ui::transparent()))
        })
        .ok()
    })
}

/// The HUD view of a window from [`open_hud_window`]; the window root wraps it.
pub fn hud_view(cx: &mut AsyncApp, window: WindowHandle<Root>) -> Option<Entity<HudView>> {
    cx.update(|cx| window.update(cx, |root, _, _| root.view().clone().downcast::<HudView>().ok()).ok().flatten())
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
