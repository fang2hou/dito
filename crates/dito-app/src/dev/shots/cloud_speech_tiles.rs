//! The cloud speech tab with its configuration tiles, driven the way a person drives it: simulated pointer events on the
//! tiles, the plus menu and the trash button, typing through the input handler. It checks what each action did to the
//! saved settings, the key store and the editor's state per configuration, and takes the shots that `20.05.1`, `20.05.5`
//! and `20.05.11` are compared with. Opt-in: `DITO_SHOTS_ONLY=cloudspeechtiles`, with `--mock --data-dir <dir>`.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use dito_core::config::{Appearance, CloudAsrConfigurations, CloudAsrTemplate, ConfigStore, DisplayLanguage, Source};
use dito_core::i18n::{Lang, Tr};
use dito_core::secrets::{Role, asr_account};
use gpui_kit::*;
use serde_json::Value;

use super::capture::{page_shot, resize_settings_window, theme_tag};
use super::inline_asr::{command, settle, with_editor};
use super::input::{click, drag_to, hover, on_settings_window, press, press_key, redraw, release};
use crate::services;
use crate::settings::asr_editor::{AsrEditor, CloudAsrDraftState, CloudAsrFormState, EditorCommand};
use crate::settings::{self, Page};
use crate::state::AppState;
use crate::ui::TestState;

/// The design's 1120 wide window has a 1 pt frame on each side. The shots use the window without it, so the card is
/// the 774 wide of `20.05.1` and its rows line up with the reference.
const WIDTH: f32 = 1118.;
const HEIGHT: f32 = 1100.;
/// The compact window of `20.05.5`: 900 wide is the smallest the settings window gets. Below 1024 its sidebar is the
/// icon rail, which the design's compact window does not show, so the card is wider than the design's 554 and the row
/// overflows by 62 instead of 220.
const COMPACT: (f32, f32) = (900., 818.);
/// The narrowest window that still has the full sidebar, as the design's compact window has: the row overflows by 94.
const COMPACT_WITH_SIDEBAR: (f32, f32) = (1024., 818.);

/// Left edge of the card's content: 232 sidebar, 56 page padding, 1 border and 26 padding.
const LEFT: f32 = 315.;
/// The tile row's scrolling area; the 36 pt plus button follows after a 12 pt gap.
const VIEW: f32 = 672.;
/// Every tile is 216 wide, left-aligned, and the next one starts a 12 gap later, whatever the number of tiles.
const TILE_W: f32 = 216.;
const TILE_PITCH: f32 = TILE_W + 12.;
/// Top of the tile row.
const ROW_TOP: f32 = 277.;
const ROW_CENTRE: f32 = ROW_TOP + 32.;
const API_KEY: &str = "sk-test-0123456789ab";

/// The plus button, and the rows of the menu that hangs 8 pt under it: a 28 caption, three 34 templates with the 2 pt gaps
/// of the panel, a 9 rule and the duplicate row.
fn plus() -> Point<Pixels> {
    point(px(LEFT + VIEW + 12. + 18.), px(ROW_CENTRE))
}

fn menu_row(index: usize) -> Point<Pixels> {
    let panel_top = ROW_CENTRE - 18. + 36. + 8.;
    let first_row = panel_top + 1. + 6. + 28. + 2.;
    let y = match index {
        0..=2 => first_row + index as f32 * 36. + 17.,
        _ => first_row + 3. * 36. + 9. + 2. + 17.,
    };
    point(px(LEFT + VIEW + 12. + 36. - 120.), px(y))
}

/// The trash button, left of the test button in the footer.
fn trash() -> Point<Pixels> {
    point(px(798.), px(990.))
}

/// The answers of the delete dialog: it is centred in the window, its 176 pt height ends in a row of 36 pt buttons
/// 28 pt above its bottom edge, and the answers sit at its right.
fn dialog_confirm() -> Point<Pixels> {
    point(px(WIDTH / 2. + 252.), px(HEIGHT / 2. + 42.))
}

