use dito_core::i18n::Tr;
use dito_core::secrets::Role;
use dito_local::catalog::ModelSpec;
use dito_local::{ModelState, Snapshot};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::page::ModelsPage;
use crate::assets::Ic;
use crate::settings::{Ctx, role};
use crate::theme::Palette;
use crate::ui::{self, BtnKind, cbk, text};

/// Model-row layout: every row occupies a fixed slot sized for the tallest state
/// (name + meta + status lines), so the section height never changes; state transitions animate
/// inside the slot.
const ROW_SLOT_H: f32 = 112.;

/// The name and status column and the controls column of one model row, built up by the row's state.
pub(super) struct RowParts {
    pub(super) body: Div,
    pub(super) ctl: Div,
}

impl RowParts {
    fn new(p: &Palette, spec: &ModelSpec, state: &ModelState) -> Self {
        let ctl = div().flex().items_center().gap(px(8.)).flex_none();
        let mut body = div().flex_1().min_w(px(0.)).flex().flex_col().gap(px(6.));
        if *state != ModelState::Installed {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(text(spec.name.clone(), 14., FontWeight::MEDIUM, p.text))
                    .child(text(ui::fmt_bytes(spec.total_size()), 12., FontWeight::NORMAL, p.model_meta)),
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
        let parts = RowParts::new(&c.p, spec, state);
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

    /// Card plates: a dashed blue plate for models still to download, a gold
    /// plate with a 3 pt border for the one in use, an outlined plate for downloaded ones whose border darkens
    /// on hover, and a plain plate while a download is in progress.
    fn row_plate(p: &Palette, state: &ModelState, selected: bool, tile_id: &ElementId) -> AnyElement {
        let plate = || div().absolute().inset_0().rounded(px(12.));
        if *state == ModelState::NotInstalled {
            plate()
                .bg(p.model_download_bg)
                .border(px(1.5))
                .border_dashed()
                .border_color(p.model_download_border)
                .into_any_element()
        } else if selected {
            plate().bg(p.model_in_use_bg).border(px(3.)).border_color(p.model_in_use_border).into_any_element()
        } else if *state == ModelState::Installed {
            let (bg, idle_c, hover_c) = (p.model_idle_bg, p.model_idle_border, p.model_idle_border_hover);
            let hover = ui::is_hovered(tile_id);
            ui::tweened(tile_id.clone(), if hover { 1. } else { 0. }, ui::HOVER_MS, move |t| {
                plate().bg(bg).border_1().border_color(ui::mix(idle_c, hover_c, t)).into_any_element()
            })
        } else {
            plate().bg(p.row_idle).into_any_element()
        }
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
        let frac = done as f32 / total.max(1) as f32;
        let progress = match state.eta_secs() {
            Some(s) => {
                format!("{} / {}（{}）", ui::fmt_bytes(done), ui::fmt_bytes(total), ui::fmt_eta(c.lang, s))
            }
            None => format!("{} / {}", ui::fmt_bytes(done), ui::fmt_bytes(total)),
        };
        body = body.child(ui::progress_bar(p, frac)).child(text(progress, 12., FontWeight::NORMAL, p.model_meta));
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
        body = body.child(ui::progress_bar(p, done as f32 / total.max(1) as f32)).child(text(
            format!("{}（{} / {}）", c.t(Tr::ModelsVerifying), ui::fmt_bytes(done), ui::fmt_bytes(total)),
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
        body = body.child(ui::progress_bar(p, done as f32 / total.max(1) as f32));
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
        // All bytes on disk → the checksum pass failed; otherwise the download itself.
        let title = if done >= total { c.t(Tr::ModelsVerifyFailed) } else { c.t(Tr::ModelsDownloadFailed) };
        body = body.child(text(title, 12., FontWeight::MEDIUM, p.model_error_text)).child(text(
            format!("{}（{} / {}）", message, ui::fmt_bytes(done), ui::fmt_bytes(total)),
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
        body = body.child(text(c.t(Tr::ModelsDownloadAvailable), 12., FontWeight::NORMAL, p.model_download));
        let id2 = id.clone();
        ctl = ctl.child(ui::button(
            SharedString::from(format!("dl-{}-{id}", role::index(role))),
            p,
            &c.t(Tr::MenubarDownload),
            BtnKind::Download,
            Some(Ic::Download),
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
        .py(px(18.))
        .flex()
        .items_center()
        .gap(px(16.))
        .rounded(px(12.))
        .child(plate)
        .child(body)
        .child(ctl)
}
