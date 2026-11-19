use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::*;

use super::input::redraw;
use super::{demo, hud, local_api, onboarding, pages, readme_film, startup, text_model_form, tile_row, wordbook};
use crate::settings::{self, Page};
use crate::state::{AppGlobal, AppState};

const ONLY_VAR: &str = "DITO_SHOTS_ONLY";

type RunGroup = for<'a> fn(&'a Arc<AppState>, &'a Path, &'a mut AsyncApp) -> Pin<Box<dyn Future<Output = ()> + 'a>>;

/// `DITO_SHOTS_ONLY=hud,demo` runs the named groups. Without it every default group runs and the opt-in ones stay
/// off.
struct ShotGroup {
    name: &'static str,
    default: bool,
    run: RunGroup,
}

const fn default_group(name: &'static str, run: RunGroup) -> ShotGroup {
    ShotGroup { name, default: true, run }
}

const fn opt_in(name: &'static str, run: RunGroup) -> ShotGroup {
    ShotGroup { name, default: false, run }
}

/// Every group, in the order they run. Groups share the app state and its config, so the order is part of what a
/// run renders.
const GROUPS: &[ShotGroup] = &[
    default_group("pages", |st, dir, cx| Box::pin(pages::page_shots(st, dir, cx))),
    opt_in("navrepro", |_, _, cx| Box::pin(pages::nav_repro(cx))),
    opt_in("promptmotion", |st, dir, cx| Box::pin(pages::prompt_motion_shots(st, dir, cx))),
    opt_in("promptediting", |st, dir, cx| Box::pin(pages::prompt_editing_shots(st, dir, cx))),
    opt_in("prompts", |st, dir, cx| Box::pin(pages::prompt_shots(st, dir, cx))),
    default_group("tools", |st, dir, cx| Box::pin(pages::tools_shots(st, dir, cx))),
    opt_in("localapi", |st, dir, cx| Box::pin(local_api::local_api_shots(st, dir, cx))),
    opt_in("chat", |st, dir, cx| Box::pin(super::chat::run(st, dir, cx))),
    opt_in("transcriptfilm", |st, dir, cx| Box::pin(super::transcript_film::run(st, dir, cx))),
    opt_in("wordbook", |st, dir, cx| Box::pin(wordbook::wordbook_failure_shots(st, dir, cx))),
    opt_in("tilerow", |_, dir, cx| Box::pin(tile_row::tile_row_shots(dir, cx))),
    opt_in("status", |st, dir, cx| Box::pin(pages::status_shots(st, dir, cx))),
    opt_in("statuslog", |st, dir, cx| Box::pin(pages::status_log_shots(st, dir, cx))),
    opt_in("inlineasr", |st, dir, cx| Box::pin(super::inline_asr::run(st, dir, cx))),
    opt_in("cloudspeechtiles", |st, dir, cx| Box::pin(super::cloud_speech_tiles::run(st, dir, cx))),
    opt_in("models", |st, dir, cx| Box::pin(pages::model_shots(st, dir, cx))),
    opt_in("chatmodel", |st, dir, cx| Box::pin(pages::chat_model_shots(st, dir, cx))),
    opt_in("chatmodelstatus", |st, dir, cx| Box::pin(pages::chat_model_status_shots(st, dir, cx))),
    opt_in("compare", |st, dir, cx| Box::pin(super::compare::compare_shots(st, dir, cx))),
    opt_in("textmodelform", |st, dir, cx| Box::pin(text_model_form::text_model_form_shots(st, dir, cx))),
    default_group("extra", |st, dir, cx| Box::pin(pages::extra_shots(st, dir, cx))),
    default_group("onboarding", |st, dir, cx| Box::pin(onboarding::onboarding_shots(st, dir, cx))),
    default_group("startup", |st, dir, cx| Box::pin(startup::startup_failure_shots(st, dir, cx))),
    default_group("hud", |st, dir, cx| Box::pin(hud::hud_shots(st, dir, cx))),
    default_group("demo", |_, dir, cx| Box::pin(demo::demo_shots(dir, cx))),
    opt_in("readmefilm", |st, dir, cx| Box::pin(readme_film::run(st, dir, cx))),
    opt_in("hudinteractions", |st, dir, cx| Box::pin(hud::hud_interaction_shots(st, dir, cx))),
    opt_in("hudstylepreview", |st, dir, cx| Box::pin(pages::hud_style_preview_shots(st, dir, cx))),
    opt_in("hudoutcome", |st, dir, cx| Box::pin(hud::hud_outcome_shots(st, dir, cx))),
    opt_in("hudwave", |st, dir, cx| Box::pin(hud::hud_wave_shots(st, dir, cx))),
    opt_in("hudmotion", |st, dir, cx| Box::pin(hud::hud_motion_shots(st, dir, cx))),
    opt_in("settingshud", |st, dir, cx| Box::pin(pages::hud_settings_shots(st, dir, cx))),
    opt_in("hudheight", |st, dir, cx| Box::pin(pages::hud_height_shots(st, dir, cx))),
    opt_in("hudtooltip", |st, dir, cx| Box::pin(hud::hud_tooltip_shots(st, dir, cx))),
    opt_in("hudlive", |st, dir, cx| Box::pin(hud::hud_live_shots(st, dir, cx))),
    opt_in("hudcommand", |st, dir, cx| Box::pin(hud::hud_command_live_shots(st, dir, cx))),
    opt_in("hudchord", |st, dir, cx| Box::pin(hud::hud_chord_live_shots(st, dir, cx))),
    opt_in("demolive", |_, dir, cx| Box::pin(demo::demo_live_shots(dir, cx))),
    opt_in("appearance", |st, dir, cx| Box::pin(pages::appearance_hover_shots(st, dir, cx))),
    opt_in("health", |st, dir, cx| Box::pin(pages::health_shots(st, dir, cx))),
    default_group("min", |st, dir, cx| Box::pin(pages::min_size_shots(st, dir, cx))),
];

