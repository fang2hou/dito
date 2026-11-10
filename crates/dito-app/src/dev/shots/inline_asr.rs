//! Exercise real editor history, autosave and help ownership against the in-process cloud.

use std::{path::Path, sync::Arc, time::Duration};

use dito_core::{
    config::{DisplayLanguage, Source},
    provider::configured_asr::AsrProfiles,
    secrets::Role,
};
use gpui_kit::*;
use serde_json::Value;

use super::{
    capture::{page_shot, resize_settings_window},
    input::on_settings_window,
};
use crate::{
    services,
    settings::{
        self, Page,
        asr_editor::{AsrEditor, EditorCommand},
    },
    state::AppState,
    ui::TestState,
};

fn with_editor(cx: &mut AsyncApp, action: impl FnOnce(&Entity<AsrEditor>, &mut Window, &mut App)) {
    on_settings_window(cx, |window, cx, view| {
        let editor = view.read(cx).asr_config_editor(cx);
        action(&editor, window, cx);
    });
}

async fn settle(cx: &mut AsyncApp) {
    cx.background_executor().timer(Duration::from_millis(450)).await;
}

fn command(cx: &mut AsyncApp, command: EditorCommand) {
    with_editor(cx, |editor, window, cx| editor.update(cx, |editor, cx| editor.probe_command(command, window, cx)));
}

fn draft(cx: &mut AsyncApp) -> String {
    let mut value = String::new();
    with_editor(cx, |editor, _, cx| value = editor.read(cx).probe_state(cx).0);
    value
}

pub async fn run(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let server = st._mock_server.as_ref().expect("inline ASR verification requires --mock");
    let mut profiles = AsrProfiles { selected: 1, ..Default::default() };
    profiles.profiles[1]["connection"]["base_url"] = format!("{}/v1", server.url).into();
    profiles.profiles[1]["connection"]["model"] = "inline-before".into();
    st.secrets.set(&profiles.account(), "test-key").expect("isolated mock credentials");
    st.factory.invalidate_engines();
    services::save_config(st, |config| {
        config.general.display_language = DisplayLanguage::Ja;
        config.asr.source = Source::Provider;
        config.asr.cloud_asr = Some(profiles.clone());
    });
    cx.update(|cx| {
        settings::open(st.clone(), Page::Models, cx);
        resize_settings_window(cx, 1120., 1120.);
        let (window, view) = settings::handle(cx).unwrap();
        window
            .update(cx, |_, window, cx| view.update(cx, |view, cx| view.preview_models(Role::Asr, None, window, cx)))
            .unwrap();
    });
    page_shot(cx, dir, "inline-asr-before.png").await;
    let original = profiles.active().clone();
    with_editor(cx, |editor, window, cx| editor.update(cx, |editor, cx| editor.probe_replace("{".into(), window, cx)));
    settle(cx).await;
    assert_eq!(
        st.cfg.get().asr.cloud_asr.as_ref().unwrap().active(),
        &original,
        "invalid edits must retain the last valid configuration"
    );
    page_shot(cx, dir, "inline-asr-invalid.png").await;
    let mut valid = original.clone();
    valid["connection"]["model"] = "inline-after".into();
    with_editor(cx, |editor, window, cx| {
        editor.update(cx, |editor, cx| editor.probe_replace(serde_json::to_string_pretty(&valid).unwrap(), window, cx))
    });
    settle(cx).await;
    assert_eq!(
        st.cfg.get().asr.cloud_asr.as_ref().unwrap().active(),
        &valid,
        "valid edits autosave without a Save action"
    );
    command(cx, EditorCommand::Undo);
    settle(cx).await;
    assert_eq!(draft(cx), "{", "Undo must use the editor's real edit history");
    assert_eq!(st.cfg.get().asr.cloud_asr.as_ref().unwrap().active(), &valid);
    command(cx, EditorCommand::Redo);
    settle(cx).await;
    assert_eq!(serde_json::from_str::<Value>(&draft(cx)).unwrap(), valid);
    command(cx, EditorCommand::MapLocales);
    settle(cx).await;
    assert_eq!(st.cfg.get().asr.cloud_asr.as_ref().unwrap().active()["language_map"]["ja"], "ja-JP");
    command(cx, EditorCommand::Undo);
    settle(cx).await;
    assert_eq!(
        st.cfg.get().asr.cloud_asr.as_ref().unwrap().active(),
        &valid,
        "language shortcuts must also be undoable and autosave the restored value"
    );
    let before_help = draft(cx);
    command(cx, EditorCommand::OpenHelp);
    page_shot(cx, dir, "inline-asr-help.png").await;
    cx.update(|cx| resize_settings_window(cx, 1000., 680.));
    page_shot(cx, dir, "inline-asr-help-top.png").await;
    command(cx, EditorCommand::ScrollHelp(0.5));
    page_shot(cx, dir, "inline-asr-help-middle.png").await;
    command(cx, EditorCommand::ScrollHelp(1.));
    page_shot(cx, dir, "inline-asr-help-bottom.png").await;
    command(cx, EditorCommand::CloseHelp);
    cx.update(|cx| resize_settings_window(cx, 1120., 1120.));
    settle(cx).await;
    assert_eq!(draft(cx), before_help, "closing the sole help dialog must retain the editor and its draft");
    let saved_before_test = st.cfg.get().asr.cloud_asr.clone();
    command(cx, EditorCommand::Test);
    let mut result = TestState::Running;
    for _ in 0..50 {
        cx.background_executor().timer(Duration::from_millis(100)).await;
        with_editor(cx, |editor, _, cx| result = editor.read(cx).probe_state(cx).2);
        if result != TestState::Running {
            break;
        }
    }
    assert!(
        matches!(result, TestState::Ok(_)),
        "the current draft must reach the local mock connection test: {result:?}"
    );
    assert_eq!(st.cfg.get().asr.cloud_asr, saved_before_test, "testing must not mutate saved settings");
    page_shot(cx, dir, "inline-asr-connected.png").await;
    println!(
        "verified inline ASR autosave, invalid-draft retention, native Undo/Redo, language shortcut history, sole help dialog and draft connection test"
    );
}
