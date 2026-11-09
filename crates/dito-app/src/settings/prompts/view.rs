//! Prompt editors and the side-by-side preview from 20.03 Prompts.

use super::*;
use crate::assets::Ic;
use crate::theme::Palette;
use crate::ui::{BtnKind, cbk, cbk_idx, text};
use gpui_kit::EntityInputHandler;
use gpui_kit::base::input::{Redo, Undo};

impl PromptsPage {
    pub(super) fn view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        let langs = [Lang::Ja, Lang::En, Lang::Zh];
        let language = ui::seg(
            "prompt-lang",
            &p,
            &langs.map(|language| language.name(c.lang).into()),
            langs.iter().position(|l| *l == self.lang).unwrap_or(0),
            cbk_idx(cx, move |this: &mut Self, i, w, cx| this.switch(langs[i], this.style.clone(), w, cx)),
        );
        super::super::page(
            p,
            "prompts",
            c.t(Tr::NavPrompts),
            Some(language.into_any_element()),
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(20.))
                .child(self.base_card(&c, window, cx))
                .child(self.style_card(&c, window, cx))
                .child(self.try_card(&c, window, cx)),
        )
    }

    fn history_actions(&self, c: &Ctx, editor: Editor, cx: &mut Context<Self>) -> Div {
        let history = &self.histories[&self.history_key(editor)];
        let mut actions = div().flex_none().flex().items_center().gap(px(8.));
        for (name, glyph, label, enabled) in [
            ("undo", Ic::Undo, Tr::AsrEditorUndo, history.can_undo()),
            ("redo", Ic::Redo, Tr::AsrEditorRedo, history.can_redo()),
            ("restore", Ic::Reset, Tr::PromptsRestoreDefaults, true),
        ] {
            let id = ElementId::from(SharedString::from(format!("prompt-{editor:?}-{name}")));
            ui::tip_label(id.clone(), c.t(label));
            actions = actions.child(
                ui::icon_button_with(
                    id.clone(),
                    &c.p,
                    glyph,
                    false,
                    ui::CircleSize::Small,
                    cbk(cx, move |this: &mut Self, window, cx| {
                        if !enabled {
                            return;
                        }
                        match name {
                            "undo" => this.undo(editor, window, cx),
                            "redo" => this.redo(editor, window, cx),
                            _ => this.open_dialog(Dialog::Reset(editor), window, cx),
                        }
                    }),
                )
                .opacity(if enabled { 1. } else { 0.35 })
                .role(Role::Button)
                .aria_label(c.t(label))
                .child(ui::tip_above(&c.p, &id)),
            );
        }
        actions
    }

    fn save_status(&self, c: &Ctx, editor: Editor) -> Div {
        let saved = self.st.cfg.get();
        let current = self.drafts.set(self.lang);
        let persisted = saved.prompts.set(self.lang);
        let dirty = match editor {
            Editor::Common => current.base != persisted.base,
            Editor::Application => current.style(&self.style) != persisted.style(&self.style),
        };
        let label = if dirty && self.save_error.is_some() {
            Tr::PromptsSaveFailed
        } else if dirty {
            Tr::PromptsUnsaved
        } else {
            Tr::PromptsSaved
        };
        text(c.t(label), 12., FontWeight::NORMAL, c.p.muted).line_height(px(18.)).flex_none()
    }

    fn composing(&mut self, editor: Editor, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match editor {
            Editor::Common => self.base.update(cx, |state, cx| state.marked_text_range(window, cx).is_some()),
            Editor::Application => {
                self.instruction.update(cx, |state, cx| state.marked_text_range(window, cx).is_some())
                    || self
                        .samples
                        .iter()
                        .any(|input| input.update(cx, |state, cx| state.marked_text_range(window, cx).is_some()))
            }
        }
    }

    fn base_card(&self, c: &Ctx, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = c.p;
        ui::card(&p)
            .id("prompt-common-card")
            .capture_action(cx.listener(|this, _: &Undo, window, cx| {
                if this.composing(Editor::Common, window, cx) {
                    cx.propagate();
                    return;
                }
                this.undo(Editor::Common, window, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|this, _: &Redo, window, cx| {
                if this.composing(Editor::Common, window, cx) {
                    cx.propagate();
                    return;
                }
                this.redo(Editor::Common, window, cx);
                cx.stop_propagation();
            }))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_group(Editor::Common, cx)))
            .px(px(22.))
            .py(px(18.))
            .gap(px(12.))
            .child(
                div()
                    .h(px(36.))
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                text(c.t(Tr::PromptsBase), 16., FontWeight::BOLD, p.text)
                                    .line_height(px(24.))
                                    .truncate(),
                            )
                            .child(context_help(
                                &p,
                                "prompt-common-help",
                                c.t(Tr::PromptsCommonHelp),
                                &self.help_focus[0],
                                window,
                                cx,
                            ))
                            .child(self.save_status(c, Editor::Common)),
                    )
                    .child(self.history_actions(c, Editor::Common, cx)),
            )
            .child(
                prompt_editor(&p, &self.base, window, cx, 168.)
                    .id("prompt-common-editor")
                    .on_mouse_down_out(ui::blur_on_outside(self.base.read(cx).focus_handle(cx)))
                    .on_mouse_down(MouseButton::Left, ui::focus_on_click(self.base.read(cx).focus_handle(cx)))
                    .on_hover(cx.listener(|this, inside: &bool, _, cx| {
                        if !*inside {
                            this.close_group(Editor::Common, cx);
                        }
                    })),
            )
    }

    fn style_card(&self, c: &Ctx, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = c.p;
        let mut card = ui::card(&p)
            .id("prompt-application-card")
            .capture_action(cx.listener(|this, _: &Undo, window, cx| {
                if this.composing(Editor::Application, window, cx) {
                    cx.propagate();
                    return;
                }
                this.undo(Editor::Application, window, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|this, _: &Redo, window, cx| {
                if this.composing(Editor::Application, window, cx) {
                    cx.propagate();
                    return;
                }
                this.redo(Editor::Application, window, cx);
                cx.stop_propagation();
            }))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_group(Editor::Application, cx)))
            .px(px(22.))
            .py(px(18.))
            .gap(px(12.))
            .child(
                div()
                    .h(px(36.))
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                text(c.t(Tr::PromptsApplicationSettings), 16., FontWeight::BOLD, p.text)
                                    .line_height(px(24.))
                                    .truncate(),
                            )
                            .child(self.save_status(c, Editor::Application)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .flex_none()
                            .child(self.history_actions(c, Editor::Application, cx))
                            .children(self.remove_app_control(c, cx))
                            .child(self.application_picker(c, cx, false)),
                    ),
            )
            .child(
                prompt_editor(&p, &self.instruction, window, cx, 144.)
                    .id("prompt-application-editor")
                    .on_mouse_down_out(ui::blur_on_outside(self.instruction.read(cx).focus_handle(cx)))
                    .on_mouse_down(MouseButton::Left, ui::focus_on_click(self.instruction.read(cx).focus_handle(cx)))
                    .on_hover(cx.listener(|this, inside: &bool, _, cx| {
                        if !*inside {
                            this.close_group(Editor::Application, cx);
                        }
                    })),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(text(c.t(Tr::PromptsExamples), 13., FontWeight::BOLD, p.text))
                            .child(context_help(
                                &p,
                                "prompt-examples-help",
                                c.t(Tr::PromptsExamplesHelp),
                                &self.help_focus[1],
                                window,
                                cx,
                            )),
                    )
                    .child(ui::button(
                        "add-sample",
                        &p,
                        &c.t(Tr::PromptsAddExample),
                        BtnKind::Secondary,
                        Some(Ic::Plus),
                        cbk(cx, |this: &mut Self, w, cx| {
                            let input = this.sample_input("", w, cx);
                            input.read(cx).focus_handle(cx).focus(w, cx);
                            this.samples.push(input);
                            this.changed(Editor::Application, cx);
                            cx.notify();
                        }),
                    )),
            );
        if self.samples.is_empty() {
            card = card.child(ui::muted12(&p, c.t(Tr::PromptsExamplesEmpty)));
        }
        for (i, sample) in self.samples.iter().enumerate() {
            card = card.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(div().flex_1().min_w(px(0.)).child(ui::text_input(&p, sample, None)))
                    .child(ui::bare_icon_button(
                        SharedString::from(format!("rm-sample-{i}")),
                        &p,
                        Ic::X,
                        cbk(cx, move |this: &mut Self, _, cx| {
                            if i < this.samples.len() {
                                this.samples.remove(i);
                                this.changed(Editor::Application, cx);
                                cx.notify();
                            }
                        }),
                    )),
            );
        }
        card
    }

    fn try_card(&self, c: &Ctx, window: &Window, cx: &mut Context<Self>) -> Div {
        let p = c.p;
        let running = matches!(self.try_state, Try::Running);
        let (value, muted) = match &self.try_state {
            Try::Done(value, _) => (value.clone(), false),
            Try::Failed(error) => (ui::error_line(c.lang, error), false),
            _ => (c.t(Tr::PromptsPreviewPlaceholder).to_string(), true),
        };
        let output_color = if matches!(self.try_state, Try::Failed(_)) {
            p.danger
        } else if muted {
            p.muted
        } else {
            p.text
        };
        let latency = match self.try_state {
            Try::Done(_, ms) => Some(format!("{ms} ms")),
            _ => None,
        };
        ui::card(&p)
            .px(px(22.))
            .py(px(18.))
            .gap(px(12.))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(text(c.t(Tr::PromptsTryIt), 15., FontWeight::BOLD, p.text))
                    .child(
                        ui::button(
                            "try-run",
                            &p,
                            &if running { c.t(Tr::HudPolishing) } else { c.t(Tr::PromptsSampleButton) },
                            BtnKind::Primary,
                            Some(Ic::Sparkles),
                            cbk(cx, |this: &mut Self, _, cx| this.run_try(cx)),
                        )
                        .opacity(if running { 0.6 } else { 1. }),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_start()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(ui::muted12(&p, c.t(Tr::MenubarWhatYouSaid)))
                            .child(
                                prompt_editor(&p, &self.try_in, window, cx, 135.)
                                    .id("prompt-preview-input")
                                    .on_mouse_down_out(ui::blur_on_outside(self.try_in.read(cx).focus_handle(cx)))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        ui::focus_on_click(self.try_in.read(cx).focus_handle(cx)),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(ui::muted12(&p, c.t(Tr::MenubarPolished)))
                                    .children(latency.map(|s| ui::tag(&p, s, crate::theme::Tint::Neutral))),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .min_h(px(135.))
                                    .px(px(12.))
                                    .py(px(10.))
                                    .rounded(px(12.))
                                    .border_1()
                                    .border_color(p.border)
                                    .bg(p.sub)
                                    .child(text(value, 13., FontWeight::NORMAL, output_color).line_height(px(20.))),
                            ),
                    ),
            )
    }
}

