//! Real Settings and Welcome frames for the intro film. Opt-in: `DITO_SHOTS_ONLY=introfilm`, `DITO_FILM_LANG` (ja, en, zh, ko).
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::config::{Appearance, DisplayLanguage, Source};
use gpui_kit::*;

use super::capture::{close_window, render, render_settings_window, resize_settings_window, save};
use super::input::{click, hover, on_settings_window};
use crate::onboarding::{self, demo};
use crate::services;
use crate::settings::{self, Page};
use crate::state::AppState;

const FRAME: Duration = Duration::from_millis(33);

fn env_f(name: &str, default: f32) -> f32 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

async fn grab(cx: &mut AsyncApp, dir: &Path, scene: &str, start: Instant, for_ms: u64) {
    let until = Instant::now() + Duration::from_millis(for_ms);
    while Instant::now() < until {
        if let Some(Ok(img)) = render_settings_window(cx) {
            save_quiet(dir, &format!("{scene}-{:06}.png", start.elapsed().as_millis()), img);
        }
        cx.background_executor().timer(FRAME).await;
    }
}

fn save_quiet(dir: &Path, name: &str, img: image::RgbaImage) {
    let _ = img.save(dir.join(name));
}

pub async fn run(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let code = std::env::var("DITO_FILM_LANG").unwrap_or_else(|_| "ja".into());
    let lang = match code.as_str() {
        "en" => DisplayLanguage::En,
        "zh" => DisplayLanguage::Zh,
        "ko" => DisplayLanguage::Ko,
        _ => DisplayLanguage::Ja,
    };
    let scenes = std::env::var("DITO_FILM_SCENES").unwrap_or_else(|_| "welcome,wordbook,models,history".into());
    let wants = |s: &str| scenes.split(',').any(|x| x == s);
    services::save_config(st, |c| {
        c.general.display_language = lang;
        c.general.appearance = Appearance::Dark;
    });
    if wants("welcome") {
        demo::freeze(Some(demo::Frozen::Still));
        if let Some((window, view)) = cx.update(|cx| onboarding::open_with_view(st.clone(), cx)) {
            view.update(cx, |o, cx| o.show_step(0, cx));
            cx.background_executor().timer(Duration::from_millis(900)).await;
            if let Some(img) = render(cx, window) {
                save(dir, "welcome-dark.png", img);
            }
            close_window(cx, window);
        }
        demo::freeze(None);
    }
    services::save_config(st, |c| c.general.appearance = Appearance::Light);
    if wants("wordbook") {
        cx.update(|cx| {
            settings::open(st.clone(), Page::Wordbook, cx);
            resize_settings_window(cx, 1120., 860.);
        });
        cx.background_executor().timer(Duration::from_millis(900)).await;
        let start = Instant::now();
        grab(cx, dir, "wordbook", start, 700).await;
        on_settings_window(cx, |w, cx, view| view.update(cx, |s, cx| s.probe_wordbook("add", "Dito", w, cx)));
        grab(cx, dir, "wordbook", start, 2200).await;
    }
    if wants("history") {
        cx.update(|cx| {
            settings::open(st.clone(), Page::History, cx);
            resize_settings_window(cx, 1120., 860.);
        });
        cx.background_executor().timer(Duration::from_millis(900)).await;
        let start = Instant::now();
        grab(cx, dir, "history", start, 700).await;
        let (hx, hy) = (env_f("HIST_X", 600.), env_f("HIST_Y", 330.));
        on_settings_window(cx, |w, cx, _| hover(w, cx, point(px(hx), px(hy))));
        grab(cx, dir, "history", start, 900).await;
        on_settings_window(cx, |w, cx, _| click(w, cx, point(px(hx), px(hy))));
        grab(cx, dir, "history", start, 1600).await;
        let (px_, py_) = (env_f("COPY_X", 976.), env_f("COPY_Y", 251.));
        on_settings_window(cx, |w, cx, _| click(w, cx, point(px(px_), px(py_))));
        grab(cx, dir, "history", start, 1600).await;
    }
    if wants("models") {
        services::save_config(st, |c| {
            c.asr.source = Source::Local;
            c.text_model.source = Source::Local;
        });
        cx.update(|cx| {
            settings::open(st.clone(), Page::Models, cx);
            resize_settings_window(cx, 1120., 860.);
        });
        cx.background_executor().timer(Duration::from_millis(900)).await;
        let start = Instant::now();
        grab(cx, dir, "models", start, 800).await;
        let (dx, dy) = (env_f("DL_X", 996.), env_f("DL_Y", 315.));
        on_settings_window(cx, |w, cx, _| click(w, cx, point(px(dx), px(dy))));
        grab(cx, dir, "models", start, 2200).await;
        let (cx_seg, cy_seg) = (env_f("SEG_X", 992.), env_f("SEG_Y", 220.));
        on_settings_window(cx, |w, cx, _| click(w, cx, point(px(cx_seg), px(cy_seg))));
        grab(cx, dir, "models", start, 1500).await;
        let (tx, ty) = (env_f("TILE_X", 650.), env_f("TILE_Y", 308.));
        on_settings_window(cx, |w, cx, _| click(w, cx, point(px(tx), px(ty))));
        grab(cx, dir, "models", start, 1500).await;
    }
}