fn dialog_cancel() -> Point<Pixels> {
    point(px(WIDTH / 2. + 131.), px(HEIGHT / 2. + 42.))
}

/// Where the tile `index` is centred in the row scrolled to `offset`.
fn tile(index: usize, offset: f32) -> Point<Pixels> {
    point(px(LEFT + index as f32 * TILE_PITCH + TILE_W / 2. - offset), px(ROW_CENTRE))
}

fn read<R>(cx: &mut AsyncApp, f: impl FnOnce(&AsrEditor, &Window, &App) -> R) -> R {
    let mut out = None;
    with_editor(cx, |editor, window, cx| out = Some(f(editor.read(cx), window, cx)));
    out.expect("the editor answered")
}

fn model_of(config: &Value) -> &str {
    config["connection"]["model"].as_str().unwrap_or("")
}

/// The speech configurations as saved: id, name and model, in order, and the selected id.
fn saved(st: &AppState) -> (Vec<(String, String, String)>, String) {
    let config = st.cfg.get();
    let list = config.asr.cloud_asr.clone().unwrap_or_default();
    let rows = list.items.iter().map(|p| (p.id.clone(), p.name.clone(), model_of(&p.config).to_owned())).collect();
    (rows, list.selected)
}

fn ids(st: &AppState) -> Vec<String> {
    saved(st).0.into_iter().map(|(id, ..)| id).collect()
}

fn config_of(st: &AppState, id: &str) -> Value {
    let list = st.cfg.get().asr.cloud_asr.clone().unwrap_or_default();
    list.items.iter().find(|p| p.id == id).map(|p| p.config.clone()).unwrap_or_else(|| panic!("no configuration {id}"))
}

struct Rig<'a> {
    st: &'a Arc<AppState>,
    dir: &'a Path,
    cx: &'a mut AsyncApp,
}

