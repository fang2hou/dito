use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use dito_core::config::{EngineConfig, PolishTarget, Source};
use dito_core::factory::LocalBackend;
use dito_core::i18n::{Lang, Tr};
use dito_core::provider::configured_asr::AsrProfiles;
use dito_core::secrets::{Role, polish_account};
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
use crate::settings::{self, Ctx, Page, global_state, page, role};
use crate::state::AppState;
use crate::ui::{self, BannerAction, BtnKind, Cb, InputEvent, TestState, cbk, cbk_idx, text};

pub struct ModalChanged;

/// The cloud polish configuration a delete dialog asks about.
pub(super) struct CloudPolishDeletion {
    pub(super) id: String,
    pub(super) name: String,
}

pub struct ModelsPage {
    pub(super) st: Arc<AppState>,
    pub(in crate::settings) editor: Entity<AsrEditor>,
    pub(super) tab: Role,
    pub(super) menu: Option<String>,
    pub(super) select: Option<&'static str>,
    /// The cloud polish form: its inputs show the configuration in use. Cloud speech has its own editor.
    pub(super) cloud_polish: CloudPolishForm,
    /// The parameters of the local polish model in use.
    pub(super) local_polish: LocalPolishForm,
    /// The last save failed: the banner above the card says so until it is closed or a save works.
    pub(super) save_failed: bool,
    pub(super) deleting_cloud_polish: Option<CloudPolishDeletion>,
    pub(super) delete_dialog_focus: FocusHandle,
    pub(super) cloud_polish_test: TestState,
    pub(super) cloud_polish_test_requests: ui::Requests,
    pub(super) cloud_polish_test_config: Option<EngineConfig>,
    pub(super) profiles: Vec<String>,
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

