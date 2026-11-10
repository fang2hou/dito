use dito_core::factory::LocalBackend;
use dito_core::health::EngineStatus;
use dito_core::i18n::Tr;
use dito_core::secrets::Role;
use dito_local::catalog::ModelSpec;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::action::{in_use_status, model_action};
use super::local_rows::{RowParts, status_line};
use super::page::ModelsPage;
use crate::assets::Ic;
use crate::settings::{Ctx, role};
use crate::ui::{self, BtnKind, cbk, text};

/// All model statuses occupy the same line height.
const STATUS_H: f32 = 24.;

/// The engine state line of the selected row: a dot, a label and its colours.
struct EngineLine {
    dot: Hsla,
    text: SharedString,
    fg: Hsla,
}

impl EngineLine {
    fn new(c: &Ctx, status: &EngineStatus) -> Self {
        let p = &c.p;
        match status {
            EngineStatus::Running => {
                Self { dot: p.model_running_dot, text: c.t(Tr::ModelsRunning), fg: p.model_running_text }
            }
            EngineStatus::Stopping => Self { dot: p.model_meta, text: c.t(Tr::ModelsStopping), fg: p.model_meta },
            EngineStatus::Loading => Self { dot: p.primary, text: c.t(Tr::MenubarLoading), fg: p.primary },
            EngineStatus::Failed(reason) => Self {
                dot: p.danger,
                text: SharedString::from(format!("{}：{reason}", c.t(Tr::ModelsFailed))),
                fg: if p.dark { p.danger } else { p.danger_text },
            },
            _ => Self { dot: p.model_meta, text: c.t(Tr::MenubarStopped), fg: p.model_meta },
        }
    }

    fn element(&self) -> Div {
        div()
            .h(px(STATUS_H))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(div().size(px(6.)).flex_none().rounded_full().bg(self.dot))
            .child(text(self.text.clone(), 13., FontWeight::MEDIUM, self.fg).truncate())
    }
}

impl ModelsPage {
    /// `design:settings/local-models`: a fixed name line and a fixed status line, without memory details.
    pub(super) fn installed_row(
        &self,
        c: &Ctx,
        cx: &mut Context<Self>,
        role: Role,
        spec: &ModelSpec,
        selected: bool,
        parts: RowParts,
    ) -> RowParts {
        let RowParts { body, ctl } = parts;
        let body = body
            .child(
                div()
                    .h(px(26.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(text(spec.name.clone(), 16., FontWeight::SEMIBOLD, c.p.text).truncate())
                    .child(text(super::metadata::model_metadata(c, spec), 12., FontWeight::NORMAL, c.p.model_meta)),
            )
            .child(if selected {
                EngineLine::new(c, &self.st.local.status(&spec.id)).element()
            } else {
                status_line(c.t(Tr::ModelsDownloaded), c.p.model_badge_text)
            });
        let ctl = ctl
            .child(self.use_action(c, cx, role, spec, selected))
            .child(self.more_control(c, cx, role, spec, selected));
        RowParts { body, ctl }
    }

    fn use_action(&self, c: &Ctx, cx: &mut Context<Self>, role: Role, spec: &ModelSpec, selected: bool) -> AnyElement {
        let p = &c.p;
        if selected {
            return in_use_status(
                SharedString::from(format!("in-use-{}-{}", role::index(role), spec.id)),
                p,
                c.t(Tr::ModelsInUse),
            )
            .into_any_element();
        }
        let id = spec.id.clone();
        model_action(
            SharedString::from(format!("use-{}-{id}", role::index(role))),
            p,
            &c.t(Tr::ModelsUse),
            BtnKind::Secondary,
            Ic::Check,
            cbk(cx, move |this: &mut Self, _, cx| {
                cx.stop_propagation();
                this.use_model(role, &id);
                cx.notify();
            }),
        )
        .into_any_element()
    }

    /// Make `id` the role's engine; a running previous model hands over to it (unload, then restart). A selection
    /// that cannot be saved changes nothing.
    pub(super) fn use_model(&mut self, role: Role, id: &str) {
        let previous = role::engine(&self.st.cfg.get(), role).local_model.clone();
        let hand_over = matches!(self.st.local.status(&previous), EngineStatus::Running | EngineStatus::Loading);
        if !self.select_local_model(role, id) {
            return;
        }
        if hand_over {
            let local = self.st.local.clone();
            let id = id.to_string();
            self.st.rt.spawn(async move {
                let retiring = local.clone();
                let _ = tokio::task::spawn_blocking(move || retiring.unload(&previous)).await;
                local.restart(&id).await;
            });
        }
    }

    fn more_control(
        &self,
        c: &Ctx,
        cx: &mut Context<Self>,
        role: Role,
        spec: &ModelSpec,
        selected: bool,
    ) -> AnyElement {
        let p = &c.p;
        let id = spec.id.clone();
        let more_open = self.menu.as_deref() == Some(id.as_str());
        let id2 = id.clone();
        let btn = ui::menu_button(
            SharedString::from(format!("more-{}-{id}", role::index(role))),
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
            .id(ElementId::from(SharedString::from(format!("more-wrap-{}-{id}", role::index(role)))))
            .child(btn)
            .on_click(ui::click_cb(cbk(cx, |_: &mut Self, _, cx| cx.stop_propagation())));
        if more_open {
            wrap = wrap.child(self.model_menu(c, cx, role, spec, selected));
        }
        wrap.into_any_element()
    }
}
