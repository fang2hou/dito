use std::rc::Rc;
use std::sync::Arc;

use dito_core::config::Source;
use dito_core::i18n::{Lang, Tr};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{Ctx, watch_config};
use crate::assets::Ic;
use crate::state::AppState;
use crate::timefmt::now;
use crate::ui::{self, BtnKind, cbk, text};

pub struct DictionaryPage {
    st: Arc<AppState>,
    add: Entity<InputState>,
    search: Entity<InputState>,
    query: String,
    revision: u64,
    placeholder_lang: Lang,
    /// The page's own motion scope: its controls' transient state, reset on navigation.
    motion: ui::MotionScope,
}

impl DictionaryPage {
    pub fn new(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        watch_config(&st, cx);
        let lang = st.cfg.get().general.lang();
        let add = cx.new(|cx| InputState::new(window, cx).placeholder(lang.tr(Tr::GeneralExampleHint)));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(lang.tr(Tr::GeneralSearchWords)));
        ui::notify_on_focus(&add, window, cx);
        ui::notify_on_focus(&search, window, cx);
        cx.subscribe_in(&add, window, |this: &mut Self, _, ev: &InputEvent, window, cx| {
            if matches!(ev, InputEvent::PressEnter { .. }) {
                this.add_word(window, cx);
            }
        })
        .detach();
        cx.subscribe_in(&search, window, |this: &mut Self, state, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change) {
                this.query = state.read(cx).value().to_string();
                cx.notify();
            }
        })
        .detach();
        Self {
            motion: ui::MotionScope::new(),
            st,
            add,
            search,
            query: String::new(),
            revision: 0,
            placeholder_lang: lang,
        }
    }

    fn add_word(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let word = self.add.read(cx).value().to_string();
        match self.st.store.add_word(&word, now()) {
            Ok(true) => {
                self.add.update(cx, |s, cx| s.set_value("", window, cx));
                self.revision += 1;
            }
            Ok(false) => {}
            Err(e) => tracing::warn!("dictionary: {e}"),
        }
        cx.notify();
    }

    fn import(&mut self, cx: &mut Context<Self>) {
        let rx =
            cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: None });
        let st = self.st.clone();
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await
                && let Some(path) = paths.into_iter().next()
            {
                match std::fs::read_to_string(&path) {
                    Ok(text) => {
                        if let Err(e) = st.store.import_words(&text, now()) {
                            tracing::warn!("import: {e}");
                        }
                    }
                    Err(e) => tracing::warn!("import: {e}"),
                }
            }
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    fn export(&mut self, cx: &mut Context<Self>) {
        let dir = dirs_download();
        let rx = cx.prompt_for_new_path(&dir, Some("dito-dictionary.txt"));
        let st = self.st.clone();
        cx.spawn(async move |_, _| {
            if let Ok(Ok(Some(path))) = rx.await
                && let Ok(text) = st.store.export_words()
                && let Err(e) = std::fs::write(&path, text)
            {
                tracing::warn!("export: {e}");
            }
        })
        .detach();
    }
}

fn dirs_download() -> std::path::PathBuf {
    std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join("Downloads")).unwrap_or_else(|| ".".into())
}

impl DictionaryPage {
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

impl Render for DictionaryPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The page draws under its own scope: after a page switch its controls mount statically,
        // never sliding to values remembered from another page or an earlier visit.
        ui::with_motion_scope(self.motion.handle(), || self.view(window, cx))
    }
}

impl DictionaryPage {
    fn view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        let cfg = self.st.cfg.get();
        let _ = self.revision;
        if self.placeholder_lang != c.lang {
            self.placeholder_lang = c.lang;
            self.add.update(cx, |s, cx| s.set_placeholder(c.lang.tr(Tr::GeneralExampleHint), window, cx));
            self.search.update(cx, |s, cx| s.set_placeholder(c.lang.tr(Tr::GeneralSearchWords), window, cx));
        }
        let words = self.st.store.words(Some(&self.query)).unwrap_or_default();
        let total = self.st.store.words(None).map(|w| w.len()).unwrap_or(0);

        let actions = div()
            .flex()
            .gap(px(10.))
            .child(ui::button(
                "import",
                &p,
                &c.t(Tr::DictionaryImport),
                BtnKind::Secondary,
                Some(Ic::Upload),
                cbk(cx, |this: &mut Self, _, cx| this.import(cx)),
            ))
            .child(ui::button(
                "export",
                &p,
                &c.t(Tr::DictionaryExport),
                BtnKind::Secondary,
                Some(Ic::Download),
                cbk(cx, |this: &mut Self, _, cx| this.export(cx)),
            ))
            .into_any_element();