impl Rig<'_> {
    async fn wait(&mut self, ms: u64) {
        self.cx.background_executor().timer(Duration::from_millis(ms)).await;
    }

    async fn shot(&mut self, name: &str) {
        page_shot(self.cx, self.dir, name).await;
    }

    fn click(&mut self, at: Point<Pixels>) {
        on_settings_window(self.cx, |window, cx, _| click(window, cx, at));
    }

    /// Turn the wheel over the tile row.
    fn scroll(&mut self, dx: f32, dy: f32) {
        on_settings_window(self.cx, |window, cx, _| {
            let at = point(px(LEFT + VIEW / 2.), px(ROW_CENTRE));
            hover(window, cx, at);
            let delta = ScrollDelta::Pixels(point(px(dx), px(dy)));
            window.dispatch_event(PlatformInput::Scroll(ScrollEvent { position: at, delta, ..Default::default() }), cx);
            redraw(window, cx);
        });
    }

    fn command(&mut self, what: EditorCommand) {
        command(self.cx, what);
    }

    fn escape(&mut self) {
        on_settings_window(self.cx, |window, cx, _| {
            press_key(window, cx, "escape");
            redraw(window, cx);
        });
    }

    /// Click the trash and answer the dialog that opens: with Delete, or with Cancel.
    async fn delete_shown(&mut self, confirm: bool) {
        let shown = self.form().name;
        self.click(trash());
        self.wait(300).await;
        assert_eq!(self.form().deleting, Some(shown), "the trash asks first, about the configuration it shows");
        self.click(if confirm { dialog_confirm() } else { dialog_cancel() });
        self.wait(400).await;
        assert_eq!(self.form().deleting, None, "the dialog closed");
    }

    fn form(&mut self) -> CloudAsrFormState {
        read(self.cx, |editor, window, cx| editor.probe_form(window, cx))
    }

    fn drafts(&mut self) -> Vec<CloudAsrDraftState> {
        read(self.cx, |editor, _, cx| editor.probe_drafts(cx))
    }

    fn draft(&mut self, id: &str) -> CloudAsrDraftState {
        self.drafts().into_iter().find(|draft| draft.id == id).unwrap_or_else(|| panic!("no draft for {id}"))
    }

    fn offset(&mut self) -> f32 {
        read(self.cx, |editor, _, _| editor.probe_tiles().0)
    }

    /// Replace the request JSON of the configuration the form shows, as a paste would.
    fn replace(&mut self, change: impl FnOnce(&mut Value)) {
        let (text, _, _) = read(self.cx, |editor, _, cx| editor.probe_state(cx));
        let mut value: Value = serde_json::from_str(&text).expect("the draft is JSON");
        change(&mut value);
        let text = serde_json::to_string_pretty(&value).expect("JSON serializes");
        with_editor(self.cx, |editor, window, cx| {
            editor.update(cx, |editor, cx| editor.probe_replace(text, window, cx))
        });
    }

    /// Start from three configurations: the bundled templates with the models of the design's `20.05.1`, Qwen ASR
    /// selected and holding a key. Rebuilds the editor so that it starts from them.
    async fn start(&mut self, appearance: Appearance, width: f32, height: f32) {
        let mut configurations = CloudAsrConfigurations::default();
        for (id, model) in
            [("funasr", "paraformer-v2"), ("qwen_asr", "my-asr-model"), ("openrouter", "openai/whisper-large-v3")]
        {
            let configuration = configurations.items.iter_mut().find(|p| p.id == id).expect("a bundled template");
            configuration.config["connection"]["model"] = model.into();
        }
        configurations.select("qwen_asr");
        for account in ["funasr", "qwen_asr", "openrouter"].map(asr_account) {
            self.st.secrets.delete(&account).expect("isolated mock credentials");
        }
        self.st.secrets.set(&asr_account("qwen_asr"), API_KEY).expect("isolated mock credentials");
        self.st.factory.invalidate_engines();
        services::save_config(self.st, |config| {
            config.general.appearance = appearance;
            config.general.display_language = DisplayLanguage::Ja;
            config.asr.source = Source::Provider;
            config.asr.cloud_asr = Some(configurations);
        });
        self.cx.update(|cx| {
            settings::open(self.st.clone(), Page::Models, cx);
            resize_settings_window(cx, width, height);
            let (window, view) = settings::handle(cx).expect("settings window");
            window
                .update(cx, |_, window, cx| {
                    view.update(cx, |view, cx| view.preview_models(Role::Asr, None, window, cx))
                })
                .expect("settings window update");
        });
        self.wait(300).await;
    }

    fn resize(&mut self, (width, height): (f32, f32)) {
        self.cx.update(|cx| resize_settings_window(cx, width, height));
    }

    /// Drag the tile under `from` to `to` in steps, the way the tile row's own shots do.
    async fn drag(&mut self, from: Point<Pixels>, to: Point<Pixels>) {
        on_settings_window(self.cx, |window, cx, _| {
            hover(window, cx, from);
            press(window, cx, from);
        });
        for step in 1..=8 {
            let at = point(
                from.x + (to.x - from.x) * (step as f32 / 8.),
                from.y + (to.y - from.y) * (step as f32 / 8.) - px(6.),
            );
            on_settings_window(self.cx, |window, cx, _| drag_to(window, cx, at));
        }
        self.wait(350).await;
        on_settings_window(self.cx, |window, cx, _| release(window, cx, to));
        self.wait(400).await;
    }

    /// Click the plus button and the row `index` of its menu.
    async fn add_through_menu(&mut self, index: usize) {
        self.click(plus());
        self.wait(300).await;
        assert!(self.form().menu_open, "the plus button opens its menu");
        self.click(menu_row(index));
        self.wait(700).await;
    }

    /// A configuration was just added: the form shows it under the name `新しい設定`, focused with all of it selected.
    fn assert_added(&mut self, id: &str) {
        let new_name = Lang::Ja.tr(Tr::ModelsNewConfiguration);
        let (rows, selected) = saved(self.st);
        assert_eq!(rows.last().map(|(last, ..)| last.as_str()), Some(id), "the new tile joins the end");
        assert_eq!(rows.last().map(|(_, name, _)| name.as_str()), Some(new_name));
        assert_eq!(selected, id, "and is selected");
        let form = self.form();
        assert_eq!(form.shown, id);
        assert_eq!(form.name, new_name);
        assert!(form.name_focused, "the name field has the focus");
        assert_eq!(form.name_selection, 0..new_name.len(), "with all of its text selected");
        assert!(!form.menu_open, "the menu closed");
        assert_eq!(form.key, "", "a new configuration has no key");
        let draft = self.draft(id);
        assert_eq!(draft.key, "");
    }
}

