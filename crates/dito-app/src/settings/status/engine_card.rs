use dito_core::config::{EngineConfig, Source};
use dito_core::factory::LocalBackend;
use dito_core::health::EngineStatus;
use dito_core::i18n::Tr;
use dito_core::secrets::Role;
use gpui_kit::*;

use super::page::StatusPage;
use crate::assets::Ic;
use crate::settings::{Ctx, role};
use crate::theme::Palette;
use crate::ui::{self, StatusTone, text};

impl StatusPage {
    pub(super) fn engine_card(&self, c: &Ctx, role: Role) -> Div {
        let p = &c.p;
        let cfg = self.st.cfg.get();
        let e = role::engine(&cfg, role);
        let (title, icon) = if role == Role::Asr {
            (c.t(Tr::StatusAsrModel), Ic::Mic)
        } else {
            (c.t(Tr::StatusPolishModel), Ic::Sparkles)
        };
        let (chip, body) = if role == Role::Polish && !cfg.polish.enabled {
            Self::polish_off_card(c)
        } else if e.source == Source::Local {
            self.local_engine_card(c, e)
        } else {
            self.provider_engine_card(c, role, e)
        };
        let head_left = div()
            .h(px(24.))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(ui::icon(icon, 18., p.text))
            .child(text(title, 16., FontWeight::BOLD, p.text).line_height(px(24.)));
        let header = div()
            .w_full()
            .px(px(10.))
            .pt(px(4.))
            .flex()
            .items_center()
            .justify_between()
            .child(head_left)
            .child(div().flex().items_center().child(chip));
        ui::card(p).p(px(11.)).gap(px(8.)).child(header).children(body)
    }

    /// Off is a first-class state: neutral chip plus a pointer to the General toggle.
    fn polish_off_card(c: &Ctx) -> EngineParts {
        let p = &c.p;
        let chip = ui::status_chip(p, c.t(Tr::ModelsPolishOff), StatusTone::Neutral);
        (chip, vec![hint(p, c.t(Tr::ModelsTurnOnInGeneral)).into_any_element()])
    }

    fn local_engine_card(&self, c: &Ctx, e: &EngineConfig) -> EngineParts {
        let p = &c.p;
        let mut body: Vec<AnyElement> = Vec::new();
        let status = self.st.local.status(&e.local_model);
        let spec = self.st.models.spec(&e.local_model).cloned();
        let (label, tone) = match &status {
            EngineStatus::Running => (c.t(Tr::ModelsRunning), StatusTone::Success),
            EngineStatus::Stopping => (c.t(Tr::ModelsStopping), StatusTone::Neutral),
            EngineStatus::Loading => (c.t(Tr::MenubarLoading), StatusTone::Warning),
            EngineStatus::Stopped => (c.t(Tr::MenubarStopped), StatusTone::Danger),
            EngineStatus::NotInstalled => (c.t(Tr::ModelsNotInstalled), StatusTone::Neutral),
            EngineStatus::Failed(_) => (c.t(Tr::ModelsStartupError), StatusTone::Danger),
            EngineStatus::Remote => (SharedString::from("—"), StatusTone::Neutral),
        };
        let chip = ui::status_chip(p, label, tone);
        let running = matches!(status, EngineStatus::Running);
        body.push(
            model_line(
                c,
                spec.as_ref().map(|s| s.name.clone()).unwrap_or_else(|| e.local_model.clone()),
                Source::Local,
            )
            .into_any_element(),
        );
        match &status {
            EngineStatus::Stopped => {
                body.push(hint(p, c.t(Tr::ModelsNoLocalRunning)).into_any_element());
            }
            EngineStatus::Failed(r) => {
                body.push(
                    hint(p, r.clone().into())
                        .text_color(if p.dark { p.danger } else { p.danger_text })
                        .into_any_element(),
                );
            }
            _ => {}
        }
        if running {
            let m = self.st.local.metrics(&e.local_model);
            let mem_gb = spec.as_ref().map(|s| s.memory_mb as f32 / 1000.0).unwrap_or(0.0);
            body.push(
                detail_grid(
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
        (chip, body)
    }
}

/// The status chip and the model slot's content.
pub(super) type EngineParts = (Div, Vec<AnyElement>);

pub(super) fn model_line(c: &Ctx, name: impl Into<SharedString>, source: Source) -> Div {
    let p = &c.p;
    let (label, icon) = match source {
        Source::Local => (c.t(Tr::ModelsLocal), Ic::Cpu),
        Source::Provider => (c.t(Tr::ModelsProvider), Ic::Globe),
    };
    let badge = div()
        .h(px(24.))
        .px(px(8.))
        .rounded_full()
        .border_1()
        .border_color(p.border_strong)
        .bg(p.card)
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.))
        .child(ui::icon(icon, 14., p.muted))
        .child(text(label, 12., FontWeight::MEDIUM, p.muted).line_height(px(18.)));
    div()
        .w_full()
        .pl(px(12.))
        .pr(px(10.))
        .flex()
        .items_center()
        .gap(px(10.))
        .child(text(name, 15., FontWeight::MEDIUM, p.text).min_w(px(0.)).line_height(px(22.)))
        .child(badge)
}

pub(super) fn hint(p: &Palette, label: SharedString) -> Div {
    text(label, 13., FontWeight::NORMAL, p.muted).px(px(10.)).pb(px(4.)).line_height(px(20.))
}

/// Two equal columns, with each row growing to fit its longest value.
pub(super) fn detail_grid(p: &Palette, cells: Vec<(SharedString, String)>) -> Div {
    div().w_full().px(px(10.)).grid().grid_cols(2).gap(px(12.)).children(cells.into_iter().map(|(label, value)| {
        div()
            .min_w(px(0.))
            .min_h(px(64.))
            .px(px(16.))
            .py(px(10.))
            .rounded(px(16.))
            .border_1()
            .border_color(p.border)
            .bg(p.sub)
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(text(label, 12., FontWeight::NORMAL, p.muted).line_height(px(18.)))
            .child(
                text(value, 13., FontWeight::MEDIUM, p.text)
                    .w_full()
                    .min_w(px(0.))
                    .whitespace_normal()
                    .line_height(px(20.)),
            )
    }))
}
