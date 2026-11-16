use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use dito_core::config::{CloudTextConfiguration, CloudTextTarget, EngineConfig, Source};
use dito_core::factory::LocalBackend;
use dito_core::i18n::{Lang, Tr};
use dito_core::provider::configured_asr::CloudAsrConfigurations;
use dito_core::secrets::Role;
use dito_local::catalog::ModelKind;
use dito_local::{ModelState, Snapshot};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::cloud_text_form::CloudTextForm;
use super::local_text_form::LocalTextForm;
use super::text_field_rules::{CloudTextField, LocalTextField, settled_cloud_text_name};
use crate::assets::Ic;
use crate::services;
use crate::settings::asr_editor::{self, AsrEditor};
use crate::settings::configuration_panel::ConfigurationPanel;
use crate::settings::{self, Ctx, Page, global_state, page, role};
use crate::state::AppState;
use crate::ui::{self, BtnKind, Cb, InputEvent, TestState, cbk, cbk_idx, text};

pub struct ModalChanged;

pub(super) struct CloudTextPanel {
    pub(super) form: CloudTextForm,
    pub(super) tiles: ConfigurationPanel,
    pub(super) test: TestState,
    pub(super) test_requests: ui::Requests,
    /// The engine settings the last test ran against: its result belongs to them, and any later change clears it.
    pub(super) test_config: Option<EngineConfig>,
}

impl CloudTextPanel {
    /// Shows nothing yet: the first draw puts the configuration in use into the inputs.
    pub(super) fn new(window: &mut Window, cx: &mut Context<ModelsPage>) -> Self {
        let form = CloudTextForm::new(window, cx);
        for field in CloudTextField::ALL {
            cx.subscribe_in(
                &form[field],
                window,
                move |this: &mut ModelsPage, _, ev: &InputEvent, window, cx| match ev {
                    InputEvent::Change => this.cloud_text_edited(field, cx),
                    InputEvent::Blur if field == CloudTextField::Name => this.settle_cloud_text_name(window, cx),
                    _ => {}
                },
            )
            .detach();
        }
        Self {
            form,
            tiles: ConfigurationPanel::new(cx),
            test: TestState::Idle,
            test_requests: Default::default(),
            test_config: None,
        }
    }
}

/// The tabs of the Models page: the two engine roles, plus the speech detector. The detector is always local and,
/// once its file is on disk, always the one in use.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Tab {
    Role(Role),
    Vad,
}

impl Tab {
    pub(super) fn index(self) -> usize {
        match self {
            Self::Role(role) => role::index(role),
            Self::Vad => 2,
        }
    }

    /// The stable part of a row's element id: controls of different tabs never share one.
    pub(super) fn tag(self) -> &'static str {
        match self {
            Self::Role(Role::Asr) => "asr",
            Self::Role(Role::Text) => "text",
            Self::Vad => "vad",
        }
    }
}

pub struct ModelsPage {
    pub(super) st: Arc<AppState>,
    pub(in crate::settings) editor: Entity<AsrEditor>,
    pub(super) tab: Tab,
    pub(super) menu: Option<String>,
    pub(super) select: Option<&'static str>,
    pub(super) cloud_text: CloudTextPanel,
    pub(super) local_text: LocalTextForm,
    /// The last save on this page failed: the banner above the card says so until it is closed or a save works. The
    /// cloud speech editor reports its saves here too.
    pub(super) save_failed: bool,
    pub(super) aws_profiles: Vec<String>,
    pub(super) last_status: String,
    #[cfg(feature = "screenshots")]
    pub(super) preview_snapshot: Option<Snapshot>,
    pub(super) motion: ui::MotionScope,
}

impl EventEmitter<ModalChanged> for ModelsPage {}

