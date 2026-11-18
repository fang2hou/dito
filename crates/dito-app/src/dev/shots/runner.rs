use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::*;

use super::input::redraw;
use super::{demo, hud, local_api, onboarding, pages, readme_film, startup, text_model_form, tile_row, wordbook};
use crate::settings::{self, Page};
use crate::state::{AppGlobal, AppState};

/// Which shot groups `DITO_SHOTS_ONLY=hud,demo` selects. Without it every default group runs and the opt-in ones
/// stay off.
struct Groups(Option<String>);

impl Groups {
    fn from_env() -> Self {
        Self(std::env::var("DITO_SHOTS_ONLY").ok())
    }

    fn selects(&self, group: &str) -> bool {
        self.0.as_ref().is_some_and(|only| only.split(',').any(|g| g == group))
    }

    fn runs_by_default(&self, group: &str) -> bool {
        self.0.is_none() || self.selects(group)
    }
}

pub fn run(dir: PathBuf, cx: &mut App) {
    std::fs::create_dir_all(&dir).expect("screenshot dir");
    let st = cx.global::<AppGlobal>().0.clone();
    settings::open(st.clone(), Page::General, cx);
    let groups = Groups::from_env();
    pump_frames(cx);
    cx.spawn(async move |cx| {
        render_app_shots(&st, &dir, &groups, cx).await;
        render_preview_shots(&st, &dir, &groups, cx).await;
        if groups.runs_by_default("min") {
            pages::min_size_shots(&st, &dir, cx).await;
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

async fn render_app_shots(st: &Arc<AppState>, dir: &Path, groups: &Groups, cx: &mut AsyncApp) {
    if groups.runs_by_default("pages") {
        pages::page_shots(st, dir, cx).await;
    }
    if groups.selects("navrepro") {
        pages::nav_repro(cx).await;
    }
    if groups.selects("promptmotion") {
        pages::prompt_motion_shots(st, dir, cx).await;
    }
    if groups.selects("promptediting") {
        pages::prompt_editing_shots(st, dir, cx).await;
    }
    if groups.selects("prompts") {
        pages::prompt_shots(st, dir, cx).await;
    }
    if groups.runs_by_default("tools") {
        pages::tools_shots(st, dir, cx).await;
    }
    if groups.selects("localapi") {
        local_api::local_api_shots(st, dir, cx).await;
    }
    if groups.selects("chat") {
        super::chat::run(st, dir, cx).await;
    }
    if groups.selects("transcriptfilm") {
        super::transcript_film::run(st, dir, cx).await;
    }
    if groups.selects("wordbook") {
        wordbook::wordbook_failure_shots(st, dir, cx).await;
    }
    if groups.selects("tilerow") {
        tile_row::tile_row_shots(dir, cx).await;
    }
    if groups.selects("status") {
        pages::status_shots(st, dir, cx).await;
    }
    if groups.selects("statuslog") {
        pages::status_log_shots(st, dir, cx).await;
    }
    #[cfg(feature = "mock")]
    if groups.selects("inlineasr") {
        super::inline_asr::run(st, dir, cx).await;
    }
    #[cfg(feature = "mock")]
    if groups.selects("cloudspeechtiles") {
        super::cloud_speech_tiles::run(st, dir, cx).await;
    }
    if groups.selects("models") {
        pages::model_shots(st, dir, cx).await;
    }
    if groups.selects("chatmodel") {
        pages::chat_model_shots(st, dir, cx).await;
    }
    if groups.selects("compare") {
        super::compare::compare_shots(st, dir, cx).await;
    }
    if groups.selects("textmodelform") {
        text_model_form::text_model_form_shots(st, dir, cx).await;
    }
    if groups.runs_by_default("extra") {
        pages::extra_shots(st, dir, cx).await;
    }
    if groups.runs_by_default("onboarding") {
        onboarding::onboarding_shots(st, dir, cx).await;
    }
    if groups.runs_by_default("startup") {
        startup::startup_failure_shots(st, dir, cx).await;
    }
    if groups.runs_by_default("hud") {
        hud::hud_shots(st, dir, cx).await;
    }
    if groups.runs_by_default("demo") {
        demo::demo_shots(dir, cx).await;
    }
}

async fn render_preview_shots(st: &Arc<AppState>, dir: &Path, groups: &Groups, cx: &mut AsyncApp) {
    if groups.selects("readmefilm") {
        readme_film::run(st, dir, cx).await;
    }
    if groups.selects("hudinteractions") {
        hud::hud_interaction_shots(st, dir, cx).await;
    }
    if groups.selects("hudstylepreview") {
        pages::hud_style_preview_shots(st, dir, cx).await;
    }
    if groups.selects("hudoutcome") {
        hud::hud_outcome_shots(st, dir, cx).await;
    }
    if groups.selects("hudwave") {
        hud::hud_wave_shots(st, dir, cx).await;
    }
    if groups.selects("hudmotion") {
        hud::hud_motion_shots(st, dir, cx).await;
    }
    if groups.selects("settingshud") {
        pages::hud_settings_shots(st, dir, cx).await;
    }
    if groups.selects("hudheight") {
        pages::hud_height_shots(st, dir, cx).await;
    }
    if groups.selects("hudtooltip") {
        hud::hud_tooltip_shots(st, dir, cx).await;
    }
    if groups.selects("hudlive") {
        hud::hud_live_shots(st, dir, cx).await;
    }
    if groups.selects("hudcommand") {
        hud::hud_command_live_shots(st, dir, cx).await;
    }
    if groups.selects("hudchord") {
        hud::hud_chord_live_shots(st, dir, cx).await;
    }
    if groups.selects("demolive") {
        demo::demo_live_shots(dir, cx).await;
    }
    if groups.selects("appearance") {
        pages::appearance_hover_shots(st, dir, cx).await;
    }
    if groups.selects("health") {
        pages::health_shots(st, dir, cx).await;
    }
}
