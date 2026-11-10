use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use dito_core::config::Appearance;
use dito_core::i18n::{Lang, set_system_language, system_language};
use gpui_kit::*;

use super::capture::{BACKDROP_WHITE, close_window, over_backdrop, render, save, theme_tag};
use crate::startup_failure::{Failure, Launch, open};
use crate::state::AppState;

/// The two failures of design 20.10 as a person with the default data folder would see them.
fn samples() -> [(&'static str, Failure); 2] {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let data = home.join("Library/Application Support/com.fang2hou.Dito");
    [
        (
            "unreadable",
            Failure::UnreadableSettings {
                path: data.join("config.toml"),
                position: Some((12, 5)),
                message: "invalid type: string \"fast\", expected a boolean".into(),
            },
        ),
        ("newer", Failure::NewerData { path: data.join("dito.db"), found: 3, supported: 2 }),
    ]
}

/// The startup failure window (`startup-failure-<failure>-<theme>[-<lang>].png`): both failures in both appearances
/// in Japanese, and in English and Chinese, whose longer lines wrap, in the light one.
pub async fn startup_failure_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let launch = Launch { opts: st.opts.clone(), rt: st.rt.clone() };
    let original = system_language();
    let languages = [
        (Lang::Ja, vec![Appearance::Light, Appearance::Dark]),
        (Lang::En, vec![Appearance::Light]),
        (Lang::Zh, vec![Appearance::Light]),
    ];
    for (lang, appearances) in languages {
        set_system_language(lang);
        for appearance in appearances {
            for (name, failure) in samples() {
                let Some(window) = cx.update(|cx| open(failure, appearance, launch.clone(), false, cx)) else {
                    continue;
                };
                cx.background_executor().timer(Duration::from_millis(300)).await;
                if let Some(img) = render(cx, window) {
                    let suffix = if lang == Lang::Ja { String::new() } else { format!("-{}", lang.code()) };
                    let file = format!("startup-failure-{name}-{}{suffix}.png", theme_tag(appearance));
                    save(dir, &file, over_backdrop(&img, BACKDROP_WHITE));
                }
                close_window(cx, window);
            }
        }
    }
    set_system_language(original);
}
