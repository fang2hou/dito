use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::config::{Appearance, DisplayLanguage, HudPosition, ProviderKind, Source};
use dito_core::controller::{Command, HudState};
use dito_core::factory::LocalBackend;
use dito_core::provider::configured_asr::AsrProfiles;
use dito_core::secrets::Role;
use dito_core::store::HistoryFilter;
use gpui_kit::*;

use super::capture::{
    page_shot, refresh_settings_window, render, render_settings_window, resize_settings_window, save, theme_tag,
    wait_for_frame,
};
use super::input::{click, hover, on_settings_window, press, release};
use super::verify;
use crate::hotkeys::CaptureState;
use crate::services;
use crate::settings::asr_editor::EditorPreview;
use crate::settings::{self, Page, Settings};
use crate::state::AppState;
use crate::ui::{self, ScopeHandle};

fn open_page(st: &Arc<AppState>, page: Page, cx: &mut AsyncApp) {
    cx.update(|cx| settings::open(st.clone(), page, cx));
}

fn open_page_sized(st: &Arc<AppState>, page: Page, width: f32, height: f32, cx: &mut AsyncApp) {
    cx.update(|cx| {
        settings::open(st.clone(), page, cx);
        resize_settings_window(cx, width, height);
    });
}

/// Every page in both appearances at the default window size.
pub async fn page_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    for appearance in [Appearance::Light, Appearance::Dark] {
        services::save_config(st, |c| c.general.appearance = appearance);
        for page in Page::ALL {
            open_page(st, page, cx);
            cx.background_executor().timer(Duration::from_millis(900)).await;
            let path = dir.join(format!("{}-{}.png", page.english().to_lowercase(), theme_tag(appearance)));
            match render_settings_window(cx) {
                Some(Ok(img)) => match img.save(&path) {
                    Ok(()) => println!("wrote {}", path.display()),
                    Err(e) => eprintln!("save {}: {e}", path.display()),
                },
                Some(Err(e)) => eprintln!("render {}: {e}", path.display()),
                None => eprintln!("no window for {}", path.display()),
            }
        }
    }
}

/// Capture contextual-help entrance, exit, and interrupted re-entry at native render frames.
pub async fn prompt_motion_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let original = st.cfg.get();
    for appearance in [Appearance::Light, Appearance::Dark] {
        services::save_config(st, |c| {
            c.general.appearance = appearance;
            c.general.display_language = DisplayLanguage::Ja;
        });
        open_page_sized(st, Page::Prompts, 1120., 980., cx);
        on_settings_window(cx, |w, cx, _| {
            if let Some((_, view)) = settings::handle(cx) {
                view.update(cx, |s, cx| s.preview_prompts("normal", w, cx));
            }
        });
        page_shot(cx, dir, &format!("help-{}-before.png", theme_tag(appearance))).await;
        let scope = cx.update(|cx| settings::page_motion(Page::Prompts, cx)).expect("prompt motion scope");
        let mut frames = Vec::new();
        for (phase, on, moments) in [
            ("enter", true, &[0, 50, 100, 150, 220, 350][..]),
            ("exit", false, &[0, 50, 100, 150, 220, 350][..]),
            ("interrupted-enter", true, &[0][..]),
            ("interrupted-exit", false, &[0][..]),
            ("reenter", true, &[0, 50, 100, 150, 220, 350][..]),
        ] {
            ui::force_hover(&scope, ElementId::from("prompt-examples-help"), on);
            let start = Instant::now();
            for &elapsed in moments {
                if let Some(remaining) = Duration::from_millis(elapsed).checked_sub(start.elapsed()) {
                    cx.background_executor().timer(remaining).await;
                }
                if let Some(Ok(image)) = render_settings_window(cx) {
                    println!("captured help {} {phase} at {} ms", theme_tag(appearance), start.elapsed().as_millis());
                    frames.push((format!("help-{}-{phase}-{elapsed:03}.png", theme_tag(appearance)), image));
                }
            }
        }
        // Encoding during capture stalls the UI thread and skips the intermediate frames.
        for (name, image) in frames {
            save(dir, &name, image);
        }
    }
    services::save_config(st, |c| {
        c.general.appearance = original.general.appearance;
        c.general.display_language = original.general.display_language;
    });
}

