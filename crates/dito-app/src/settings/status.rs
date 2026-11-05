use std::sync::Arc;
use std::time::Duration;

use dito_core::config::Source;
use dito_core::factory::LocalBackend;
use dito_core::health::EngineStatus;
use dito_core::i18n::Tr;
use dito_core::provider::TestOutcome;
use dito_core::secrets::Role;
use dito_platform::{Permission, Status, permissions};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{Ctx, Page, watch_config};
use crate::assets::Ic;
use crate::services;
use crate::state::AppState;
use crate::theme::Palette;
use crate::ui::{self, BtnKind, TestState, cbk, text};

/// Status cards are read-only: starting, restarting, retrying and enabling live in Models or
/// General settings. This page reports engine state, provider facts, permissions — nothing else.
pub struct StatusPage {
    st: Arc<AppState>,
    /// Last provider connection check per role; probed once on open and via "recheck all".
    checks: [TestState; 2],
    checked_at: [Option<u64>; 2],
    perms: [Status; 2],
    /// The page's own motion scope: its controls' transient state, reset on navigation.
    motion: ui::MotionScope,
}

const PERMS: [Permission; 2] = [Permission::Microphone, Permission::Accessibility];

fn read_perms() -> [Status; 2] {
    PERMS.map(permissions::status)
}

fn role_idx(r: Role) -> usize {
    if r == Role::Asr { 0 } else { 1 }
}

fn now_epoch() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Local wall-clock "HH:MM" stamp for the last check; offset comes from the platform.
fn hh_mm(epoch: Option<u64>) -> String {
    match epoch {
        None => "—".into(),
        Some(e) => {
            let local = (e as i64 + dito_platform::local_utc_offset_secs() as i64).rem_euclid(86_400);
            format!("{:02}:{:02}", local / 3600, (local % 3600) / 60)
        }
    }
}

fn url_host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split('/').next().unwrap_or(rest).to_string()
}

/// Azure accounts surface as the resource name inside the endpoint host.
fn azure_resource(endpoint: &str) -> String {
    let host = url_host(endpoint);
    host.strip_suffix(".openai.azure.com").unwrap_or(&host).to_string()
}

impl StatusPage {
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

impl Render for StatusPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The page draws under its own scope: after a page switch its controls mount statically,
        // never sliding to values remembered from another page or an earlier visit.
        ui::with_motion_scope(self.motion.handle(), || self.view(window, cx))
    }
}

