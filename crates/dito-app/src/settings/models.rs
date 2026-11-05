use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use dito_core::config::{Config, EngineConfig, ProviderKind, Source};
use dito_core::factory::LocalBackend;
use dito_core::health::EngineStatus;
use dito_core::i18n::Tr;
use dito_core::provider::TestOutcome;
use dito_core::provider::configured_asr::AsrProfiles;
use dito_core::secrets::{Role, account};
use dito_local::catalog::{ModelKind, ModelSpec};
use dito_local::{ModelState, Snapshot};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{Ctx, Page, watch_config};
use crate::assets::Ic;
use crate::services;
use crate::state::AppState;
use crate::ui::{self, BtnKind, Cb, MenuItem, MenuTone, TestState, cbk, cbk_idx, text};

/// Model-row layout (design 04d): every row occupies a fixed slot sized for the tallest state
/// (name + meta + status lines), so the section height never changes; state transitions animate
/// inside the slot. STATUS_H is the height of the engine-state pill that grows in under the memory line.
const ROW_SLOT_H: f32 = 112.;
const STATUS_H: f32 = 24.;
const EXPAND_MS: u64 = 200;

fn idx(r: Role) -> usize {
    match r {
        Role::Asr => 0,
        Role::Polish => 1,
    }
}

fn engine(cfg: &Config, r: Role) -> &EngineConfig {
    match r {
        Role::Asr => &cfg.asr,
        Role::Polish => &cfg.polish.engine,
    }
}

fn engine_mut(cfg: &mut Config, r: Role) -> &mut EngineConfig {
    match r {
        Role::Asr => &mut cfg.asr,
        Role::Polish => &mut cfg.polish.engine,
    }
}

#[derive(Clone, Copy)]
enum Field {
    AzureEndpoint,
    AzureDeployment,
    BedrockRegion,
    BedrockModel,
    CompatBase,
    CompatModel,
    EffortAzure,
    EffortBedrock,
    EffortCompat,
    KeyAzure,
    KeyCompat,
}

struct RoleInputs {
    azure_endpoint: Entity<InputState>,
    azure_deployment: Entity<InputState>,
    bedrock_region: Entity<InputState>,
    bedrock_model: Entity<InputState>,
    compat_base: Entity<InputState>,
    compat_model: Entity<InputState>,
    effort_azure: Entity<InputState>,
    effort_bedrock: Entity<InputState>,
    effort_compat: Entity<InputState>,
    key_azure: Entity<InputState>,
    key_compat: Entity<InputState>,
}

pub(super) struct ModalChanged;

pub struct ModelsPage {
    st: Arc<AppState>,
    editor: Option<Entity<super::asr_editor::AsrEditor>>,
    tab: Role,
    menu: Option<String>,
    select: Option<&'static str>,
    inputs: [RoleInputs; 2],
    tests: [TestState; 2],
    test_generations: [u64; 2],
    test_configs: [Option<(EngineConfig, dito_core::config::PreferredLanguage)>; 2],
    profiles: Vec<String>,
    last_status: String,
    /// The page's own motion scope: its controls' transient state, reset on navigation.
    motion: ui::MotionScope,
}

impl EventEmitter<ModalChanged> for ModelsPage {}

