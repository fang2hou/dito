use std::sync::Arc;
use std::time::Duration;

use dito_core::config::{Appearance, HudPosition};
use gpui_kit::*;
use image::{Rgba, RgbaImage};

use super::input::{click, drag_to, hover, on_settings_window, press, press_key, redraw, release};
use crate::services;
use crate::settings;
use crate::state::AppState;

const KNOB_RADIUS: f32 = 14.;

/// Clicking the ends, dragging the thumb, the arrow keys and the mode switch, with the fill measured on both themes.
pub async fn hud_height_slider(st: &Arc<AppState>, cx: &mut AsyncApp) {
    for expected in [-2, 12] {
        on_settings_window(cx, |w, cx, view| {
            let bounds = view.read(cx).hud_height_bounds(cx);
            assert!(bounds.size.width > px(100.), "slider has laid out its hit region");
            let x = if expected == -2 { bounds.left() + px(1.) } else { bounds.right() - px(1.) };
            click(w, cx, point(x, bounds.center().y));
            redraw(w, cx);
        });
        cx.background_executor().timer(Duration::from_millis(100)).await;
        assert_eq!(st.cfg.get().general.hud_height_level(), expected, "track click selects the endpoint");
    }
    hud_height_fill(st, cx).await;
    // Let the animated thumb reach its end before grabbing it.
    cx.background_executor().timer(Duration::from_millis(350)).await;
    let bounds = slider_bounds(cx);
    let at = |fraction: f32| point(bounds.left() + bounds.size.width * fraction, bounds.center().y);
    on_settings_window(cx, |w, cx, _| {
        hover(w, cx, at(1.));
        press(w, cx, at(1.));
    });
    for fraction in [0.9, 0.65, 0.4, 0.15, 0.] {
        on_settings_window(cx, |w, cx, _| drag_to(w, cx, at(fraction)));
        cx.background_executor().timer(Duration::from_millis(20)).await;
    }
    on_settings_window(cx, |w, cx, _| release(w, cx, at(0.)));
    cx.background_executor().timer(Duration::from_millis(100)).await;
    assert_eq!(st.cfg.get().general.hud_height_level(), -2, "dragging the thumb reaches the minimum");
    on_settings_window(cx, |w, cx, _| press_key(w, cx, "right"));
    cx.background_executor().timer(Duration::from_millis(100)).await;
    assert_eq!(st.cfg.get().general.hud_height_level(), -1, "arrow keys move one level");
    services::save_config(st, |c| c.general.hud_position = HudPosition::Pointer);
    cx.background_executor().timer(Duration::from_millis(100)).await;
    assert_eq!(st.cfg.get().general.hud_height_level(), -1, "pointer mode preserves the height");
    services::save_config(st, |c| c.general.hud_position = HudPosition::Centered);
    cx.background_executor().timer(Duration::from_millis(100)).await;
    assert_eq!(st.cfg.get().general.hud_height_level(), -1, "returning restores the previous height");
    services::save_config(st, |c| c.general.hud_height_level = Some(0));
    println!("verified HUD height slider clicks, dragging, keyboard and mode persistence");
}

fn slider_bounds(cx: &mut AsyncApp) -> Bounds<Pixels> {
    cx.update(|cx| {
        let (_, view) = settings::handle(cx).expect("HUD height settings window");
        view.read(cx).hud_height_bounds(cx)
    })
}

/// The slider's fill must be invisible at the lowest level and end in a straight vertical edge at
/// every other one (the left end follows the track's own rounding). Measured from the rendered
/// window, cropped to the track, ignoring the knob's own columns.
async fn hud_height_fill(st: &Arc<AppState>, cx: &mut AsyncApp) {
    let original = st.cfg.get();
    for appearance in [Appearance::Light, Appearance::Dark] {
        services::save_config(st, |c| c.general.appearance = appearance);
        cx.background_executor().timer(Duration::from_millis(120)).await;
        let level = st.cfg.get().general.hud_height_level();
        let img = cx.update(|cx| {
            let (window, _view) = settings::handle(cx).expect("HUD height settings window");
            window.update(cx, |_, w, _cx| w.render_to_image().map_err(|e| e.to_string())).ok()
        });
        let Some(Ok(img)) = img else { continue };
        // The slider reports its indicator's bounds: inset by half the knob on each side, so
        // expand them back out to the full 220 × 28 track.
        let indicator = slider_bounds(cx);
        let track = Bounds::new(
            point(indicator.origin.x - px(KNOB_RADIUS), indicator.origin.y),
            size(indicator.size.width + px(2. * KNOB_RADIUS), indicator.size.height),
        );
        let (x0, y0) = (track.left().as_f32() as u32, track.top().as_f32() as u32);
        let (w, h) = (track.size.width.as_f32() as u32, track.size.height.as_f32() as u32);
        if img.width() < x0 + w || img.height() < y0 + h {
            continue;
        }
        let track = TrackShot { img: &img, x0, y0, w, h, dark: appearance == Appearance::Dark };
        if level <= -2 {
            track.assert_no_fill_right_of_knob(appearance, level);
        } else {
            track.assert_fill_reaches_knob_and_left_end_rounded(appearance, level);
        }
    }
    services::save_config(st, |c| {
        c.general.appearance = original.general.appearance;
    });
}