        let cloud_polish = CloudPolishForm::new(window, cx);
        for field in CloudPolishField::ALL {
            cx.subscribe_in(&cloud_polish[field], window, move |this: &mut Self, _, ev: &InputEvent, window, cx| {
                match ev {
                    InputEvent::Change => this.cloud_polish_edited(field, cx),
                    InputEvent::Blur if field == CloudPolishField::Name => this.settle_cloud_polish_name(window, cx),
                    _ => {}
                }
            })
            .detach();
        }
        let local_polish = LocalPolishForm::new(window, cx);
        for field in LocalPolishField::ALL {
            cx.subscribe_in(&local_polish[field], window, move |this: &mut Self, _, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    this.local_polish_edited(field, cx);
                }
            })
            .detach();
        }
        // A saved profile stays in the list even when the AWS files no longer name it.
        let mut profiles = services::aws_profiles();
        let cfg = st.cfg.get();
        for configuration in &cfg.polish.engine.polish_configurations(cfg.general.lang()).items {
            if let PolishTarget::Bedrock { profile, .. } = &configuration.target
                && !profile.is_empty()
                && !profiles.contains(profile)
            {
                profiles.push(profile.clone());
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
            deleting_cloud_polish: None,
            delete_dialog_focus: cx.focus_handle(),
            cloud_polish_test: TestState::Idle,
            cloud_polish_test_requests: Default::default(),
            cloud_polish_test_config: None,
            profiles,
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
        if self.cloud_polish_test_config.as_ref().is_some_and(|engine| engine != &cfg.polish.engine) {
            self.cloud_polish_test = TestState::Idle;
            self.cloud_polish_test_config = None;
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
        editor
    }

    /// What the shell paints over the whole window: the cloud polish delete dialog, or the help of the speech editor.
    pub(in crate::settings) fn overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.cloud_polish_delete_dialog(window, cx)
            .or_else(|| self.editor.update(cx, |editor, cx| editor.help_overlay(window, cx)))
    }

    fn status_key(&self) -> String {
        let cfg = self.st.cfg.get();
        let mut k = String::new();
        for id in [&cfg.asr.local_model, &cfg.polish.engine.local_model] {
            k.push_str(&format!("{:?}|", self.st.local.status(id)));
        }
        k
    }

    /// Record how a save went: the failure banner shows while the last save failed.
    pub(super) fn note_save(&mut self, saved: bool) {
        self.save_failed = !saved;
    }

    /// A field of the cloud polish form was typed into. Everything saves as it changes, into the configuration the
    /// form shows: the key into the Keychain account of that configuration, the rest into the settings. Text the
    /// setting cannot take is not saved, so the last valid value stays in use.
    fn cloud_polish_edited(&mut self, field: CloudPolishField, cx: &mut Context<Self>) {
        let Some(id) = self.cloud_polish.shown().map(str::to_owned) else { return };
        let text = self.cloud_polish.text(field, cx);
        if !field.accepts(&text) {
            cx.notify();
            return;
        }
        let saved = if field == CloudPolishField::Key {
            services::try_store_secret(&self.st, &polish_account(&id), &text).is_ok()
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
        self.cloud_polish_test = TestState::Idle;
        cx.notify();
    }

    /// Focus left the name field of the cloud polish form. A name that was emptied goes back to the last saved one.
    fn settle_cloud_polish_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.cloud_polish.shown().map(str::to_owned) else { return };
        let typed = self.cloud_polish.text(CloudPolishField::Name, cx);
        let cfg = self.st.cfg.get();
        let configurations = cfg.polish.engine.polish_configurations(cfg.general.lang());
        let Some(saved) = configurations.get(&id).map(|config| config.name.as_str()) else { return };
        let settled = settled_cloud_polish_name(&typed, saved);
        if settled != typed {
            self.cloud_polish.set_name(settled.to_owned(), window, cx);
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

    /// Make `id` the cloud polish configuration in use. The form follows the selection when the page is drawn.
    pub(super) fn select_cloud_polish(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.cloud_polish.shown() == Some(id) {
            return;
        }
        let saved = services::try_save_config(&self.st, |cfg| {
            let lang = cfg.general.lang();
            cfg.polish.engine.polish_configurations_mut(lang).select(id);
        });
        self.note_save(saved.is_ok());
        self.cloud_polish_test = TestState::Idle;
        cx.notify();
    }

    /// The trash was pressed: ask whether to delete the cloud polish configuration in use. The last one cannot go.
    pub(super) fn ask_delete_cloud_polish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cfg = self.st.cfg.get();
        let configurations = cfg.polish.engine.polish_configurations(cfg.general.lang());
        let Some(active) = configurations.active().filter(|_| configurations.items.len() > 1) else { return };
        self.deleting_cloud_polish = Some(CloudPolishDeletion { id: active.id.clone(), name: active.name.clone() });
        self.delete_dialog_focus.focus(window, cx);
        cx.emit(ModalChanged);
        cx.notify();
    }

    pub(super) fn close_cloud_polish_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.deleting_cloud_polish = None;
        window.blur(cx);
        cx.emit(ModalChanged);
        cx.notify();
    }

    /// Delete was confirmed: the configuration and its Keychain key go together, and its neighbour is selected.
    pub(super) fn delete_cloud_polish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(CloudPolishDeletion { id, .. }) = self.deleting_cloud_polish.take() else { return };
        window.blur(cx);
        let removed = self.st.cfg.remove_polish_configuration(&id, &*self.st.secrets);
        if let Err(error) = &removed {
            tracing::warn!("could not delete a cloud polish configuration: {error:#}");
        }
        // An engine built from the deleted configuration must not keep its key.
        self.st.factory.invalidate_engines();
        self.note_save(removed.is_ok());
        self.cloud_polish_test = TestState::Idle;
        cx.emit(ModalChanged);
        cx.notify();
    }

    /// The inputs show what the settings say: the cloud polish configuration in use and the local polish model in
    /// use. They follow a selection however it was made, and the text typed into them is the only other way their
    /// content changes.
    fn follow_polish_settings(&mut self, lang: Lang, window: &mut Window, cx: &mut Context<Self>) {
        let cfg = self.st.cfg.get();
        let secrets = self.st.secrets.clone();
        let configurations = cfg.polish.engine.polish_configurations(lang);
        self.cloud_polish.follow(configurations.active(), lang, &*secrets, window, cx);
        let model = &cfg.polish.engine.local_model;
        self.local_polish.follow(model, &cfg.polish.engine.local_params_for(model), lang, window, cx);
    }

    pub(super) fn update_engine(&self, role: Role, f: impl FnOnce(&mut EngineConfig)) {
        services::save_config(&self.st, |c| f(role::engine_mut(c, role)));
    }

    pub(super) fn run_cloud_polish_test(&mut self, cx: &mut Context<Self>) {
        if self.cloud_polish_test == TestState::Running {
            return;
        }
        let tested_engine = self.st.cfg.get().polish.engine.clone();
        self.cloud_polish_test_config = Some(tested_engine.clone());
        let request = self.cloud_polish_test_requests.begin();
        self.cloud_polish_test = TestState::Running;
        cx.notify();
        let st = self.st.clone();
        cx.spawn(async move |this, cx| {
            let outcome = services::test_connection(st, Role::Polish).await;
            let _ = this.update(cx, |t, cx| {
                if !t.cloud_polish_test_requests.is_current(request)
                    || t.st.cfg.get().polish.engine != tested_engine
                    || t.cloud_polish_test != TestState::Running
                {
                    return;
                }
                t.cloud_polish_test = outcome.into();
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
        // A dialog belongs to the visit that opened it.
        self.deleting_cloud_polish = None;
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

        let mut card = ui::card(&p).p(px(26.)).gap(px(20.)).child(self.card_header(&c, role, is_local));
        let mut sections: Vec<AnyElement> = Vec::new();
        if is_local {
            card = self.local_models(&c, cx, role, card);
            sections.push(self.storage_card(&c, cx).into_any_element());
        } else if role == Role::Asr {
            card = card.child(self.editor.clone());
        } else {
            card = card.child(self.cloud_polish_tiles(&c, cx)).child(self.cloud_polish_view(&c, cx));
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

    /// The shared failure banner, while the last save of a setting on this page failed.
    fn save_banner(&self, c: &Ctx, cx: &mut Context<Self>) -> Option<Div> {
        self.save_failed.then(|| {
            ui::banner(
                &c.p,
                c.t(Tr::ModelsSaveFailed),
                Some(BannerAction {
                    id: "models-save-failed-close".into(),
                    label: c.t(Tr::HudClose),
                    kind: BtnKind::Secondary,
                    on_click: cbk(cx, |this: &mut Self, _, cx| {
                        this.save_failed = false;
                        cx.notify();
                    }),
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
                cx.notify();
            }),
        )
    }

    fn card_header(&self, c: &Ctx, role: Role, is_local: bool) -> Div {
        let p = &c.p;
        let (title, desc) = match role {
            Role::Asr => (c.t(Tr::ModelsDictation), c.t(Tr::ModelsDictationDesc)),
            Role::Polish => (c.t(Tr::ModelsPolishShort), c.t(Tr::ModelsPolishDesc)),
        };
        let st = self.st.clone();
        let source_seg = ui::seg(
            ("source", role::index(role)),
            p,
            &[c.t(Tr::ModelsLocal), c.t(Tr::ModelsProvider)],
            if is_local { 0 } else { 1 },
            Rc::new(move |i, _, _| {
                services::save_config(&st, |cfg| {
                    role::engine_mut(cfg, role).source = if i == 0 { Source::Local } else { Source::Provider };
                    if i == 1 {
                        // The first save of the cloud list starts from the templates, named in the UI language.
                        let lang = cfg.general.lang();
                        match role {
                            Role::Asr => {
                                cfg.asr.cloud_asr.get_or_insert_with(AsrProfiles::default);
                            }
                            Role::Polish => {
                                cfg.polish.engine.polish_configurations_mut(lang);
                            }
                        }
                    }
                });
            }),
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