/// The scene of the references in one appearance: the three tiles, the compact window, the open menu and the state
/// after adding.
async fn scene(rig: &mut Rig<'_>, appearance: Appearance) {
    let tag = theme_tag(appearance);
    rig.start(appearance, WIDTH, HEIGHT).await;
    rig.shot(&format!("speech-tiles-{tag}.png")).await;

    // The sidebar folds into the rail below 1024 and only unfolds again from 1088: go down the widths, not up.
    for (size, name, overflow) in [(COMPACT_WITH_SIDEBAR, "compact-sidebar", 94.), (COMPACT, "compact", 62.)] {
        rig.resize(size);
        rig.shot(&format!("speech-tiles-{name}-{tag}.png")).await;
        let (offset, furthest, _) = read(rig.cx, |editor, _, _| editor.probe_tiles());
        assert_eq!(
            (offset, furthest),
            (0., overflow),
            "three tiles of 216 in the {name} window overflow their row, scrolled to its start (right fade only)"
        );
    }
    rig.resize((WIDTH, HEIGHT));
    rig.wait(300).await;

    rig.click(plus());
    rig.wait(300).await;
    rig.shot(&format!("speech-tiles-menu-{tag}.png")).await;
    rig.click(menu_row(1));
    rig.wait(700).await;
    rig.shot(&format!("speech-tiles-added-{tag}.png")).await;

    // The delete dialog (`20.05.12`): Escape backs out and nothing is deleted.
    let before = saved(rig.st);
    let name = rig.form().name;
    rig.click(trash());
    rig.wait(300).await;
    assert_eq!(rig.form().deleting, Some(name), "the trash asks first, about the configuration it shows");
    rig.shot(&format!("speech-delete-{tag}.png")).await;
    rig.escape();
    assert_eq!(rig.form().deleting, None, "Escape closes the dialog");
    assert_eq!(saved(rig.st), before, "and nothing was deleted");
}