/// The groups to run, in registry order. `only` is the value of `DITO_SHOTS_ONLY`: names separated by commas, where
/// spaces around a name and empty entries are ignored. A value that names no group, or a name that is not in the
/// registry, is an error: either would otherwise render nothing and look like a passing run.
fn select(only: Option<&str>) -> Result<Vec<&'static ShotGroup>, String> {
    let Some(only) = only else {
        return Ok(GROUPS.iter().filter(|group| group.default).collect());
    };
    let names: Vec<&str> = only.split(',').map(str::trim).filter(|name| !name.is_empty()).collect();
    if names.is_empty() {
        return Err("is set but names no shot group".to_owned());
    }
    let unknown: Vec<&str> =
        names.iter().copied().filter(|name| GROUPS.iter().all(|group| group.name != *name)).collect();
    if !unknown.is_empty() {
        return Err(format!("unknown shot group: {}", unknown.join(", ")));
    }
    Ok(GROUPS.iter().filter(|group| names.contains(&group.name)).collect())
}

fn requested() -> Option<String> {
    match std::env::var(ONLY_VAR) {
        Ok(only) => Some(only),
        Err(std::env::VarError::NotPresent) => None,
        // Reads as an unknown name instead of as an unset variable.
        Err(std::env::VarError::NotUnicode(raw)) => Some(raw.to_string_lossy().into_owned()),
    }
}

fn names(default: bool) -> String {
    GROUPS.iter().filter(|group| group.default == default).map(|group| group.name).collect::<Vec<_>>().join(", ")
}

fn selected_or_exit() -> Vec<&'static ShotGroup> {
    select(requested().as_deref()).unwrap_or_else(|problem| {
        eprintln!("{ONLY_VAR}: {problem}");
        eprintln!("default groups: {}", names(true));
        eprintln!("opt-in groups: {}", names(false));
        std::process::exit(1);
    })
}

pub fn run(dir: PathBuf, cx: &mut App) {
    let groups = selected_or_exit();
    std::fs::create_dir_all(&dir).expect("screenshot dir");
    let st = cx.global::<AppGlobal>().0.clone();
    settings::open(st.clone(), Page::General, cx);
    pump_frames(cx);
    cx.spawn(async move |cx| {
        for group in groups {
            (group.run)(&st, &dir, cx).await;
        }
        cx.update(|cx| cx.quit());
    })
    .detach();
}

/// Shot windows are never shown, so no display link drives their frames and animations would stay at their first
/// frame. Redraw every window at the display rate instead, as a visible window would be.
fn pump_frames(cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(Duration::from_millis(16)).await;
            cx.update(|cx| {
                let live_hud = crate::hud::live_window(cx).map(|window| window.window_id());
                for window in cx.windows() {
                    if live_hud == Some(window.window_id()) {
                        continue;
                    }
                    let _ = cx.update_window(window, |_, w, cx| redraw(w, cx));
                }
            });
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::select;

    fn selected(only: Option<&str>) -> Result<Vec<&'static str>, String> {
        select(only).map(|groups| groups.iter().map(|group| group.name).collect())
    }

    #[test]
    fn only_named_groups_run_and_unknown_names_fail() {
        assert_eq!(selected(None).unwrap(), ["pages", "tools", "extra", "onboarding", "startup", "hud", "demo", "min"]);
        assert_eq!(selected(Some("hud")).unwrap(), ["hud"]);
        assert_eq!(selected(Some(" demo ,,readmefilm,pages,")).unwrap(), ["pages", "demo", "readmefilm"]);

        assert_eq!(selected(Some("hudd,hud,nope")).unwrap_err(), "unknown shot group: hudd, nope");
        assert_eq!(selected(Some("Hud")).unwrap_err(), "unknown shot group: Hud");
        assert!(selected(Some("")).is_err());
        assert!(selected(Some(" , ")).is_err());
    }
}
