use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use dito_core::config::{CloudPolishConfiguration, CloudPolishTarget, EngineConfig, Source};
use dito_core::factory::LocalBackend;
use dito_core::i18n::{Lang, Tr};
use dito_core::provider::configured_asr::CloudAsrConfigurations;
use dito_core::secrets::Role;
use dito_local::catalog::ModelKind;
use dito_local::{ModelState, Snapshot};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::cloud_polish_form::CloudPolishForm;
use super::local_polish_form::LocalPolishForm;
use super::polish_field_rules::{CloudPolishField, LocalPolishField, settled_cloud_polish_name};
use crate::assets::Ic;
use crate::services;
use crate::settings::asr_editor::{self, AsrEditor};
use crate::settings::configuration_panel::ConfigurationPanel;
use crate::settings::{self, Ctx, Page, global_state, page, role};
use crate::state::AppState;
use crate::ui::{self, BtnKind, Cb, InputEvent, TestState, cbk, cbk_idx, text};

pub struct ModalChanged;

/// What the cloud polish half of the page keeps: the form that edits the configuration in use, the tiles that choose
/// it with their menu and delete dialog, and the result of the connection test. Cloud speech has its own editor.
pub(super) struct CloudPolishPanel {
    /// The inputs show the configuration in use.
    pub(super) form: CloudPolishForm,
    pub(super) tiles: ConfigurationPanel,
    pub(super) test: TestState,
    pub(super) test_requests: ui::Requests,
    /// The engine settings the last test ran against: its result belongs to them, and any later change clears it.
    pub(super) test_config: Option<EngineConfig>,
}

