//! Shots compared with `design:settings/cloud-models/configurations`, `design:settings/cloud-models/failed-test`,
//! `design:settings/cloud-models/add-from-template`, `design:settings/cloud-models/delete-configuration` and
//! `design:settings/cloud-models/tile-row`.
//! Opt-in: `DITO_SHOTS_ONLY=textmodelform`, with `--mock --data-dir <dir>` so the settings file is known. Run with
//! `--lang en` to see that the stand-in names follow the display language, not the system's.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::config::{
    Appearance, CloudTextConfiguration, CloudTextConfigurations, CloudTextTemplate, ConfigStore, DisplayLanguage,
    LocalTextParams, Source,
};
use dito_core::i18n::{Lang, Tr};
use dito_core::secrets::{Role, text_account};
use gpui_kit::*;

use super::capture::{close_window, page_shot, resize_settings_window, theme_tag};
use super::input::{click, drag_to, hover, on_settings_window, press, press_key, redraw, release};
use crate::services;
use crate::settings::{self, ModelsProbe, Page};
use crate::state::AppState;

/// The design's 1120 wide window has a 1 pt frame on each side that the shots leave out, so the card is the 774 wide
/// of the references and its rows line up with them.
const WIDTH: f32 = 1118.;
const HEIGHT: f32 = 978.;
/// Left edge of the card's content: 232 sidebar, 56 page padding, 1 border and 26 padding.
const LEFT: f32 = 315.;
/// The tile row's scrolling area; the 36 pt plus button follows after a 12 pt gap.
const VIEW: f32 = 672.;
/// Every tile is 216 wide, left-aligned, and the next one starts a 12 gap later, whatever the number of tiles.
const TILE_W: f32 = 216.;
const TILE_PITCH: f32 = TILE_W + 12.;
/// Top of the tile row, and the middle of its 64 high tiles.
const ROW_TOP: f32 = 277.;
const ROW_CENTRE: f32 = ROW_TOP + 32.;
/// The buttons of the footer, 36 pt high: the trash, then the 140 wide test button. The name field starts 48 pt under
/// the tile row, the rule over the footer 342 pt under that, and the buttons 16 pt under the rule.
const FOOTER_CENTRE: f32 = ROW_TOP + 64. + 48. + 342. + 1. + 16. + 18.;

fn plus() -> Point<Pixels> {
    point(px(LEFT + VIEW + 12. + 18.), px(ROW_CENTRE))
}

/// A row of the menu that hangs 8 pt under the plus button: a 28 caption, three 36 templates and, after a 9 rule, the
/// duplicate row.
fn menu_row(index: usize) -> Point<Pixels> {
    let panel_top = ROW_CENTRE - 18. + 36. + 8.;
    let first_row = panel_top + 1. + 6. + 28. + 2.;
    let y = match index {
        0..=2 => first_row + index as f32 * 36. + 17.,
        _ => first_row + 3. * 36. + 9. + 2. + 17.,
    };
    point(px(LEFT + VIEW + 12. + 36. - 120.), px(y))
}

fn trash() -> Point<Pixels> {
    point(px(LEFT + VIEW + 12. + 36. - 140. - 10. - 18.), px(FOOTER_CENTRE))
}

fn test_button() -> Point<Pixels> {
    point(px(LEFT + VIEW + 12. + 36. - 70.), px(FOOTER_CENTRE))
}

/// The answers of the delete dialog: it is centred in the window, its 176 pt height ends in a row of 36 pt buttons
/// 28 pt above its bottom edge, and the answers sit at its right.
fn dialog_confirm() -> Point<Pixels> {
    point(px(WIDTH / 2. + 252.), px(HEIGHT / 2. + 42.))
}

fn dialog_cancel() -> Point<Pixels> {
    point(px(WIDTH / 2. + 131.), px(HEIGHT / 2. + 42.))
}

fn tile(index: usize, offset: f32) -> Point<Pixels> {
    point(px(LEFT + index as f32 * TILE_PITCH + TILE_W / 2. - offset), px(ROW_CENTRE))
}

/// Where a dragged tile is dropped in a row that scrolls: near its left edge, but outside the fade over it. A row that
/// scrolls gives the slot under the pointer by where the row is scrolled to.
const DROP_X: f32 = LEFT + 130.;