impl ModelsPage {
    pub fn new(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        watch_config(&st, cx);

        let mut rx = st.models.subscribe();
        cx.spawn(async move |this, cx| {
            while rx.changed().await.is_ok() {
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();

        // Engine status has no event stream; poll gently while the page exists.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(800)).await;
                let alive = this.update(cx, |t, cx| {
                    let cfg = t.st.cfg.get();
                    for role in [Role::Asr, Role::Polish] {
                        if t.test_configs[idx(role)].as_ref().is_some_and(|(e, lang)| {
                            e != engine(&cfg, role) || (role == Role::Asr && *lang != cfg.general.preferred_language)
                        }) {
                            t.tests[idx(role)] = TestState::Idle;
                            t.test_configs[idx(role)] = None;
                            cx.notify();
                        }
                    }
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

        let cfg = st.cfg.get();
        let mk_role = |role: Role, window: &mut Window, cx: &mut Context<Self>| {
            let e = engine(&cfg, role);
            let key = |kind| st.secrets.get(&account(role, kind)).unwrap_or_default();
            let asr = e.cloud_asr.clone().unwrap_or_default();
            let base = if role == Role::Asr {
                asr.active()["connection"]["base_url"].as_str().unwrap_or("")
            } else {
                &e.provider.compat.base_url
            };
            let model = if role == Role::Asr {
                asr.active()["connection"]["model"].as_str().unwrap_or("")
            } else {
                &e.provider.compat.model
            };
            let me = RoleInputs {
                azure_endpoint: make_input(window, cx, "https://…openai.azure.com/", &e.provider.azure.endpoint, false),
                azure_deployment: make_input(window, cx, "gpt-6.1-terra", &e.provider.azure.deployment, false),
                bedrock_region: make_input(window, cx, "ap-northeast-1", &e.provider.bedrock.region, false),
                bedrock_model: make_input(
                    window,
                    cx,
                    "anthropic.claude-sonnet-4-…",
                    &e.provider.bedrock.model_id,
                    false,
                ),
                compat_base: make_input(window, cx, "https://…", base, false),
                compat_model: make_input(window, cx, cfg.general.lang().tr(Tr::ModelsModelPlaceholder), model, false),
                effort_azure: make_input(
                    window,
                    cx,
                    cfg.general.lang().tr(Tr::ModelsEffortPlaceholder),
                    e.provider.azure.reasoning_effort.as_str(),
                    false,
                ),
                effort_bedrock: make_input(
                    window,
                    cx,
                    cfg.general.lang().tr(Tr::ModelsEffortPlaceholder),
                    e.provider.bedrock.reasoning_effort.as_str(),
                    false,
                ),
                effort_compat: make_input(
                    window,
                    cx,
                    cfg.general.lang().tr(Tr::ModelsEffortPlaceholder),
                    e.provider.compat.reasoning_effort.as_str(),
                    false,
                ),
                key_azure: make_input(window, cx, "••••••••", &key(ProviderKind::Azure), true),
                key_compat: make_input(
                    window,
                    cx,
                    "••••••••",
                    &if role == Role::Asr {
                        st.secrets.get(&asr.account()).unwrap_or_default()
                    } else {
                        key(ProviderKind::Compat)
                    },
                    true,
                ),
            };
            for (field, ent) in [
                (Field::AzureEndpoint, &me.azure_endpoint),
                (Field::AzureDeployment, &me.azure_deployment),
                (Field::BedrockRegion, &me.bedrock_region),
                (Field::BedrockModel, &me.bedrock_model),
                (Field::CompatBase, &me.compat_base),
                (Field::CompatModel, &me.compat_model),
                (Field::EffortAzure, &me.effort_azure),
                (Field::EffortBedrock, &me.effort_bedrock),
                (Field::EffortCompat, &me.effort_compat),
                (Field::KeyAzure, &me.key_azure),
                (Field::KeyCompat, &me.key_compat),
            ] {
                cx.subscribe_in(ent, window, move |this: &mut Self, state, ev: &InputEvent, _, cx| {
                    if matches!(ev, InputEvent::Change) {
                        let v = state.read(cx).value().to_string();
                        this.field_changed(role, field, v, cx);
                    }
                })
                .detach();
            }
            me
        };
        let inputs = [mk_role(Role::Asr, window, cx), mk_role(Role::Polish, window, cx)];
        let mut profiles = services::aws_profiles();
        if !profiles.contains(&cfg.asr.provider.bedrock.profile) && !cfg.asr.provider.bedrock.profile.is_empty() {
            profiles.push(cfg.asr.provider.bedrock.profile.clone());
        }
        Self {
            motion: ui::MotionScope::new(),
            st,
            editor: None,
            tab: Role::Asr,
            menu: None,
            select: None,
            inputs,
            tests: [TestState::Idle, TestState::Idle],
            test_generations: [0, 0],
            test_configs: [None, None],
            profiles,
            last_status: String::new(),
        }
    }

    fn open_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let editor = cx.new(|cx| super::asr_editor::AsrEditor::new(self.st.clone(), window, cx));
        cx.subscribe_in(&editor, window, |this, _, event: &super::asr_editor::Closed, window, cx| {
            if event.0 {
                let profiles = this.st.cfg.get().asr.cloud_asr.clone().unwrap_or_default();
                let value = profiles.active();
                this.inputs[0].compat_base.update(cx, |s, cx| {
                    s.set_value(value["connection"]["base_url"].as_str().unwrap_or(""), window, cx)
                });
                this.inputs[0]
                    .compat_model
                    .update(cx, |s, cx| s.set_value(value["connection"]["model"].as_str().unwrap_or(""), window, cx));
                let key = this.st.secrets.get(&profiles.account()).unwrap_or_default();
                this.inputs[0].key_compat.update(cx, |s, cx| s.set_value(key, window, cx));
                this.tests[0] = TestState::Idle;
            }
            this.editor = None;
            cx.emit(ModalChanged);
            cx.notify();
        })
        .detach();
        self.editor = Some(editor);
        cx.emit(ModalChanged);
        cx.notify();
    }

    #[cfg(feature = "screenshots")]
    pub(super) fn preview(
        &mut self,
        polish: bool,
        editor: Option<bool>,
        help: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tab = if polish { Role::Polish } else { Role::Asr };
        let config = self.st.cfg.get();
        let profiles = config.asr.cloud_asr.clone().unwrap_or_default();
        self.inputs[0].compat_base.update(cx, |s, cx| {
            s.set_value(profiles.active()["connection"]["base_url"].as_str().unwrap_or(""), window, cx)
        });
        self.inputs[0].compat_model.update(cx, |s, cx| {
            s.set_value(profiles.active()["connection"]["model"].as_str().unwrap_or(""), window, cx)
        });

        self.editor = None;
        if let Some(error) = editor {
            self.open_editor(window, cx);
            if let Some(view) = &self.editor {
                view.update(cx, |v, cx| v.preview(error, help, window, cx));
            }
        }
        cx.notify();
    }

    pub(super) fn editor_view(&self) -> Option<Entity<super::asr_editor::AsrEditor>> {
        self.editor.clone()
    }

    fn status_key(&self) -> String {
        let cfg = self.st.cfg.get();
        let mut k = String::new();
        for id in [&cfg.asr.local_model, &cfg.polish.engine.local_model] {
            k.push_str(&format!("{:?}|", self.st.local.status(id)));
        }
        k
    }

    fn field_changed(&mut self, role: Role, field: Field, v: String, cx: &mut Context<Self>) {
        // A changed setting invalidates the previous test result.
        self.tests[idx(role)] = TestState::Idle;
        if role == Role::Asr && matches!(field, Field::CompatBase | Field::CompatModel | Field::KeyCompat) {
            let profiles = self.st.cfg.get().asr.cloud_asr.clone().unwrap_or_default();
            if matches!(field, Field::KeyCompat) {
                let result = if v.is_empty() {
                    self.st.secrets.delete(&profiles.account())
                } else {
                    self.st.secrets.set(&profiles.account(), &v)
                };
                if let Err(error) = result {
                    tracing::warn!("could not store ASR key: {error}");
                }
            } else {
                let _ = self.st.cfg.update(|c| {
                    let p = c.asr.cloud_asr.get_or_insert_with(AsrProfiles::default);
                    let selected = p.selected;
                    if let Some(config) = p.profiles.get_mut(selected) {
                        config["connection"][if matches!(field, Field::CompatBase) { "base_url" } else { "model" }] =
                            v.into();
                    }
                });
            }
            cx.notify();
            return;
        }
        match field {
            Field::KeyAzure | Field::KeyCompat => {
                let kind = if matches!(field, Field::KeyAzure) { ProviderKind::Azure } else { ProviderKind::Compat };
                let acct = account(role, kind);
                let res = if v.is_empty() { self.st.secrets.delete(&acct) } else { self.st.secrets.set(&acct, &v) };
                if let Err(e) = res {
                    tracing::warn!("could not store API key: {e}");
                }
            }
            _ => {
                let r = self.st.cfg.update(|c| {
                    let p = &mut engine_mut(c, role).provider;
                    match field {
                        Field::EffortAzure => p.azure.reasoning_effort = v.into(),
                        Field::EffortBedrock => p.bedrock.reasoning_effort = v.into(),
                        Field::EffortCompat => p.compat.reasoning_effort = v.into(),
                        Field::AzureEndpoint => p.azure.endpoint = v,
                        Field::AzureDeployment => p.azure.deployment = v,
                        Field::BedrockRegion => p.bedrock.region = v,
                        Field::BedrockModel => p.bedrock.model_id = v,
                        Field::CompatBase => p.compat.base_url = v,
                        Field::CompatModel => p.compat.model = v,
                        Field::KeyAzure | Field::KeyCompat => {}
                    }
                });
                if let Err(e) = r {
                    tracing::warn!("could not save settings: {e}");
                }
            }
        }
        cx.notify();
    }

    fn update_engine(&self, role: Role, f: impl FnOnce(&mut EngineConfig) + 'static) {
        if let Err(e) = self.st.cfg.update(|c| f(engine_mut(c, role))) {
            tracing::warn!("could not save settings: {e}");
        }
    }

    fn run_test(&mut self, role: Role, cx: &mut Context<Self>) {
        if self.tests[idx(role)] == TestState::Running {
            return;
        }
        if role == Role::Asr {
            let profiles = self.st.cfg.get().asr.cloud_asr.clone().unwrap_or_default();
            if dito_core::provider::configured_asr::validate(profiles.active()).is_err() {
                return;
            }
            let _ = self.st.cfg.update(|c| {
                c.asr.cloud_asr = Some(profiles);
            });
        }
        let cfg = self.st.cfg.get();
        self.test_configs[idx(role)] = Some((engine(&cfg, role).clone(), cfg.general.preferred_language));
        self.test_generations[idx(role)] += 1;
        let generation = self.test_generations[idx(role)];
        self.tests[idx(role)] = TestState::Running;
        cx.notify();
        let st = self.st.clone();
        let tested_config = st.cfg.get();
        let tested_engine = engine(&tested_config, role).clone();
        cx.spawn(async move |this, cx| {
            let outcome = services::test_connection(st, role).await;
            let _ = this.update(cx, |t, cx| {
                if t.test_generations[idx(role)] != generation
                    || (role == Role::Asr
                        && t.st.cfg.get().general.preferred_language != tested_config.general.preferred_language)
                    || engine(&t.st.cfg.get(), role) != &tested_engine
                    || t.tests[idx(role)] != TestState::Running
                {
                    return;
                }
                t.tests[idx(role)] = match outcome {
                    TestOutcome::Ok { latency_ms } => TestState::Ok(latency_ms),
                    TestOutcome::Failed { kind, message } => TestState::Failed(format!("{kind:?}: {message}")),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn toggle_select(&self, cx: &mut Context<Self>, id: &'static str) -> Cb {
        cbk(cx, move |this: &mut Self, _, cx| {
            this.select = if this.select == Some(id) { None } else { Some(id) };
            cx.notify();
        })
    }

    // ---- local models ----

    fn model_row(&self, c: &Ctx, cx: &mut Context<Self>, role: Role, spec: &ModelSpec, snap: &Snapshot) -> AnyElement {
        let p = &c.p;
        let cfg = self.st.cfg.get();
        let selected = engine(&cfg, role).local_model == spec.id;
        let state = snap.get(&spec.id).cloned().unwrap_or(ModelState::NotInstalled);
        let status = self.st.local.status(&spec.id);
        let id = spec.id.clone();

        let more_open = self.menu.as_deref() == Some(id.as_str());
        let more = {
            let id2 = id.clone();
            let btn = ui::menu_button(
                SharedString::from(format!("more-{}-{id}", idx(role))),
                p,
                Ic::MoreV,
                more_open,
                cbk(cx, move |this: &mut Self, _, cx| {
                    // Keep menu clicks local to the menu control.
                    cx.stop_propagation();
                    this.menu = if this.menu.as_deref() == Some(id2.as_str()) { None } else { Some(id2.clone()) };
                    cx.notify();
                }),
            );
            // Keep presses within the menu area from reaching enclosing controls.
            let mut wrap = div()
                .relative()
                .id(ElementId::from(SharedString::from(format!("more-wrap-{}-{id}", idx(role)))))
                .child(btn)
                .on_click(ui::click_cb(cbk(cx, |_: &mut Self, _, cx| cx.stop_propagation())));
            if more_open {
                wrap = wrap.child(self.model_menu(c, cx, role, spec, selected));
            }
            wrap
        };

        let mut ctl = div().flex().items_center().gap(px(8.)).flex_none();
        let mut body = div().flex_1().min_w(px(0.)).flex().flex_col().gap(px(6.));

        // Ghost rows (not downloaded) dim their name; selection needs no tag — the active
        // plate and ring mark it.
        let ghost = state == ModelState::NotInstalled;
        let name_line = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(text(spec.name.clone(), 14., FontWeight::MEDIUM, p.text))
            .child(text(ui::fmt_bytes(spec.total_size()), 12., FontWeight::NORMAL, p.model_meta));
        if state != ModelState::Installed {
            body = body.child(name_line);
        }

        match &state {
            ModelState::Installed => {
                // Fixed-height slots with an animated interior (design 04d): idle rows show a
                // single line, the active row expands to meta + live status, easing between the
                // two inside the same slot. The section height never jumps — the expansion moves
                // from row to row.
                let running = matches!(status, EngineStatus::Running);
                let pill = !matches!(status, EngineStatus::Failed(_));
                let (dot, status_txt, status_fg) = match &status {
                    EngineStatus::Running => (p.model_running_dot, c.t(Tr::ModelsRunning), p.model_running_text),
                    EngineStatus::Loading => (p.primary, c.t(Tr::MenubarLoading), p.primary),
                    EngineStatus::Failed(reason) => (
                        p.danger,
                        SharedString::from(format!("{}：{reason}", c.t(Tr::ModelsFailed))),
                        if p.dark { p.danger } else { p.danger_text },
                    ),
                    _ => (p.model_meta, c.t(Tr::MenubarStopped), p.model_meta),
                };
                let (name, size_txt) = (spec.name.clone(), SharedString::from(ui::fmt_bytes(spec.total_size())));
                let mem_txt =
                    SharedString::from(format!("{} {:.1} GB", c.t(Tr::ModelsMemory), spec.memory_mb as f32 / 1000.0));
                let name_c = p.text;
                let muted_c = p.model_meta;
                let palette = *p;
                let downloaded = c.t(Tr::ModelsDownloaded);
                let expand_id = ElementId::from(SharedString::from(format!("model-expand-{}-{}", idx(role), spec.id)));
                // One progress value `t` (0 idle, 1 in use) drives the whole block, so nothing pops: the "downloaded"
                // tag fades while the block rises, the memory line only rises, and the status pill grows in from
                // nothing. The block is vertically centred in the card, so growing it is what moves the lines up.
                body =
                    body.child(ui::tweened(expand_id, if selected { 1. } else { 0. }, EXPAND_MS, move |t| {
                        let mut name_line = div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .h(px(20. + 3. * (1. - t)))
                            .child(text(name.clone(), 14., FontWeight::MEDIUM, name_c))
                            .child(text(size_txt.clone(), 12., FontWeight::NORMAL, muted_c));
                        if t < 0.99 {
                            name_line =
                                name_line.child(
                                    div().px(px(8.)).py(px(3.)).rounded(px(6.)).bg(palette.seg).opacity(1. - t).child(
                                        text(downloaded.clone(), 11., FontWeight::MEDIUM, palette.model_badge_text),
                                    ),
                                );
                        }
                        let mut col = div().flex().flex_col().child(name_line).child(div().mt(px(6.)).child(text(
                            mem_txt.clone(),
                            12.,
                            FontWeight::NORMAL,
                            muted_c,
                        )));
                        if t > 0.01 {
                            col = col.child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .mt(px(6. * t))
                                    .h(px(STATUS_H * t))
                                    .overflow_hidden()
                                    .opacity(t)
                                    .child(
                                        // Engine state as a pill (design 20.04, model card 7): green while running, grey
                                        // while loading or stopped. A failure keeps plain text because it carries a reason.
                                        div()
                                            .flex()
                                            .flex_none()
                                            .self_start()
                                            .items_center()
                                            .gap(px(6.))
                                            .when(pill, |d| {
                                                let bg = if running { palette.model_running_bg } else { palette.seg };
                                                d.h(px(STATUS_H)).px(px(8.)).rounded_full().bg(bg)
                                            })
                                            .child(div().size(px(6.)).rounded_full().bg(dot))
                                            .child(
                                                text(status_txt.clone(), 12., FontWeight::MEDIUM, status_fg)
                                                    .whitespace_nowrap(),
                                            ),
                                    ),
                            );
                        }
                        col
                    }));
                let use_action = if selected {
                    div()
                        .h(px(36.))
                        .px(px(16.))
                        .rounded_full()
                        .bg(p.model_in_use_pill_bg)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(text(c.t(Tr::ModelsInUse), 13., FontWeight::MEDIUM, p.model_in_use_pill_text))
                        .into_any_element()
                } else {
                    let id_use = id.clone();
                    ui::button(
                        SharedString::from(format!("use-{id}")),
                        p,
                        &c.t(Tr::ModelsUse),
                        BtnKind::Outline,
                        None,
                        cbk(cx, move |this: &mut Self, _, cx| {
                            cx.stop_propagation();
                            let prev = engine(&this.st.cfg.get(), role).local_model.clone();
                            let hand_over =
                                matches!(this.st.local.status(&prev), EngineStatus::Running | EngineStatus::Loading);
                            let selected = id_use.clone();
                            this.update_engine(role, move |e| e.local_model = selected);
                            if hand_over {
                                this.st.local.unload(&prev);
                                let local = this.st.local.clone();
                                let id = id_use.clone();
                                this.st.rt.spawn(async move {
                                    local.restart(&id).await;
                                });
                            }
                            cx.notify();
                        }),
                    )
                    .into_any_element()
                };
                ctl = ctl.child(use_action).child(more);
            }
            ModelState::Downloading { done, total, .. } => {
                let frac = *done as f32 / (*total).max(1) as f32;
                // Plain status language: "780 MB / 1.0 GB（残り約 20 秒）" — no dot-separated run-ons.
                let progress = match state.eta_secs() {
                    Some(s) => {
                        format!("{} / {}（{}）", ui::fmt_bytes(*done), ui::fmt_bytes(*total), ui::fmt_eta(c.lang, s))
                    }
                    None => format!("{} / {}", ui::fmt_bytes(*done), ui::fmt_bytes(*total)),
                };
                body =
                    body.child(ui::progress_bar(p, frac)).child(text(progress, 12., FontWeight::NORMAL, p.model_meta));
                let (id_a, id_b) = (id.clone(), id.clone());
                ctl = ctl
                    .child(ui::button(
                        SharedString::from(format!("pause-{id}")),
                        p,
                        &c.t(Tr::ModelsPause),
                        BtnKind::Secondary,
                        Some(Ic::Pause),
                        cbk(cx, move |this: &mut Self, _, _| this.st.models.pause(&id_a)),
                    ))
                    .child(ui::button(
                        SharedString::from(format!("cancel-{id}")),
                        p,
                        &c.t(Tr::ModelsCancel),
                        BtnKind::Danger,
                        Some(Ic::X),
                        cbk(cx, move |this: &mut Self, _, _| this.st.models.cancel(&id_b)),
                    ));
            }
            ModelState::Verifying { done, total } => {
                body = body.child(ui::progress_bar(p, *done as f32 / (*total).max(1) as f32)).child(text(
                    format!("{}（{} / {}）", c.t(Tr::ModelsVerifying), ui::fmt_bytes(*done), ui::fmt_bytes(*total)),
                    12.,
                    FontWeight::NORMAL,
                    p.model_meta,
                ));
                let id2 = id.clone();
                ctl = ctl.child(ui::button(
                    SharedString::from(format!("cancel-{id}")),
                    p,
                    &c.t(Tr::ModelsCancel),
                    BtnKind::Danger,
                    Some(Ic::X),
                    cbk(cx, move |this: &mut Self, _, _| {
                        // "Cancel" during verification keeps the completed bytes (pause
                        // semantics): a restart skips the download and re-verifies, instead
                        // of throwing away a finished multi-GB fetch.
                        this.st.models.pause(&id2);
                    }),
                ));
            }
            ModelState::Paused { done, total } => {
                body = body.child(ui::progress_bar(p, *done as f32 / (*total).max(1) as f32));
                let (id_a, id_b) = (id.clone(), id.clone());
                ctl = ctl
                    .child(ui::button(
                        SharedString::from(format!("resume-{id}")),
                        p,
                        &c.t(Tr::MenubarResume),
                        BtnKind::Secondary,
                        Some(Ic::Play),
                        cbk(cx, move |this: &mut Self, _, _| {
                            let _ = this.st.models.start_download(&id_a);
                        }),
                    ))
                    .child(ui::button(
                        SharedString::from(format!("cancel-{id}")),
                        p,
                        &c.t(Tr::ModelsCancel),
                        BtnKind::Danger,
                        Some(Ic::X),
                        cbk(cx, move |this: &mut Self, _, _| this.st.models.cancel(&id_b)),
                    ));
            }
            ModelState::Failed { message, done, total } => {
                // All bytes on disk → the checksum pass failed; otherwise the download itself.
                let title = if *done >= *total { c.t(Tr::ModelsVerifyFailed) } else { c.t(Tr::ModelsDownloadFailed) };
                body = body.child(text(title, 12., FontWeight::MEDIUM, p.model_error_text)).child(text(
                    format!("{}（{} / {}）", message, ui::fmt_bytes(*done), ui::fmt_bytes(*total)),
                    12.,
                    FontWeight::NORMAL,
                    p.model_meta,
                ));
                let (id_a, id_b) = (id.clone(), id.clone());
                ctl = ctl
                    .child(ui::button(
                        SharedString::from(format!("retry-{id}")),
                        p,
                        &c.t(Tr::HistoryRetry),
                        BtnKind::Secondary,
                        Some(Ic::Refresh),
                        cbk(cx, move |this: &mut Self, _, _| {
                            let _ = this.st.models.start_download(&id_a);
                        }),
                    ))
                    .child(ui::button(
                        SharedString::from(format!("cancel-{id}")),
                        p,
                        &c.t(Tr::ModelsCancel),
                        BtnKind::Danger,
                        Some(Ic::X),
                        cbk(cx, move |this: &mut Self, _, _| this.st.models.cancel(&id_b)),
                    ));
            }
            ModelState::NotInstalled => {
                body = body.child(text(c.t(Tr::ModelsDownloadAvailable), 12., FontWeight::NORMAL, p.model_download));
                let id2 = id.clone();
                ctl = ctl.child(ui::button(
                    SharedString::from(format!("dl-{}-{id}", idx(role))),
                    p,
                    &c.t(Tr::MenubarDownload),
                    BtnKind::Download,
                    Some(Ic::Download),
                    cbk(cx, move |this: &mut Self, _, _| {
                        let _ = this.st.models.start_download(&id2);
                    }),
                ));
            }
        }

        // Card plates (design 20.04, Model card states): a dashed blue plate for models still to download, a gold
        // plate with a 3 pt border for the one in use, an outlined plate for downloaded ones whose border darkens
        // on hover, and a plain plate while a download is in progress.
        let tile_id = ElementId::from(SharedString::from(format!("model-row-{}-{}", idx(role), spec.id)));
        let plate = || div().absolute().inset_0().rounded(px(12.));
        let tile: AnyElement = if ghost {
            plate()
                .bg(p.model_download_bg)
                .border(px(1.5))
                .border_dashed()
                .border_color(p.model_download_border)
                .into_any_element()
        } else if selected {
            plate().bg(p.model_in_use_bg).border(px(3.)).border_color(p.model_in_use_border).into_any_element()
        } else if state == ModelState::Installed {
            let (bg, idle_c, hover_c) = (p.model_idle_bg, p.model_idle_border, p.model_idle_border_hover);
            let hover = ui::is_hovered(&tile_id);
            ui::tweened(tile_id.clone(), if hover { 1. } else { 0. }, ui::HOVER_MS, move |t| {
                plate().bg(bg).border_1().border_color(ui::mix(idle_c, hover_c, t)).into_any_element()
            })
        } else {
            plate().bg(p.row_idle).into_any_element()
        };

        let base = div()
            .w_full()
            .h(px(ROW_SLOT_H))
            .relative()
            // The plate's border is a separate absolute layer, so it never moves the content: the same inset in every
            // state keeps the text from jumping sideways when a card is selected.
            .px(px(22.))
            .py(px(18.))
            .flex()
            .items_center()
            .gap(px(16.))
            .rounded(px(12.))
            .child(tile)
            .child(body)
            .child(ctl);
        // Rows provide hover feedback only; selection belongs to the explicit Use button.
        // Track selected rows too so deselecting under the pointer cannot leave a stale hover.
        if state == ModelState::Installed {
            base.id(tile_id.clone()).on_hover(ui::hover_tracker(tile_id)).into_any_element()
        } else {
            base.into_any_element()
        }
    }

    fn model_menu(&self, c: &Ctx, cx: &mut Context<Self>, role: Role, spec: &ModelSpec, selected: bool) -> Div {
        let id = spec.id.clone();
        let close = cbk(cx, |this: &mut Self, _, cx| {
            this.menu = None;
            cx.notify();
        });
        let mut items = Vec::new();
        // Engine commands follow the runtime (04d): Stop + Restart while the engine runs, Start
        // otherwise. Starting a non-selected model also switches the engine to it.
        let running = matches!(self.st.local.status(&id), EngineStatus::Running | EngineStatus::Loading);
        if running && selected {
            let (st, id2) = (self.st.clone(), id.clone());
            items.push(MenuItem {
                icon: Some(Ic::Stop),
                label: c.t(Tr::ModelsStop),
                tone: MenuTone::Default,
                on_click: Rc::new(move |_, _| st.local.unload(&id2)),
            });
            let (st, id2) = (self.st.clone(), id.clone());
            items.push(MenuItem {
                icon: Some(Ic::Refresh),
                label: c.t(Tr::ModelsRestart),
                tone: MenuTone::Default,
                on_click: Rc::new(move |_, _| {
                    let (local, id3) = (st.local.clone(), id2.clone());
                    st.rt.spawn(async move {
                        local.restart(&id3).await;
                    });
                }),
            });
        } else {
            let (st, id2) = (self.st.clone(), id.clone());
            items.push(MenuItem {
                icon: Some(Ic::Play),
                label: c.t(Tr::ModelsStart),
                tone: MenuTone::Success,
                on_click: Rc::new(move |_, _| {
                    let _ = st.cfg.update(|cfg| engine_mut(cfg, role).local_model = id2.clone());
                    let (local, id3) = (st.local.clone(), id2.clone());
                    st.rt.spawn(async move {
                        local.restart(&id3).await;
                    });
                }),
            });
        }
        let (st, id2) = (self.st.clone(), id.clone());
        items.push(MenuItem {
            icon: Some(Ic::Download),
            label: c.t(Tr::ModelsDownloadAgain),
            tone: MenuTone::Default,
            on_click: Rc::new(move |_, _| {
                if st.models.delete(&id2).is_ok() {
                    let _ = st.models.start_download(&id2);
                }
            }),
        });
        let dir = self.st.models.model_dir(&id);
        items.push(MenuItem {
            icon: Some(Ic::External),
            label: c.t(Tr::MenubarShowInFinder),
            tone: MenuTone::Default,
            on_click: Rc::new(move |_, _| {
                let _ = std::process::Command::new("open").arg(&dir).spawn();
            }),
        });
        let (st, id2) = (self.st.clone(), id);
        items.push(MenuItem {
            icon: Some(Ic::Trash),
            label: c.t(Tr::ModelsDelete),
            tone: MenuTone::Danger,
            on_click: Rc::new(move |_, _| {
                st.local.unload(&id2);
                let _ = st.models.delete(&id2);
            }),
        });
        ui::popup_menu(SharedString::from(format!("menu-{}", spec.id)), &c.p, items, close)
    }

    fn storage_card(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = &c.p;
        let s = self.st.models.storage();
        let total = (s.used_bytes + s.free_bytes).max(1);
        let dir = s.dir.clone();
        let dir2 = s.dir.clone();
        ui::card(p)
            .px(px(26.))
            .py(px(18.))
            .gap(px(12.))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(16.))
                    .child(
                        // Let the path column shrink (with an ellipsis) instead of pushing the
                        // buttons out of the card on narrow windows.
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(text(c.t(Tr::ModelsFolder), 14., FontWeight::BOLD, p.text))
                            .child(ui::muted12(p, s.dir.display().to_string()).truncate()),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(10.))
                            .child(ui::button(
                                "reveal",
                                p,
                                &c.t(Tr::MenubarShowInFinder),
                                BtnKind::Secondary,
                                Some(Ic::External),
                                Rc::new(move |_, _| {
                                    let _ = std::fs::create_dir_all(&dir);
                                    let _ = std::process::Command::new("open").arg(&dir).spawn();
                                }),
                            ))
                            .child(ui::button(
                                "change-dir",
                                p,
                                &c.t(Tr::ModelsChangeFolder),
                                BtnKind::Secondary,
                                None,
                                cbk(cx, move |this: &mut Self, _, cx| {
                                    let _ = &dir2;
                                    this.choose_folder(cx);
                                }),
                            )),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .justify_between()
                            .child(text(
                                format!("{} {}", c.t(Tr::ModelsInUse), ui::fmt_bytes(s.used_bytes)),
                                12.,
                                FontWeight::MEDIUM,
                                p.text,
                            ))
                            .child(ui::muted12(p, format!("{} {}", c.t(Tr::MenubarFree), ui::fmt_bytes(s.free_bytes)))),
                    )
                    .child(ui::progress_bar(p, s.used_bytes as f32 / total as f32)),
            )
    }

    fn choose_folder(&mut self, cx: &mut Context<Self>) {
        let rx =
            cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: None });
        let st = self.st.clone();
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await
                && let Some(dir) = paths.into_iter().next()
                && let Err(e) = st.models.change_dir(&dir.join("Dito Models"))
            {
                tracing::warn!("could not move models: {e}");
            }
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    // ---- provider forms ----

    fn provider_tiles(&self, c: &Ctx, cx: &mut Context<Self>, role: Role, kind: ProviderKind) -> Div {
        let p = &c.p;
        let mut row = div().w_full().flex().gap(px(12.));
        if role == Role::Asr {
            let profiles = self.st.cfg.get().asr.cloud_asr.clone().unwrap_or_default();
            for (i, name) in AsrProfiles::NAMES.iter().enumerate() {
                let on = profiles.selected == i;
                let card_id = ElementId::from(SharedString::from(format!("asr-preset-{i}")));
                row = row.child(
                    div()
                        .id(card_id.clone())
                        .flex_1()
                        .h(px(56.))
                        .px(px(18.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .rounded(px(14.))
                        .cursor_pointer()
                        .bg(if on { p.accent_soft } else { p.card })
                        .relative()
                        .border_1()
                        .border_color(ui::transparent())
                        .on_hover(ui::hover_tracker(card_id.clone()))
                        .child(ui::hover_fill_if(&card_id, Some(14.), ui::hex_alpha(p.seg, 0.6), !on))
                        .child(ui::ring_overlay(
                            SharedString::from(format!("asr-preset-ring-{i}")),
                            on,
                            14.,
                            p.border_strong,
                            p.accent,
                        ))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            let _ = this
                                .st
                                .cfg
                                .update(|c| c.asr.cloud_asr.get_or_insert_with(AsrProfiles::default).selected = i);
                            let profiles = this.st.cfg.get().asr.cloud_asr.clone().unwrap_or_default();
                            let value = profiles.active();
                            this.inputs[0].compat_base.update(cx, |s, cx| {
                                s.set_value(value["connection"]["base_url"].as_str().unwrap_or(""), window, cx)
                            });
                            this.inputs[0].compat_model.update(cx, |s, cx| {
                                s.set_value(value["connection"]["model"].as_str().unwrap_or(""), window, cx)
                            });
                            let key = this.st.secrets.get(&profiles.account()).unwrap_or_default();
                            this.inputs[0].key_compat.update(cx, |s, cx| s.set_value(key, window, cx));
                            this.tests[0] = TestState::Idle;
                            cx.notify();
                        }))
                        .child(ui::choice_label(&card_id, p, SharedString::from(*name), 14., on))
                        .child(ui::radio(p, on)),
                );
            }
            return row;
        }
        for k in ProviderKind::ALL {
            let on = k == kind;
            let card_id = ElementId::from(SharedString::from(format!("prov-{}-{}", idx(role), k.id())));
            row = row.child(
                div()
                    .id(card_id.clone())
                    .flex_1()
                    .h(px(56.))
                    .px(px(18.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .rounded(px(14.))
                    .cursor_pointer()
                    .bg(if on { p.accent_soft } else { p.card })
                    .relative()
                    .border_1()
                    .border_color(ui::transparent())
                    .on_hover(ui::hover_tracker(card_id.clone()))
                    .child(ui::hover_fill_if(&card_id, Some(14.), ui::hex_alpha(p.seg, 0.6), !on))
                    .child(ui::ring_overlay(
                        SharedString::from(format!("prov-ring-{}-{}", idx(role), k.id())),
                        on,
                        14.,
                        p.border_strong,
                        p.accent,
                    ))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.update_engine(role, move |e| e.provider.kind = k);
                        this.tests[idx(role)] = TestState::Idle;
                        cx.notify();
                    }))
                    .child(ui::choice_label(&card_id, p, c.lang.tr(k.label()).into(), 14., on))
                    .child(ui::radio(p, on)),
            );
        }
        row
    }

    fn provider_fields(&self, c: &Ctx, cx: &mut Context<Self>, role: Role) -> Div {
        let p = &c.p;
        let cfg = self.st.cfg.get();
        let e = engine(&cfg, role);
        let inp = &self.inputs[idx(role)];
        let input = |ent: &Entity<InputState>| ui::text_input(p, ent, None).form();
        let wide =
            |label: SharedString, ent: &Entity<InputState>| ui::form_field(p, label, input(ent)).flex_1().min_w(px(0.));
        let key =
            |ent: &Entity<InputState>| ui::form_field(p, c.t(Tr::ModelsApiKey), input(ent)).flex_1().min_w(px(0.));
        let hint = || text(c.t(Tr::ModelsReasoningHint), 12., FontWeight::NORMAL, p.muted);
        let llm = role == Role::Polish;
        let mut col = div().w_full().flex().flex_col().gap(px(18.));
        if role == Role::Asr {
            return col
                .child(ui::form_field(p, c.t(Tr::ModelsBaseUrl), input(&inp.compat_base)))
                .child(ui::form_field(p, c.t(Tr::ModelsModelId), input(&inp.compat_model)))
                .child(ui::form_field(p, c.t(Tr::ModelsApiKey), input(&inp.key_compat)))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(text(c.t(Tr::ModelsConnectionSettings), 14., FontWeight::MEDIUM, p.text))
                        .child(ui::button(
                            "asr-edit",
                            p,
                            &c.t(Tr::AsrEditorEdit),
                            BtnKind::Subtle,
                            None,
                            cbk(cx, |this: &mut Self, w, cx| this.open_editor(w, cx)),
                        )),
                );
        }
        match e.provider.kind {
            ProviderKind::Azure => {
                col = col.child(ui::form_field(p, c.t(Tr::ModelsEndpoint), input(&inp.azure_endpoint)));
                let mut r =
                    div().w_full().flex().gap(px(16.)).child(wide(c.t(Tr::ModelsDeployment), &inp.azure_deployment));
                if llm {
                    r = r.child(div().w(px(200.)).flex_none().child(ui::form_field(
                        p,
                        c.t(Tr::ModelsReasoningEffort),
                        input(&inp.effort_azure),
                    )));
                    col = col.child(r).child(hint()).child(ui::form_field(
                        p,
                        c.t(Tr::ModelsApiKey),
                        input(&inp.key_azure),
                    ));
                } else {
                    col = col.child(r.child(key(&inp.key_azure)));
                }
            }
            ProviderKind::Bedrock => {
                let profile_opts: Vec<SharedString> =
                    self.profiles.iter().map(|s| SharedString::from(s.clone())).collect();
                let sel = self.profiles.iter().position(|n| *n == e.provider.bedrock.profile).unwrap_or(0);
                let (st, profiles) = (self.st.clone(), self.profiles.clone());
                let profile = if self.profiles.is_empty() {
                    // Nothing to pick: the control stays visible but inert.
                    ui::select_disabled(p, c.t(Tr::ModelsNoProfiles))
                } else {
                    ui::select(
                        "bedrock-profile",
                        p,
                        &profile_opts,
                        sel,
                        self.select == Some("bedrock-profile"),
                        None,
                        self.toggle_select(cx, "bedrock-profile"),
                        Rc::new(move |i, _, _| {
                            if let Some(name) = profiles.get(i).cloned() {
                                let _ = st.cfg.update(|cfg| engine_mut(cfg, role).provider.bedrock.profile = name);
                            }
                        }),
                    )
                };
                col = col.child(ui::form_field(p, c.t(Tr::ModelsRegion), input(&inp.bedrock_region)));
                let mut r = div().w_full().flex().gap(px(16.)).child(wide(c.t(Tr::ModelsModelId), &inp.bedrock_model));
                if llm {
                    r = r.child(div().w(px(200.)).flex_none().child(ui::form_field(
                        p,
                        c.t(Tr::ModelsReasoningEffort),
                        input(&inp.effort_bedrock),
                    )));
                }
                if llm {
                    col = col.child(r).child(hint());
                } else {
                    col = col.child(r);
                }
                col = col.child(ui::form_field(p, c.t(Tr::ModelsAwsProfile), profile));
            }
            ProviderKind::Compat => {
                col = col.child(ui::form_field(p, c.t(Tr::ModelsBaseUrl), input(&inp.compat_base)));
                let mut r = div().w_full().flex().gap(px(16.)).child(wide(c.t(Tr::NavModels), &inp.compat_model));
                if llm {
                    r = r.child(div().w(px(200.)).flex_none().child(ui::form_field(
                        p,
                        c.t(Tr::ModelsReasoningEffort),
                        input(&inp.effort_compat),
                    )));
                    col = col.child(r).child(hint()).child(ui::form_field(
                        p,
                        c.t(Tr::ModelsApiKey),
                        input(&inp.key_compat),
                    ));
                } else {
                    col = col.child(r.child(key(&inp.key_compat)));
                }
            }
        }
        col
    }

    fn test_footer(&self, c: &Ctx, cx: &mut Context<Self>, role: Role) -> Div {
        let p = &c.p;
        let state = &self.tests[idx(role)];
        let mut ready = true;
        if role == Role::Asr {
            let profiles = self.st.cfg.get().asr.cloud_asr.clone().unwrap_or_default();
            ready = dito_core::provider::configured_asr::validate(profiles.active()).is_ok()
                && (!profiles.active()["request"].to_string().contains("{{api_key}}")
                    || !self.st.secrets.get(&profiles.account()).unwrap_or_default().trim().is_empty());
        }
        // Test state as a small tag (design 20.05): neutral while unchecked or running, green on success with the
        // response time beside it, and the danger tones on failure with the reason underneath.
        let (label, bg, fg, weight) = match state {
            TestState::Idle => (c.t(Tr::ModelsUnchecked), p.seg, p.muted, FontWeight::NORMAL),
            TestState::Running => (c.t(Tr::ModelsChecking), p.seg, p.muted, FontWeight::NORMAL),
            TestState::Ok(_) => (c.t(Tr::ModelsTestOk), p.model_running_bg, p.model_running_text, FontWeight::MEDIUM),
            TestState::Failed(_) => {
                (c.t(Tr::ModelsConnectionError), p.button_danger_bg, p.button_danger_text, FontWeight::MEDIUM)
            }
        };
        let tag = div()
            .flex_none()
            .px(px(10.))
            .py(px(4.))
            .rounded(px(6.))
            .bg(bg)
            .child(text(label, 12., weight, fg).line_height(relative(1.5)));
        let mut status = div().flex().items_center().gap(px(12.)).child(tag);
        if role == Role::Polish
            && let TestState::Ok(ms) = state
        {
            status = status.child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(6.))
                    .child(text(c.t(Tr::ModelsLatency), 12., FontWeight::NORMAL, p.muted))
                    .child(text(ms.to_string(), 13., FontWeight::MEDIUM, p.text))
                    .child(text("ms", 12., FontWeight::NORMAL, p.muted)),
            );
        }
        let mut left = div().flex_1().min_w(px(0.)).flex().flex_col().gap(px(6.)).child(status);
        if let TestState::Failed(message) = state {
            left = left.child(text(ui::error_line(c.lang, message), 12., FontWeight::NORMAL, p.text));
        }
        let enabled = ready && *state != TestState::Running;
        let button = if enabled {
            ui::button(
                SharedString::from(format!("test-{}", idx(role))),
                p,
                &c.t(Tr::ModelsTestConnection),
                BtnKind::Subtle,
                None,
                cbk(cx, move |this: &mut Self, _, cx| this.run_test(role, cx)),
            )
            .w(px(140.))
            .into_any_element()
        } else {
            div()
                .h(px(36.))
                .w(px(140.))
                .rounded_full()
                .bg(p.seg)
                .flex()
                .items_center()
                .justify_center()
                .child(text(c.t(Tr::ModelsTestConnection), 13., FontWeight::MEDIUM, p.muted))
                .into_any_element()
        };
        div()
            .w_full()
            .pt(px(16.))
            .border_t_1()
            .border_color(p.border)
            .flex()
            .items_center()
            .gap(px(16.))
            .child(left)
            .child(button)
    }
}

