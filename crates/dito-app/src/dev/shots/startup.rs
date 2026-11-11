use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use dito_core::config::{Appearance, ConfigStore};
use dito_core::i18n::{Lang, set_system_language, system_language};
use gpui_kit::*;

use super::capture::{BACKDROP_WHITE, close_window, over_backdrop, render, save, theme_tag};
use crate::opts::Opts;
use crate::startup_failure::{Failure, Launch, open};
use crate::state::AppState;

/// Compared with `design:settings/startup-failure`.
fn samples() -> Vec<(&'static str, Failure)> {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let data = home.join("Library/Application Support/com.fang2hou.Dito");
    vec![
        (
            "unreadable",
            Failure::UnreadableSettings {
                path: data.join("config.toml"),
                position: Some((12, 5)),
                message: "invalid type: string \"fast\", expected a boolean".into(),
            },
        ),
        ("newer", Failure::NewerData { path: data.join("dito.db"), found: 3, supported: 2 }),
        (
            "unreadable-file",
            Failure::UnreadableSettings {
                path: data.join("config.toml"),
                position: None,
                message: undecodable_file_reason(),
            },
        ),
    ]
}

/// Taken from the error of a real file, so the shot shows what the app would.
fn undecodable_file_reason() -> String {
    let dir = std::env::temp_dir().join(format!("dito-shot-undecodable-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("shot data folder");
    let file = dir.join("config.toml");
    std::fs::write(&file, b"onboarded = true\n# caf\xe9\n").expect("shot settings file");
    let error = ConfigStore::open(&file).err().expect("a file that is not UTF-8 does not load");
    let opts = Opts { data_dir: Some(dir.clone()), ..Opts::default() };
    let failure = Failure::from_error(&anyhow::Error::from(error), &opts);
    let _ = std::fs::remove_dir_all(&dir);
    let Some(Failure::UnreadableSettings { message, .. }) = failure else { panic!("{failure:?}") };
    message
}

/// English and Chinese, whose longer lines wrap, are shot in the light appearance only.
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
