use dito_core::i18n::Tr;
use dito_core::secrets::Role;
use dito_local::catalog::ModelSpec;
use dito_local::{ModelState, Snapshot};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::action::model_action;
use super::page::ModelsPage;
use crate::assets::Ic;
use crate::settings::{Ctx, role};
use crate::theme::Palette;
use crate::ui::{self, BtnKind, cbk, text};

/// Fixed two-line cards matching `design:settings/local-models`.
const ROW_SLOT_H: f32 = 88.;

/// The name and status column and the controls column of one model row, built up by the row's state.
pub(super) struct RowParts {
    pub(super) body: Div,
    pub(super) ctl: Div,
}

impl RowParts {
    fn new(c: &Ctx, spec: &ModelSpec, state: &ModelState) -> Self {
        let p = &c.p;
        let ctl = div().flex().items_center().gap(px(10.)).flex_none();
        let mut body = div().flex_1().min_w(px(0.)).flex().flex_col().gap(px(4.));
        if *state != ModelState::Installed {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .h(px(26.))
                    .child(text(spec.name.clone(), 16., FontWeight::SEMIBOLD, p.text).truncate())
                    .child(text(super::metadata::model_metadata(c, spec), 12., FontWeight::NORMAL, p.model_meta)),
            );
        }
        Self { body, ctl }
    }
}

/// Bytes moved so far and in total, for the states that show a transfer.
#[derive(Clone, Copy)]
struct Transfer {
    done: u64,
    total: u64,
}

impl ModelsPage {
    pub(super) fn model_row(
        &self,
        c: &Ctx,
        cx: &mut Context<Self>,
        role: Role,
        spec: &ModelSpec,
        snap: &Snapshot,
    ) -> AnyElement {
        let p = &c.p;
        let selected = role::engine(&self.st.cfg.get(), role).local_model == spec.id;
        let state = snap.get(&spec.id).cloned().unwrap_or(ModelState::NotInstalled);
        let RowParts { body, ctl } = self.state_parts(c, cx, role, spec, &state, selected);
        let tile_id = ElementId::from(SharedString::from(format!("model-row-{}-{}", role::index(role), spec.id)));
        let base = row_slot(Self::row_plate(p, &state, selected, &tile_id), body, ctl);
        // Rows provide hover feedback only; selection belongs to the explicit Use button.
        // Track selected rows too so deselecting under the pointer cannot leave a stale hover.
        if state == ModelState::Installed {
            base.id(tile_id.clone()).on_hover(ui::hover_tracker(tile_id)).into_any_element()
        } else {
            base.into_any_element()
        }
    }

    fn state_parts(
        &self,
        c: &Ctx,
        cx: &mut Context<Self>,
        role: Role,
        spec: &ModelSpec,
        state: &ModelState,
        selected: bool,
    ) -> RowParts {
        let parts = RowParts::new(c, spec, state);
        match state {
            ModelState::Installed => self.installed_row(c, cx, role, spec, selected, parts),
            ModelState::Downloading { done, total, .. } => {
                self.downloading_row(c, cx, spec, state, Transfer { done: *done, total: *total }, parts)
            }
            ModelState::Verifying { done, total } => {
                self.verifying_row(c, cx, spec, Transfer { done: *done, total: *total }, parts)
            }
            ModelState::Paused { done, total } => {
                self.paused_row(c, cx, spec, Transfer { done: *done, total: *total }, parts)
            }
            ModelState::Failed { message, done, total } => {
                self.failed_row(c, cx, spec, message, Transfer { done: *done, total: *total }, parts)
            }
            ModelState::NotInstalled => self.not_installed_row(c, cx, role, spec, parts),
        }
    }

