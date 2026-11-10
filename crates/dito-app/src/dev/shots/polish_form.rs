//! The cloud polish form, the delete dialog, the failure banner and the local polish parameters. Every state is reached
//! through the inputs the user types into, and each step checks what was saved, so the shots show the behavior and not
//! a staged copy of it. Opt-in: `DITO_SHOTS_ONLY=polishform`, with `--mock --data-dir <dir>` so the settings file is
//! known. Run with `--lang en` to see that the stand-in names follow the display language, not the system's.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::config::{Appearance, CloudPolishConfiguration, DisplayLanguage, LocalPolishParams, Source};
use dito_core::secrets::{Role, polish_account};
use gpui_kit::*;

use super::capture::{page_shot, resize_settings_window, theme_tag};
use super::input::{on_settings_window, press_key, redraw};
use crate::services;
use crate::settings::{self, ModelsProbe, Page};
use crate::state::AppState;

/// One probe step, followed by a frame: the page follows the settings when it is drawn, as it does between a click and
/// the typing that comes after it.
fn step(cx: &mut AsyncApp, case: &'static str, arg: &str, text: &str) {
    on_settings_window(cx, |window, cx, view| {
        view.update(cx, |s, cx| s.probe_models(case, arg, text, window, cx));
        redraw(window, cx);
    });
}

fn type_cloud(cx: &mut AsyncApp, field: &'static str, text: &str) {
    step(cx, "type", field, text);
}

fn type_local(cx: &mut AsyncApp, field: &'static str, text: &str) {
    step(cx, "type-local", field, text);
}

fn draw(cx: &mut AsyncApp) {
    on_settings_window(cx, |window, cx, _| redraw(window, cx));
}

fn probe(cx: &mut AsyncApp) -> ModelsProbe {
    cx.update(|cx| settings::handle(cx).map(|(_, view)| view.read(cx).models_probe(cx))).expect("settings window")
}

/// The cloud polish configuration in use, as saved.
fn active(st: &AppState) -> CloudPolishConfiguration {
    let cfg = st.cfg.get();
    cfg.polish.engine.polish_configurations(cfg.general.lang()).active().cloned().expect("one is selected")
}

fn ids(st: &AppState) -> (Vec<String>, String) {
    let cfg = st.cfg.get();
    let list = cfg.polish.engine.polish_configurations(cfg.general.lang());
    (list.items.iter().map(|c| c.id.clone()).collect(), list.selected.clone())
}

fn key_of(st: &AppState, id: &str) -> Option<String> {
    st.secrets.get(&polish_account(id))
}

/// Wait until the connection test of the form has a result.
async fn tested(cx: &mut AsyncApp) -> &'static str {
    let start = Instant::now();
    loop {
        let state = probe(cx).test;
        if state != "idle" && state != "running" {
            return state;
        }
        assert!(start.elapsed() < Duration::from_secs(15), "the connection test never finished");
        cx.background_executor().timer(Duration::from_millis(100)).await;
    }
}

fn shot_name(name: &str, language: &str, tag: &str) -> String {
    if language == "ja" { format!("polish-{name}-{tag}.png") } else { format!("polish-{name}-{language}-{tag}.png") }
}

pub async fn polish_form_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let original = st.cfg.get();
    let mock_url = {
        let list = original.polish.engine.polish_configurations(original.general.lang());
        list.get("azure").map(|azure| azure.target.address().to_owned()).unwrap_or_default()
    };
    for appearance in [Appearance::Light, Appearance::Dark] {
        let tag = theme_tag(appearance);
        services::save_config(st, |c| {
            c.general.appearance = appearance;
            c.general.display_language = DisplayLanguage::Ja;
            c.polish.engine.source = Source::Provider;
            // Nothing saved yet: the templates stand in.
            c.polish.engine.cloud_polish = None;
            c.polish.engine.local_params.clear();
        });
        cx.update(|cx| {
            settings::open(st.clone(), Page::Models, cx);
            resize_settings_window(cx, 1120., 980.);
        });
        step(cx, "select", "azure", "");
        on_settings_window(cx, |window, cx, view| {
            view.update(cx, |s, cx| s.preview_models(Role::Polish, None, window, cx));
            redraw(window, cx);
        });
        page_shot(cx, dir, &shot_name("fresh", "ja", tag)).await;
        let names: Vec<_> = {
            let cfg = st.cfg.get();
            cfg.polish.engine.polish_configurations(cfg.general.lang()).items.iter().map(|c| c.name.clone()).collect()
        };
        assert_eq!(names[2], "OpenAI 互換 API", "the stand-ins are named in the display language, not the system's");

        templates(st, dir, cx, tag).await;
        invalid_texts(st, dir, cx, tag).await;
        connection_tests(dir, cx, tag, &mock_url).await;
        save_failure(st, dir, cx, tag).await;
        deleting(st, dir, cx, tag).await;
        local_parameters(st, dir, cx, tag).await;
        languages(st, dir, cx, tag).await;
    }
    services::save_config(st, |c| *c = (*original).clone());
}

