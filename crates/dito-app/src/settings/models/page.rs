use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use dito_core::config::{EngineConfig, Source};
use dito_core::factory::LocalBackend;
use dito_core::i18n::{Lang, Tr};
use dito_core::provider::configured_asr::AsrProfiles;
use dito_core::secrets::{Role, account};
use dito_local::catalog::ModelKind;
use dito_local::{ModelState, Snapshot};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::inputs::{Field, ProviderInputs};
use crate::assets::Ic;
use crate::services;
use crate::settings::asr_editor::{self, AsrEditor};
use crate::settings::{self, Ctx, Page, global_state, page, role};
use crate::state::AppState;
use crate::ui::{self, BtnKind, Cb, InputEvent, TestState, cbk, cbk_idx, text};

pub struct ModalChanged;

pub struct ModelsPage {
    pub(super) st: Arc<AppState>,
    pub(in crate::settings) editor: Entity<AsrEditor>,
    pub(super) tab: Role,
    pub(super) menu: Option<String>,
    pub(super) select: Option<&'static str>,
    /// Provider fields, tests and test results exist for polish only; speech has its own editor.
    pub(super) inputs: ProviderInputs,
    pub(super) test: TestState,
    pub(super) test_requests: ui::Requests,
    pub(super) test_config: Option<EngineConfig>,
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

        let inputs = ProviderInputs::new(&st, window, cx);
        for field in Field::ALL {
            cx.subscribe_in(&inputs[field], window, move |this: &mut Self, state, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    let value = state.read(cx).value().to_string();
                    this.field_changed(field, value, cx);
                }
            })
            .detach();
        }
        let bedrock_profile = st.cfg.get().polish.engine.provider.bedrock.profile.clone();
        let mut profiles = services::aws_profiles();
        if !profiles.contains(&bedrock_profile) && !bedrock_profile.is_empty() {
            profiles.push(bedrock_profile);
        }
        let editor = Self::make_editor(st.clone(), window, cx);
        Self {
            motion: ui::MotionScope::new(),
            st,
            editor,
            tab: Role::Asr,
            menu: None,
            select: None,
            inputs,
            test: TestState::Idle,
            test_requests: Default::default(),
            test_config: None,
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
                    t.reset_stale_tests(cx);
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
    fn reset_stale_tests(&mut self, cx: &mut Context<Self>) {
        let cfg = self.st.cfg.get();
        if self.test_config.as_ref().is_some_and(|engine| engine != &cfg.polish.engine) {
            self.test = TestState::Idle;
            self.test_config = None;
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

    pub(in crate::settings) fn help_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.editor.update(cx, |editor, cx| editor.help_overlay(window, cx))
    }

    fn status_key(&self) -> String {
        let cfg = self.st.cfg.get();
        let mut k = String::new();
        for id in [&cfg.asr.local_model, &cfg.polish.engine.local_model] {
            k.push_str(&format!("{:?}|", self.st.local.status(id)));
        }
        k
    }

    fn field_changed(&mut self, field: Field, value: String, cx: &mut Context<Self>) {
        // A changed setting invalidates the previous test result.
        self.test = TestState::Idle;
        if let Some(kind) = field.key_provider() {
            services::store_secret(&self.st, &account(Role::Polish, kind), &value);
        } else {
            services::save_config(&self.st, |c| field.write(&mut c.polish.engine.provider, value));
        }
        cx.notify();
    }

    pub(super) fn update_engine(&self, role: Role, f: impl FnOnce(&mut EngineConfig)) {
        services::save_config(&self.st, |c| f(role::engine_mut(c, role)));
    }

    pub(super) fn run_test(&mut self, cx: &mut Context<Self>) {
        if self.test == TestState::Running {
            return;
        }
        let tested_engine = self.st.cfg.get().polish.engine.clone();
        self.test_config = Some(tested_engine.clone());
        let request = self.test_requests.begin();
        self.test = TestState::Running;
        cx.notify();
        let st = self.st.clone();
        cx.spawn(async move |this, cx| {
            let outcome = services::test_connection(st, Role::Polish).await;
            let _ = this.update(cx, |t, cx| {
                if !t.test_requests.is_current(request)
                    || t.st.cfg.get().polish.engine != tested_engine
                    || t.test != TestState::Running
                {
                    return;
                }
                t.test = outcome.into();
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
        self.editor.update(cx, |editor, _| editor.reset_motion());
    }
}

impl Render for ModelsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The page draws under its own scope: after a page switch its controls mount statically,
        // never sliding to values remembered from another page or an earlier visit.
        ui::with_motion_scope(self.motion.handle(), || self.view(cx))
    }
}

impl ModelsPage {
    fn view(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
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
            card = card
                .child(self.provider_tiles(&c, cx, role, engine.provider.kind))
                .child(self.provider_fields(&c, cx, role))
                .child(self.test_footer(&c, cx, role));
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
                    .child(card)
                    .children(sections),
            ))
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
                    if role == Role::Asr && i == 1 {
                        cfg.asr.cloud_asr.get_or_insert_with(AsrProfiles::default);
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
        card.child(list)
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