    /// Background progress and borders never participate in content layout.
    fn row_plate(p: &Palette, state: &ModelState, selected: bool, tile_id: &ElementId) -> AnyElement {
        let plate = || div().absolute().inset_0().rounded(px(12.));
        let error = p.danger;
        if matches!(state, ModelState::Failed { .. }) {
            return plate()
                .bg(p.row_idle)
                .child(div().absolute().inset_0().bg(ui::hex_alpha(error, 0.14)).rounded(px(12.)))
                .child(plate().border_1().border_color(error))
                .into_any_element();
        }
        if *state == ModelState::Installed {
            if selected {
                return plate().bg(p.model_in_use_bg).border_2().border_color(p.model_in_use_border).into_any_element();
            }
            let (bg, idle_c, hover_c) = (p.model_idle_bg, p.model_idle_border, p.model_idle_border_hover);
            return ui::tweened(
                tile_id.clone(),
                if ui::is_hovered(tile_id) { 1. } else { 0. },
                ui::HOVER_MS,
                move |t| {
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(px(12.))
                        .bg(bg)
                        .border_1()
                        .border_color(ui::mix(idle_c, hover_c, t))
                        .into_any_element()
                },
            );
        }
        let fraction = match state {
            ModelState::Downloading { done, total, .. }
            | ModelState::Verifying { done, total }
            | ModelState::Paused { done, total } => Some((*done as f32 / (*total).max(1) as f32).clamp(0., 1.)),
            _ => None,
        };
        plate()
            .bg(if *state == ModelState::NotInstalled { p.model_download_bg } else { p.row_idle })
            .overflow_hidden()
            .when_some(fraction, |d, f| d.child(progress_fill(f, ui::hex_alpha(p.accent, 0.16))))
            .child(plate().border_1().border_color(if fraction.is_some() {
                p.model_idle_border
            } else {
                p.model_download_border
            }))
            .into_any_element()
    }

    fn downloading_row(
        &self,
        c: &Ctx,
        cx: &mut Context<Self>,
        spec: &ModelSpec,
        state: &ModelState,
        transfer: Transfer,
        parts: RowParts,
    ) -> RowParts {
        let RowParts { mut body, mut ctl } = parts;
        let Transfer { done, total } = transfer;
        let p = &c.p;
        let id = spec.id.clone();
        let progress = match state.eta_secs() {
            Some(s) => {
                format!("{} / {}（{}）", ui::fmt_bytes(done), ui::fmt_bytes(total), ui::fmt_eta(c.lang, s))
            }
            None => format!("{} / {}", ui::fmt_bytes(done), ui::fmt_bytes(total)),
        };
        body = body.child(status_line(progress, p.model_meta));
        let (id_a, id_b) = (id.clone(), id.clone());
        ctl = ctl
            .child(model_action(
                SharedString::from(format!("pause-{id}")),
                p,
                &c.t(Tr::ModelsPause),
                BtnKind::Secondary,
                Ic::Pause,
                cbk(cx, move |this: &mut Self, _, _| this.st.models.pause(&id_a)),
            ))
            .child(model_action(
                SharedString::from(format!("cancel-{id}")),
                p,
                &c.t(Tr::ModelsCancel),
                BtnKind::Danger,
                Ic::X,
                cbk(cx, move |this: &mut Self, _, _| this.st.models.cancel(&id_b)),
            ));
        RowParts { body, ctl }
    }

    fn verifying_row(
        &self,
        c: &Ctx,
        cx: &mut Context<Self>,
        spec: &ModelSpec,
        transfer: Transfer,
        parts: RowParts,
    ) -> RowParts {
        let RowParts { mut body, mut ctl } = parts;
        let Transfer { done, total } = transfer;
        let p = &c.p;
        let id = spec.id.clone();
        body = body.child(status_line(
            format!("{}（{} / {}）", c.t(Tr::ModelsVerifying), ui::fmt_bytes(done), ui::fmt_bytes(total)),
            p.model_meta,
        ));
        let id2 = id.clone();
        ctl = ctl.child(model_action(
            SharedString::from(format!("cancel-{id}")),
            p,
            &c.t(Tr::ModelsCancel),
            BtnKind::Danger,
            Ic::X,
            cbk(cx, move |this: &mut Self, _, _| {
                // "Cancel" during verification keeps the completed bytes (pause
                // semantics): a restart skips the download and re-verifies, instead
                // of throwing away a finished multi-GB fetch.
                this.st.models.pause(&id2);
            }),
        ));
        RowParts { body, ctl }
    }

