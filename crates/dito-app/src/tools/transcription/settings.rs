//! The transcription module's own settings (`design:tools/settings`): the gear beside the page title opens the shared
//! tool settings dialog with this module's content. Every change saves at once; the close button and Esc dismiss it.

use dito_core::config::DEFAULT_TITLE_TEMPLATE;
use dito_core::i18n::Tr;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::TranscriptionPage;
use crate::assets::Ic;
use crate::services;
use crate::settings::Ctx;
use crate::ui::{self, CircleAppearance, CircleSize, InputEvent, TextareaState, cbk, click_cb, text};

/// Turning automatic titles off clips and fades the prompt area while the dialog resizes (`design:tools/settings`).
const PROMPT_MS: u64 = 180;
/// Taller than the prompt area ever is, so the clip only bites while it closes.
const PROMPT_MAX_H: f32 = 340.;
const CONTENT_GAP: f32 = 16.;
/// The prompt editor starts at five lines and grows to eight before it scrolls (`design:tools/settings`).
const PROMPT_ROWS: (usize, usize) = (5, 8);

pub(super) struct ModuleSettings {
    pub(super) open: bool,
    focus: FocusHandle,
    template: Entity<TextareaState>,
}

impl ModuleSettings {
    pub(super) fn new(instruction: &str, window: &mut Window, cx: &mut Context<TranscriptionPage>) -> Self {
        let value = instruction.to_owned();
        let template =
            cx.new(|cx| TextareaState::new(window, cx).auto_grow(PROMPT_ROWS.0, PROMPT_ROWS.1).default_value(value));
        cx.subscribe_in(&template, window, |page: &mut TranscriptionPage, state, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change) {
                let value = state.read(cx).value().to_string();
                services::save_config(&page.st, move |cfg| cfg.transcription.title_template = value);
            }
        })
        .detach();
        Self { open: false, focus: cx.focus_handle(), template }
    }
}

/// The gear: the shared Icon Button, Active while its dialog is open.
pub(super) fn gear(page: &TranscriptionPage, c: &Ctx, cx: &mut Context<TranscriptionPage>) -> Stateful<Div> {
    let id = ElementId::from("transcription-settings");
    let label = c.t(Tr::NavSettings);
    if !page.module.open {
        ui::tip_label(id.clone(), label.clone());
    }
    let button = ui::icon_button(
        id.clone(),
        &c.p,
        Ic::Settings,
        page.module.open,
        cbk(cx, |page: &mut TranscriptionPage, window, cx| {
            page.module.open = true;
            ui::dismiss_tips(&page.motion.handle());
            page.module.focus.focus(window, cx);
            cx.notify();
        }),
    )
    .role(Role::Button)
    .aria_label(label);
    if page.module.open { button } else { button.child(ui::tip_above(&c.p, &id)) }
}

/// The dialog over the whole page, or nothing while it is closed.
pub(super) fn dialog(
    page: &TranscriptionPage,
    c: &Ctx,
    window: &Window,
    cx: &mut Context<TranscriptionPage>,
) -> Option<AnyElement> {
    if !page.module.open {
        return None;
    }
    let close = cbk(cx, |page: &mut TranscriptionPage, _, cx| {
        page.module.open = false;
        cx.notify();
    });
    let content = content(page, c, window, cx);
    let dialog = ui::tool_settings_dialog(
        window,
        &c.p,
        "transcription-settings-dialog",
        &page.module.focus,
        c.t(Tr::NavSettings),
        content.into_any_element(),
        close,
    );
    // The backdrop covers the whole window, sidebar included, not only the page that owns the gear.
    let size = window.viewport_size();
    Some(
        deferred(
            anchored()
                .position(point(px(0.), px(0.)))
                .child(div().relative().w(size.width).h(size.height).child(dialog)),
        )
        .with_priority(1)
        .into_any_element(),
    )
}

/// The automatic title switch, then the prompt that only shows while it is on.
fn content(page: &TranscriptionPage, c: &Ctx, window: &Window, cx: &mut Context<TranscriptionPage>) -> Div {
    let p = c.p;
    let auto = page.st.cfg.get().transcription.auto_title;
    let st = page.st.clone();
    let switch = ui::switch(
        "transcription-auto-title",
        &p,
        auto,
        std::rc::Rc::new(move |on, _, _| {
            services::save_config(&st, move |cfg| cfg.transcription.auto_title = on);
        }),
    );
    let toggle = div()
        .w_full()
        .flex()
        .items_start()
        .gap(px(16.))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(text(c.t(Tr::TranscriptionAutoTitle), 13., FontWeight::MEDIUM, p.text).line_height(px(20.)))
                .child(
                    text(c.t(Tr::TranscriptionAutoTitleHint), 12., FontWeight::NORMAL, p.muted).line_height(px(18.)),
                ),
        )
        .child(switch);
    let ms = if ui::Motion::system().is_reduced() { 0 } else { PROMPT_MS };
    let (k, moving) = ui::fade_value(&ElementId::from("transcription-title-prompt"), f32::from(auto), ms);
    if moving {
        window.request_animation_frame();
    }
    let mut body = div().w_full().flex().flex_col().child(toggle);
    if k > 0.01 {
        body = body.child(
            div()
                .w_full()
                .max_h(px(k * (PROMPT_MAX_H + CONTENT_GAP)))
                .overflow_hidden()
                .opacity(k)
                .pt(px(CONTENT_GAP))
                .child(prompt(page, c, window, cx)),
        );
    }
    body
}

/// The prompt label with its ghost reset, the editor, and where the title comes from.
fn prompt(page: &TranscriptionPage, c: &Ctx, window: &Window, cx: &mut Context<TranscriptionPage>) -> Div {
    let p = c.p;
    let focused = page.module.template.read(cx).focus_handle(cx).contains_focused(window, cx);
    let style = ui::TextAreaStyle {
        bg: p.input_bg,
        edge: None,
        radius: 12.,
        text_size: 13.,
        line_height: 20.,
        pad: [12., 10., 12., 10.],
        fill: false,
        reserve_bottom: 0.,
    };
    let editor = ui::with_ring(
        div().w_full().rounded(px(12.)).child(ui::text_area(&page.module.template, style)),
        "transcription-title-prompt-editor",
        focused,
        12.,
        p.border_strong,
        p.accent,
    );
    let reset = ui::circle_control(
        "transcription-reset-prompt",
        Ic::RotateCcw,
        CircleSize::Small,
        CircleAppearance { background: None, foreground: p.muted, border: None, hover: Some(p.seg) },
    )
    .cursor_pointer()
    .role(Role::Button)
    .aria_label(c.t(Tr::TranscriptionResetFormat))
    .on_click(click_cb(cbk(cx, |page: &mut TranscriptionPage, window, cx| {
        page.module.template.update(cx, |t, cx| t.set_value(DEFAULT_TITLE_TEMPLATE, window, cx));
        services::save_config(&page.st, |cfg| cfg.transcription.title_template = DEFAULT_TITLE_TEMPLATE.into());
        cx.notify();
    })));
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(div().flex_1().child(text(c.t(Tr::TranscriptionTitleFormat), 12., FontWeight::MEDIUM, p.muted)))
                .child(reset),
        )
        .child(editor)
        .child(text(c.t(Tr::TranscriptionModelHint), 12., FontWeight::NORMAL, p.muted).line_height(px(18.)))
}

impl TranscriptionPage {
    /// Opens or closes the settings dialog for the screenshot harness.
    #[cfg(feature = "screenshots")]
    pub fn pose_settings(&mut self, open: bool, cx: &mut Context<Self>) {
        self.module.open = open;
        cx.notify();
    }
}