/// Run every action once against the tab and check the saved result. Selecting, adding from each template, duplicating,
/// renaming, reordering and deleting persist, and each configuration keeps its draft, key, history and test result.
async fn behavior(rig: &mut Rig<'_>, server: &str) {
    rig.start(Appearance::Light, WIDTH, HEIGHT).await;

    // The form starts on the selected configuration; only that one has a draft.
    let form = rig.form();
    assert_eq!((form.shown.as_str(), form.name.as_str(), form.key.as_str()), ("qwen_asr", "Qwen ASR", API_KEY));
    assert_eq!(rig.drafts().iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), ["qwen_asr"]);

    // A click on a tile selects it at once; its form shows its own name and key.
    rig.click(tile(0, 0.));
    rig.wait(200).await;
    assert_eq!(saved(rig.st).1, "funasr");
    let form = rig.form();
    assert_eq!((form.shown.as_str(), form.name.as_str(), form.key.as_str()), ("funasr", "FunASR", ""));
    assert_eq!(rig.drafts().iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), ["funasr", "qwen_asr"]);

    // Edits of one configuration stay with it: its JSON, key and history survive the others being selected.
    rig.command(EditorCommand::TypeKey("fun-key".into()));
    rig.replace(|config| config["connection"]["model"] = "fun-edited".into());
    settle(rig.cx).await;
    assert_eq!(model_of(&config_of(rig.st, "funasr")), "fun-edited", "an edit saves by itself");
    assert_eq!(rig.st.secrets.get(&asr_account("funasr")).as_deref(), Some("fun-key"));
    rig.click(tile(1, 0.));
    rig.wait(200).await;
    rig.replace(|config| config["connection"]["model"] = "qwen-edited".into());
    settle(rig.cx).await;
    rig.click(tile(0, 0.));
    rig.wait(200).await;
    let funasr = rig.draft("funasr");
    assert_eq!(
        (model_of(&serde_json::from_str(&funasr.text).unwrap()), funasr.key.as_str()),
        ("fun-edited", "fun-key")
    );
    assert_eq!(rig.form().key, "fun-key");
    rig.command(EditorCommand::Undo);
    settle(rig.cx).await;
    let funasr = rig.draft("funasr");
    assert_eq!(model_of(&serde_json::from_str(&funasr.text).unwrap()), "paraformer-v2", "Undo reverts this edit");
    assert_eq!(model_of(&config_of(rig.st, "funasr")), "paraformer-v2", "and the revert is saved");
    let qwen = rig.draft("qwen_asr");
    assert_eq!(
        model_of(&serde_json::from_str(&qwen.text).unwrap()),
        "qwen-edited",
        "the other history was not touched"
    );
    rig.click(tile(1, 0.));
    rig.wait(200).await;
    rig.command(EditorCommand::Undo);
    settle(rig.cx).await;
    assert_eq!(model_of(&config_of(rig.st, "qwen_asr")), "my-asr-model", "each configuration has a history of its own");

    // A test result belongs to its configuration.
    rig.replace(|config| {
        config["connection"]["base_url"] = format!("{server}/v1").into();
        config["connection"]["model"] = "mock-model".into();
    });
    settle(rig.cx).await;
    rig.command(EditorCommand::Test);
    for _ in 0..50 {
        rig.wait(100).await;
        if !matches!(rig.draft("qwen_asr").test, TestState::Running) {
            break;
        }
    }
    assert!(
        matches!(rig.draft("qwen_asr").test, TestState::Ok(_)),
        "the mock endpoint answers: {:?}",
        rig.draft("qwen_asr").test
    );
    rig.click(tile(0, 0.));
    rig.wait(200).await;
    assert_eq!(rig.draft("funasr").test, TestState::Idle);
    assert!(matches!(rig.draft("qwen_asr").test, TestState::Ok(_)), "the result waits for its configuration");

    // Reordering saves the order on the drop and keeps every draft and key with its configuration.
    let before = saved(rig.st).1;
    rig.drag(tile(2, 0.), tile(0, 0.)).await;
    assert_eq!(ids(rig.st), ["openrouter", "funasr", "qwen_asr"]);
    assert_eq!(saved(rig.st).1, before, "dragging does not select");
    assert_eq!(rig.draft("funasr").key, "fun-key");
    assert_eq!(rig.st.secrets.get(&asr_account("funasr")).as_deref(), Some("fun-key"));
    assert!(matches!(rig.draft("qwen_asr").test, TestState::Ok(_)), "the result moved with its tile");
    assert_eq!(rig.form().shown, "funasr");
    rig.click(tile(1, 0.));
    rig.wait(200).await;
    assert_eq!(saved(rig.st).1, "funasr", "a plain click still selects after a drag");

    // Adding from each template: a tile named 新しい設定 joins the end, selected, shown, and its name is selected.
    let list_before = ids(rig.st);
    for (index, template) in CloudAsrTemplate::ALL.iter().enumerate() {
        rig.add_through_menu(index).await;
        let list = ids(rig.st);
        assert_eq!(list.len(), list_before.len() + index + 1);
        let added = list.last().expect("a configuration was added").clone();
        assert!(added.starts_with(&format!("{}-", template.id)), "{added} is made from {}", template.name);
        assert_eq!(config_of(rig.st, &added), template.config(), "it starts from the bundled request JSON");
        rig.assert_added(&added);
        let (offset, furthest, _) = read(rig.cx, |editor, _, _| editor.probe_tiles());
        if list.len() > 3 {
            assert_eq!(offset, furthest, "the new tile is scrolled into view at the end of the row");
        }
    }
    assert_eq!(ids(rig.st).len(), 6);
    rig.shot("speech-tiles-six.png").await;

    // Renaming saves as it is typed: the name is selected right after adding, so typing replaces it. A blank name is
    // not saved, and the field goes back to the saved one when it loses focus.
    let added = saved(rig.st).1;
    rig.command(EditorCommand::TypeName("Mine".into()));
    assert_eq!(saved(rig.st).0.last().unwrap().1, "Mine", "the tile follows while typing");
    rig.command(EditorCommand::SelectName);
    rig.command(EditorCommand::TypeName(String::new()));
    assert_eq!(rig.form().name, "");
    assert_eq!(saved(rig.st).0.last().unwrap().1, "Mine", "an empty name is not saved");
    rig.command(EditorCommand::LeaveName);
    rig.wait(100).await;
    let form = rig.form();
    assert_eq!((form.name.as_str(), form.name_focused), ("Mine", false), "it returns to the last saved name");
    rig.command(EditorCommand::SelectName);
    rig.command(EditorCommand::TypeName("   ".into()));
    rig.command(EditorCommand::LeaveName);
    rig.wait(100).await;
    assert_eq!(rig.form().name, "Mine", "a name of blanks counts as empty");

    // Validation is kept per configuration too: the new one has no model yet and says so, the others are fine, and
    // going to another tile and back changes neither.
    let new_draft = rig.draft(&added);
    assert!(new_draft.error.is_some() && !new_draft.pending, "a template without a model does not validate");
    rig.scroll(0., 1000.);
    rig.wait(200).await;
    rig.click(tile(2, 0.));
    rig.wait(300).await;
    assert_eq!(saved(rig.st).1, "qwen_asr");
    assert_eq!(rig.draft("qwen_asr").error, None);
    assert!(rig.draft(&added).error.is_some(), "the other configuration's error stays with it");
    rig.scroll(0., -1000.);
    rig.wait(200).await;
    let offset = rig.offset();
    rig.click(tile(5, offset));
    rig.wait(300).await;
    assert_eq!(saved(rig.st).1, added);

    // A key goes to the Keychain account of this configuration only, once its JSON is valid.
    assert_eq!(model_of(&config_of(rig.st, &added)), "", "the new configuration has no model yet");
    rig.command(EditorCommand::TypeKey("mine-key".into()));
    settle(rig.cx).await;
    assert_eq!(rig.st.secrets.get(&asr_account(&added)), None, "an invalid JSON waits for its model");
    rig.replace(|config| config["connection"]["model"] = "mine-model".into());
    settle(rig.cx).await;
    assert_eq!(rig.st.secrets.get(&asr_account(&added)).as_deref(), Some("mine-key"));
    assert_eq!(rig.st.secrets.get(&asr_account("funasr")).as_deref(), Some("fun-key"));

    // Duplicating copies what is typed, even before it was saved, and never the key.
    rig.replace(|config| config["connection"]["model"] = "typed-just-now".into());
    rig.click(plus());
    rig.wait(60).await;
    rig.click(menu_row(3));
    rig.wait(700).await;
    let copy = saved(rig.st).1;
    assert_ne!(copy, added);
    assert_eq!(model_of(&config_of(rig.st, &copy)), "typed-just-now", "the copy starts from the draft");
    assert_eq!(model_of(&config_of(rig.st, &added)), "typed-just-now", "and the draft was saved too");
    assert_eq!(rig.st.secrets.get(&asr_account(&copy)), None, "the copy has no key");
    assert_eq!(rig.st.secrets.get(&asr_account(&added)).as_deref(), Some("mine-key"));
    rig.assert_added(&copy);
    assert!(copy.starts_with(added.split('-').next().unwrap()), "a copy starts from the template of its source");
    assert_eq!(ids(rig.st).len(), 7);

    // Deleting removes the configuration with its key and draft; the neighbour is shown.
    let (rows, _) = saved(rig.st);
    let at = rows.iter().position(|(id, ..)| *id == added).expect("listed");
    let offset = rig.offset();
    rig.click(tile(at, offset));
    rig.wait(300).await;
    assert_eq!(saved(rig.st).1, added);
    assert_eq!(rig.form().key, "mine-key");
    rig.delete_shown(false).await;
    assert_eq!(saved(rig.st).0.len(), rows.len(), "Cancel keeps the configuration");
    rig.delete_shown(true).await;
    let (after, selected) = saved(rig.st);
    assert_eq!(after.len(), rows.len() - 1);
    assert!(after.iter().all(|(id, ..)| *id != added), "the configuration is gone");
    assert_eq!(selected, rows[at + 1].0, "the tile that followed took over");
    assert_eq!(rig.st.secrets.get(&asr_account(&added)), None, "its key went with it");
    assert!(rig.drafts().iter().all(|draft| draft.id != added), "and so did its draft");
    let form = rig.form();
    assert_eq!(form.shown, selected);
    assert_eq!(form.name, rows[at + 1].1);

    // Down to one: the trash is disabled and does nothing.
    while ids(rig.st).len() > 1 {
        rig.delete_shown(true).await;
    }
    assert_eq!(ids(rig.st).len(), 1);
    rig.shot("speech-tiles-single.png").await;
    let last = ids(rig.st);
    rig.click(trash());
    rig.wait(300).await;
    assert_eq!(rig.form().deleting, None, "the last configuration cannot be deleted");
    assert_eq!(ids(rig.st), last, "the last configuration stays");

    // A save that fails raises the banner above the card, and a save that works takes it down. The settings file is
    // written through a temporary file, which a directory of the same name blocks.
    let blocker = rig.st.cfg.path().with_extension("toml.tmp");
    std::fs::create_dir_all(&blocker).expect("block the settings file's temporary name");
    let before = saved(rig.st);
    rig.command(EditorCommand::SelectName);
    rig.command(EditorCommand::TypeName("Blocked".into()));
    assert!(rig.form().save_failed, "a rename that cannot be saved raises the banner");
    assert_eq!(saved(rig.st), before, "and nothing changed");
    rig.shot("speech-save-failed.png").await;
    std::fs::remove_dir(&blocker).expect("unblock the settings file");
    rig.command(EditorCommand::SelectName);
    rig.command(EditorCommand::TypeName(before.0[0].1.clone()));
    assert!(!rig.form().save_failed, "the next save that works takes the banner down");
    // A draft that cannot be saved raises it too, and stays valid: the banner says why it was not kept.
    std::fs::create_dir_all(&blocker).expect("block again");
    rig.replace(|config| config["connection"]["model"] = "unsaved".into());
    settle(rig.cx).await;
    assert!(rig.form().save_failed, "a draft that cannot be saved raises the banner");
    assert_eq!(rig.draft(&before.0[0].0).error, None, "the draft itself is fine");
    std::fs::remove_dir(&blocker).expect("unblock the settings file");
    rig.replace(|config| config["connection"]["model"] = before.0[0].2.clone().into());
    settle(rig.cx).await;
    assert!(!rig.form().save_failed);

    // Everything is in the file, not only in memory.
    let on_disk = ConfigStore::open(rig.st.cfg.path()).expect("the settings file reads back").get();
    assert_eq!(on_disk.asr.cloud_asr, rig.st.cfg.get().asr.cloud_asr);
    println!(
        "verified speech tiles: select, edit, undo, test, reorder, add from each template, rename, key, duplicate, delete"
    );
}

/// Opt-in: `DITO_SHOTS_ONLY=cloudspeechtiles`, with `--mock --data-dir <dir>`.
pub async fn run(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let server = st._mock_server.as_ref().expect("the cloud speech tile shots need --mock").url.clone();
    let original = st.cfg.get();
    let mut rig = Rig { st, dir, cx };
    for appearance in [Appearance::Light, Appearance::Dark] {
        scene(&mut rig, appearance).await;
    }
    behavior(&mut rig, &server).await;
    services::save_config(st, |config| *config = (*original).clone());
}