    fn paused_row(
        &self,
        c: &Ctx,
        cx: &mut Context<Self>,
        spec: &ModelSpec,
        transfer: Transfer,
        parts: RowParts,
    ) -> RowParts {
        let RowParts { mut body, mut ctl } = parts;
        let Transfer { done, total } = transfer;
        let p = &c.p;
        let id = spec.id.clone();
        body = body.child(status_line(
            format!("{}（{} / {}）", c.t(Tr::ModelsPause), ui::fmt_bytes(done), ui::fmt_bytes(total)),
            p.model_meta,
        ));
        let (id_a, id_b) = (id.clone(), id.clone());
        ctl = ctl
            .child(model_action(
                SharedString::from(format!("resume-{id}")),
                p,
                &c.t(Tr::MenubarResume),
                BtnKind::Secondary,
                Ic::Play,
                cbk(cx, move |this: &mut Self, _, _| {
                    let _ = this.st.models.start_download(&id_a);
                }),
            ))
            .child(model_action(
                SharedString::from(format!("cancel-{id}")),
                p,
                &c.t(Tr::ModelsCancel),
                BtnKind::Danger,
                Ic::X,
                cbk(cx, move |this: &mut Self, _, _| this.st.models.cancel(&id_b)),
            ));
        RowParts { body, ctl }
    }

    fn failed_row(
        &self,
        c: &Ctx,
        cx: &mut Context<Self>,
        spec: &ModelSpec,
        message: &str,
        transfer: Transfer,
        parts: RowParts,
    ) -> RowParts {
        let RowParts { mut body, mut ctl } = parts;
        let Transfer { done, total } = transfer;
        let p = &c.p;
        let id = spec.id.clone();
        body = body.child(status_line(
            format!("{}（{} / {}）", message, ui::fmt_bytes(done), ui::fmt_bytes(total)),
            p.model_error_text,
        ));
        let (id_a, id_b) = (id.clone(), id.clone());
        ctl = ctl
            .child(model_action(
                SharedString::from(format!("retry-{id}")),
                p,
                &c.t(Tr::HistoryRetry),
                BtnKind::Secondary,
                Ic::Refresh,
                cbk(cx, move |this: &mut Self, _, _| {
                    let _ = this.st.models.start_download(&id_a);
                }),
            ))
            .child(model_action(
                SharedString::from(format!("cancel-{id}")),
                p,
                &c.t(Tr::ModelsCancel),
                BtnKind::Danger,
                Ic::X,
                cbk(cx, move |this: &mut Self, _, _| this.st.models.cancel(&id_b)),
            ));
        RowParts { body, ctl }
    }

    fn not_installed_row(
        &self,
        c: &Ctx,
        cx: &mut Context<Self>,
        role: Role,
        spec: &ModelSpec,
        parts: RowParts,
    ) -> RowParts {
        let RowParts { mut body, mut ctl } = parts;
        let p = &c.p;
        let id = spec.id.clone();
        body = body.child(status_line(c.t(Tr::ModelsDownloadAvailable), p.model_meta));
        let id2 = id.clone();
        ctl = ctl.child(model_action(
            SharedString::from(format!("dl-{}-{id}", role::index(role))),
            p,
            &c.t(Tr::MenubarDownload),
            BtnKind::Download,
            Ic::Download,
            cbk(cx, move |this: &mut Self, _, _| {
                let _ = this.st.models.start_download(&id2);
            }),
        ));
        RowParts { body, ctl }
    }
}

fn row_slot(plate: AnyElement, body: Div, ctl: Div) -> Div {
    div()
        .w_full()
        .h(px(ROW_SLOT_H))
        .relative()
        // The plate's border is a separate absolute layer, so it never moves the content: the same inset in every
        // state keeps the text from jumping sideways when a card is selected.
        .px(px(22.))
        .py(px(12.))
        .flex()
        .items_center()
        .gap(px(16.))
        .rounded(px(12.))
        .child(plate)
        .child(body)
        .child(ctl)
}

/// Reserve the same status line in every state; long errors never move the controls.
pub(super) fn status_line(label: impl Into<SharedString>, color: Hsla) -> Div {
    div().h(px(24.)).flex().items_center().child(text(label, 13., FontWeight::NORMAL, color).truncate())
}

/// Clip a full-radius card to the advancing edge, preserving its arc even below 12 px.
fn progress_fill(fraction: f32, tint: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let mask = ContentMask {
                bounds: Bounds::new(bounds.origin, size(bounds.size.width * fraction, bounds.size.height)),
            };
            window.with_content_mask(Some(mask), |window| {
                window.paint_quad(quad(
                    bounds,
                    Corners::all(px(12.)),
                    tint,
                    Edges::all(px(0.)),
                    rgba(0),
                    BorderStyle::Solid,
                ));
            });
        },
    )
    .absolute()
    .inset_0()
}