        let hotwords_unsupported = cfg.asr.source == Source::Local && cfg.asr.local_model == "sensevoice-small";
        let mut add_card = ui::card(&p).bg(p.sub).px(px(26.)).py(px(18.)).gap(px(12.)).child(
            div()
                .w_full()
                .flex()
                .items_end()
                .gap(px(12.))
                .child(div().flex_1().min_w(px(0.)).child(ui::field(
                    &p,
                    c.t(Tr::DictionaryWord),
                    ui::text_input(&p, &self.add, None),
                )))
                .child(ui::button(
                    "add-word",
                    &p,
                    &c.t(Tr::DictionaryAdd),
                    BtnKind::Primary,
                    Some(Ic::Plus),
                    cbk(cx, |this: &mut Self, w, cx| this.add_word(w, cx)),
                )),
        );
        if hotwords_unsupported {
            add_card = add_card.child(
                div().w_full().flex().items_start().gap(px(10.)).child(ui::icon(Ic::Alert, 16., p.accent_text)).child(
                    div().flex_1().min_w(px(0.)).child(text(
                        c.t(Tr::DictionaryNoHotwordSupport),
                        12.,
                        FontWeight::NORMAL,
                        p.accent_text,
                    )),
                ),
            );
        }

        let st = self.st.clone();
        let polish_opt = ui::card(&p)
            .px(px(24.))
            .py(px(16.))
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(16.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(text(c.t(Tr::DictionarySameSpelling), 14., FontWeight::MEDIUM, p.text))
                    .child(ui::muted12(&p, c.t(Tr::DictionaryWordsSentToPolish))),
            )
            .child(ui::switch(
                "dict-polish",
                &p,
                cfg.dictionary_for_polish,
                Rc::new(move |on, _, _| {
                    let _ = st.cfg.update(|c| c.dictionary_for_polish = on);
                }),
            ));

        let toolbar = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(ui::muted12(&p, format!("{} {}", total, c.t(Tr::UnitsItems))).text_size(px(13.)))
            .child(div().w(px(240.)).child(ui::text_input(&p, &self.search, Some(Ic::Search))));

        // Content width = window - sidebar (232) - page padding (2 x 56); 4 columns when there is room, else 3.
        let avail = f32::from(window.viewport_size().width) - 232. - 112.;
        let cols: u16 = if avail >= 720. { 4 } else { 3 };
        let mut table = div().w_full().grid().grid_cols(cols).gap(px(10.));
        if words.is_empty() {
            table = div()
                .w_full()
                .py(px(28.))
                .flex()
                .justify_center()
                .child(ui::muted12(&p, c.t(Tr::DictionaryNoWordsYet)).text_size(px(13.)));
        }
        for (i, w) in words.into_iter().enumerate() {
            let st = self.st.clone();
            let w2 = w.clone();
            let group = SharedString::from(format!("chip-{i}"));
            let remove = div()
                .id(SharedString::from(format!("del-{i}")))
                .size(px(24.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .cursor_pointer()
                .opacity(0.)
                .group_hover(group.clone(), |s| s.opacity(1.))
                .hover(|s| s.bg(p.seg))
                .on_click(ui::click_cb(cbk(cx, move |this: &mut Self, _, cx| {
                    let _ = st.store.remove_word(&w2);
                    this.revision += 1;
                    cx.notify();
                })))
                .child(ui::icon(Ic::X, 14., p.muted));
            table = table.child(
                div()
                    .group(group)
                    .min_w(px(0.))
                    .h(px(44.))
                    .pl(px(16.))
                    .pr(px(10.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .rounded_full()
                    .bg(p.card)
                    .border_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .overflow_hidden()
                            .child(text(w, 14., FontWeight::MEDIUM, p.text).whitespace_nowrap().text_ellipsis()),
                    )
                    .child(remove),
            );
        }

        super::page(
            p,
            "dictionary",
            c.t(Tr::NavDictionary),
            Some(actions),
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(20.))
                .child(div().child(text(c.t(Tr::DictionaryWordsPassedToAsr), 14., FontWeight::NORMAL, p.muted)))
                .child(add_card)
                .child(polish_opt)
                .child(toolbar)
                .child(table),
        )
    }
}