fn slot_at(x: f32, offset: f32, count: usize) -> usize {
    (((x - LEFT + offset) / TILE_PITCH).floor().max(0.) as usize).min(count - 1)
}

fn moved(ids: &[String], from: usize, to: usize) -> Vec<String> {
    let mut order = ids.to_vec();
    let tile = order.remove(from);
    order.insert(to, tile);
    order
}

fn active(st: &AppState) -> CloudTextConfiguration {
    let cfg = st.cfg.get();
    cfg.text_model.text_configurations(cfg.general.lang()).active().cloned().expect("one is selected")
}

fn saved(st: &AppState) -> CloudTextConfigurations {
    let cfg = st.cfg.get();
    cfg.text_model.text_configurations(cfg.general.lang()).into_owned()
}

fn ids(st: &AppState) -> Vec<String> {
    saved(st).items.into_iter().map(|config| config.id).collect()
}

fn key_of(st: &AppState, id: &str) -> Option<String> {
    st.secrets.get(&text_account(id))
}

fn shot_name(name: &str, language: &str, tag: &str) -> String {
    if language == "ja" { format!("polish-{name}-{tag}.png") } else { format!("polish-{name}-{language}-{tag}.png") }
}

/// The configurations of `design:settings/cloud-models/configurations`, in its order. `four` adds one that makes the
/// row overflow.
fn staged(four: bool) -> CloudTextConfigurations {
    let mut list = CloudTextConfigurations::new(Lang::Ja);
    for (id, name, address, model, temperature) in [
        ("compat", "ふだん使い", "https://openrouter.ai/api/v1/chat/completions", "openai/gpt-4o-mini", Some(0.2)),
        ("azure", "高精度", "https://dito-prod-japaneast.openai.azure.com/", "gpt-6.1-terra", None),
        ("bedrock", "Bedrock 検証", "ap-northeast-1", "apac.anthropic.claude-sonnet-4-20250514-v1:0", Some(0.3)),
    ] {
        let config = list.get_mut(id).expect("a starting configuration");
        config.name = name.into();
        *config.target.address_mut() = address.into();
        *config.target.model_mut() = model.into();
        config.params.temperature = temperature;
    }
    list.move_to("compat", 0);
    if four {
        let id = list.add_from_template(CloudTextTemplate::Compat, "長文用");
        *list.get_mut(&id).expect("just added").target.model_mut() = "openai/gpt-4.1-mini".into();
    }
    list.select("compat");
    list
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

    fn key(&mut self, keystroke: &'static str) {
        on_settings_window(self.cx, |window, cx, _| {
            press_key(window, cx, keystroke);
            redraw(window, cx);
        });
    }

    /// Followed by a frame: the page only follows the settings when it is drawn, as between a click and the typing
    /// that comes after it.
    fn step(&mut self, case: &'static str, arg: &str, text: &str) {
        on_settings_window(self.cx, |window, cx, view| {
            view.update(cx, |s, cx| s.probe_models(case, arg, text, window, cx));
            redraw(window, cx);
        });
    }

    fn type_cloud(&mut self, field: &'static str, text: &str) {
        self.step("type", field, text);
    }

    fn type_local(&mut self, field: &'static str, text: &str) {
        self.step("type-local", field, text);
    }

    fn draw(&mut self) {
        on_settings_window(self.cx, |window, cx, _| redraw(window, cx));
    }

    fn probe(&mut self) -> ModelsProbe {
        let mut out = None;
        on_settings_window(self.cx, |window, cx, view| out = Some(view.read(cx).models_probe(window, cx)));
        out.expect("settings window")
    }

    async fn tested(&mut self) -> &'static str {
        let start = Instant::now();
        loop {
            let state = self.probe().test;
            if state != "idle" && state != "running" {
                return state;
            }
            assert!(start.elapsed() < Duration::from_secs(15), "the connection test never finished");
            self.wait(100).await;
        }
    }

    /// A `list` of `None` means nothing saved, where the templates stand in. The page is rebuilt so that nothing
    /// typed, opened or scrolled before is left in it.
    async fn start(
        &mut self,
        appearance: Appearance,
        language: DisplayLanguage,
        list: Option<CloudTextConfigurations>,
        size: (f32, f32),
    ) {
        for config in &saved(self.st).items {
            self.st.secrets.delete(&config.account()).expect("isolated mock credentials");
        }
        for (id, key) in [("compat", "sk-demo-0123456789abcdef"), ("azure", "azure-demo-key-0123456789")] {
            self.st.secrets.set(&text_account(id), key).expect("isolated mock credentials");
        }
        self.st.factory.invalidate_engines();
        services::save_config(self.st, |c| {
            c.general.appearance = appearance;
            c.general.display_language = language;
            c.text_model.source = Source::Provider;
            c.text_model.local_params.clear();
            c.text_model.cloud_text = list;
        });
        self.cx.update(|cx| {
            settings::open(self.st.clone(), Page::Models, cx);
            resize_settings_window(cx, size.0, size.1);
        });
        on_settings_window(self.cx, |window, cx, view| {
            view.update(cx, |s, cx| s.preview_models(Role::Text, None, window, cx));
            redraw(window, cx);
        });
        self.wait(300).await;
    }

    /// `lifted` names a shot taken while the tile is in the air.
    async fn drag(&mut self, from: Point<Pixels>, to: Point<Pixels>, lifted: Option<&str>) {
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
        if let Some(name) = lifted {
            assert!(self.probe().tiles.2.is_some(), "a tile is in the air");
            self.shot(name).await;
        }
        on_settings_window(self.cx, |window, cx, _| release(window, cx, to));
        self.wait(400).await;
    }

    async fn add_through_menu(&mut self, index: usize) {
        self.click(plus());
        self.wait(300).await;
        assert!(self.probe().menu_open, "the plus button opens its menu");
        self.click(menu_row(index));
        self.wait(700).await;
    }

    fn assert_added(&mut self, before: &[String]) -> String {
        let new_name = Lang::Ja.tr(Tr::ModelsNewConfiguration);
        let list = saved(self.st);
        assert_eq!(list.items.len(), before.len() + 1);
        assert_eq!(
            list.items[..before.len()].iter().map(|c| &c.id).collect::<Vec<_>>(),
            before.iter().collect::<Vec<_>>()
        );
        let added = list.items.last().expect("a configuration was added").clone();
        assert_eq!(list.selected, added.id, "the new tile is selected");
        assert_eq!(added.name, new_name);
        assert_eq!(key_of(self.st, &added.id), None, "a new configuration has no key");
        let state = self.probe();
        assert_eq!(state.shown.as_deref(), Some(added.id.as_str()), "the form shows it");
        assert_eq!(state.text("name"), new_name);
        assert!(state.name_focused, "the name field has the focus");
        assert_eq!(state.name_selection, 0..new_name.len(), "with all of its text selected");
        assert!(!state.menu_open, "the menu closed");
        assert_eq!(state.text("key"), "");
        added.id
    }

    async fn delete_shown(&mut self, confirm: bool) {
        let name = active(self.st).name;
        self.click(trash());
        self.wait(300).await;
        assert_eq!(self.probe().deleting, Some(name), "the trash asks first");
        self.click(if confirm { dialog_confirm() } else { dialog_cancel() });
        self.wait(400).await;
        assert_eq!(self.probe().deleting, None, "the dialog closed");
    }
}