struct TrackShot<'a> {
    img: &'a RgbaImage,
    x0: u32,
    y0: u32,
    w: u32,
    h: u32,
    dark: bool,
}

impl TrackShot<'_> {
    fn is_fill(&self, p: Rgba<u8>) -> bool {
        if self.dark { p[0] > 190 && p[1] > 150 && p[2] < 190 } else { p[0] < 110 && p[1] < 110 && p[2] < 110 }
    }

    fn knob_center(&self, level: i8) -> f32 {
        KNOB_RADIUS + (self.w as f32 - 2. * KNOB_RADIUS) * (level as f32 + 2.) / 14.
    }

    fn top_row(&self) -> u32 {
        self.y0 + 3
    }

    fn bottom_row(&self) -> u32 {
        self.y0 + self.h - 4
    }

    fn rightmost_fill(&self, row: u32, from: u32) -> u32 {
        (from..self.x0 + self.w - 1).rev().find(|&x| self.is_fill(*self.img.get_pixel(x, row))).unwrap_or(from)
    }

    fn leftmost_fill(&self, row: u32, to: u32) -> u32 {
        (self.x0 + 1..to).find(|&x| self.is_fill(*self.img.get_pixel(x, row))).unwrap_or(to)
    }

    /// The straight edge sits on the thumb's centre line: at the rows near the track's top and bottom the thumb's
    /// circle has curved away, so the edge is in the open. At the lowest level no fill may exist anywhere right of
    /// the thumb.
    fn assert_no_fill_right_of_knob(&self, appearance: Appearance, level: i8) {
        let knob_right = self.x0 as f32 + self.knob_center(level) + KNOB_RADIUS;
        let from = knob_right as u32 + 3;
        for row in [self.y0 + self.h / 2, self.top_row(), self.bottom_row()] {
            assert_eq!(
                self.rightmost_fill(row, from),
                from,
                "{appearance:?} level {level}: fill visible at the lowest level"
            );
        }
    }

    /// The straight edge hides under the thumb; what the eye sees is that the fill still reaches the thumb's
    /// silhouette near the top and bottom rows. A rounded trailing edge would leave a gray gap there (the bug this
    /// guards against). The left end keeps the track's rounding: near the top and bottom rows the fill starts
    /// noticeably further right than on the track's centre row.
    fn assert_fill_reaches_knob_and_left_end_rounded(&self, appearance: Appearance, level: i8) {
        let center = self.knob_center(level);
        let silhouette_at_row = center - 9.;
        for row in [self.top_row(), self.bottom_row()] {
            let reach = self.rightmost_fill(row, self.x0 + 2) as f32 - self.x0 as f32;
            assert!(
                reach + 3. >= silhouette_at_row,
                "{appearance:?} level {level}: fill falls short of the thumb (reach {reach}, need {silhouette_at_row})"
            );
        }
        let clear_of_knob = (self.x0 as f32 + center - 15.) as u32;
        let inset_top = self.leftmost_fill(self.top_row(), clear_of_knob) as f32 - self.x0 as f32;
        let inset_mid = self.leftmost_fill(self.y0 + self.h / 2, clear_of_knob) as f32 - self.x0 as f32;
        assert!(
            inset_top - inset_mid >= 3.,
            "{appearance:?} level {level}: left end not rounded (top inset {inset_top}, mid {inset_mid})"
        );
    }
}

/// The health dot opens its card, moving into the card keeps it open, and leaving closes it.
pub fn health_hover_lifecycle(cx: &mut AsyncApp, dot: Point<Pixels>) {
    on_settings_window(cx, |w, cx, view| {
        let elsewhere = point(px(800.), px(700.));
        hover(w, cx, elsewhere);
        hover(w, cx, dot);
        assert!(view.read(cx).health_hover_probe().1, "dot opens the card");
        hover(w, cx, dot + point(px(140.), px(50.)));
        assert!(view.read(cx).health_hover_probe().1, "moving into card preserves hover");
        hover(w, cx, elsewhere);
        assert!(!view.read(cx).health_hover_probe().1, "leaving the card clears hover");
    });
}
