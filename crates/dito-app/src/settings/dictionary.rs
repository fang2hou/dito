use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use dito_core::config::Source;
use dito_core::i18n::{Lang, Tr};
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
use crate::ui::{self, BtnKind, InputEvent, InputState, cbk, text};

/// The operation behind the failure banner. One at a time: the newest failure replaces the last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Failure {
    Add,
    Delete,
    Import,
    Export,
}

impl Failure {
    fn message(self) -> Tr {
        match self {
            Self::Add => Tr::DictionaryAddFailed,
            Self::Delete => Tr::DictionaryDeleteFailed,
            Self::Import => Tr::DictionaryImportFailed,
            Self::Export => Tr::DictionaryExportFailed,
        }
    }
}

pub struct DictionaryPage {
    st: Arc<AppState>,
    add: Entity<InputState>,
    search: Entity<InputState>,
    query_text: String,
    words: PageQuery<DictionaryWords>,
    loaded: bool,
    placeholder_lang: Lang,
    failure: Option<Failure>,
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
            failure: None,
        }
    }

    fn add_word(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let word = self.add.read(cx).value().to_string();
        let st = self.st.clone();
        cx.spawn_in(window, async move |this, cx| {
            let store = st.store.clone();
            let added = off_main(&st, move || Ok(store.add_word(&word, now())?)).await;
            let _ = this.update_in(cx, |this, window, cx| match added {
                Ok(true) => {
                    this.add.update(cx, |s, cx| s.set_value("", window, cx));
                    this.succeeded(cx);
                    this.load(cx);
                }
                // A blank or repeated word changes nothing, so it neither fails nor clears a banner.
                Ok(false) => {}
                Err(e) => this.failed(Failure::Add, &e, cx),
            });
        })
        .detach();
    }

    fn remove_word(&mut self, word: String, cx: &mut Context<Self>) {
        let st = self.st.clone();
        cx.spawn(async move |this, cx| {
            let store = st.store.clone();
            let removed = off_main(&st, move || Ok(store.remove_word(&word)?)).await;
            let _ = this.update(cx, |this, cx| match removed {
                Ok(()) => {
                    this.succeeded(cx);
                    this.load(cx);
                }
                Err(e) => this.failed(Failure::Delete, &e, cx),
            });
        })
        .detach();
    }

    fn import(&mut self, cx: &mut Context<Self>) {
        let rx =
            cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: None });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, cx| this.import_from(path, cx));
            }
        })
        .detach();
    }

    fn import_from(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let st = self.st.clone();
        cx.spawn(async move |this, cx| {
            let store = st.store.clone();
            let imported = off_main(&st, move || {
                let text = std::fs::read_to_string(&path)?;
                Ok(store.import_words(&text, now())?)
            })
            .await;
            let _ = this.update(cx, |this, cx| {
                match imported {
                    Ok(_) => this.succeeded(cx),
                    Err(e) => this.failed(Failure::Import, &e, cx),
                }
                // A failed import may still have added the lines before the error.
                this.load(cx);
            });
        })
        .detach();
    }

    fn export(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_new_path(&downloads_dir(), Some("dito-dictionary.txt"));
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(path))) = rx.await {
                let _ = this.update(cx, |this, cx| this.export_to(path, cx));
            }
        })
        .detach();
    }

    fn export_to(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let st = self.st.clone();
        cx.spawn(async move |this, cx| {
            let store = st.store.clone();
            let exported = off_main(&st, move || {
                let text = store.export_words()?;
                Ok(std::fs::write(&path, text)?)
            })
            .await;
            let _ = this.update(cx, |this, cx| match exported {
                Ok(()) => this.succeeded(cx),
                Err(e) => this.failed(Failure::Export, &e, cx),
            });
        })
        .detach();
    }

    /// The next successful action clears the banner.
    fn succeeded(&mut self, cx: &mut Context<Self>) {
        if self.failure.take().is_some() {
            cx.notify();
        }
    }

    fn failed(&mut self, failure: Failure, error: &anyhow::Error, cx: &mut Context<Self>) {
        tracing::warn!("dictionary: {failure:?} failed: {error:#}");
        self.failure = Some(failure);
        cx.notify();
    }

    fn dismiss_failure(&mut self, cx: &mut Context<Self>) {
        self.failure = None;
        cx.notify();
    }
}

/// Run store or file work on the blocking pool so it never stalls the UI thread.
async fn off_main<T: Send + 'static>(
    st: &AppState,
    work: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<T> {
    st.rt.spawn_blocking(work).await.map_err(|e| anyhow::anyhow!("blocking task failed: {e}"))?
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
                .children(self.failure_banner(&c, cx))
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

    fn failure_banner(&self, c: &Ctx, cx: &mut Context<Self>) -> Option<Div> {
        let failure = self.failure?;
        Some(ui::dismissible_banner(
            &c.p,
            "dictionary-failure-close",
            c.t(failure.message()),
            c.t(Tr::HudClose),
            cbk(cx, |this: &mut Self, _, cx| this.dismiss_failure(cx)),
        ))
    }

    fn add_card(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = c.p;
        let cfg = self.st.cfg.get();
        let hotwords_unsupported = cfg.asr.source == Source::Local
            && self.st.models.spec(&cfg.asr.local_model).is_some_and(|spec| !spec.format.supports_hotwords());
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
            .on_click(ui::click_cb(cbk(cx, move |this: &mut Self, _, cx| this.remove_word(word.clone(), cx))))
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

/// What the screenshot harness drives and reads: the same handlers the controls call.
#[cfg(feature = "screenshots")]
impl DictionaryPage {
    pub(super) fn probe(&mut self, case: &str, arg: &str, window: &mut Window, cx: &mut Context<Self>) {
        match case {
            "add" => {
                self.add.update(cx, |s, cx| s.set_value(arg, window, cx));
                self.add_word(window, cx);
            }
            "remove" => self.remove_word(arg.to_string(), cx),
            "import" => self.import_from(arg.into(), cx),
            "export" => self.export_to(arg.into(), cx),
            "dismiss" => self.dismiss_failure(cx),
            other => panic!("unknown dictionary probe {other:?}"),
        }
    }

    /// The operation the banner reports, if any.
    pub(super) fn failure_name(&self) -> Option<&'static str> {
        self.failure.map(|f| match f {
            Failure::Add => "add",
            Failure::Delete => "delete",
            Failure::Import => "import",
            Failure::Export => "export",
        })
    }
}