async fn scene(rig: &mut Rig<'_>, appearance: Appearance) {
    let tag = theme_tag(appearance);
    rig.start(appearance, DisplayLanguage::Ja, Some(staged(false)), (WIDTH, HEIGHT)).await;
    rig.shot(&shot_name("tiles", "ja", tag)).await;

    rig.start(appearance, DisplayLanguage::Ja, Some(staged(true)), (WIDTH, HEIGHT)).await;
    let state = rig.probe();
    assert_eq!((state.tiles.0, state.tiles.1 > 0.), (0., true), "four tiles of 216 overflow their row");
    rig.shot(&shot_name("tiles-four", "ja", tag)).await;

    rig.click(plus());
    rig.wait(300).await;
    assert!(rig.probe().menu_open);
    rig.shot(&shot_name("tiles-menu", "ja", tag)).await;
    let before = ids(rig.st);
    rig.click(menu_row(1));
    rig.wait(700).await;
    rig.assert_added(&before);
    rig.shot(&shot_name("tiles-added", "ja", tag)).await;

    let before = ids(rig.st);
    rig.add_through_menu(3).await;
    rig.assert_added(&before);
    rig.shot(&shot_name("tiles-duplicated", "ja", tag)).await;

    let offset = rig.probe().tiles.0;
    let before = ids(rig.st);
    let count = before.len();
    let to = point(px(DROP_X), px(ROW_CENTRE));
    rig.drag(tile(count - 1, offset), to, Some(&shot_name("tiles-dragging", "ja", tag))).await;
    let slot = slot_at(DROP_X, offset, count);
    assert_eq!(ids(rig.st), moved(&before, count - 1, slot), "the dragged tile took the slot it was dropped in");
    rig.shot(&shot_name("tiles-reordered", "ja", tag)).await;
}

