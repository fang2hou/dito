use dito_core::factory::LocalBackend;
use dito_core::health::EngineStatus;
use dito_core::i18n::Tr;
use dito_core::secrets::Role;
use dito_local::catalog::ModelSpec;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::local_rows::RowParts;
use super::page::ModelsPage;
use crate::assets::Ic;
use crate::settings::{Ctx, role};
use crate::theme::Palette;
use crate::ui::{self, BtnKind, cbk, text};

/// STATUS_H is the height of the engine-state pill that grows in under the memory line.
const STATUS_H: f32 = 24.;
const EXPAND_MS: u64 = 200;

/// The engine state line of the selected row: a dot, a label and its colours.
struct EngineLine {
    dot: Hsla,
    text: SharedString,
    fg: Hsla,
    pill: Pill,
}

/// A running or idle engine sits in a filled pill; a failure keeps plain text because it carries a reason.
enum Pill {
    Running,
    Idle,
    Plain,
}

impl EngineLine {
    fn new(c: &Ctx, status: &EngineStatus) -> Self {
        let p = &c.p;
        match status {
            EngineStatus::Running => Self {
                dot: p.model_running_dot,
                text: c.t(Tr::ModelsRunning),
                fg: p.model_running_text,
                pill: Pill::Running,
            },
            EngineStatus::Loading => {
                Self { dot: p.primary, text: c.t(Tr::MenubarLoading), fg: p.primary, pill: Pill::Idle }
            }
            EngineStatus::Failed(reason) => Self {
                dot: p.danger,
                text: SharedString::from(format!("{}：{reason}", c.t(Tr::ModelsFailed))),
                fg: if p.dark { p.danger } else { p.danger_text },
                pill: Pill::Plain,
            },
            _ => Self { dot: p.model_meta, text: c.t(Tr::MenubarStopped), fg: p.model_meta, pill: Pill::Idle },
        }
    }

    /// Engine state as a pill: green while running, grey while loading or stopped.
    fn element(&self, p: &Palette) -> Div {
        div()
            .flex()
            .flex_none()
            .self_start()
            .items_center()
            .gap(px(6.))
            .map(|d| match self.pill {
                Pill::Running => d.h(px(STATUS_H)).px(px(8.)).rounded_full().bg(p.model_running_bg),
                Pill::Idle => d.h(px(STATUS_H)).px(px(8.)).rounded_full().bg(p.seg),
                Pill::Plain => d,
            })
            .child(div().size(px(6.)).rounded_full().bg(self.dot))
            .child(text(self.text.clone(), 12., FontWeight::MEDIUM, self.fg).whitespace_nowrap())
    }
}

impl ModelsPage {
    /// Fixed-height slots with an animated interior: idle rows show a single line, the
    /// active row expands to meta + live status, easing between the two inside the same slot. The
    /// section height never jumps: the expansion moves from row to row.
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
        let status = self.st.local.status(&spec.id);
        let expand_id = ElementId::from(SharedString::from(format!("model-expand-{}-{}", role::index(role), spec.id)));
        let body = body.child(ui::tweened(
            expand_id,
            if selected { 1. } else { 0. },
            EXPAND_MS,
            Self::expanding_block(c, spec, &status),
        ));
        let ctl = ctl
            .child(self.use_action(c, cx, role, spec, selected))
            .child(self.more_control(c, cx, role, spec, selected));
        RowParts { body, ctl }
    }

    /// One progress value `t` (0 idle, 1 in use) drives the whole block, so nothing pops: the "downloaded"
    /// tag fades while the block rises, the memory line only rises, and the status pill grows in from
    /// nothing. The block is vertically centred in the card, so growing it is what moves the lines up.
    fn expanding_block(c: &Ctx, spec: &ModelSpec, status: &EngineStatus) -> impl Fn(f32) -> Div + 'static {
        let p = c.p;
        let engine = EngineLine::new(c, status);
        let (name, size_txt) = (spec.name.clone(), SharedString::from(ui::fmt_bytes(spec.total_size())));
        let mem_txt = SharedString::from(format!("{} {:.1} GB", c.t(Tr::ModelsMemory), spec.memory_mb as f32 / 1000.0));
        let downloaded = c.t(Tr::ModelsDownloaded);
        move |t| {
            let mut name_line = div()
                .flex()
                .items_center()
                .gap(px(10.))
                .h(px(20. + 3. * (1. - t)))
                .child(text(name.clone(), 14., FontWeight::MEDIUM, p.text))
                .child(text(size_txt.clone(), 12., FontWeight::NORMAL, p.model_meta));
            if t < 0.99 {
                name_line =
                    name_line.child(div().px(px(8.)).py(px(3.)).rounded(px(6.)).bg(p.seg).opacity(1. - t).child(text(
                        downloaded.clone(),
                        11.,
                        FontWeight::MEDIUM,
                        p.model_badge_text,
                    )));
            }
            let mut col = div().flex().flex_col().child(name_line).child(div().mt(px(6.)).child(text(
                mem_txt.clone(),
                12.,
                FontWeight::NORMAL,
                p.model_meta,
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
                        .child(engine.element(&p)),
                );
            }
            col
        }
    }

    fn use_action(&self, c: &Ctx, cx: &mut Context<Self>, role: Role, spec: &ModelSpec, selected: bool) -> AnyElement {
        let p = &c.p;
        if selected {
            return div()
                .h(px(36.))
                .px(px(16.))
                .rounded_full()
                .bg(p.model_in_use_pill_bg)
                .flex()
                .items_center()
                .justify_center()
                .child(text(c.t(Tr::ModelsInUse), 13., FontWeight::MEDIUM, p.model_in_use_pill_text))
                .into_any_element();
        }
        let id = spec.id.clone();
        ui::button(
            SharedString::from(format!("use-{id}")),
            p,
            &c.t(Tr::ModelsUse),
            BtnKind::Outline,
            None,
            cbk(cx, move |this: &mut Self, _, cx| {
                cx.stop_propagation();
                this.use_model(role, &id);
                cx.notify();
            }),
        )
        .into_any_element()
    }

    /// Make `id` the role's engine; a running previous model hands over to it (unload, then restart).
    fn use_model(&self, role: Role, id: &str) {
        let previous = role::engine(&self.st.cfg.get(), role).local_model.clone();
        let hand_over = matches!(self.st.local.status(&previous), EngineStatus::Running | EngineStatus::Loading);
        let selected = id.to_string();
        self.update_engine(role, move |e| e.local_model = selected);
        if hand_over {
            self.st.local.unload(&previous);
            let local = self.st.local.clone();
            let id = id.to_string();
            self.st.rt.spawn(async move {
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