/// Prompt controls at default and narrow widths, with both appearances.
pub async fn prompt_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let original = st.cfg.get();
    for language in [DisplayLanguage::Ja, DisplayLanguage::En, DisplayLanguage::Zh] {
        for appearance in [Appearance::Light, Appearance::Dark] {
            services::save_config(st, |c| {
                c.general.appearance = appearance;
                c.general.display_language = language;
            });
            for (width, label) in [(1120., "default"), (900., "narrow")] {
                for case in [
                    "normal",
                    "menu",
                    "add-empty",
                    "add",
                    "reset",
                    "help",
                    "herdr",
                    "custom",
                    "reset-app",
                    "history",
                    "preview",
                    "removeapp",
                ] {
                    open_page_sized(st, Page::Prompts, width, 980., cx);
                    on_settings_window(cx, |w, cx, _| {
                        if let Some((_, view)) = settings::handle(cx) {
                            view.update(cx, |s, cx| s.preview_prompts(case, w, cx));
                        }
                    });
                    page_shot(
                        cx,
                        dir,
                        &format!(
                            "prompts-{}-{}-{label}-{case}.png",
                            match language {
                                DisplayLanguage::En => "en",
                                DisplayLanguage::Zh => "zh",
                                _ => "ja",
                            },
                            theme_tag(appearance)
                        ),
                    )
                    .await;
                }
            }
        }
    }
    services::save_config(st, |c| {
        c.general.appearance = original.general.appearance;
        c.general.display_language = original.general.display_language;
    });
}

/// Verify committed input, IME, autosave and native Undo/Redo through real input events.
pub async fn prompt_editing_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let original = st.cfg.get();
    for appearance in [Appearance::Light, Appearance::Dark] {
        services::save_config(st, |config| {
            config.general.appearance = appearance;
            config.general.display_language = DisplayLanguage::Ja;
        });
        open_page_sized(st, Page::Prompts, 1120., 980., cx);
        for case in [
            "probe-ime",
            "probe-input",
            "probe-not-saved",
            "probe-undo",
            "probe-redo",
            "probe-reset",
            "probe-save-failure",
            "probe-retry",
        ] {
            on_settings_window(cx, |window, cx, view| {
                view.update(cx, |view, cx| view.preview_prompts(case, window, cx))
            });
            page_shot(cx, dir, &format!("prompt-editing-{}-{case}.png", theme_tag(appearance))).await;
            if case == "probe-not-saved" {
                cx.background_executor().timer(Duration::from_millis(2500)).await;
            }
        }
        for (case, position) in [
            ("probe-blur", point(px(350.), px(64.))),
            ("probe-menu-open", point(px(940.), px(428.))),
            ("probe-menu-closed", point(px(940.), px(428.))),
            ("probe-menu-open", point(px(940.), px(428.))),
            ("probe-add-open", point(px(940.), px(478.))),
        ] {
            on_settings_window(cx, |window, cx, _| click(window, cx, position));
            page_shot(cx, dir, &format!("prompt-control-{}-{case}.png", theme_tag(appearance))).await;
            on_settings_window(cx, |window, cx, view| {
                view.update(cx, |view, cx| view.preview_prompts(case, window, cx))
            });
        }
    }
    services::save_config(st, |config| {
        config.general.appearance = original.general.appearance;
        config.general.display_language = original.general.display_language;
        config.prompts = original.prompts.clone();
    });
}

/// Status layout in both appearances, including narrow windows and long provider facts.
pub async fn status_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let original = st.cfg.get();
    for appearance in [Appearance::Light, Appearance::Dark] {
        for (width, size) in [(1120., "default"), (900., "narrow")] {
            for case in ["local", "cloud", "disabled"] {
                services::save_config(st, |c| {
                    c.general.appearance = appearance;
                    c.general.display_language = DisplayLanguage::Ja;
                    c.asr.source = Source::Local;
                    c.polish.enabled = case != "disabled";
                    c.polish.engine.source = if case == "local" { Source::Local } else { Source::Provider };
                    c.polish.engine.provider.kind = ProviderKind::Compat;
                    c.polish.engine.provider.compat.base_url =
                        "https://regional-transcription-gateway.ap-northeast-1.example.com/v1".into();
                    c.polish.engine.provider.compat.model = "gpt-4o-mini".into();
                });
                open_page_sized(st, Page::Status, width, 1000., cx);
                page_shot(cx, dir, &format!("status-{}-{size}-{case}.png", theme_tag(appearance))).await;
            }
        }
    }
    services::save_config(st, |c| {
        c.general.appearance = original.general.appearance;
        c.general.display_language = original.general.display_language;
        c.asr = original.asr.clone();
        c.polish = original.polish.clone();
    });
}