async fn behavior(rig: &mut Rig<'_>, server: &str) {
    // A new window builds a new page, which has read no key yet when it first shows a configuration.
    if let Some((window, _)) = rig.cx.update(|cx| settings::handle(cx)) {
        close_window(rig.cx, window);
    }
    rig.start(Appearance::Light, DisplayLanguage::Ja, Some(staged(false)), (WIDTH, HEIGHT)).await;
    let st = rig.st.clone();

    let state = rig.probe();
    assert_eq!((state.shown.as_deref(), state.text("name").as_str()), (Some("compat"), "ふだん使い"));
    assert_eq!(
        state.text("key"),
        "sk-demo-0123456789abcdef",
        "the configuration shown on opening gets its key once the Keychain read lands"
    );
    rig.click(tile(1, 0.));
    rig.wait(200).await;
    assert_eq!(saved(&st).selected, "azure");
    let state = rig.probe();
    assert_eq!((state.shown.as_deref(), state.text("name").as_str()), (Some("azure"), "高精度"));
    assert_eq!(state.text("key"), "azure-demo-key-0123456789", "each configuration shows its own key");
    rig.click(tile(0, 0.));
    rig.wait(200).await;
    assert_eq!(rig.probe().text("key"), "sk-demo-0123456789abcdef");

    rig.type_cloud("name", "ふだん使い 改");
    rig.type_cloud("address", "https://openrouter.ai/api/v1/chat/completions");
    rig.type_cloud("model", "openai/gpt-4o");
    rig.type_cloud("key", "sk-demo-changed");
    rig.type_cloud("temperature", "0.5");
    let compat = active(&st);
    assert_eq!(
        (compat.id.as_str(), compat.name.as_str(), compat.target.model()),
        ("compat", "ふだん使い 改", "openai/gpt-4o")
    );
    assert_eq!(compat.params.temperature, Some(0.5));
    assert_eq!(key_of(&st, "compat").as_deref(), Some("sk-demo-changed"), "the key goes to the Keychain");
    assert_eq!(key_of(&st, "azure").as_deref(), Some("azure-demo-key-0123456789"), "one key per configuration");
    rig.type_cloud("name", "ふだん使い");
    rig.type_cloud("model", "openai/gpt-4o-mini");
    rig.type_cloud("key", "sk-demo-0123456789abcdef");
    rig.type_cloud("temperature", "0.2");
    println!("verified: a click selects, the form follows, typing saves as it changes, one key per configuration");

    invalid_texts(rig).await;
    connection_tests(rig, server).await;
    save_failure(rig).await;
    adding_and_reordering(rig).await;
    deleting(rig).await;
    key_after_delete(rig).await;
    local_parameters(rig).await;
}