fn prompt_editor(p: &Palette, ent: &Entity<TextareaState>, window: &Window, cx: &App, height: f32) -> Div {
    let focused = ent.read(cx).focus_handle(cx).contains_focused(window, cx);
    let body = div().w_full().h(px(height)).rounded(px(12.)).bg(p.input_bg).child(
        Textarea::new(ent)
            .appearance(false)
            .bordered(false)
            .h_full()
            .text_size(px(13.))
            .font_weight(FontWeight::NORMAL)
            .line_height(px(20.))
            .pl(px(11.))
            .pr(px(11.))
            .pt(px(9.))
            .pb(px(9.)),
    );
    ui::with_ring(body, ("textarea", ent.entity_id().as_u64() as usize), focused, 12., p.border_strong, p.accent)
}

fn context_help(
    p: &Palette,
    id: &'static str,
    label: SharedString,
    focus: &FocusHandle,
    window: &Window,
    cx: &App,
) -> Stateful<Div> {
    let key = ElementId::from(id);
    let on = ui::is_hovered(&key) || focus.contains_focused(window, cx);
    let p = *p;
    div()
        .id(id)
        .track_focus(focus)
        .relative()
        .size(px(24.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .on_hover(ui::hover_tracker(key.clone()))
        .on_key_down(|event, w, cx| {
            if event.keystroke.key == "escape" {
                w.blur(cx);
                w.refresh();
            }
        })
        .child(ui::tweened((key.clone(), "opacity").into(), if on { 1. } else { 0.6 }, 160, move |a| {
            div()
                .size(px(16.))
                .rounded_full()
                .bg(p.text)
                .opacity(a)
                .flex()
                .items_center()
                .justify_center()
                .child(text("i", 13., FontWeight::BOLD, p.card).font_family("Outfit").line_height(px(16.)))
        }))
        // Keep the positioned box out of the icon's flex layout at every animation frame.
        // Only the bubble moves; animating a wrapper changes the containing block mid-pop.
        .child(
            div().absolute().bottom(px(30.)).left(px(-148.)).w(px(320.)).h(px(0.)).child(
                deferred(anchored().anchor(Anchor::BottomLeft).snap_to_window_with_margin(px(8.)).child(ui::tweened(
                    ElementId::from((key, "help-pop")),
                    if on { 1. } else { 0. },
                    150,
                    move |v| {
                        let rise = if dito_platform::system::reduce_motion() { 0. } else { (1. - v) * 6. };
                        div()
                            .relative()
                            .top(px(rise))
                            .opacity(v)
                            .w(px(320.))
                            .p(px(14.))
                            .rounded(px(12.))
                            .bg(p.menu_bg)
                            .border_1()
                            .border_color(p.border_strong)
                            .shadow(vec![BoxShadow {
                                color: p.shadow,
                                offset: point(px(0.), px(4.)),
                                blur_radius: px(16.),
                                spread_radius: px(0.),
                                inset: false,
                            }])
                            .child(text(label.clone(), 13., FontWeight::NORMAL, p.text).line_height(px(20.)))
                    },
                )))
                .with_priority(2),
            ),
        )
}