/// Click through the sidebar in a fixed pattern, to reproduce navigation glitches. Opt-in: `DITO_SHOTS_ONLY=navrepro`.
pub async fn nav_repro(cx: &mut AsyncApp) {
    cx.background_executor().timer(Duration::from_millis(700)).await;
    for _ in 0..3 {
        for index in [0, 1, 1, 0, 0, 1, 1, 0, 1, 2, 2, 2, 1] {
            let position = point(px(100.), px(175. + index as f32 * 48.));
            on_settings_window(cx, |w, cx, _| click(w, cx, position));
            cx.background_executor().timer(Duration::from_millis(130)).await;
        }
    }
    cx.background_executor().timer(Duration::from_millis(600)).await;
}

/// Every page at the settings window's minimum width (900): catches narrow-layout overflow that
/// the default 1120×860 shots cannot see. Height goes past the 640 minimum so below-fold cards
/// (e.g. the models storage card) are captured too.
pub async fn min_size_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    for appearance in [Appearance::Light, Appearance::Dark] {
        services::save_config(st, |c| c.general.appearance = appearance);
        for page in Page::ALL {
            open_page_sized(st, page, 900., 820., cx);
            let name = format!("{}-min-{}.png", page.english().to_lowercase(), theme_tag(appearance));
            page_shot(cx, dir, &name).await;
        }
    }
}

/// Pages in non-default states (local models, shortcut recording, open selects), then hover states.
pub async fn extra_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let original = st.cfg.get();
    for appearance in [Appearance::Light, Appearance::Dark] {
        non_default_page_shots(st, dir, cx, appearance).await;
    }
    // force_hover writes the scope the id actually renders under: the shell (nav, health) or the
    // open page, so no guessing a visit counter.
    let (shell, general) = cx.update(|cx| {
        settings::open(st.clone(), Page::General, cx);
        (settings::shell_motion(cx), settings::page_motion(Page::General, cx))
    });
    let (shell, general) = (shell.expect("settings shell scope"), general.expect("general page scope"));
    general_hover_shots(dir, cx, &shell, &general).await;
    models_hover_shots(st, dir, cx).await;
    health_dot_hover_shots(dir, cx, &shell).await;
    services::save_config(st, |c| {
        c.asr.source = original.asr.source;
        c.polish.engine.source = original.polish.engine.source;
    });
}

async fn non_default_page_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp, appearance: Appearance) {
    let tag = theme_tag(appearance);
    services::save_config(st, |c| {
        c.general.appearance = appearance;
        c.asr.source = Source::Local;
        c.polish.engine.source = Source::Local;
    });
    open_page(st, Page::Models, cx);
    page_shot(cx, dir, &format!("models-local-{tag}.png")).await;
    services::save_config(st, |c| c.general.appearance = appearance);
    open_page(st, Page::General, cx);
    st.hotkeys.capture.send_replace(CaptureState::Waiting);
    page_shot(cx, dir, &format!("general-recording-{tag}.png")).await;
    st.hotkeys.capture.send_replace(CaptureState::Idle);
    open_page(st, Page::General, cx);
    open_general_select(cx, "ui-lang");
    page_shot(cx, dir, &format!("general-select-open-{tag}.png")).await;
    open_general_select(cx, "mic");
    page_shot(cx, dir, &format!("general-mic-open-{tag}.png")).await;
}

fn open_general_select(cx: &mut AsyncApp, id: &'static str) {
    cx.update(|cx| {
        if let Some((_, view)) = settings::handle(cx) {
            view.update(cx, |s, cx| s.open_general_select(id, cx));
        }
    });
}

/// A nav item, a segment cell, a select.
async fn general_hover_shots(dir: &Path, cx: &mut AsyncApp, shell: &ScopeHandle, general: &ScopeHandle) {
    let mode_cell = ElementId::from((ElementId::from("mode"), SharedString::from("i1")));
    let nav_item = || ElementId::from(("nav", 2usize));
    ui::force_hover(shell, nav_item(), true);
    ui::force_hover(general, mode_cell.clone(), true);
    ui::force_hover(general, ElementId::from("ui-lang"), true);
    page_shot(cx, dir, "hover-warmup.png").await;
    page_shot(cx, dir, "general-hover.png").await;
    ui::force_hover(shell, nav_item(), false);
    ui::force_hover(general, mode_cell, false);
    ui::force_hover(general, ElementId::from("ui-lang"), false);
}

