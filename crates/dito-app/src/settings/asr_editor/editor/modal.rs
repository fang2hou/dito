use std::rc::Rc;

use dito_core::i18n::Tr;
use gpui_kit::component::scroll::{Scrollbar, ScrollbarHandle, ScrollbarMode};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::AsrEditor;
use crate::settings::Ctx;
use crate::settings::asr_editor::help::help_body;
use crate::settings::asr_highlight::JsonHighlight;
use crate::settings::edge_fade::{Edge, edge_fade};
use crate::theme::Palette;
use crate::ui::{self, BtnKind, cbk, text};

impl AsrEditor {
    pub(super) fn modal(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        self.style_source(&p, cx);

        let height =
            (f32::from(window.viewport_size().height) - 48.).min(if self.help { 836. } else { 900. }).max(400.);
        let width = (f32::from(window.viewport_size().width) - 48.).clamp(320., 900.);
        // A short window trims its vertical padding so the close cross keeps its golden-ratio offsets.
        let pad_v = if height < 700. { 14. } else { 28. };
        let body = div().w_full().flex_1().min_h(px(0.)).flex().flex_col().gap(px(14.));
        let (body, footer) = if self.help {
            (self.help_panel(body, &c, cx).into_any_element(), self.help_footer(&c, cx))
        } else {
            (
                body.id("onboarding-asr-editor").overflow_y_scroll().child(self.inline(window, cx)).into_any_element(),
                div(),
            )
        };
        div()
            .id("asr-help-modal")
            .track_focus(&self.help_focus)
            .absolute()
            .inset_0()
            .occlude()
            .bg(hsla(0., 0., 0., 0.3))
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.help_focus.focus(window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    this.back_or_close(window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .w(px(width))
                    .h(px(height))
                    .px(px(28.))
                    .py(px(pad_v))
                    .rounded(px(20.))
                    .bg(p.bg)
                    .border_1()
                    .border_color(p.border_strong)
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(self.header(&c, cx))
                    .child(body)
                    .child(div().flex_none().child(footer)),
            )
    }

    pub(super) fn style_source(&mut self, p: &Palette, cx: &mut Context<Self>) {
        let p = *p;
        if self.highlighter_dark != Some(p.dark) {
            self.highlighter_dark = Some(p.dark);
            self.source.update(cx, |source, cx| {
                source.set_highlighter_factory(Rc::new(move |_| Some(Box::new(JsonHighlight::new(p.dark)))), cx)
            });
        }
        self.source.update(cx, |source, _| {
            source.set_scrollbar_renderer(Some(Rc::new(move |geometry, _, _| {
                let offset = -f32::from(geometry.offset().y);
                let remaining =
                    f32::from(geometry.content_size().height - geometry.viewport_bounds().size.height) - offset;
                div()
                    .absolute()
                    .inset_0()
                    .when(offset > 1., |d| d.child(edge_fade(Edge::Top, p.sub, 20., 16.)))
                    .when(remaining > 1., |d| d.child(edge_fade(Edge::Bottom, p.sub, 20., 16.)))
                    .child(Scrollbar::new(&geometry).axis(geometry.axis()).mode(ScrollbarMode::Always).styles(|s| {
                        s.track(|s| s.bg(p.border)).thumb(|s| s.bg(p.muted).width(px(4.)).inset(px(6.)).radius(px(2.)))
                    }))
                    .into_any_element()
            })))
        });
    }

    fn header(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let title = if self.help { Tr::AsrEditorHelpTitle } else { Tr::AsrEditorTitle };
        div()
            .w_full()
            .h(px(32.))
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .child(text(c.t(title), 26., FontWeight::BOLD, c.p.text).line_height(px(32.)))
            .child(ui::close_button(
                "asr-close",
                &c.p,
                cbk(cx, |this: &mut Self, window, cx| this.back_or_close(window, cx)),
            ))
    }

    pub(super) fn help_panel(&self, body: Div, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = &c.p;
        let scroll = self.help_scroll.clone();
        let offset = -f32::from(scroll.offset().y);
        let max = f32::from(scroll.max_offset().y);
        body.child(
            div()
                .relative()
                .flex_1()
                .min_h(px(0.))
                .child(
                    div()
                        .id("asr-help-scroll")
                        .h_full()
                        .overflow_y_scroll()
                        .track_scroll(&scroll)
                        .on_scroll(cx.listener(|_, _, _, cx| {
                            cx.emit(super::Changed);
                            cx.notify();
                        }))
                        .child(help_body(p, c.lang)),
                )
                .when(offset > 1., |d| d.child(edge_fade(Edge::Top, p.bg, 24., 0.)))
                .when(max - offset > 1., |d| d.child(edge_fade(Edge::Bottom, p.bg, 24., 0.))),
        )
    }

    pub(super) fn help_footer(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        div().flex().justify_end().child(ui::button(
            "asr-copy-help",
            &c.p,
            &c.t(Tr::AsrEditorCopyHelp),
            BtnKind::Primary,
            None,
            cbk(cx, |this: &mut Self, _, cx| {
                let language = this.st.cfg.get().general.lang();
                cx.write_to_clipboard(ClipboardItem::new_string(
                    dito_core::provider::configured_asr::configuration_spec(language, this.selected),
                ));
            }),
        ))
    }
}
