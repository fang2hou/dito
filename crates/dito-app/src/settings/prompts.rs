use std::sync::Arc;

use dito_core::i18n::{Lang, Tr};
use dito_core::prompts::{Prompts, Style};
use gpui_kit::component::input::{InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::Ctx;
use crate::assets::Ic;
use crate::services;
use crate::state::AppState;
use crate::theme::Palette;
use crate::ui::{self, BtnKind, cbk, cbk_idx, text};

enum Try {
    Idle,
    Running,
    Done(String, u64),
    Failed(String),
}

pub struct PromptsPage {
    st: Arc<AppState>,
    lang: Lang,
    style: Style,
    base: Entity<TextareaState>,
    instruction: Entity<TextareaState>,
    samples: Vec<Entity<InputState>>,
    try_in: Entity<TextareaState>,
    try_state: Try,
    dirty: bool,
    /// The page's own motion scope: its controls' transient state, reset on navigation.
    motion: ui::MotionScope,
}

fn textarea(window: &mut Window, cx: &mut Context<PromptsPage>, value: &str) -> Entity<TextareaState> {
    let v = value.to_string();
    cx.new(|cx| TextareaState::new(window, cx).default_value(v))
}

impl PromptsPage {
    pub fn new(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        services::watch_config(&st, cx);
        let cfg = st.cfg.get();
        let (lang, style) = (cfg.general.lang(), Style::General);
        let set = cfg.prompts.set(lang);
        let base = textarea(window, cx, &set.base);
        let instruction = textarea(window, cx, &set.style(style).instruction);
        let try_in = textarea(window, cx, dito_core::i18n::sample_utterance(lang));
        for ent in [&base, &instruction, &try_in] {
            let handle = ent.read(cx).focus_handle(cx);
            cx.on_focus_in(&handle, window, |_, _, cx| cx.notify()).detach();
            cx.on_focus_out(&handle, window, |_, _, _, cx| cx.notify()).detach();
        }
        for ent in [&base, &instruction] {
            cx.subscribe_in(ent, window, |this: &mut Self, _, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    this.dirty = true;
                    cx.notify();
                }
            })
            .detach();
        }
        let mut page = Self {
            motion: ui::MotionScope::new(),
            st,
            lang,
            style,
            base,
            instruction,
            samples: Vec::new(),
            try_in,
            try_state: Try::Idle,
            dirty: false,
        };
        page.rebuild_samples(window, cx);
        page
    }

    fn rebuild_samples(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cfg = self.st.cfg.get();
        let list = cfg.prompts.set(self.lang).style(self.style).samples.clone();
        self.samples = list.into_iter().map(|s| self.sample_input(&s, window, cx)).collect();
    }

    fn sample_input(&self, value: &str, window: &mut Window, cx: &mut Context<Self>) -> Entity<InputState> {
        let v = value.to_string();
        let ent = cx.new(|cx| InputState::new(window, cx).default_value(v));
        ui::notify_on_focus(&ent, window, cx);
        cx.subscribe_in(&ent, window, |this: &mut Self, _, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change) {
                this.dirty = true;
                cx.notify();
            }
        })
        .detach();
        ent
    }

    /// Current edits applied to a copy of the saved prompts.
    fn edited(&self, cx: &App) -> Prompts {
        let mut prompts = self.st.cfg.get().prompts.clone();
        let set = prompts.set_mut(self.lang);
        set.base = self.base.read(cx).value().to_string();
        let sp = set.style_mut(self.style);
        sp.instruction = self.instruction.read(cx).value().to_string();
        sp.samples =
            self.samples.iter().map(|s| s.read(cx).value().to_string()).filter(|s| !s.trim().is_empty()).collect();
        prompts
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let edited = self.edited(cx);
        services::save_config(&self.st, |c| c.prompts = edited);
        self.dirty = false;
        cx.notify();
    }

    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cfg = self.st.cfg.get();
        let set = cfg.prompts.set(self.lang);
        let (base, instr) = (set.base.clone(), set.style(self.style).instruction.clone());
        self.base.update(cx, |s, cx| s.set_value(base, window, cx));
        self.instruction.update(cx, |s, cx| s.set_value(instr, window, cx));
        self.rebuild_samples(window, cx);
        self.dirty = false;
        cx.notify();
    }

    fn switch(&mut self, lang: Lang, style: Style, window: &mut Window, cx: &mut Context<Self>) {
        if self.dirty {
            self.save(cx);
        }
        if self.lang != lang {
            // Keep the try-it sentence in the prompt language unless the user wrote their own.
            let old = dito_core::i18n::sample_utterance(self.lang);
            if self.try_in.read(cx).value().as_ref() == old {
                let new = dito_core::i18n::sample_utterance(lang);
                self.try_in.update(cx, |s, cx| s.set_value(new, window, cx));
            }
        }
        self.lang = lang;
        self.style = style;
        self.load(window, cx);
    }

    fn run_try(&mut self, cx: &mut Context<Self>) {
        let text = self.try_in.read(cx).value().to_string();
        if text.trim().is_empty() {
            return;
        }
        self.try_state = Try::Running;
        cx.notify();
        let (st, prompts, style) = (self.st.clone(), self.edited(cx), self.style);
        cx.spawn(async move |this, cx| {
            let r = services::try_polish(st, prompts, style, text).await;
            let _ = this.update(cx, |t, cx| {
                t.try_state = match r {
                    Ok((out, ms)) => Try::Done(out, ms),
                    Err(e) => Try::Failed(e),
                };
                cx.notify();
            });
        })
        .detach();
    }
}