/// A provider card and a button on the Models page.
async fn models_hover_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let models = cx
        .update(|cx| {
            settings::open(st.clone(), Page::Models, cx);
            settings::page_motion(Page::Models, cx)
        })
        .expect("models page scope");
    ui::force_hover(&models, ElementId::from("prov-0-bedrock"), true);
    ui::force_hover(&models, ElementId::from("test-0"), true);
    page_shot(cx, dir, "hover-warmup.png").await;
    page_shot(cx, dir, "models-hover.png").await;
}

/// Health dot hover: the halo + status card on the expanded logo, then on the rail's mark.
/// force_hover only writes the hover table: without a refresh the window would keep painting
/// the pre-hover element tree, so nudge a rebuild before each pair.
async fn health_dot_hover_shots(dir: &Path, cx: &mut AsyncApp, shell: &ScopeHandle) {
    ui::force_hover(shell, ElementId::from("health-dot-hit"), true);
    cx.update(refresh_settings_window);
    page_shot(cx, dir, "hover-warmup.png").await;
    page_shot(cx, dir, "health-hover.png").await;
    cx.update(|cx| resize_settings_window(cx, 900., 820.));
    page_shot(cx, dir, "hover-warmup.png").await;
    page_shot(cx, dir, "health-rail-hover.png").await;
    ui::force_hover(shell, ElementId::from("health-dot-hit"), false);
}

struct ModelCase {
    name: &'static str,
    role: Role,
    local: bool,
    preset: usize,
    editor: Option<EditorPreview>,
}

const fn model_case(
    name: &'static str,
    role: Role,
    local: bool,
    preset: usize,
    editor: Option<EditorPreview>,
) -> ModelCase {
    ModelCase { name, role, local, preset, editor }
}

const MODEL_CASES: [ModelCase; 12] = [
    model_case("local-asr", Role::Asr, true, 0, None),
    model_case("local-polish", Role::Polish, true, 0, None),
    model_case("funasr", Role::Asr, false, 0, None),
    model_case("qwen-asr", Role::Asr, false, 1, None),
    model_case("openrouter", Role::Asr, false, 2, None),
    model_case("azure", Role::Polish, false, 0, None),
    model_case("bedrock", Role::Polish, false, 1, None),
    model_case("compatible", Role::Polish, false, 2, None),
    model_case("editor", Role::Asr, false, 1, Some(EditorPreview::Valid)),
    model_case("editor-end", Role::Asr, false, 1, Some(EditorPreview::End)),
    model_case("editor-error", Role::Asr, false, 1, Some(EditorPreview::Invalid)),
    model_case("help", Role::Asr, false, 1, Some(EditorPreview::Help)),
];

/// The Models page in each preset state. Opt-in: `DITO_SHOTS_ONLY=models`, `DITO_MODEL_SHOTS=<case names>`.
pub async fn model_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let original = st.cfg.get();
    let only = std::env::var("DITO_MODEL_SHOTS").ok();
    // Opt in to a real engine state, without fabricating a running status for the screenshot.
    if let Ok(id) = std::env::var("DITO_MODEL_WARM") {
        let local = st.local.clone();
        let selected = id.clone();
        st.rt.spawn(async move { local.warm(&selected).await }).await.expect("model warmup task");
        assert_eq!(st.local.status(&id), dito_core::health::EngineStatus::Running, "screenshot model failed to load");
    }
    for appearance in [Appearance::Light, Appearance::Dark] {
        let tag = theme_tag(appearance);
        for case in &MODEL_CASES {
            if only.as_ref().is_some_and(|filter| !filter.split(',').any(|entry| entry == case.name)) {
                continue;
            }
            apply_model_case(st, appearance, case);
            open_page(st, Page::Models, cx);
            if case.role == Role::Asr && !case.local {
                cx.update(|cx| resize_settings_window(cx, 1120., 1120.));
            }
            cx.update(|cx| {
                let (w, view) = settings::handle(cx).expect("settings window");
                let _ = w.update(cx, |_, window, cx| {
                    view.update(cx, |s, cx| s.preview_models(case.role, case.editor, window, cx))
                });
            });
            page_shot(cx, dir, &format!("models-{}-{tag}.png", case.name)).await;
            if matches!(case.editor, Some(EditorPreview::Valid | EditorPreview::End | EditorPreview::Help)) {
                cx.update(|cx| resize_settings_window(cx, 1000., 680.));
                page_shot(cx, dir, &format!("models-{}-compact-{tag}.png", case.name)).await;
                cx.update(|cx| resize_settings_window(cx, 1120., 860.));
            }
        }
    }
    services::save_config(st, |c| *c = (*original).clone());
}