fn make_input(
    window: &mut Window,
    cx: &mut Context<ModelsPage>,
    placeholder: &str,
    value: &str,
    masked: bool,
) -> Entity<InputState> {
    let (ph, val) = (placeholder.to_string(), value.to_string());
    let state = cx.new(|cx| {
        let mut s = InputState::new(window, cx).placeholder(ph).default_value(val);
        if masked {
            s = s.masked(true);
        }
        s
    });
    ui::notify_on_focus(&state, window, cx);
    state
}

impl ModelsPage {
    /// Reset the page's transient motion state; its controls mount statically on the next entry.
    pub(super) fn reset_motion(&mut self) {
        self.motion.reset();
    }

    /// The page's scope, for the screenshot harness.
    #[cfg(feature = "screenshots")]
    pub(super) fn motion_scope(&self) -> ui::ScopeHandle {
        self.motion.handle()
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
        let cfg = self.st.cfg.get();
        let role = self.tab;
        let e = engine(&cfg, role).clone();
        let is_local = e.source == Source::Local;
        let snap = self.st.models.subscribe().borrow().clone();

        let tabs = ui::seg(
            "models-tab",
            &p,
            &[c.t(Tr::ModelsDictation), c.t(Tr::ModelsPolishShort)],
            idx(role),
            cbk_idx(cx, |this: &mut Self, i, _, cx| {
                this.tab = if i == 0 { Role::Asr } else { Role::Polish };
                this.menu = None;
                this.select = None;
                cx.notify();
            }),
        );

        let (title, desc) = match role {
            Role::Asr => (c.t(Tr::ModelsDictation), c.t(Tr::ModelsDictationDesc)),
            Role::Polish => (c.t(Tr::ModelsPolishShort), c.t(Tr::ModelsPolishDesc)),
        };
        let source_seg = ui::seg(
            ("source", idx(role)),
            &p,
            &[c.t(Tr::ModelsLocal), c.t(Tr::ModelsProvider)],
            if is_local { 0 } else { 1 },
            {
                let st = self.st.clone();
                Rc::new(move |i, _, _| {
                    let _ = st.cfg.update(|cfg| {
                        engine_mut(cfg, role).source = if i == 0 { Source::Local } else { Source::Provider };
                        if role == Role::Asr && i == 1 {
                            cfg.asr.cloud_asr.get_or_insert_with(AsrProfiles::default);
                        }
                    });
                })
            },
        );

        let mut card = ui::card(&p).p(px(26.)).gap(px(20.)).child(
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
                        .when(is_local || role == Role::Polish, |d| {
                            d.child(text(desc, 13., FontWeight::NORMAL, p.muted))
                        }),
                )
                .child(source_seg),
        );

