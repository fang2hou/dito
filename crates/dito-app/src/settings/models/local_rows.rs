use dito_core::i18n::Tr;
use dito_local::catalog::ModelSpec;
use dito_local::{ModelManager, ModelState, Snapshot};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::action::model_action;
use super::page::{ModelsPage, Tab};
use crate::assets::Ic;
use crate::settings::{Ctx, role};
use crate::theme::Palette;
use crate::ui::{self, BtnKind, cbk, text};

/// Fixed two-line cards matching `design:settings/local-models`.
const ROW_SLOT_H: f32 = 88.;

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
                    .child(super::metadata::model_labels(c, spec)),
            );
        }
        Self { body, ctl }
    }
}

/// One circular control of a row whose model is not ready.
struct RowAction {
    id: SharedString,
    label: Tr,
    kind: BtnKind,
    icon: Ic,
    run: fn(&ModelManager, &str),
}

impl ModelsPage {
    pub(super) fn model_row(
        &self,
        c: &Ctx,
        cx: &mut Context<Self>,
        tab: Tab,
        spec: &ModelSpec,
        snap: &Snapshot,
    ) -> AnyElement {
        let p = &c.p;
        let state = snap.get(&spec.id).cloned().unwrap_or(ModelState::NotInstalled);
        let selected = match tab {
            Tab::Role(role) => role::engine(&self.st.cfg.get(), role).local_model == spec.id,
            // The detector is in use from the moment its file is on disk: there is only one, and nothing to switch.
            Tab::Vad => state == ModelState::Installed,
        };
        let RowParts { body, ctl } = self.state_parts(c, cx, tab, spec, &state, selected);
        let tile_id = ElementId::from(SharedString::from(format!("model-row-{}-{}", tab.tag(), spec.id)));
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
        tab: Tab,
        spec: &ModelSpec,
        state: &ModelState,
        selected: bool,
    ) -> RowParts {
        let parts = RowParts::new(c, spec, state);
        let p = &c.p;
        let action = |prefix: &str, label, kind, icon, run| RowAction {
            id: format!("{prefix}-{}", spec.id).into(),
            label,
            kind,
            icon,
            run,
        };
        let cancel = || action("cancel", Tr::ModelsCancel, BtnKind::Danger, Ic::X, ModelManager::cancel);
        let amount = |done: u64, total: u64| format!("{} / {}", ui::fmt_bytes(done), ui::fmt_bytes(total));
        let (status, color, actions): (SharedString, Hsla, Vec<RowAction>) = match state {
            ModelState::Installed => return self.installed_row(c, cx, tab, spec, selected, parts),
            ModelState::Downloading { done, total, .. } => {
                let transferred = amount(*done, *total);
                let progress = match state.eta_secs() {
                    Some(secs) => format!("{transferred}（{}）", ui::fmt_eta(c.lang, secs)),
                    None => transferred,
                };
                let pause = action("pause", Tr::ModelsPause, BtnKind::Secondary, Ic::Pause, ModelManager::pause);
                (progress.into(), p.model_meta, vec![pause, cancel()])
            }
            ModelState::Verifying { done, total } => (
                format!("{}（{}）", c.t(Tr::ModelsVerifying), amount(*done, *total)).into(),
                p.model_meta,
                // "Cancel" during verification keeps the completed bytes (pause
                // semantics): a restart skips the download and re-verifies, instead
                // of throwing away a finished multi-GB fetch.
                vec![RowAction { run: ModelManager::pause, ..cancel() }],
            ),
            ModelState::Paused { done, total } => (
                format!("{}（{}）", c.t(Tr::ModelsPaused), amount(*done, *total)).into(),
                p.model_meta,
                vec![action("resume", Tr::MenubarResume, BtnKind::Secondary, Ic::Play, start_download), cancel()],
            ),
            ModelState::Failed { message, done, total } => (
                format!("{message}（{}）", amount(*done, *total)).into(),
                p.model_error_text,
                vec![action("retry", Tr::HistoryRetry, BtnKind::Secondary, Ic::Refresh, start_download), cancel()],
            ),
            ModelState::NotInstalled => {
                let prefix = format!("dl-{}", tab.tag());
                (
                    c.t(Tr::ModelsDownloadAvailable),
                    p.model_meta,
                    vec![action(&prefix, Tr::MenubarDownload, BtnKind::Secondary, Ic::Download, start_download)],
                )
            }
        };
        Self::pending_row(c, cx, spec, parts, (status, color), actions)
    }

    /// The status line and controls of a row whose model is not ready.
    fn pending_row(
        c: &Ctx,
        cx: &mut Context<Self>,
        spec: &ModelSpec,
        parts: RowParts,
        (status, color): (SharedString, Hsla),
        actions: Vec<RowAction>,
    ) -> RowParts {
        let RowParts { body, ctl } = parts;
        let body = body.child(status_line(status, color));
        let ctl = actions.into_iter().fold(ctl, |ctl, action| {
            let (model, run) = (spec.id.clone(), action.run);
            ctl.child(model_action(
                action.id,
                &c.p,
                &c.t(action.label),
                action.kind,
                action.icon,
                cbk(cx, move |this: &mut Self, _, _| run(&this.st.models, &model)),
            ))
        });
        RowParts { body, ctl }
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
                return plate().bg(p.accent_soft).border_2().border_color(p.accent).into_any_element();
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
}

fn start_download(models: &ModelManager, id: &str) {
    let _ = models.start_download(id);
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

/// The arc is preserved even below 12 px.
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