/// Each template is filled in through its own form; the first typed character saves the list.
async fn templates(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp, tag: &str) {
    step(cx, "select", "compat", "");
    type_cloud(cx, "name", "ふだん使い");
    type_cloud(cx, "address", "https://openrouter.ai/api/v1/chat/completions");
    type_cloud(cx, "model", "openai/gpt-4o-mini");
    type_cloud(cx, "key", "sk-demo-0123456789abcdef");
    type_cloud(cx, "temperature", "0.2");
    let compat = active(st);
    assert_eq!(compat.id, "compat");
    assert_eq!(
        (compat.name.as_str(), compat.target.address(), compat.target.model()),
        ("ふだん使い", "https://openrouter.ai/api/v1/chat/completions", "openai/gpt-4o-mini"),
        "everything typed is saved as it changes"
    );
    assert_eq!(compat.params.temperature, Some(0.2));
    assert_eq!(key_of(st, "compat").as_deref(), Some("sk-demo-0123456789abcdef"), "the key goes to the Keychain");

    step(cx, "select", "azure", "");
    type_cloud(cx, "name", "高精度");
    type_cloud(cx, "address", "https://dito-prod-japaneast.openai.azure.com/");
    type_cloud(cx, "model", "gpt-6.1-terra");
    type_cloud(cx, "key", "azure-demo-key-0123456789");
    step(cx, "select", "bedrock", "");
    type_cloud(cx, "name", "Bedrock 検証");
    type_cloud(cx, "model", "apac.anthropic.claude-sonnet-4-20250514-v1:0");
    type_cloud(cx, "temperature", "0.3");
    assert_eq!(key_of(st, "azure").as_deref(), Some("azure-demo-key-0123456789"), "one key per configuration");
    assert_eq!(key_of(st, "bedrock"), None, "Bedrock has no key field");
    // Tiles in the order of the design: the everyday configuration first.
    services::save_config(st, |c| {
        let lang = c.general.lang();
        c.polish.engine.polish_configurations_mut(lang).move_to("compat", 0);
    });
    for id in ["compat", "azure", "bedrock"] {
        step(cx, "select", id, "");
        let state = probe(cx);
        assert_eq!(state.shown.as_deref(), Some(id), "the form shows the configuration in use");
        assert_eq!(state.text("name"), active(st).name, "and follows the selection");
        page_shot(cx, dir, &shot_name(id, "ja", tag)).await;
    }
    println!("verified: typing saves as it changes, the form follows the selection, one key per configuration");
}

