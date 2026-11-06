use dito_core::config::{EngineConfig, Source};
use dito_core::factory::LocalBackend;
use dito_core::health::EngineStatus;
use dito_core::i18n::Tr;
use dito_core::secrets::Role;
use gpui_kit::*;

use super::page::StatusPage;
use crate::settings::{Ctx, role};
use crate::theme::Palette;
use crate::ui::{self, text};

impl StatusPage {
    pub(super) fn engine_card(&self, c: &Ctx, role: Role) -> Div {
        let p = &c.p;
        let cfg = self.st.cfg.get();
        let e = role::engine(&cfg, role);
        let pill = if role == Role::Asr { c.t(Tr::ModelsDictation) } else { c.t(Tr::ModelsPolish) };
        let (chip, head_extra, body) = if role == Role::Polish && !cfg.polish.enabled {
            Self::polish_off_card(c)
        } else if e.source == Source::Local {
            self.local_engine_card(c, e)
        } else {
            self.provider_engine_card(c, role, e)
        };
        let head_left = div().flex().items_center().gap(px(12.)).child(role_pill(p, pill)).children(head_extra);
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(head_left)
            .child(div().flex().items_center().child(chip));
        ui::card(p).p(px(22.)).gap(px(16.)).child(header).children(body)
    }

    /// Off is a first-class state: neutral chip plus a pointer to the General toggle.
    fn polish_off_card(c: &Ctx) -> EngineParts {
        let p = &c.p;
        let chip = ui::status_chip(c.t(Tr::ModelsPolishOff), p.muted);
        (chip, None, vec![hint(p, c.t(Tr::ModelsTurnOnInGeneral)).into_any_element()])
    }

    fn local_engine_card(&self, c: &Ctx, e: &EngineConfig) -> EngineParts {
        let p = &c.p;
        let mut body: Vec<AnyElement> = Vec::new();
        let status = self.st.local.status(&e.local_model);
        let spec = self.st.models.spec(&e.local_model).cloned();
        let head_extra = Some(text(c.t(Tr::ModelsLocal), 12., FontWeight::NORMAL, p.muted));
        let (label, tone) = match &status {
            EngineStatus::Running => (c.t(Tr::ModelsRunning), p.success),
            EngineStatus::Loading => (c.t(Tr::MenubarLoading), p.warning_text),
            EngineStatus::Stopped => (c.t(Tr::MenubarStopped), p.danger),
            EngineStatus::NotInstalled => (c.t(Tr::ModelsNotInstalled), p.muted),
            EngineStatus::Failed(_) => (c.t(Tr::ModelsStartupError), p.danger),
            EngineStatus::Remote => (SharedString::from("—"), p.muted),
        };
        let chip = ui::status_chip(label, tone);
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
        (chip, head_extra, body)
    }
}

/// The status chip, an optional engine name beside the role badge, and the card body.
pub(super) type EngineParts = (Div, Option<Div>, Vec<AnyElement>);

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
pub(super) fn info_row(p: &Palette, cells: Vec<(SharedString, String)>) -> Div {
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