        let mut sections: Vec<AnyElement> = Vec::new();
        if is_local {
            let kind = if role == Role::Asr { ModelKind::Asr } else { ModelKind::Polish };
            let specs: Vec<ModelSpec> = self.st.models.catalog().iter().filter(|m| m.kind == kind).cloned().collect();
            // Downloads for the *other* tab keep running; surface them here so switching tabs
            // never looks like the transfer silently stopped.
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
                    (spec.kind != kind)
                        .then(|| format!("{} {:.0}%", spec.name, done as f32 / (total).max(1) as f32 * 100.))
                })
                .collect();
            if !elsewhere.is_empty() {
                let sep = if c.lang == dito_core::i18n::Lang::Ja { "、" } else { ", " };
                card = card.child(ui::muted12(&p, format!("{}{}", c.t(Tr::ModelsDownloadingBg), elsewhere.join(sep))));
            }
            let mut list = div().w_full().flex().flex_col().gap(px(8.));
            for s in specs.iter() {
                list = list.child(self.model_row(&c, cx, role, s, &snap));
            }
            card = card.child(list);
            sections.push(self.storage_card(&c, cx).into_any_element());
        } else {
            card = card
                .child(self.provider_tiles(&c, cx, role, e.provider.kind))
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
                    let st = super::global_state(cx);
                    super::open(st, Page::Status, cx);
                }),
            )
            .into_any_element()
        });

        div().relative().size_full().child(super::page(
            p,
            "models",
            c.t(Tr::NavModels),
            actions,
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(24.))
                .child(div().flex().child(tabs))
                .child(card)
                .children(sections),
        ))
    }
}