impl StatusPage {
    pub fn new(st: Arc<AppState>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        watch_config(&st, cx);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(1500)).await;
                let perms = read_perms();
                if this
                    .update(cx, |t, cx| {
                        t.perms = perms;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        // Probe provider connections once so cards can show latency without a manual recheck.
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(350)).await;
            let _ = this.update(cx, |t, cx| t.probe_providers(cx));
        })
        .detach();
        Self {
            motion: ui::MotionScope::new(),
            st,
            checks: [TestState::Idle, TestState::Idle],
            checked_at: [None, None],
            perms: read_perms(),
        }
    }

    fn probe_providers(&mut self, cx: &mut Context<Self>) {
        let cfg = self.st.cfg.get();
        if cfg.asr.source == Source::Provider {
            self.run_check(Role::Asr, cx);
        }
        if cfg.polish.enabled && cfg.polish.engine.source == Source::Provider {
            self.run_check(Role::Polish, cx);
        }
    }

    fn run_check(&mut self, role: Role, cx: &mut Context<Self>) {
        let idx = role_idx(role);
        if self.checks[idx] == TestState::Running {
            return;
        }
        self.checks[idx] = TestState::Running;
        cx.notify();
        let st = self.st.clone();
        cx.spawn(async move |this, cx| {
            let outcome = services::test_connection(st, role).await;
            let _ = this.update(cx, |t, cx| {
                let idx = role_idx(role);
                t.checked_at[idx] = Some(now_epoch());
                t.checks[idx] = match outcome {
                    TestOutcome::Ok { latency_ms } => TestState::Ok(latency_ms),
                    TestOutcome::Failed { kind, message } => TestState::Failed(format!("{kind:?}: {message}")),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn check_all(&mut self, cx: &mut Context<Self>) {
        self.perms = read_perms();
        self.probe_providers(cx);
        cx.notify();
    }

    fn view(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = Ctx::new(&self.st, cx);
        let p = c.p;

        let actions = div()
            .flex()
            .gap(px(10.))
            .child(ui::button(
                "manage-models",
                &p,
                &c.t(Tr::StatusManageModels),
                BtnKind::Secondary,
                Some(Ic::Cpu),
                std::rc::Rc::new(|_, cx| {
                    let st = super::global_state(cx);
                    super::open(st, Page::Models, cx);
                }),
            ))
            .child(ui::button(
                "check-all",
                &p,
                &c.t(Tr::StatusCheckAll),
                BtnKind::Secondary,
                Some(Ic::Refresh),
                cbk(cx, |this: &mut Self, _, cx| this.check_all(cx)),
            ))
            .into_any_element();

        let asr = self.engine_card(&c, Role::Asr);
        let polish = self.engine_card(&c, Role::Polish);

        // Permissions poll on their own while the page is open; the only manual recheck is
        // the page-wide button above, so the card header carries no button of its own.
        let rows = [
            (Ic::Mic, c.t(Tr::GeneralMicrophone), c.t(Tr::OnboardingCardMic)),
            (Ic::Zap, c.t(Tr::StatusAccessibility), c.t(Tr::OnboardingCardPolish)),
        ];
        let mut perms = ui::card(&p).child(
            div().w_full().px(px(26.)).py(px(16.)).flex().items_center().justify_between().child(text(
                c.t(Tr::ModelsSystemPermissions),
                17.,
                FontWeight::BOLD,
                p.text,
            )),
        );
        let n = rows.len();
        // Attention first: ungranted permissions lead the list.
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by_key(|&i| self.perms[i] == Status::Granted);
        for (k, &i) in order.iter().enumerate() {
            let (ic, name, desc) = (&rows[i].0, rows[i].1.clone(), rows[i].2.clone());
            let ok = self.perms[i] == Status::Granted;
            let perm = PERMS[i];
            let mut right = div().flex().items_center().gap(px(12.)).child(ui::status_chip(
                if ok { c.t(Tr::StatusAllowed) } else { c.t(Tr::StatusNotAllowed) },
                if ok { p.success } else { p.danger },
            ));
            if !ok {
                right = right.child(ui::button(
                    SharedString::from(format!("open-perm-{i}")),
                    &p,
                    &c.t(Tr::StatusOpenSystemSettings),
                    BtnKind::Secondary,
                    Some(Ic::Sliders),
                    cbk(cx, move |_: &mut Self, _, _| permissions::request(perm)),
                ));
            }
            let mut r = div()
                .w_full()
                .px(px(26.))
                .py(px(14.))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(16.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(16.))
                        .child(
                            div()
                                .size(px(44.))
                                .rounded(px(14.))
                                .bg(p.seg)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(ui::icon(*ic, 20., p.text)),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(text(name, 14., FontWeight::MEDIUM, p.text))
                                .child(ui::muted12(&p, desc)),
                        ),
                )
                .child(right);
            if k + 1 < n {
                r = r.border_b_1().border_color(p.border);
            }
            perms = perms.child(r);
        }

        super::page(
            p,
            "status",
            c.t(Tr::NavStatus),
            Some(actions),
            div().w_full().flex().flex_col().gap(px(20.)).child(asr).child(polish).child(perms),
        )
    }

    fn engine_card(&self, c: &Ctx, role: Role) -> Div {
        let p = &c.p;
        let cfg = self.st.cfg.get();
        let e = match role {
            Role::Asr => &cfg.asr,
            Role::Polish => &cfg.polish.engine,
        };
        let pill = if role == Role::Asr { c.t(Tr::ModelsDictation) } else { c.t(Tr::ModelsPolish) };
        let mut head_left = div().flex().items_center().gap(px(12.)).child(role_pill(p, pill));
        let chip: Div;
        let mut body: Vec<AnyElement> = Vec::new();

        if role == Role::Polish && !cfg.polish.enabled {
            // Off is a first-class state: neutral chip plus a pointer to the General toggle.
            chip = ui::status_chip(c.t(Tr::ModelsPolishOff), p.muted);
            body.push(hint(p, c.t(Tr::ModelsTurnOnInGeneral)).into_any_element());
        } else if e.source == Source::Local {
            let status = self.st.local.status(&e.local_model);
            let spec = self.st.models.spec(&e.local_model).cloned();
            head_left = head_left.child(text(c.t(Tr::ModelsLocal), 12., FontWeight::NORMAL, p.muted));
            let (label, tone) = match &status {
                EngineStatus::Running => (c.t(Tr::ModelsRunning), p.success),
                EngineStatus::Loading => (c.t(Tr::MenubarLoading), p.warning_text),
                EngineStatus::Stopped => (c.t(Tr::MenubarStopped), p.danger),
                EngineStatus::NotInstalled => (c.t(Tr::ModelsNotInstalled), p.muted),
                EngineStatus::Failed(_) => (c.t(Tr::ModelsStartupError), p.danger),
                EngineStatus::Remote => (SharedString::from("—"), p.muted),
            };
            chip = ui::status_chip(label, tone);
            let running = matches!(status, EngineStatus::Running);
            body.push(
                text(
                    spec.as_ref().map(|s| s.name.clone()).unwrap_or_else(|| e.local_model.clone()),
                    15.,
                    FontWeight::MEDIUM,
                    p.text,
                )
                .into_any_element(),
            );
            match &status {
                EngineStatus::Stopped => {
                    body.push(hint(p, c.t(Tr::ModelsNoLocalRunning)).into_any_element());
                }
                EngineStatus::Failed(r) => {
                    body.push(
                        text(r.clone(), 13., FontWeight::NORMAL, if p.dark { p.danger } else { p.danger_text })
                            .into_any_element(),
                    );
                }
                _ => {}
            }
            if running {
                let m = self.st.local.metrics(&e.local_model);
                let mem_gb = spec.as_ref().map(|s| s.memory_mb as f32 / 1000.0).unwrap_or(0.0);
                body.push(
                    info_row(
                        p,
                        vec![
                            (c.t(Tr::ModelsMemory), format!("{mem_gb:.1} GB")),
                            (c.t(Tr::StatusQueuedRequests), format!("{} {}", m.inflight, c.t(Tr::UnitsItems))),
                            (
                                c.t(Tr::ModelsProcessingTime),
                                format!("{:.1} {}", m.last_ms as f32 / 1000.0, c.t(Tr::UnitsSeconds)),
                            ),
                        ],
                    )
                    .into_any_element(),
                );
            }
        } else {
            let kind = e.provider.kind;
            let name = e
                .cloud_asr
                .as_ref()
                .and_then(|p| dito_core::provider::configured_asr::AsrProfiles::NAMES.get(p.selected).copied())
                .unwrap_or_else(|| c.lang.tr(kind.label()));
            head_left = head_left.child(text(name, 12., FontWeight::NORMAL, p.muted));
            let model = if let Some(profiles) = &e.cloud_asr {
                profiles.active()["connection"]["model"].as_str().unwrap_or("")
            } else {
                match kind {
                    dito_core::config::ProviderKind::Azure => &e.provider.azure.deployment,
                    dito_core::config::ProviderKind::Bedrock => &e.provider.bedrock.model_id,
                    dito_core::config::ProviderKind::Compat => &e.provider.compat.model,
                }
            };
            let idx = role_idx(role);
            let (label, tone) = match &self.checks[idx] {
                TestState::Ok(_) => (c.t(Tr::ModelsConnected), p.success),
                TestState::Failed(_) => (c.t(Tr::ModelsConnectionError), p.danger),
                TestState::Running | TestState::Idle => (c.t(Tr::ModelsChecking), p.warning_text),
            };
            chip = ui::status_chip(label, tone);
            body.push(text(model.to_owned(), 15., FontWeight::MEDIUM, p.text).into_any_element());
            let second = if let Some(profiles) = &e.cloud_asr {
                (c.t(Tr::ModelsEndpoint), url_host(profiles.active()["connection"]["base_url"].as_str().unwrap_or("")))
            } else {
                match kind {
                    dito_core::config::ProviderKind::Azure => {
                        let r = azure_resource(&e.provider.azure.endpoint);
                        (c.t(Tr::ModelsResource), if r.is_empty() { "—".into() } else { r })
                    }
                    dito_core::config::ProviderKind::Bedrock => {
                        (c.t(Tr::ModelsRegion), e.provider.bedrock.region.clone())
                    }
                    dito_core::config::ProviderKind::Compat => {
                        let h = url_host(&e.provider.compat.base_url);
                        (c.t(Tr::ModelsEndpoint), if h.is_empty() { "—".into() } else { h })
                    }
                }
            };
            let mut cells = vec![(c.t(Tr::ModelsProvider), name.to_string()), second];
            match &self.checks[idx] {
                TestState::Ok(ms) => {
                    cells.push((c.t(Tr::ModelsLatency), format!("{ms} ms")));
                    cells.push((c.t(Tr::ModelsLastChecked), hh_mm(self.checked_at[idx])));
                }
                TestState::Failed(msg) => {
                    cells.push((c.t(Tr::ModelsLatency), "—".into()));
                    let kind_txt = msg.split(':').next().unwrap_or("—").trim().to_string();
                    cells.push((c.t(Tr::HudError), kind_txt));
                    body.push(
                        text(msg.clone(), 13., FontWeight::NORMAL, if p.dark { p.danger } else { p.danger_text })
                            .into_any_element(),
                    );
                }
                TestState::Running | TestState::Idle => {
                    cells.push((c.t(Tr::ModelsLatency), "—".into()));
                    cells.push((c.t(Tr::ModelsLastChecked), "—".into()));
                }
            }
            body.push(info_row(p, cells).into_any_element());
        }

        let header = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(head_left)
            .child(div().flex().items_center().child(chip));
        ui::card(p).p(px(22.)).gap(px(16.)).child(header).children(body)
    }
}

/// Neutral role badge: muted text on the secondary surface, never a status colour.
fn role_pill(p: &Palette, label: SharedString) -> Div {
    div()
        .px(px(8.))
        .py(px(3.))
        .rounded(px(5.))
        .bg(p.sub)
        .flex()
        .items_center()
        .flex_none()
        .child(text(label, 11., FontWeight::MEDIUM, p.muted).line_height(relative(1.45)))
}

fn hint(p: &Palette, label: SharedString) -> Div {
    text(label, 13., FontWeight::NORMAL, p.muted)
}

/// Equal-width metric cells: label left, value right — mirrors the card header's
/// left-text / right-value rhythm.
fn info_row(p: &Palette, cells: Vec<(SharedString, String)>) -> Div {
    div().w_full().flex().gap(px(28.)).children(cells.into_iter().map(|(label, value)| {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.))
            .child(text(label, 12., FontWeight::MEDIUM, p.muted))
            .child(text(value, 13., FontWeight::MEDIUM, p.text))
    }))
}