impl CloudPolishPanel {
    /// A panel that shows nothing yet: the first draw puts the configuration in use into the inputs. What is typed into
    /// them is saved as it changes.
    pub(super) fn new(window: &mut Window, cx: &mut Context<ModelsPage>) -> Self {
        let form = CloudPolishForm::new(window, cx);
        for field in CloudPolishField::ALL {
            cx.subscribe_in(
                &form[field],
                window,
                move |this: &mut ModelsPage, _, ev: &InputEvent, window, cx| match ev {
                    InputEvent::Change => this.cloud_polish_edited(field, cx),
                    InputEvent::Blur if field == CloudPolishField::Name => this.settle_cloud_polish_name(window, cx),
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

pub struct ModelsPage {
    pub(super) st: Arc<AppState>,
    pub(in crate::settings) editor: Entity<AsrEditor>,
    pub(super) tab: Role,
    pub(super) menu: Option<String>,
    pub(super) select: Option<&'static str>,
    pub(super) cloud_polish: CloudPolishPanel,
    /// The parameters of the local polish model in use.
    pub(super) local_polish: LocalPolishForm,
    /// The last save on this page failed: the banner above the card says so until it is closed or a save works. The
    /// cloud speech editor reports its saves here too.
    pub(super) save_failed: bool,
    pub(super) aws_profiles: Vec<String>,
    pub(super) last_status: String,
    #[cfg(feature = "screenshots")]
    pub(super) preview_snapshot: Option<Snapshot>,
    /// The page's own motion scope: its controls' transient state, reset on navigation.
    pub(super) motion: ui::MotionScope,
}

impl EventEmitter<ModalChanged> for ModelsPage {}

impl ModelsPage {
    pub fn new(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        services::watch_config(&st, cx);
        ui::notify_on_change(st.models.subscribe(), cx);
        Self::poll_engine_state(cx);

        let cloud_polish = CloudPolishPanel::new(window, cx);
        let local_polish = LocalPolishForm::new(window, cx);
        for field in LocalPolishField::ALL {
            cx.subscribe_in(&local_polish[field], window, move |this: &mut Self, _, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    this.local_polish_edited(field, cx);
                }
            })
            .detach();
        }
        // A saved AWS profile stays in the list even when the AWS files no longer name it.
        let mut aws_profiles = services::aws_profiles();
        let cfg = st.cfg.get();
        for configuration in &cfg.polish.engine.polish_configurations(cfg.general.lang()).items {
            if let CloudPolishTarget::Bedrock { profile, .. } = &configuration.target
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
            tab: Role::Asr,
            menu: None,
            select: None,
            cloud_polish,
            local_polish,
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
                    t.reset_stale_cloud_polish_test(cx);
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

    /// A test result belongs to the engine settings it ran against; any later change clears it.
    fn reset_stale_cloud_polish_test(&mut self, cx: &mut Context<Self>) {
        let cfg = self.st.cfg.get();
        if self.cloud_polish.test_config.as_ref().is_some_and(|engine| engine != &cfg.polish.engine) {
            self.cloud_polish.test = TestState::Idle;
            self.cloud_polish.test_config = None;
            cx.notify();
        }
    }

    /// The cloud speech editor, listened to: what it changes in the window shell's overlay, and how its saves go.
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

    /// What the shell paints over the whole window: the cloud polish delete dialog, or what the speech editor has to
    /// show there (its delete dialog, its help).
    pub(in crate::settings) fn overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.cloud_polish_delete_dialog(window, cx)
            .or_else(|| self.editor.update(cx, |editor, cx| editor.overlay(window, cx)))
    }

    fn status_key(&self) -> String {
        let cfg = self.st.cfg.get();
        let mut k = String::new();
        for id in [&cfg.asr.local_model, &cfg.polish.engine.local_model] {
            k.push_str(&format!("{:?}|", self.st.local.status(id)));
        }
        k
    }

    /// Record how a save on this page went: the failure banner shows while the last save failed, whichever control
    /// made it, and the next one that works takes it down.
    pub(super) fn note_save(&mut self, saved: bool) {
        self.save_failed = !saved;
    }

    /// A field of the cloud polish form was typed into. Everything saves as it changes, into the configuration the
    /// form shows: the key into the Keychain account of that configuration, the rest into the settings. Text the
    /// setting cannot take is not saved, so the last valid value stays in use.
    pub(super) fn cloud_polish_edited(&mut self, field: CloudPolishField, cx: &mut Context<Self>) {
        let Some(id) = self.cloud_polish.form.shown().map(str::to_owned) else { return };
        let text = self.cloud_polish.form.text(field, cx);
        if !field.accepts(&text) {
            cx.notify();
            return;
        }
        let saved = if field == CloudPolishField::Key {
            // The form shows a deleted configuration until the page is drawn again: a key is not stored for one that
            // is gone, whose account no configuration would ever use again.
            let cfg = self.st.cfg.get();
            let Some(account) = cfg
                .polish
                .engine
                .polish_configurations(cfg.general.lang())
                .get(&id)
                .map(CloudPolishConfiguration::account)
            else {
                return;
            };
            services::try_store_secret(&self.st, &account, &text).is_ok()
        } else {
            services::try_save_config(&self.st, |cfg| {
                let lang = cfg.general.lang();
                if let Some(config) = cfg.polish.engine.polish_configurations_mut(lang).get_mut(&id) {
                    field.apply(config, &text);
                }
            })
            .is_ok()
        };
        self.note_save(saved);
        // A changed setting invalidates the previous test result.
        self.cloud_polish.test = TestState::Idle;
        cx.notify();
    }

    /// Focus left the name field of the cloud polish form. A name that was emptied goes back to the last saved one.
    fn settle_cloud_polish_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.cloud_polish.form.shown().map(str::to_owned) else { return };
        let typed = self.cloud_polish.form.text(CloudPolishField::Name, cx);
        let cfg = self.st.cfg.get();
        let configurations = cfg.polish.engine.polish_configurations(cfg.general.lang());
        let Some(saved) = configurations.get(&id).map(|config| config.name.as_str()) else { return };
        let settled = settled_cloud_polish_name(&typed, saved);
        if settled != typed {
            self.cloud_polish.form.set_name(settled.to_owned(), window, cx);
        }
    }

    /// A field of the parameters under the local polish models was typed into: it changes the parameters of the model
    /// the fields show, and nothing else.
    fn local_polish_edited(&mut self, field: LocalPolishField, cx: &mut Context<Self>) {
        let Some(model) = self.local_polish.shown().map(str::to_owned) else { return };
        let text = self.local_polish.text(field, cx);
        if field.invalid(&text) {
            cx.notify();
            return;
        }
        let saved = services::try_save_config(&self.st, |cfg| {
            let engine = &mut cfg.polish.engine;
            let mut params = engine.local_params_for(&model);
            field.apply(&mut params, &text);
            engine.set_local_params(&model, params);
        });
        self.note_save(saved.is_ok());
        cx.notify();
    }

    /// The inputs show what the settings say: the cloud polish configuration in use and the local polish model in
    /// use. They follow a selection however it was made, and the text typed into them is the only other way their
    /// content changes.
    pub(super) fn follow_polish_settings(&mut self, lang: Lang, window: &mut Window, cx: &mut Context<Self>) {
        let cfg = self.st.cfg.get();
        let secrets = self.st.secrets.clone();
        let configurations = cfg.polish.engine.polish_configurations(lang);
        let before = self.cloud_polish.form.shown().map(str::to_owned);
        self.cloud_polish.form.follow(configurations.active(), lang, &*secrets, window, cx);
        if let Some(shown) = self.cloud_polish.form.shown().filter(|shown| before.as_deref() != Some(shown)) {
            self.cloud_polish.tiles.reveal(shown);
        }
        let model = &cfg.polish.engine.local_model;
        self.local_polish.follow(model, &cfg.polish.engine.local_params_for(model), lang, window, cx);
    }

    /// Make `id` the local model `role` uses. `false` when the save failed: the model in use stays as it was, and so
    /// does everything that would have followed from the change.
    pub(super) fn select_local_model(&mut self, role: Role, id: &str) -> bool {
        let id = id.to_owned();
        let saved = services::try_save_config(&self.st, |cfg| role::engine_mut(cfg, role).local_model = id).is_ok();
        self.note_save(saved);
        saved
    }

    /// The Local and Cloud switch of `role` was set to the segment `index`.
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
                    Role::Polish => {
                        cfg.polish.engine.polish_configurations_mut(lang);
                    }
                }
            }
        });
        self.note_save(saved.is_ok());
        cx.notify();
    }

    pub(super) fn run_cloud_polish_test(&mut self, cx: &mut Context<Self>) {
        if self.cloud_polish.test == TestState::Running {
            return;
        }
        let tested_engine = self.st.cfg.get().polish.engine.clone();
        self.cloud_polish.test_config = Some(tested_engine.clone());
        let request = self.cloud_polish.test_requests.begin();
        self.cloud_polish.test = TestState::Running;
        cx.notify();
        let st = self.st.clone();
        cx.spawn(async move |this, cx| {
            let outcome = services::test_connection(st, Role::Polish).await;
            let _ = this.update(cx, |t, cx| {
                if !t.cloud_polish.test_requests.is_current(request)
                    || t.st.cfg.get().polish.engine != tested_engine
                    || t.cloud_polish.test != TestState::Running
                {
                    return;
                }
                t.cloud_polish.test = outcome.into();
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
    /// Reset the page's transient motion state; its controls mount statically on the next entry.
    pub(in crate::settings) fn reset_motion(&mut self, cx: &mut Context<Self>) {
        self.motion.reset();
        self.cloud_polish.tiles.reset();
        self.editor.update(cx, |editor, _| editor.reset_motion());
    }
}

impl Render for ModelsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The page draws under its own scope: after a page switch its controls mount statically,
        // never sliding to values remembered from another page or an earlier visit.
        ui::with_motion_scope(self.motion.handle(), || self.view(window, cx))
    }
}

impl ModelsPage {
    fn view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        self.follow_polish_settings(c.lang, window, cx);
        let role = self.tab;
        let engine = role::engine(&self.st.cfg.get(), role).clone();
        let is_local = engine.source == Source::Local;

        let mut card = ui::card(&p).p(px(26.)).gap(px(20.)).child(self.card_header(&c, cx, role, is_local));
        let mut sections: Vec<AnyElement> = Vec::new();
        if is_local {
            card = self.local_models(&c, cx, role, card);
            sections.push(self.storage_card(&c, cx).into_any_element());
        } else if role == Role::Asr {
            card = card.child(self.editor.clone());
        } else {
            card = card.child(self.cloud_polish_tile_row(&c, cx)).child(self.cloud_polish_view(&c, cx));
        }

        let actions = is_local.then(|| {
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
            .child(page(
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
            ))
    }

    /// The shared failure banner, while the last save on this page failed, whichever control made it.
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
            &[c.t(Tr::ModelsDictation), c.t(Tr::ModelsPolishShort)],
            role::index(self.tab),
            cbk_idx(cx, |this: &mut Self, i, _, cx| {
                ui::dismiss_tips(&this.motion.handle());
                this.tab = if i == 0 { Role::Asr } else { Role::Polish };
                this.menu = None;
                this.select = None;
                this.cloud_polish.tiles.close_menu();
                cx.notify();
            }),
        )
    }

    fn card_header(&self, c: &Ctx, cx: &mut Context<Self>, role: Role, is_local: bool) -> Div {
        let p = &c.p;
        let (title, desc) = match role {
            Role::Asr => (c.t(Tr::ModelsDictation), c.t(Tr::ModelsDictationDesc)),
            Role::Polish => (c.t(Tr::ModelsPolishShort), c.t(Tr::ModelsPolishDesc)),
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
        let kind = if role == Role::Asr { ModelKind::Asr } else { ModelKind::Polish };
        let snap = self.st.models.subscribe().borrow().clone();
        #[cfg(feature = "screenshots")]
        let snap = self.preview_snapshot.clone().unwrap_or(snap);
        if let Some(note) = self.background_downloads(c, kind, &snap) {
            card = card.child(note);
        }
        let mut list = div().w_full().flex().flex_col().gap(px(8.));
        for spec in self.st.models.catalog().iter().filter(|m| m.kind == kind) {
            list = list.child(self.model_row(c, cx, role, spec, &snap));
        }
        card = card.child(list);
        if role == Role::Polish {
            card = card.child(self.local_polish_parameters(c, cx));
        }
        card
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
        let separator = if c.lang == Lang::Ja { "、" } else { ", " };
        Some(ui::muted12(&c.p, format!("{}{}", c.t(Tr::ModelsDownloadingBg), elsewhere.join(separator))))
    }
}