impl ModelsPage {
    pub fn new(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        services::watch_config(&st, cx);
        ui::notify_on_change(st.models.subscribe(), cx);
        Self::poll_engine_state(cx);

        let cloud_text = CloudTextPanel::new(window, cx);
        let local_text = LocalTextForm::new(window, cx);
        for field in LocalTextField::ALL {
            cx.subscribe_in(&local_text[field], window, move |this: &mut Self, _, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    this.local_text_edited(field, cx);
                }
            })
            .detach();
        }
        // A saved AWS profile stays in the list even when the AWS files no longer name it.
        let mut aws_profiles = services::aws_profiles();
        let cfg = st.cfg.get();
        for configuration in &cfg.text_model.text_configurations(cfg.general.lang()).items {
            if let CloudTextTarget::Bedrock { profile, .. } = &configuration.target
                && !profile.is_empty()
                && !aws_profiles.contains(profile)
            {
                aws_profiles.push(profile.clone());
            }
        }
        let editor = Self::make_editor(st.clone(), window, cx);
        Self {
            motion: ui::MotionScope::new(),
            st,
            editor,
            tab: Tab::Role(Role::Asr),
            menu: None,
            select: None,
            cloud_text,
            local_text,
            save_failed: false,
            aws_profiles,
            last_status: String::new(),
            #[cfg(feature = "screenshots")]
            preview_snapshot: None,
        }
    }

    /// Engine status has no event stream; poll gently while the page exists.
    fn poll_engine_state(cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(800)).await;
                let alive = this.update(cx, |t, cx| {
                    t.reset_stale_cloud_text_test(cx);
                    let now = t.status_key();
                    if now != t.last_status {
                        t.last_status = now;
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn reset_stale_cloud_text_test(&mut self, cx: &mut Context<Self>) {
        let cfg = self.st.cfg.get();
        if self.cloud_text.test_config.as_ref().is_some_and(|engine| engine != &cfg.text_model) {
            self.cloud_text.test = TestState::Idle;
            self.cloud_text.test_config = None;
            cx.notify();
        }
    }

    pub(super) fn make_editor(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Entity<AsrEditor> {
        let editor = cx.new(|cx| AsrEditor::new(st, window, cx));
        cx.subscribe(&editor, |_, _, _: &asr_editor::Changed, cx| {
            cx.emit(ModalChanged);
            cx.notify();
        })
        .detach();
        cx.subscribe(&editor, |this, _, outcome: &asr_editor::SaveOutcome, cx| {
            this.note_save(outcome.0);
            cx.notify();
        })
        .detach();
        editor
    }

    pub(in crate::settings) fn overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.cloud_text_delete_dialog(window, cx)
            .or_else(|| self.editor.update(cx, |editor, cx| editor.overlay(window, cx)))
    }

    fn status_key(&self) -> String {
        let cfg = self.st.cfg.get();
        let mut k = String::new();
        for id in [&cfg.asr.local_model, &cfg.text_model.local_model] {
            k.push_str(&format!("{:?}|", self.st.local.status(id)));
        }
        k
    }

    /// The next save that works takes the banner down, whichever control made it.
    pub(super) fn note_save(&mut self, saved: bool) {
        self.save_failed = !saved;
    }

    /// The key goes into the Keychain account of the configuration the form shows, the rest into the settings. Text the
    /// setting cannot take is not saved, so the last valid value stays in use.
    pub(super) fn cloud_text_edited(&mut self, field: CloudTextField, cx: &mut Context<Self>) {
        let Some(id) = self.cloud_text.form.shown().map(str::to_owned) else { return };
        let text = self.cloud_text.form.text(field, cx);
        if !field.accepts(&text) {
            cx.notify();
            return;
        }
        let saved = if field == CloudTextField::Key {
            // The form shows a deleted configuration until the page is drawn again: a key is not stored for one that
            // is gone, whose account no configuration would ever use again.
            let cfg = self.st.cfg.get();
            let Some(account) =
                cfg.text_model.text_configurations(cfg.general.lang()).get(&id).map(CloudTextConfiguration::account)
            else {
                return;
            };
            services::try_store_secret(&self.st, &account, &text).is_ok()
        } else {
            services::try_save_config(&self.st, |cfg| {
                let lang = cfg.general.lang();
                if let Some(config) = cfg.text_model.text_configurations_mut(lang).get_mut(&id) {
                    field.apply(config, &text);
                }
            })
            .is_ok()
        };
        self.note_save(saved);
        self.cloud_text.test = TestState::Idle;
        cx.notify();
    }

    /// A name that was emptied goes back to the last saved one.
    fn settle_cloud_text_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.cloud_text.form.shown().map(str::to_owned) else { return };
        let typed = self.cloud_text.form.text(CloudTextField::Name, cx);
        let cfg = self.st.cfg.get();
        let configurations = cfg.text_model.text_configurations(cfg.general.lang());
        let Some(saved) = configurations.get(&id).map(|config| config.name.as_str()) else { return };
        let settled = settled_cloud_text_name(&typed, saved);
        if settled != typed {
            self.cloud_text.form.set_name(settled.to_owned(), window, cx);
        }
    }

    fn local_text_edited(&mut self, field: LocalTextField, cx: &mut Context<Self>) {
        let Some(model) = self.local_text.shown().map(str::to_owned) else { return };
        let text = self.local_text.text(field, cx);
        if field.invalid(&text) {
            cx.notify();
            return;
        }
        let saved = services::try_save_config(&self.st, |cfg| {
            let engine = &mut cfg.text_model;
            let mut params = engine.local_params_for(&model);
            field.apply(&mut params, &text);
            engine.set_local_params(&model, params);
        });
        self.note_save(saved.is_ok());
        cx.notify();
    }

    /// They follow a selection however it was made, and the text typed into them is the only other way their content
    /// changes.
    pub(super) fn follow_text_model_settings(&mut self, lang: Lang, window: &mut Window, cx: &mut Context<Self>) {
        let cfg = self.st.cfg.get();
        let secrets = self.st.secrets.clone();
        let configurations = cfg.text_model.text_configurations(lang);
        let before = self.cloud_text.form.shown().map(str::to_owned);
        self.cloud_text.form.follow(configurations.active(), lang, &*secrets, window, cx);
        if let Some(shown) = self.cloud_text.form.shown().filter(|shown| before.as_deref() != Some(shown)) {
            self.cloud_text.tiles.reveal(shown);
        }
        let model = &cfg.text_model.local_model;
        self.local_text.follow(model, &cfg.text_model.local_params_for(model), lang, window, cx);
    }

    /// `false` when the save failed: the model in use stays as it was, and so does everything that would have followed
    /// from the change.
    pub(super) fn select_local_model(&mut self, role: Role, id: &str) -> bool {
        let id = id.to_owned();
        let saved = services::try_save_config(&self.st, |cfg| role::engine_mut(cfg, role).local_model = id).is_ok();
        self.note_save(saved);
        saved
    }

    pub(super) fn set_source(&mut self, role: Role, index: usize, cx: &mut Context<Self>) {
        let saved = services::try_save_config(&self.st, |cfg| {
            role::engine_mut(cfg, role).source = if index == 0 { Source::Local } else { Source::Provider };
            if index == 1 {
                // The first save of the cloud list starts from the templates, named in the UI language.
                let lang = cfg.general.lang();
                match role {
                    Role::Asr => {
                        cfg.asr.cloud_asr.get_or_insert_with(CloudAsrConfigurations::default);
                    }
                    Role::Text => {
                        cfg.text_model.text_configurations_mut(lang);
                    }
                }
            }
        });
        self.note_save(saved.is_ok());
        cx.notify();
    }

    pub(super) fn run_cloud_text_test(&mut self, cx: &mut Context<Self>) {
        if self.cloud_text.test == TestState::Running {
            return;
        }
        let tested_engine = self.st.cfg.get().text_model.clone();
        self.cloud_text.test_config = Some(tested_engine.clone());
        let request = self.cloud_text.test_requests.begin();
        self.cloud_text.test = TestState::Running;
        cx.notify();
        let st = self.st.clone();
        cx.spawn(async move |this, cx| {
            let outcome = services::test_connection(st, Role::Text).await;
            let _ = this.update(cx, |t, cx| {
                if !t.cloud_text.test_requests.is_current(request)
                    || t.st.cfg.get().text_model != tested_engine
                    || t.cloud_text.test != TestState::Running
                {
                    return;
                }
                t.cloud_text.test = outcome.into();
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn toggle_select(&self, cx: &mut Context<Self>, id: &'static str) -> Cb {
        cbk(cx, move |this: &mut Self, _, cx| {
            this.select = if this.select == Some(id) { None } else { Some(id) };
            cx.notify();
        })
    }
}

impl ModelsPage {
    pub(in crate::settings) fn reset_motion(&mut self, cx: &mut Context<Self>) {
        self.motion.reset();
        self.cloud_text.tiles.reset();
        self.editor.update(cx, |editor, _| editor.reset_motion());
    }
}

impl Render for ModelsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Own scope: after a page switch the controls mount statically instead of sliding to values remembered from an
        // earlier visit.
        ui::render_with_scope(self.motion.handle(), || self.view(window, cx))
    }
}

impl ModelsPage {
    fn view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        self.follow_text_model_settings(c.lang, window, cx);
        let mut card = ui::card(&p).p(px(26.)).gap(px(20.));
        let mut sections: Vec<AnyElement> = Vec::new();
        let local = match self.tab {
            Tab::Role(role) => {
                let engine = role::engine(&self.st.cfg.get(), role).clone();
                let is_local = engine.source == Source::Local;
                card = card.child(self.card_header(&c, cx, role, is_local));
                if is_local {
                    card = self.local_models(&c, cx, role, card);
                    sections.push(self.storage_card(&c, cx).into_any_element());
                } else if role == Role::Asr {
                    card = card.child(self.editor.clone());
                } else {
                    card = card.child(self.cloud_text_tile_row(&c, cx)).child(self.cloud_text_view(&c, cx));
                }
                is_local
            }
            Tab::Vad => {
                card = card.child(self.vad_header(&c)).child(self.vad_models(&c, cx));
                sections.push(self.storage_card(&c, cx).into_any_element());
                true
            }
        };

        let actions = local.then(|| {
            ui::button(
                "to-status",
                &p,
                &c.t(Tr::MenubarViewStatus),
                BtnKind::Secondary,
                Some(Ic::Activity),
                Rc::new(|_, cx| {
                    let st = global_state(cx);
                    settings::open(st, Page::Status, cx);
                }),
            )
            .into_any_element()
        });

        let tips = self.motion.handle();
        div()
            .relative()
            .size_full()
            .on_scroll(move |_, window, _| {
                ui::dismiss_tips(&tips);
                window.refresh();
            })
            .child(
                page(
                    p,
                    "models",
                    c.t(Tr::NavModels),
                    actions,
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(24.))
                        .child(div().flex().child(self.role_tabs(&c, cx)))
                        .children(self.save_banner(&c, cx))
                        .child(card)
                        .children(sections),
                )
                .track_scroll(self.editor.read(cx).page_scroll()),
            )
    }

    fn save_banner(&self, c: &Ctx, cx: &mut Context<Self>) -> Option<Div> {
        self.save_failed.then(|| {
            ui::dismissible_banner(
                &c.p,
                "models-save-failed-close",
                c.t(Tr::ModelsSaveFailed),
                c.t(Tr::HudClose),
                cbk(cx, |this: &mut Self, _, cx| {
                    this.save_failed = false;
                    cx.notify();
                }),
            )
        })
    }

    fn role_tabs(&self, c: &Ctx, cx: &mut Context<Self>) -> impl IntoElement {
        ui::seg(
            "models-tab",
            &c.p,
            &[c.t(Tr::ModelsDictation), c.t(Tr::ModelsTextModelShort), c.t(Tr::ModelsVad)],
            self.tab.index(),
            cbk_idx(cx, |this: &mut Self, i, _, cx| {
                ui::dismiss_tips(&this.motion.handle());
                this.tab = match i {
                    0 => Tab::Role(Role::Asr),
                    1 => Tab::Role(Role::Text),
                    _ => Tab::Vad,
                };
                this.menu = None;
                this.select = None;
                this.cloud_text.tiles.close_menu();
                cx.notify();
            }),
        )
    }

    /// The Tools window points at this tab when a transcription has no detector yet.
    pub(crate) fn show_vad(&mut self, cx: &mut Context<Self>) {
        ui::dismiss_tips(&self.motion.handle());
        self.tab = Tab::Vad;
        self.menu = None;
        self.select = None;
        self.cloud_text.tiles.close_menu();
        cx.notify();
    }

    /// The Tools window points at this tab when the user opens the text model behind a chat.
    pub(crate) fn show_text(&mut self, cx: &mut Context<Self>) {
        ui::dismiss_tips(&self.motion.handle());
        self.tab = Tab::Role(Role::Text);
        self.menu = None;
        self.select = None;
        self.cloud_text.tiles.close_menu();
        cx.notify();
    }

    /// One model, one source: the detector has no switcher beside its title.
    fn vad_header(&self, c: &Ctx) -> Div {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(text(c.t(Tr::ModelsVad), 17., FontWeight::BOLD, c.p.text))
            .child(text(c.t(Tr::ModelsVadDesc), 13., FontWeight::NORMAL, c.p.muted))
    }

    fn card_header(&self, c: &Ctx, cx: &mut Context<Self>, role: Role, is_local: bool) -> Div {
        let p = &c.p;
        let (title, desc) = match role {
            Role::Asr => (c.t(Tr::ModelsDictation), c.t(Tr::ModelsDictationDesc)),
            Role::Text => (c.t(Tr::ModelsTextModel), c.t(Tr::ModelsTextModelDesc)),
        };
        let source_seg = ui::seg(
            ("source", role::index(role)),
            p,
            &[c.t(Tr::ModelsLocal), c.t(Tr::ModelsProvider)],
            if is_local { 0 } else { 1 },
            cbk_idx(cx, move |this: &mut Self, i, _, cx| this.set_source(role, i, cx)),
        );
        div()
            .w_full()
            .flex()
            .items_start()
            .justify_between()
            .gap(px(20.))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(text(title, 17., FontWeight::BOLD, p.text))
                    .child(text(desc, 13., FontWeight::NORMAL, p.muted)),
            )
            .child(source_seg)
    }

    fn local_models(&self, c: &Ctx, cx: &mut Context<Self>, role: Role, mut card: Div) -> Div {
        let kind = if role == Role::Asr { ModelKind::Asr } else { ModelKind::Text };
        let snap = self.snapshot();
        if let Some(note) = self.background_downloads(c, kind, &snap) {
            card = card.child(note);
        }
        let mut list = div().w_full().flex().flex_col().gap(px(8.));
        for spec in self.st.models.catalog().iter().filter(|m| m.kind == kind) {
            list = list.child(self.model_row(c, cx, Tab::Role(role), spec, &snap));
        }
        card = card.child(list);
        if role == Role::Text {
            card = card.child(self.local_text_parameters(c, cx));
        }
        card
    }

    fn vad_models(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let snap = self.snapshot();
        let mut list = div().w_full().flex().flex_col().gap(px(8.));
        for spec in self.st.models.catalog().iter().filter(|m| m.kind == ModelKind::Vad) {
            list = list.child(self.model_row(c, cx, Tab::Vad, spec, &snap));
        }
        list
    }

    fn snapshot(&self) -> Snapshot {
        #[cfg(feature = "screenshots")]
        if let Some(preview) = self.preview_snapshot.clone() {
            return preview;
        }
        self.st.models.subscribe().borrow().clone()
    }

    /// Downloads for the *other* tab keep running; surface them here so switching tabs
    /// never looks like the transfer silently stopped.
    fn background_downloads(&self, c: &Ctx, kind: ModelKind, snap: &Snapshot) -> Option<Div> {
        let elsewhere: Vec<String> = snap
            .iter()
            .filter_map(|(id, s)| {
                let (done, total) = match s {
                    ModelState::Downloading { done, total, .. } | ModelState::Verifying { done, total } => {
                        (*done, *total)
                    }
                    _ => return None,
                };
                let spec = self.st.models.spec(id)?;
                (spec.kind != kind).then(|| format!("{} {:.0}%", spec.name, done as f32 / total.max(1) as f32 * 100.))
            })
            .collect();
        if elsewhere.is_empty() {
            return None;
        }
        let separator = if matches!(c.lang, Lang::Ja | Lang::Zh | Lang::ZhHant) { "、" } else { ", " };
        Some(ui::muted12(&c.p, format!("{}{}", c.t(Tr::ModelsDownloadingBg), elsewhere.join(separator))))
    }
}