fn apply_model_case(st: &AppState, appearance: Appearance, case: &ModelCase) {
    services::save_config(st, |c| {
        c.general.display_language = DisplayLanguage::Ja;
        c.general.appearance = appearance;
        c.asr.source = if case.local { Source::Local } else { Source::Provider };
        c.polish.engine.source = c.asr.source;
        c.polish.engine.provider.kind = [ProviderKind::Azure, ProviderKind::Bedrock, ProviderKind::Compat][case.preset];
        let mut profiles = AsrProfiles::default();
        profiles.select_index(case.preset);
        for profile in &mut profiles.profiles {
            profile.config["connection"]["model"] = "my-asr-model".into();
        }
        c.asr.cloud_asr = Some(profiles);
    });
}

/// Exercise independent interface/HUD selections without touching the user's config (run with --mock --data-dir).
pub async fn hud_settings_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    for appearance in [Appearance::Light, Appearance::Dark] {
        for hud in [Appearance::Light, Appearance::Dark] {
            services::save_config(st, |c| {
                c.general.appearance = appearance;
                c.general.hud_appearance = hud;
            });
            open_page_sized(st, Page::General, 1120., 1120., cx);
            let name = format!("settings-{}-hud-{}.png", theme_tag(appearance), theme_tag(hud));
            page_shot(cx, dir, &name).await;
        }
    }
}

/// Capture both position modes and themes against an isolated mock configuration.
pub async fn hud_height_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    for appearance in [Appearance::Light, Appearance::Dark] {
        for position in [HudPosition::Centered, HudPosition::Pointer] {
            services::save_config(st, |c| {
                c.general.appearance = appearance;
                c.general.hud_position = position;
                c.general.hud_height_level = Some(0);
            });
            open_page_sized(st, Page::General, 1120., 1200., cx);
            let mode = if position == HudPosition::Pointer { "pointer" } else { "centered" };
            if position == HudPosition::Pointer {
                // Keep intermediate row-collapse frames: settled screenshots cannot expose overlapping fill layers.
                let _ = render_settings_window(cx);
                for frame in 1..=3 {
                    cx.background_executor().timer(Duration::from_millis(30)).await;
                    if let Some(Ok(img)) = render_settings_window(cx) {
                        save(dir, &format!("hud-height-{}-hide-{frame}.png", theme_tag(appearance)), img);
                    }
                }
            }
            page_shot(cx, dir, &format!("hud-height-{}-{mode}.png", theme_tag(appearance))).await;
            if position == HudPosition::Centered {
                let mut thumb = Point::default();
                on_settings_window(cx, |w, cx, view| {
                    let bounds = view.read(cx).hud_height_bounds(cx);
                    thumb = point(bounds.left() + bounds.size.width / 7., bounds.center().y);
                    hover(w, cx, thumb);
                });
                page_shot(cx, dir, &format!("hud-height-{}-hover.png", theme_tag(appearance))).await;
                on_settings_window(cx, |w, cx, _| press(w, cx, thumb));
                page_shot(cx, dir, &format!("hud-height-{}-pressed.png", theme_tag(appearance))).await;
                on_settings_window(cx, |w, cx, _| {
                    release(w, cx, thumb);
                    hover(w, cx, point(px(0.), px(0.)));
                });
                page_shot(cx, dir, &format!("hud-height-{}-leave.png", theme_tag(appearance))).await;
                if appearance == Appearance::Light {
                    verify::hud_height_slider(st, cx).await;
                }
            }
        }
    }
}