/// Texts the settings cannot take: not saved, the last valid value stays, the field shows the danger border.
async fn invalid_texts(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp, tag: &str) {
    step(cx, "select", "compat", "");
    type_cloud(cx, "temperature", "abc");
    type_cloud(cx, "top_p", "1.5");
    type_cloud(cx, "max_tokens", "0");
    type_cloud(cx, "extra", "[1]");
    let params = active(st).params;
    assert_eq!(params.temperature, Some(0.2), "the last valid temperature stays in use");
    assert_eq!((params.top_p, params.max_tokens), (None, None));
    assert!(params.extra.is_empty());
    assert_eq!(probe(cx).invalid, ["temperature", "top_p", "max_tokens", "extra"]);
    page_shot(cx, dir, &shot_name("invalid", "ja", tag)).await;

    type_cloud(cx, "temperature", "-0.5");
    type_cloud(cx, "top_p", "-0.1");
    assert_eq!(active(st).params.temperature, Some(0.2), "a negative temperature is not saved");
    assert_eq!(probe(cx).invalid[..2], ["temperature", "top_p"]);

    type_cloud(cx, "temperature", "0.7");
    type_cloud(cx, "top_p", "0.95");
    type_cloud(cx, "max_tokens", "2048");
    type_cloud(cx, "effort", "high");
    type_cloud(cx, "extra", r#"{"seed": 42}"#);
    let params = active(st).params;
    assert_eq!((params.temperature, params.top_p, params.max_tokens), (Some(0.7), Some(0.95), Some(2048)));
    assert_eq!(params.reasoning_effort.as_str(), "high");
    assert_eq!(params.extra["seed"], 42);
    assert!(probe(cx).invalid.is_empty(), "the borders are gone as soon as the texts parse");
    page_shot(cx, dir, &shot_name("parameters", "ja", tag)).await;

    // Emptying a field clears the value.
    for field in ["temperature", "top_p", "max_tokens", "effort", "extra"] {
        type_cloud(cx, field, "");
    }
    let params = active(st).params;
    assert_eq!((params.temperature, params.top_p, params.max_tokens), (None, None, None));
    assert!(params.extra.is_empty() && params.reasoning_effort.as_str().is_empty());
    type_cloud(cx, "temperature", "0.2");

    // A name that is emptied stays as saved, and comes back when focus leaves the field.
    type_cloud(cx, "name", "");
    assert_eq!(active(st).name, "ふだん使い", "an empty name is not saved");
    assert_eq!(probe(cx).text("name"), "");
    step(cx, "blur-name", "", "");
    assert_eq!(probe(cx).text("name"), "ふだん使い", "the name returns when focus leaves");
    println!("verified: invalid texts stay out of the settings and are flagged; empty fields clear; the name returns");
}

/// The connection test against the mock server, and against a port nothing listens on.
async fn connection_tests(dir: &Path, cx: &mut AsyncApp, tag: &str, mock_url: &str) {
    step(cx, "select", "azure", "");
    type_cloud(cx, "address", mock_url);
    step(cx, "test", "", "");
    assert_eq!(tested(cx).await, "ok", "the mock server accepts the configuration");
    page_shot(cx, dir, &shot_name("test-ok", "ja", tag)).await;
    type_cloud(cx, "model", "gpt-6.1-terra-2");
    assert_eq!(probe(cx).test, "idle", "a changed setting invalidates the result");
    type_cloud(cx, "address", "https://dito-prod-japaneast.openai.azure.com/");
    type_cloud(cx, "model", "gpt-6.1-terra");

    step(cx, "select", "compat", "");
    type_cloud(cx, "address", "http://127.0.0.1:9/v1");
    step(cx, "test", "", "");
    assert_eq!(tested(cx).await, "failed");
    page_shot(cx, dir, &shot_name("test-failed", "ja", tag)).await;
    type_cloud(cx, "address", "https://openrouter.ai/api/v1/chat/completions");
}

/// Saving fails for real: the temporary file the settings are written through is blocked by a directory.
async fn save_failure(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp, tag: &str) {
    let blocker = st.cfg.path().with_extension("toml.tmp");
    std::fs::create_dir_all(&blocker).expect("block the settings file's temporary name");
    step(cx, "select", "compat", "");
    type_cloud(cx, "name", "ふだん使い 2");
    assert!(probe(cx).save_failed, "a failed save shows the banner");
    assert_eq!(active(st).name, "ふだん使い", "and the settings are as they were");
    page_shot(cx, dir, &shot_name("save-failed", "ja", tag)).await;
    std::fs::remove_dir(&blocker).expect("unblock the settings file");
    type_cloud(cx, "name", "ふだん使い");
    assert!(!probe(cx).save_failed, "the next save that works clears the banner");

    std::fs::create_dir_all(&blocker).expect("block again");
    type_cloud(cx, "name", "ふだん使い 3");
    assert!(probe(cx).save_failed);
    step(cx, "close-banner", "", "");
    assert!(!probe(cx).save_failed, "Close dismisses the banner");
    std::fs::remove_dir(&blocker).expect("unblock the settings file");
    type_cloud(cx, "name", "ふだん使い");
    println!("verified: a failed save shows the banner; Close and the next good save clear it");
}

async fn deleting(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp, tag: &str) {
    step(cx, "select", "azure", "");
    step(cx, "trash", "", "");
    assert_eq!(probe(cx).deleting.as_deref(), Some("高精度"));
    page_shot(cx, dir, &shot_name("delete", "ja", tag)).await;
    on_settings_window(cx, |window, cx, _| {
        press_key(window, cx, "escape");
        redraw(window, cx);
    });
    assert_eq!(probe(cx).deleting, None, "Escape closes the dialog");
    assert_eq!(ids(st).0, ["compat", "azure", "bedrock"], "and nothing is deleted");

    step(cx, "trash", "", "");
    step(cx, "confirm-delete", "", "");
    assert_eq!(ids(st), (vec!["compat".to_owned(), "bedrock".to_owned()], "bedrock".to_owned()));
    assert_eq!(key_of(st, "azure"), None, "the Keychain key goes with the configuration");
    assert_eq!(key_of(st, "compat").as_deref(), Some("sk-demo-0123456789abcdef"), "the other keys stay");
    assert_eq!(probe(cx).shown.as_deref(), Some("bedrock"), "the form follows the neighbour");
    page_shot(cx, dir, &shot_name("deleted", "ja", tag)).await;

    // Down to one: the trash is off, and a request to delete the last one is refused.
    step(cx, "trash", "", "");
    step(cx, "confirm-delete", "", "");
    step(cx, "trash", "", "");
    assert_eq!(probe(cx).deleting, None, "the last configuration cannot be deleted");
    assert_eq!(ids(st), (vec!["compat".to_owned()], "compat".to_owned()));
    page_shot(cx, dir, &shot_name("last", "ja", tag)).await;
    println!("verified: deleting removes the configuration and its key and selects the neighbour; the last one stays");
}

async fn local_parameters(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp, tag: &str) {
    services::save_config(st, |c| c.polish.engine.source = Source::Local);
    on_settings_window(cx, |window, cx, view| {
        view.update(cx, |s, cx| s.preview_models(Role::Polish, None, window, cx));
        redraw(window, cx);
    });
    step(cx, "local-states", "", "");
    let model = st.cfg.get().polish.engine.local_model.clone();
    type_local(cx, "temperature", "0.3");
    type_local(cx, "top_p", "0.9");
    type_local(cx, "top_k", "abc");
    type_local(cx, "max_tokens", "0");
    let saved = st.cfg.get().polish.engine.local_params_for(&model);
    assert_eq!(saved, LocalPolishParams { temperature: Some(0.3), top_p: Some(0.9), top_k: None, max_tokens: None });
    assert_eq!(probe(cx).local_invalid, ["top_k", "max_tokens"], "the local fields flag what does not parse");
    page_shot(cx, dir, &shot_name("local-invalid", "ja", tag)).await;

    type_local(cx, "top_k", "40");
    type_local(cx, "max_tokens", "");
    type_local(cx, "temperature", "-1");
    let saved = st.cfg.get().polish.engine.local_params_for(&model);
    assert_eq!(
        saved,
        LocalPolishParams { temperature: Some(0.3), top_p: Some(0.9), top_k: Some(40), max_tokens: None }
    );
    type_local(cx, "temperature", "0.3");
    page_shot(cx, dir, &shot_name("local", "ja", tag)).await;

    // Another model in use: the fields follow it, and the first model keeps its parameters.
    let other = probe(cx).local_models.into_iter().find(|id| *id != model).expect("a second polish model");
    services::save_config(st, |c| c.polish.engine.local_model = other.clone());
    draw(cx);
    let state = probe(cx);
    assert_eq!(state.local_shown.as_deref(), Some(other.as_str()));
    assert!(["temperature", "top_p", "top_k", "max_tokens"].iter().all(|field| state.local_text(field).is_empty()));
    assert_eq!(st.cfg.get().polish.engine.local_params_for(&model).top_k, Some(40));
    services::save_config(st, |c| c.polish.engine.local_model = model.clone());
    draw(cx);
    assert_eq!(probe(cx).local_text("top_k"), "40", "coming back shows what was saved for that model");
    for field in ["temperature", "top_p", "top_k"] {
        type_local(cx, field, "");
    }
    assert!(st.cfg.get().polish.engine.local_params.is_empty(), "parameters that set nothing are not kept");
    type_local(cx, "temperature", "0.3");
    services::save_config(st, |c| c.polish.engine.source = Source::Provider);
    println!("verified: local parameters are saved per model, follow the model in use and flag what does not parse");
}

/// The form in English and Chinese.
async fn languages(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp, tag: &str) {
    step(cx, "select", "compat", "");
    for (language, name) in [(DisplayLanguage::En, "en"), (DisplayLanguage::Zh, "zh")] {
        services::save_config(st, |c| c.general.display_language = language);
        page_shot(cx, dir, &shot_name("compat", name, tag)).await;
    }
    services::save_config(st, |c| c.general.display_language = DisplayLanguage::Ja);
}