impl PromptsPage {
    /// Reset the page's transient motion state; its controls mount statically on the next entry.
    pub(super) fn reset_motion(&mut self) {
        self.motion.reset();
    }

    /// The page's scope, for the screenshot harness.
    #[cfg(feature = "screenshots")]
    pub(super) fn motion_scope(&self) -> ui::ScopeHandle {
        self.motion.handle()
    }
}

impl Render for PromptsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The page draws under its own scope: after a page switch its controls mount statically,
        // never sliding to values remembered from another page or an earlier visit.
        ui::with_motion_scope(self.motion.handle(), || self.view(window, cx))
    }
}

impl PromptsPage {
    fn view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        super::page(
            p,
            "prompts",
            c.t(Tr::NavPrompts),
            None,
            div().w_full().flex().flex_col().gap(px(20.)).child(self.header(&c, cx)).child(
                div()
                    .w_full()
                    .flex()
                    .items_start()
                    .gap(px(20.))
                    .child(self.editors_column(&c, window, cx))
                    .child(self.try_card(&c, window, cx)),
            ),
        )
    }

    fn header(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = c.p;
        let ui_langs = [Lang::Ja, Lang::En, Lang::Zh];
        let lang_seg = ui::seg(
            "prompt-lang",
            &p,
            &["日本語".into(), "English".into(), "中文".into()],
            ui_langs.iter().position(|l| *l == self.lang).unwrap_or(0),
            cbk_idx(cx, move |this: &mut Self, i, window, cx| {
                let style = this.style;
                this.switch(ui_langs[i], style, window, cx);
            }),
        );
        let actions = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(ui::button(
                "reset-prompts",
                &p,
                &c.t(Tr::PromptsRestoreDefaults),
                BtnKind::Secondary,
                Some(Ic::Reset),
                cbk(cx, |this: &mut Self, window, cx| {
                    let lang = this.lang;
                    services::save_config(&this.st, |c| c.prompts.reset(lang));
                    this.load(window, cx);
                }),
            ))
            .child(
                ui::button(
                    "save-prompts",
                    &p,
                    &c.t(Tr::PromptsSave),
                    BtnKind::Primary,
                    None,
                    cbk(cx, |this: &mut Self, _, cx| this.save(cx)),
                )
                .opacity(if self.dirty { 1.0 } else { 0.55 }),
            );

        div().w_full().flex().items_center().justify_between().child(lang_seg).child(actions)
    }

    fn editors_column(&self, c: &Ctx, window: &Window, cx: &mut Context<Self>) -> Div {
        let p = c.p;
        div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(self.base_card(c, window, cx))
            .child(div().w_full().flex().justify_center().child(
                div().size(px(26.)).rounded_full().bg(p.seg).flex().items_center().justify_center().child(ui::icon(
                    Ic::Plus,
                    14.,
                    p.muted,
                )),
            ))
            .child(self.style_card(c, window, cx))
    }

    fn base_card(&self, c: &Ctx, window: &Window, cx: &App) -> Div {
        let p = c.p;
        let base_focused = self.base.read(cx).focus_handle(cx).contains_focused(window, cx);
        ui::card(&p)
            .px(px(22.))
            .py(px(18.))
            .gap(px(10.))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_baseline()
                    .justify_between()
                    .child(text(c.t(Tr::PromptsBase), 15., FontWeight::BOLD, p.text))
                    .child(ui::muted12(&p, c.t(Tr::PromptsAppliedFirst))),
            )
            .child(prompt_editor(&p, &self.base, base_focused))
    }

    fn style_card(&self, c: &Ctx, window: &Window, cx: &mut Context<Self>) -> Div {
        let p = c.p;
        let instr_focused = self.instruction.read(cx).focus_handle(cx).contains_focused(window, cx);
        let styles = Style::ALL;
        let style_seg = ui::seg(
            "prompt-style",
            &p,
            &styles.map(|s| ui::style_label(c.lang, s)),
            styles.iter().position(|s| *s == self.style).unwrap_or(0),
            cbk_idx(cx, move |this: &mut Self, i, window, cx| {
                let lang = this.lang;
                this.switch(lang, styles[i], window, cx);
            }),
        );
        let mut style_card = ui::card(&p)
            .px(px(22.))
            .py(px(18.))
            .gap(px(12.))
            .child(div().flex().child(style_seg))
            .child(prompt_editor(&p, &self.instruction, instr_focused));
        style_card = style_card.child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .child(text(c.t(Tr::PromptsExamples), 13., FontWeight::BOLD, p.text))
                .child(ui::button(
                    "add-sample",
                    &p,
                    &c.t(Tr::DictionaryAdd),
                    BtnKind::Secondary,
                    Some(Ic::Plus),
                    cbk(cx, |this: &mut Self, window, cx| {
                        let ent = this.sample_input("", window, cx);
                        this.samples.push(ent);
                        cx.notify();
                    }),
                )),
        );
        for (i, s) in self.samples.iter().enumerate() {
            style_card = style_card.child(
                div().w_full().flex().items_center().gap(px(10.)).child(ui::text_input(&p, s, None)).child(
                    ui::bare_icon_button(
                        SharedString::from(format!("rm-sample-{i}")),
                        &p,
                        Ic::X,
                        cbk(cx, move |this: &mut Self, _, cx| {
                            if i < this.samples.len() {
                                this.samples.remove(i);
                                this.dirty = true;
                                cx.notify();
                            }
                        }),
                    ),
                ),
            );
        }
        style_card
    }

    fn try_card(&self, c: &Ctx, window: &Window, cx: &mut Context<Self>) -> Div {
        let p = c.p;
        let try_focused = self.try_in.read(cx).focus_handle(cx).contains_focused(window, cx);
        let (out_text, out_ms, out_err) = match &self.try_state {
            Try::Done(t, ms) => (t.clone(), Some(*ms), None),
            Try::Failed(e) => (String::new(), None, Some(ui::error_line(c.lang, e))),
            _ => (String::new(), None, None),
        };
        let running = matches!(self.try_state, Try::Running);
        ui::card(&p)
            .w(px(340.))
            .flex_none()
            .p(px(22.))
            .gap(px(14.))
            .child(text(c.t(Tr::PromptsTryIt), 15., FontWeight::BOLD, p.text))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(text(c.t(Tr::MenubarWhatYouSaid), 12., FontWeight::MEDIUM, p.muted))
                    .child(prompt_editor(&p, &self.try_in, try_focused).h(px(150.))),
            )
            .child(div().w_full().flex().justify_center().child(ui::button(
                "try-run",
                &p,
                &if running { c.t(Tr::HudPolishing) } else { c.t(Tr::PromptsSampleButton) },
                BtnKind::Primary,
                Some(Ic::Sparkles),
                cbk(cx, |this: &mut Self, _, cx| this.run_try(cx)),
            )))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(text(c.t(Tr::MenubarPolished), 12., FontWeight::MEDIUM, p.muted))
                            .children(out_ms.map(|ms| ui::tag(&p, format!("{ms} ms"), crate::theme::Tint::Neutral))),
                    )
                    .child(
                        div().w_full().h(px(210.)).rounded(px(14.)).bg(p.accent_soft).px(px(16.)).py(px(14.)).child(
                            match out_err {
                                Some(e) => {
                                    text(e, 13., FontWeight::NORMAL, if p.dark { p.text } else { p.danger_text })
                                        .line_height(relative(1.7))
                                }
                                None => text(out_text, 13., FontWeight::NORMAL, p.text).line_height(relative(1.7)),
                            },
                        ),
                    ),
            )
    }
}

fn prompt_editor(p: &Palette, ent: &Entity<TextareaState>, focused: bool) -> Div {
    let base = div().w_full().h(px(176.)).rounded(px(14.)).bg(p.input_bg).child(
        // With the component's internal leading, thin padding lands near the single-line input's inset and
        // keeps the writing area open next to the tight line spacing.
        Textarea::new(ent)
            .appearance(false)
            .bordered(false)
            .h_full()
            .text_size(px(13.))
            .pl(px(6.))
            .pr(px(6.))
            .pt(px(5.))
            .pb(px(5.)),
    );
    let id = ("textarea", ent.entity_id().as_u64() as usize);
    ui::with_ring(base, id, focused, 14., p.border_strong, p.accent)
}
