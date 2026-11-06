use std::rc::Rc;
use std::sync::Arc;

use dito_core::config::Source;
use dito_core::i18n::{Lang, Tr};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::downloads_dir::downloads_dir;
use super::page_query::{PageQuery, QueryPage};
use super::{Ctx, content_width};
use crate::assets::Ic;
use crate::services::{self, DictionaryWords};
use crate::state::AppState;
use crate::theme::Palette;
use crate::timefmt::now;
use crate::ui::{self, BtnKind, cbk, text};

pub struct DictionaryPage {
    st: Arc<AppState>,
    add: Entity<InputState>,
    search: Entity<InputState>,
    query_text: String,
    words: PageQuery<DictionaryWords>,
    loaded: bool,
    placeholder_lang: Lang,
    /// The page's own motion scope: its controls' transient state, reset on navigation.
    motion: ui::MotionScope,
}

impl QueryPage for DictionaryPage {
    type Data = DictionaryWords;

    fn page_query(&mut self) -> &mut PageQuery<DictionaryWords> {
        &mut self.words
    }

    fn fetch(&self) -> impl Future<Output = anyhow::Result<DictionaryWords>> + 'static {
        services::dictionary_words(self.st.clone(), self.query_text.clone())
    }

    fn applied(&mut self, cx: &mut Context<Self>) {
        if !self.loaded {
            self.loaded = true;
            cx.notify();
        }
    }
}

impl DictionaryPage {
    /// Page visibility lifecycle, driven by `Settings`: entering loads the words, leaving
    /// drops any result still in flight.
    pub fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        self.words.set_active(active);
        if active {
            self.load(cx);
        }
    }

    pub fn new(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        services::watch_config(&st, cx);
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
                this.query_text = state.read(cx).value().to_string();
                this.changed(cx);
                cx.notify();
            }
        })
        .detach();
        Self {
            motion: ui::MotionScope::new(),
            st,
            add,
            search,
            query_text: String::new(),
            words: PageQuery::new("dictionary", DictionaryWords::default()),
            loaded: false,
            placeholder_lang: lang,
        }
    }

    fn add_word(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let word = self.add.read(cx).value().to_string();
        match self.st.store.add_word(&word, now()) {
            Ok(true) => {
                self.add.update(cx, |s, cx| s.set_value("", window, cx));
                self.load(cx);
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
            let _ = this.update(cx, |this, cx| this.load(cx));
        })
        .detach();
    }

    fn export(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_new_path(&downloads_dir(), Some("dito-dictionary.txt"));
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
        if self.placeholder_lang != c.lang {
            self.placeholder_lang = c.lang;
            self.add.update(cx, |s, cx| s.set_placeholder(c.lang.tr(Tr::GeneralExampleHint), window, cx));
            self.search.update(cx, |s, cx| s.set_placeholder(c.lang.tr(Tr::GeneralSearchWords), window, cx));
        }
        let words = self.words.data.matching.clone();
        let total = self.loaded.then_some(self.words.data.total);

        super::page(
            p,
            "dictionary",
            c.t(Tr::NavDictionary),
            Some(self.actions(&c, cx)),
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(20.))
                .child(div().child(text(c.t(Tr::DictionaryWordsPassedToAsr), 14., FontWeight::NORMAL, p.muted)))
                .child(self.add_card(&c, cx))
                .child(self.polish_option(&c))
                .child(self.toolbar(&c, total))
                .child(if self.loaded { self.word_grid(&c, cx, window, words) } else { placeholder_grid(&p, window) }),
        )
    }

    fn actions(&self, c: &Ctx, cx: &mut Context<Self>) -> AnyElement {
        let p = c.p;
        div()
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
            .into_any_element()
    }

    fn add_card(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = c.p;
        let cfg = self.st.cfg.get();
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
        add_card
    }

    fn polish_option(&self, c: &Ctx) -> Div {
        let p = c.p;
        let cfg = self.st.cfg.get();
        let st = self.st.clone();
        ui::card(&p)
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
                    services::save_config(&st, |c| c.dictionary_for_polish = on);
                }),
            ))
    }

    fn toolbar(&self, c: &Ctx, total: Option<usize>) -> Div {
        let p = c.p;
        let count = match total {
            Some(total) => {
                ui::muted12(&p, format!("{} {}", total, c.t(Tr::UnitsItems))).text_size(px(13.)).into_any_element()
            }
            None => placeholder_bar(&p),
        };
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(count)
            .child(div().w(px(240.)).child(ui::text_input(&p, &self.search, Some(Ic::Search))))
    }

    fn word_grid(&self, c: &Ctx, cx: &mut Context<Self>, window: &Window, words: Vec<String>) -> Div {
        let p = c.p;
        if words.is_empty() {
            return div()
                .w_full()
                .py(px(28.))
                .flex()
                .justify_center()
                .child(ui::muted12(&p, c.t(Tr::DictionaryNoWordsYet)).text_size(px(13.)));
        }
        let cols: u16 = if content_width(window) >= 720. { 4 } else { 3 };
        let mut table = div().w_full().grid().grid_cols(cols).gap(px(10.));
        for (i, word) in words.into_iter().enumerate() {
            table = table.child(self.word_chip(&p, cx, i, word));
        }
        table
    }

    fn word_chip(&self, p: &Palette, cx: &mut Context<Self>, index: usize, word: String) -> Div {
        let group = SharedString::from(format!("chip-{index}"));
        div()
            .group(group.clone())
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
                    .child(text(word.clone(), 14., FontWeight::MEDIUM, p.text).whitespace_nowrap().text_ellipsis()),
            )
            .child(self.remove_button(p, cx, index, word, group))
    }

    fn remove_button(
        &self,
        p: &Palette,
        cx: &mut Context<Self>,
        index: usize,
        word: String,
        group: SharedString,
    ) -> Stateful<Div> {
        let st = self.st.clone();
        div()
            .id(SharedString::from(format!("del-{index}")))
            .size(px(24.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .cursor_pointer()
            .opacity(0.)
            .group_hover(group, |s| s.opacity(1.))
            .hover(|s| s.bg(p.seg))
            .on_click(ui::click_cb(cbk(cx, move |this: &mut Self, _, cx| match st.store.remove_word(&word) {
                Ok(()) => this.load(cx),
                Err(e) => tracing::warn!("dictionary: could not remove a word: {e:#}"),
            })))
            .child(ui::icon(Ic::X, 14., p.muted))
    }
}

fn placeholder_bar(p: &Palette) -> AnyElement {
    div().w(px(44.)).h(px(16.)).rounded(px(8.)).bg(p.seg).into_any_element()
}

fn placeholder_grid(p: &Palette, window: &Window) -> Div {
    let cols = word_columns(window);
    let mut grid = div().w_full().grid().grid_cols(cols).gap(px(10.));
    for _ in 0..cols * 2 {
        grid = grid.child(div().h(px(44.)).rounded_full().bg(p.seg));
    }
    grid
}

fn word_columns(window: &Window) -> u16 {
    if content_width(window) >= 720. { 4 } else { 3 }
}