/// Compare the shared text/icon segment feedback, including hovering the selected cell.
pub async fn appearance_hover_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let original = st.cfg.get();
    let scope = cx.update(|cx| {
        settings::open(st.clone(), Page::General, cx);
        resize_settings_window(cx, 1200., 1400.);
        settings::page_motion(Page::General, cx).expect("general scope")
    });
    let segment = |control: &'static str, option: &'static str| {
        ElementId::from((ElementId::from(control), SharedString::from(option)))
    };
    for appearance in [Appearance::Light, Appearance::Dark] {
        st.cfg.update(|c| c.general.appearance = appearance).expect("save appearance");
        for glyph in ["system", "light", "dark"] {
            for control in ["appearance", "hud-appearance"] {
                for other in ["system", "light", "dark"] {
                    ui::force_hover(&scope, segment(control, other), false);
                }
            }
            ui::force_hover(&scope, segment("appearance", glyph), true);
            cx.update(refresh_settings_window);
            page_shot(cx, dir, &format!("appearance-{}-hover-{glyph}.png", theme_tag(appearance))).await;
        }
    }
    st.cfg.update(|c| c.general.appearance = original.general.appearance).expect("restore appearance");
}

struct HealthCase {
    rail: bool,
    polish_enabled: bool,
    source: Source,
}

const HEALTH_CASES: [HealthCase; 4] = [
    HealthCase { rail: false, polish_enabled: true, source: Source::Local },
    HealthCase { rail: true, polish_enabled: false, source: Source::Local },
    HealthCase { rail: false, polish_enabled: true, source: Source::Provider },
    HealthCase { rail: true, polish_enabled: false, source: Source::Provider },
];

/// The logo-anchored health card: both sidebar widths, themes, languages and disabled polish.
pub async fn health_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let original = st.cfg.get();
    // The health dot and card render under the settings shell scope; it survives navigation for
    // the window's whole life, so one handle serves every iteration.
    let shell = cx
        .update(|cx| {
            settings::open(st.clone(), Page::General, cx);
            settings::shell_motion(cx)
        })
        .expect("settings shell scope");
    for (language, language_tag) in
        [(DisplayLanguage::Ja, "ja"), (DisplayLanguage::En, "en"), (DisplayLanguage::Zh, "zh")]
    {
        for appearance in [Appearance::Light, Appearance::Dark] {
            for case in &HEALTH_CASES {
                if case.source == Source::Provider && language != DisplayLanguage::Zh {
                    continue;
                }
                reset_health_hover(&shell);
                services::save_config(st, |c| {
                    c.general.appearance = appearance;
                    c.general.display_language = language;
                    c.asr.source = case.source;
                    c.polish.enabled = case.polish_enabled;
                    c.polish.engine.source = case.source;
                });
                let name = health_case_name(language_tag, appearance, case);
                health_card_shot(st, dir, cx, &shell, case, &name).await;
            }
        }
    }
    reset_health_hover(&shell);
    services::save_config(st, |c| {
        c.general.appearance = original.general.appearance;
        c.general.display_language = original.general.display_language;
        c.asr.source = original.asr.source;
        c.polish.enabled = original.polish.enabled;
        c.polish.engine.source = original.polish.engine.source;
    });
}

fn health_case_name(language_tag: &str, appearance: Appearance, case: &HealthCase) -> String {
    let mode = if case.rail { "rail-disabled" } else { "expanded" };
    let state = if case.source == Source::Provider { "configured" } else { "stopped" };
    format!("{language_tag}-{}-{mode}-{state}", theme_tag(appearance))
}

fn reset_health_hover(shell: &ScopeHandle) {
    ui::force_hover(shell, ElementId::from("health-dot-hit"), false);
    ui::force_hover(shell, ElementId::from("health-card-hit"), false);
}

async fn health_card_shot(
    st: &Arc<AppState>,
    dir: &Path,
    cx: &mut AsyncApp,
    shell: &ScopeHandle,
    case: &HealthCase,
    name: &str,
) {
    open_page_sized(st, Page::General, if case.rail { 900. } else { 1200. }, 820., cx);
    // Settle the logo bounds and prime the closed animation before entering.
    cx.background_executor().timer(Duration::from_millis(400)).await;
    let dot = cx.update(|cx| {
        let (_, view) = settings::handle(cx).expect("health settings window");
        view.read(cx).health_hover_probe().0.expect("measured logo dot")
    });
    verify::health_hover_lifecycle(cx, dot.center());
    ui::force_hover(shell, ElementId::from("health-dot-hit"), true);
    cx.update(refresh_settings_window);
    page_shot(cx, dir, &format!("health-{name}.png")).await;
    println!("verified health hover enter / card transfer / leave: {name}");
}