async fn invalid_texts(rig: &mut Rig<'_>) {
    let st = rig.st.clone();
    rig.type_cloud("temperature", "abc");
    rig.type_cloud("top_p", "1.5");
    rig.type_cloud("max_tokens", "0");
    rig.type_cloud("extra", "[1]");
    let params = active(&st).params;
    assert_eq!(params.temperature, Some(0.2), "the last valid temperature stays in use");
    assert_eq!((params.top_p, params.max_tokens), (None, None));
    assert!(params.extra.is_empty());
    assert_eq!(rig.probe().invalid, ["temperature", "top_p", "max_tokens", "extra"]);
    rig.shot("polish-invalid-light.png").await;

    rig.type_cloud("temperature", "-0.5");
    rig.type_cloud("top_p", "-0.1");
    assert_eq!(active(&st).params.temperature, Some(0.2), "a negative temperature is not saved");
    assert_eq!(rig.probe().invalid[..2], ["temperature", "top_p"]);

    rig.type_cloud("temperature", "0.7");
    rig.type_cloud("top_p", "0.95");
    rig.type_cloud("max_tokens", "2048");
    rig.type_cloud("effort", "high");
    rig.type_cloud("extra", r#"{"seed": 42}"#);
    let params = active(&st).params;
    assert_eq!((params.temperature, params.top_p, params.max_tokens), (Some(0.7), Some(0.95), Some(2048)));
    assert_eq!(params.reasoning_effort.as_str(), "high");
    assert_eq!(params.extra["seed"], 42);
    assert!(rig.probe().invalid.is_empty(), "the borders are gone as soon as the texts parse");
    rig.shot("polish-parameters-light.png").await;

    for field in ["temperature", "top_p", "max_tokens", "effort", "extra"] {
        rig.type_cloud(field, "");
    }
    let params = active(&st).params;
    assert_eq!((params.temperature, params.top_p, params.max_tokens), (None, None, None));
    assert!(params.extra.is_empty() && params.reasoning_effort.as_str().is_empty());
    rig.type_cloud("temperature", "0.2");

    rig.type_cloud("name", "");
    assert_eq!(active(&st).name, "ふだん使い", "an empty name is not saved");
    assert_eq!(rig.probe().text("name"), "");
    rig.step("blur-name", "", "");
    assert_eq!(rig.probe().text("name"), "ふだん使い", "the name returns when focus leaves");
    println!("verified: invalid texts stay out of the settings and are flagged; empty fields clear; the name returns");
}

async fn connection_tests(rig: &mut Rig<'_>, server: &str) {
    rig.click(tile(1, 0.));
    rig.wait(200).await;
    rig.type_cloud("address", server);
    rig.click(test_button());
    assert_eq!(rig.tested().await, "ok", "the mock server accepts the configuration");
    rig.shot("polish-test-ok-light.png").await;
    rig.type_cloud("model", "gpt-6.1-terra-2");
    assert_eq!(rig.probe().test, "idle", "a changed setting invalidates the result");
    rig.type_cloud("address", "https://dito-prod-japaneast.openai.azure.com/");
    rig.type_cloud("model", "gpt-6.1-terra");

    rig.click(tile(0, 0.));
    rig.wait(200).await;
    rig.type_cloud("address", "http://127.0.0.1:9/v1");
    rig.click(test_button());
    assert_eq!(rig.tested().await, "failed");
    rig.shot("polish-test-failed-light.png").await;
    rig.type_cloud("address", "https://openrouter.ai/api/v1/chat/completions");
}

/// The settings are written through a temporary file, which a directory of the same name blocks.
async fn save_failure(rig: &mut Rig<'_>) {
    let st = rig.st.clone();
    let blocker = st.cfg.path().with_extension("toml.tmp");
    std::fs::create_dir_all(&blocker).expect("block the settings file's temporary name");
    rig.type_cloud("name", "ふだん使い 2");
    assert!(rig.probe().save_failed, "a failed save shows the banner");
    assert_eq!(active(&st).name, "ふだん使い", "and the settings are as they were");
    rig.shot("polish-save-failed-light.png").await;
    std::fs::remove_dir(&blocker).expect("unblock the settings file");
    rig.type_cloud("name", "ふだん使い");
    assert!(!rig.probe().save_failed, "the next save that works clears the banner");

    std::fs::create_dir_all(&blocker).expect("block again");
    rig.type_cloud("name", "ふだん使い 3");
    assert!(rig.probe().save_failed);
    // The banner's Close button sits at the right end of the banner, above the card.
    rig.click(point(px(1010.), px(206.)));
    rig.wait(200).await;
    assert!(!rig.probe().save_failed, "Close dismisses the banner");

    rig.type_cloud("name", "ふだん使い 4");
    assert!(rig.probe().save_failed);
    std::fs::remove_dir(&blocker).expect("unblock the settings file");
    rig.step("source", "local", "");
    assert_eq!(st.cfg.get().text_model.source, Source::Local, "the switch was saved");
    assert!(!rig.probe().save_failed, "a switch that works takes down the banner of an earlier failed save");
    std::fs::create_dir_all(&blocker).expect("block again");
    rig.step("source", "cloud", "");
    assert_eq!(st.cfg.get().text_model.source, Source::Local, "a switch that cannot be saved changes nothing");
    assert!(rig.probe().save_failed, "and raises the banner");
    std::fs::remove_dir(&blocker).expect("unblock the settings file");
    rig.step("source", "cloud", "");
    assert_eq!(st.cfg.get().text_model.source, Source::Provider);
    assert!(!rig.probe().save_failed);
    rig.type_cloud("name", "ふだん使い");
    println!("verified: a failed save shows the banner; Close and the next good save clear it, from any control");
}

async fn adding_and_reordering(rig: &mut Rig<'_>) {
    let st = rig.st.clone();
    let start = ids(&st);
    for (index, template) in CloudTextTemplate::ALL.iter().enumerate() {
        let before = ids(&st);
        rig.add_through_menu(index).await;
        let id = rig.assert_added(&before);
        assert!(id.starts_with(&format!("{}-", template.id())), "{id} is made from {}", template.id());
        let added = active(&st);
        assert_eq!(added.target.template(), *template);
        assert_eq!(added.params, template.configuration("x", "x").params, "it starts from the template's parameters");
        let state = rig.probe();
        if before.len() + 1 > 3 {
            assert_eq!(state.tiles.0, state.tiles.1, "the new tile is scrolled into view at the end of the row");
        }
    }
    assert_eq!(ids(&st).len(), start.len() + 3);
    rig.shot("polish-tiles-six-light.png").await;

    rig.type_cloud("name", "Mine");
    rig.type_cloud("model", "mine-model");
    rig.type_cloud("key", "mine-key");
    let mine = active(&st);
    assert_eq!((mine.name.as_str(), mine.target.model()), ("Mine", "mine-model"));
    assert_eq!(key_of(&st, &mine.id).as_deref(), Some("mine-key"));
    assert_eq!(rig.probe().text("name"), "Mine");

    let before = ids(&st);
    rig.add_through_menu(3).await;
    let copy = rig.assert_added(&before);
    let (copied, source) = (active(&st), saved(&st).get(&mine.id).cloned().expect("the source stays"));
    assert_eq!((copied.target.clone(), copied.params.clone()), (source.target.clone(), source.params.clone()));
    assert_eq!(key_of(&st, &copy), None, "the copy has no key");
    assert_eq!(key_of(&st, &mine.id).as_deref(), Some("mine-key"), "and the source keeps its own");
    assert!(
        copy.starts_with(mine.id.split('-').next().expect("an id")),
        "a copy starts from the template of its source"
    );
    rig.shot("polish-tiles-copy-light.png").await;

    let (offset, selected) = (rig.probe().tiles.0, saved(&st).selected);
    let before = ids(&st);
    // The row is scrolled to its end, where the last tile sits under the fade: take the one before it.
    let from = before.len() - 2;
    rig.drag(tile(from, offset), point(px(DROP_X), px(ROW_CENTRE)), None).await;
    let after = ids(&st);
    let slot = slot_at(DROP_X, offset, before.len());
    assert_ne!(slot, from, "the drop is in another slot");
    assert_eq!(after, moved(&before, from, slot), "the dragged tile took the slot it was dropped in");
    assert_eq!(saved(&st).selected, selected, "dragging does not select");
    assert_eq!(key_of(&st, &mine.id).as_deref(), Some("mine-key"), "keys stay with their configurations");
    let offset = rig.probe().tiles.0;
    let middle = (((offset + VIEW / 2. - TILE_W / 2.) / TILE_PITCH).round() as usize).min(after.len() - 1);
    rig.click(tile(middle, offset));
    rig.wait(300).await;
    assert_eq!(saved(&st).selected, after[middle], "a click after a drag still selects");

    let on_disk = ConfigStore::open(st.cfg.path()).expect("the settings file reads back").get();
    assert_eq!(on_disk.text_model.cloud_text, st.cfg.get().text_model.cloud_text);
    println!(
        "verified: adding from each template, duplicating without the key, reordering, the scroll to the new tile"
    );
}

async fn deleting(rig: &mut Rig<'_>) {
    let st = rig.st.clone();
    rig.start(Appearance::Light, DisplayLanguage::Ja, Some(staged(false)), (WIDTH, HEIGHT)).await;
    rig.click(tile(1, 0.));
    rig.wait(200).await;
    assert_eq!(saved(&st).selected, "azure");
    rig.click(trash());
    rig.wait(300).await;
    assert_eq!(rig.probe().deleting.as_deref(), Some("高精度"));
    rig.shot("polish-delete-light.png").await;
    rig.key("escape");
    assert_eq!(rig.probe().deleting, None, "Escape closes the dialog");
    rig.delete_shown(false).await;
    assert_eq!(ids(&st), ["compat", "azure", "bedrock"], "nothing was deleted");

    rig.delete_shown(true).await;
    let list = saved(&st);
    assert_eq!(list.items.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["compat", "bedrock"]);
    assert_eq!(list.selected, "bedrock", "the tile that followed took over");
    assert_eq!(key_of(&st, "azure"), None, "the Keychain key goes with the configuration");
    assert_eq!(key_of(&st, "compat").as_deref(), Some("sk-demo-0123456789abcdef"), "the other keys stay");
    assert_eq!(rig.probe().shown.as_deref(), Some("bedrock"), "the form follows the neighbour");
    rig.shot("polish-deleted-light.png").await;

    rig.delete_shown(true).await;
    assert_eq!(ids(&st), ["compat"]);
    rig.shot("polish-last-light.png").await;
    rig.click(trash());
    rig.wait(300).await;
    assert_eq!(rig.probe().deleting, None, "the last configuration cannot be deleted");
    assert_eq!(ids(&st), ["compat"]);
    println!(
        "verified: the trash asks first; Escape and Cancel keep it; Delete removes the key and selects the neighbour"
    );
}

/// The form shows a deleted configuration until the page is drawn again. A key typed into that gap must not be
/// stored: the Keychain would hold it under an account that nothing reads or deletes.
async fn key_after_delete(rig: &mut Rig<'_>) {
    let st = rig.st.clone();
    rig.start(Appearance::Light, DisplayLanguage::Ja, Some(staged(false)), (WIDTH, HEIGHT)).await;
    assert_eq!(rig.probe().shown.as_deref(), Some("compat"));
    rig.step("delete-then-key", "", "late-key");
    assert_eq!(ids(&st), ["azure", "bedrock"], "the configuration the form showed is gone");
    assert_eq!(key_of(&st, "compat"), None, "its key went with it, and the late one was not stored");
    assert_eq!(key_of(&st, "azure").as_deref(), Some("azure-demo-key-0123456789"), "the other keys stay");
    println!("verified: a key typed after its configuration was deleted is not stored");
}

async fn local_parameters(rig: &mut Rig<'_>) {
    let st = rig.st.clone();
    services::save_config(&st, |c| c.text_model.source = Source::Local);
    on_settings_window(rig.cx, |window, cx, view| {
        view.update(cx, |s, cx| s.preview_models(Role::Text, None, window, cx));
        redraw(window, cx);
    });
    rig.step("local-states", "", "");
    let model = st.cfg.get().text_model.local_model.clone();
    rig.type_local("temperature", "0.3");
    rig.type_local("top_p", "0.9");
    rig.type_local("top_k", "abc");
    rig.type_local("max_tokens", "0");
    let saved = st.cfg.get().text_model.local_params_for(&model);
    assert_eq!(saved, LocalTextParams { temperature: Some(0.3), top_p: Some(0.9), top_k: None, max_tokens: None });
    assert_eq!(rig.probe().local_invalid, ["top_k", "max_tokens"], "the local fields flag what does not parse");
    rig.shot("polish-local-invalid-light.png").await;

    rig.type_local("top_k", "40");
    rig.type_local("max_tokens", "");
    rig.type_local("temperature", "-1");
    let saved = st.cfg.get().text_model.local_params_for(&model);
    assert_eq!(saved, LocalTextParams { temperature: Some(0.3), top_p: Some(0.9), top_k: Some(40), max_tokens: None });
    rig.type_local("temperature", "0.3");
    rig.shot("polish-local-light.png").await;

    let other = rig.probe().local_models.into_iter().find(|id| *id != model).expect("a second text model");
    services::save_config(&st, |c| c.text_model.local_model = other.clone());
    rig.draw();
    let state = rig.probe();
    assert_eq!(state.local_shown.as_deref(), Some(other.as_str()));
    assert!(["temperature", "top_p", "top_k", "max_tokens"].iter().all(|field| state.local_text(field).is_empty()));
    assert_eq!(st.cfg.get().text_model.local_params_for(&model).top_k, Some(40));
    services::save_config(&st, |c| c.text_model.local_model = model.clone());
    rig.draw();
    assert_eq!(rig.probe().local_text("top_k"), "40", "coming back shows what was saved for that model");
    for field in ["temperature", "top_p", "top_k"] {
        rig.type_local(field, "");
    }
    assert!(st.cfg.get().text_model.local_params.is_empty(), "parameters that set nothing are not kept");
    rig.type_local("temperature", "0.3");

    let blocker = st.cfg.path().with_extension("toml.tmp");
    std::fs::create_dir_all(&blocker).expect("block the settings file's temporary name");
    rig.step("use-model", &other, "");
    assert_eq!(st.cfg.get().text_model.local_model, model, "a selection that cannot be saved changes nothing");
    assert!(rig.probe().save_failed, "and raises the banner");
    std::fs::remove_dir(&blocker).expect("unblock the settings file");
    rig.step("use-model", &other, "");
    assert_eq!(st.cfg.get().text_model.local_model, other, "the selection was saved");
    assert!(!rig.probe().save_failed, "a selection that works takes the banner down");
    rig.step("use-model", &model, "");
    assert_eq!(st.cfg.get().text_model.local_model, model);
    services::save_config(&st, |c| c.text_model.source = Source::Provider);
    println!("verified: local parameters are saved per model, follow the model in use and flag what does not parse");
}

async fn languages(rig: &mut Rig<'_>, appearance: Appearance) {
    let tag = theme_tag(appearance);
    for (language, name) in [(DisplayLanguage::En, "en"), (DisplayLanguage::Zh, "zh")] {
        rig.start(appearance, language, Some(staged(false)), (WIDTH, HEIGHT)).await;
        rig.shot(&shot_name("tiles", name, tag)).await;
        rig.click(plus());
        rig.wait(300).await;
        rig.shot(&shot_name("tiles-menu", name, tag)).await;
        rig.click(point(px(700.), px(900.)));
        rig.wait(200).await;
        assert!(!rig.probe().menu_open, "a click outside closes the menu");
        rig.click(tile(1, 0.));
        rig.wait(200).await;
        rig.click(trash());
        rig.wait(300).await;
        rig.shot(&shot_name("delete", name, tag)).await;
        rig.key("escape");
    }
}

async fn stand_ins(rig: &mut Rig<'_>, appearance: Appearance) {
    rig.start(appearance, DisplayLanguage::Ja, None, (WIDTH, HEIGHT)).await;
    rig.shot(&shot_name("fresh", "ja", theme_tag(appearance))).await;
    let names: Vec<_> = saved(rig.st).items.into_iter().map(|config| config.name).collect();
    assert_eq!(names, ["Azure OpenAI", "Amazon Bedrock", "OpenAI 互換 API"]);
}

pub async fn text_model_form_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let original = st.cfg.get();
    // The mock server's address is the one its azure configuration starts with.
    let server = {
        let list = original.text_model.text_configurations(original.general.lang());
        list.get("azure").map(|azure| azure.target.address().to_owned()).expect("the polish form shots need --mock")
    };
    let mut rig = Rig { st, dir, cx };
    for appearance in [Appearance::Light, Appearance::Dark] {
        stand_ins(&mut rig, appearance).await;
        scene(&mut rig, appearance).await;
        languages(&mut rig, appearance).await;
    }
    behavior(&mut rig, &server).await;
    services::save_config(st, |config| *config = (*original).clone());
}