/// Select the HUD appearance through the same action as the Settings control, including replacement and
/// side-effect checks. Opt-in: `DITO_SHOTS_ONLY=hudstylepreview`.
pub async fn hud_style_preview_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let history_before = history_len(st);
    services::save_config(st, |c| c.general.hud_appearance = Appearance::System);
    let settings_view = cx.update(|cx| {
        settings::open(st.clone(), Page::General, cx);
        resize_settings_window(cx, 1120., 1120.);
        let (_, view) = settings::handle(cx).expect("settings window");
        assert!(view.read(cx).hud_preview(cx).is_none(), "opening Settings must not start a preview");
        view
    });
    for appearance in [Appearance::Dark, Appearance::Light] {
        capture_inline_preview(st, dir, cx, &settings_view, appearance, history_before).await;
    }
    verify_preview_replacement_and_priority(st, cx, &settings_view).await;
}

fn history_len(st: &AppState) -> usize {
    st.store.list_history(&HistoryFilter::default()).expect("history").len()
}

async fn capture_inline_preview(
    st: &Arc<AppState>,
    dir: &Path,
    cx: &mut AsyncApp,
    settings_view: &Entity<Settings>,
    appearance: Appearance,
    history_before: usize,
) {
    let tag = theme_tag(appearance);
    let window = cx.update(|cx| settings::handle(cx).expect("settings window").0);
    services::save_config(st, |c| c.general.appearance = appearance);
    settings_view.update(cx, |settings, cx| settings.set_hud_appearance(appearance, cx));
    let preview = cx.update(|cx| settings_view.read(cx).hud_preview(cx).expect("selection starts inline preview"));
    let start = Instant::now();
    let mut timestamps = String::new();
    let mut frame = 0;
    while start.elapsed().as_secs_f32() < 5.1 {
        wait_for_frame(cx, start, frame).await;
        let img = render(cx, window).expect("inline preview frame");
        timestamps.push_str(&format!("{frame}\t{}\n", start.elapsed().as_secs_f64()));
        if frame == 0 {
            save(dir, &format!("inline-settings-{tag}.png"), img.clone());
        }
        // Fixed screenshot viewport: include the HUD row and its neighboring icon switcher.
        let row = image::imageops::crop_imm(&img, 288, 868, 776, 65).to_image();
        save(dir, &format!("inline-preview-{tag}-{frame:03}.png"), row);
        frame += 1;
    }
    std::fs::write(dir.join(format!("inline-preview-{tag}-times.tsv")), timestamps).expect("write frame times");
    cx.update(|cx| {
        assert!(!preview.read(cx).active(), "animation ends after one cycle");
        assert!(settings_view.read(cx).hud_preview(cx).is_none(), "expired preview is hidden");
    });
    assert_eq!(*st.dictation.hud().borrow(), HudState::Hidden);
    assert_eq!(history_len(st), history_before);
    assert_eq!(st.cfg.get().general.appearance, appearance);
    settings_view.update(cx, |settings, cx| settings.set_hud_appearance(appearance, cx));
    cx.update(|cx| assert!(settings_view.read(cx).hud_preview(cx).is_none(), "same selection does not replay"));
    println!("inline style preview verified: {tag}, animation ended, no dictation/history, appearance independent");
}

async fn verify_preview_replacement_and_priority(
    st: &Arc<AppState>,
    cx: &mut AsyncApp,
    settings_view: &Entity<Settings>,
) {
    settings_view.update(cx, |settings, cx| settings.set_hud_appearance(Appearance::Dark, cx));
    let first = cx.update(|cx| settings_view.read(cx).hud_preview(cx).expect("selection starts a preview"));
    settings_view.update(cx, |settings, cx| settings.set_hud_appearance(Appearance::System, cx));
    let replacement = cx.update(|cx| settings_view.read(cx).hud_preview(cx).expect("reselection starts a preview"));
    assert_ne!(first.entity_id(), replacement.entity_id(), "rapid selection replaces the inline view");
    st.dictation.send(Command::KeyDown);
    cx.background_executor().timer(Duration::from_millis(100)).await;
    cx.update(|cx| assert!(settings_view.read(cx).hud_preview(cx).is_none(), "real listening takes priority"));
    st.dictation.send(Command::Cancel);
    cx.background_executor().timer(Duration::from_millis(100)).await;
    println!("inline style preview verified: replacement and real-session priority");
}
