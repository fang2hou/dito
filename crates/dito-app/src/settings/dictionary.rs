use std::cell::Cell;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use dito_core::config::Source;
use dito_core::i18n::{Lang, Tr, fmt};
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

pub(super) struct ModalChanged;

// Dim both labels a little at the midpoint so the shared baseline stays legible.
const COUNT_FADE_POWER: f32 = 1.4;
const COUNT_SIZE: f32 = 13.;

struct SelectionFades {
    duration: u64,
    active: Cell<bool>,
}

impl SelectionFades {
    fn new() -> Self {
        Self { duration: if ui::Motion::system().is_reduced() { 80 } else { ui::HOVER_MS }, active: Cell::new(false) }
    }

    fn sample(&self, id: ElementId, on: bool) -> f32 {
        let (value, active) = ui::fade_value(&id, f32::from(on), self.duration);
        if active {
            self.active.set(true);
        }
        value
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
    selection: Option<BTreeSet<String>>,
    exiting_count: usize,
    confirming_delete: bool,
    deleting: bool,
    delete_focus: FocusHandle,
    /// The page's own motion scope: its controls' transient state, reset on navigation.
    motion: ui::MotionScope,
}

impl EventEmitter<ModalChanged> for DictionaryPage {}

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
        } else {
            self.selection = None;
            self.confirming_delete = false;
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
            selection: None,
            exiting_count: 0,
            confirming_delete: false,
            deleting: false,
            delete_focus: cx.focus_handle(),
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

    fn toggle_word(&mut self, word: &str, cx: &mut Context<Self>) {
        if self.deleting {
            return;
        }
        let selection = self.selection.get_or_insert_with(BTreeSet::new);
        if !selection.remove(word) {
            selection.insert(word.to_owned());
        } else if selection.is_empty() {
            self.exit_selection(cx);
            return;
        }
        cx.notify();
    }

    fn exit_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(selection) = self.selection.take() {
            self.exiting_count = selection.len().max(1);
        }
        ui::dismiss_tips(&self.motion.handle());
        cx.notify();
    }

    fn ask_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.deleting || self.selection.as_ref().is_none_or(BTreeSet::is_empty) {
            return;
        }
        self.confirming_delete = true;
        self.delete_focus.focus(window, cx);
        ui::dismiss_tips(&self.motion.handle());
        cx.emit(ModalChanged);
    }

    fn close_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.confirming_delete = false;
        window.blur(cx);
        cx.emit(ModalChanged);
    }

    fn confirm_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(selection) = &self.selection else { return };
        if !self.confirming_delete || selection.is_empty() {
            return;
        }
        let words = selection.iter().cloned().collect();
        self.close_delete(window, cx);
        self.remove_words(words, cx);
    }

    fn remove_words(&mut self, words: Vec<String>, cx: &mut Context<Self>) {
        self.deleting = true;
        cx.notify();
        let st = self.st.clone();
        cx.spawn(async move |this, cx| {
            let store = st.store.clone();
            let removed = off_main(&st, move || {
                store.remove_words(&words)?;
                Ok(words)
            })
            .await;
            let _ = this.update(cx, |this, cx| {
                this.deleting = false;
                match removed {
                    Ok(words) => {
                        this.exit_selection(cx);
                        this.succeeded(cx);
                        // Drop deleted terms immediately and discard any snapshot fetched before the deletion.
                        let words: BTreeSet<_> = words.into_iter().collect();
                        this.words.data.matching.retain(|term| !words.contains(term));
                        this.changed(cx);
                    }
                    Err(e) => this.failed(Failure::Delete, &e, cx),
                }
            });
        })
        .detach();
    }

    pub(super) fn overlay(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.confirming_delete {
            return None;
        }
        let count = self.selection.as_ref()?.len().to_string();
        let c = Ctx::new(&self.st, cx);
        let body = text(c.t(Tr::DictionaryDeleteDescription), 14., FontWeight::NORMAL, c.p.text).into_any_element();
        Some(ui::with_motion_scope(self.motion.handle(), || {
            ui::confirm_dialog(
                window,
                &c.p,
                "dictionary-delete-confirmation",
                &self.delete_focus,
                fmt::dictionary_delete_title(c.lang, &count),
                vec![body],
                ui::DialogAnswer { label: c.t(Tr::AsrEditorCancel), on_click: Some(cbk(cx, Self::close_delete)) },
                ui::DialogAnswer { label: c.t(Tr::ModelsDelete), on_click: Some(cbk(cx, Self::confirm_delete)) },
                ui::ConfirmStyle::CompactDestructive,
            )
        }))
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
        let rx = cx.prompt_for_new_path(&downloads_dir(), Some("dito-wordbook.txt"));
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

        let fades = SelectionFades::new();
        let page = super::page(
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
                .child(self.toolbar(&c, total, cx, &fades))
                .child(if self.loaded {
                    self.word_grid(&c, cx, window, words, &fades)
                } else {
                    placeholder_grid(&p, window)
                }),
        );
        if fades.active.get() {
            window.request_animation_frame();
        }
        page
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

    fn toolbar(&self, c: &Ctx, total: Option<usize>, cx: &mut Context<Self>, fades: &SelectionFades) -> Div {
        let p = &c.p;
        let selecting = self.selection.is_some();
        let opacity = fades.sample("dictionary-selection-toolbar".into(), selecting);
        let count = match total {
            Some(total) => ui::muted12(p, format!("{} {}", total, c.t(Tr::UnitsItems)))
                .text_size(px(COUNT_SIZE))
                .opacity((1. - opacity).powf(COUNT_FADE_POWER))
                .into_any_element(),
            None => placeholder_bar(p),
        };
        let mut summary = div().relative().flex_1().h(px(ui::CONTROL_H)).flex().items_center().child(count);
        if selecting || opacity > 0. {
            summary = summary.child(self.selection_toolbar(c, cx, opacity, total.unwrap_or_default()));
        }
        div().w_full().flex().items_center().child(summary).child(div().w(px(240.)).flex_none().child(ui::text_input(
            p,
            &self.search,
            Some(Ic::Search),
        )))
    }

    fn selection_toolbar(&self, c: &Ctx, cx: &mut Context<Self>, opacity: f32, total: usize) -> Div {
        let p = &c.p;
        let selecting = self.selection.is_some();
        let count = self.selection.as_ref().map_or(self.exiting_count, BTreeSet::len).to_string();
        let exit = ElementId::from("dictionary-exit-selection");
        let delete = ElementId::from("dictionary-delete-selected");
        let (exit_button, delete_button) = if !selecting {
            let appearance = || ui::CircleAppearance {
                background: None,
                foreground: p.text,
                border: Some(p.border_strong),
                hover: Some(p.seg),
            };
            (
                ui::circle_control(exit, Ic::X, ui::CircleSize::Regular, appearance()),
                ui::circle_control(delete, Ic::Trash, ui::CircleSize::Regular, appearance()),
            )
        } else if self.deleting {
            (ui::disabled_icon_button(exit, p, Ic::X), ui::disabled_icon_button(delete, p, Ic::Trash))
        } else {
            ui::tip_label(exit.clone(), c.t(Tr::DictionaryExitSelection));
            ui::tip_label(delete.clone(), c.t(Tr::DictionaryDeleteSelected));
            (
                ui::icon_button(
                    exit.clone(),
                    p,
                    Ic::X,
                    false,
                    cbk(cx, |this: &mut Self, _, cx| this.exit_selection(cx)),
                )
                .child(ui::tip_above(p, &exit)),
                ui::icon_button(delete.clone(), p, Ic::Trash, false, cbk(cx, Self::ask_delete))
                    .child(ui::tip_above(p, &delete)),
            )
        };
        div()
            .absolute()
            .top_0()
            .left_0()
            .h(px(ui::CONTROL_H))
            .flex()
            .items_center()
            .gap(px(10.))
            .child(
                ui::muted12(p, fmt::dictionary_selected(c.lang, &count))
                    .text_size(px(COUNT_SIZE))
                    .w(px(selection_count_width(c.lang, total)))
                    .flex_none()
                    .whitespace_nowrap()
                    .opacity(opacity.powf(COUNT_FADE_POWER)),
            )
            .child(exit_button.opacity(opacity))
            .child(delete_button.opacity(opacity))
    }

    fn word_grid(
        &self,
        c: &Ctx,
        cx: &mut Context<Self>,
        window: &Window,
        words: Vec<String>,
        fades: &SelectionFades,
    ) -> Div {
        let p = c.p;
        if words.is_empty() {
            return div()
                .w_full()
                .py(px(28.))
                .flex()
                .justify_center()
                .child(ui::muted12(&p, c.t(Tr::DictionaryNoWordsYet)).text_size(px(13.)));
        }
        div()
            .w_full()
            .grid()
            .grid_cols(word_columns(window))
            .gap(px(10.))
            .children(words.into_iter().map(|word| self.word_chip(&p, cx, word, fades)))
    }

    fn word_chip(&self, p: &Palette, cx: &mut Context<Self>, word: String, fades: &SelectionFades) -> Stateful<Div> {
        let word = SharedString::from(word);
        let chip_id = ElementId::from((ElementId::from("word"), word.clone()));
        let hit_id = ElementId::from((chip_id.clone(), "hit"));
        let ring_id = ElementId::from((chip_id.clone(), "ring"));
        let selecting = self.selection.is_some();
        let selected = self.selection.as_ref().is_some_and(|terms| terms.contains(word.as_ref()));
        let card_hovered = ui::is_hovered(&chip_id);
        let ring_hovered = ui::is_hovered(&ring_id);
        let opacity = fades.sample((chip_id.clone(), "marker-opacity").into(), selecting || card_hovered);
        let hover = fades.sample((chip_id.clone(), "ring-fill").into(), !selected && !self.deleting && ring_hovered);
        let chosen = fades.sample((chip_id.clone(), "chosen").into(), selected);

        let toggle = ui::click_cb(cbk(cx, {
            let word = word.clone();
            move |this: &mut Self, _, cx| this.toggle_word(&word, cx)
        }));
        let hover_fill = Hsla { a: hover, ..p.seg };
        let ring = div()
            .id(ring_id.clone())
            .size(px(16.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border_1()
            .border_color(Hsla { a: p.muted.a * (1. - chosen), ..p.muted })
            .bg(ui::mix_rgb(hover_fill, p.primary, chosen))
            .opacity(opacity)
            .on_hover(ui::hover_tracker(ring_id))
            .when(chosen > 0., |el| el.child(div().opacity(chosen).child(ui::icon(Ic::Check, 12., p.on_primary))));
        let marker = div().id(hit_id).size(px(24.)).flex_none().flex().items_center().justify_center().child(ring);
        let mut chip = div()
            .id(chip_id.clone())
            .on_hover(ui::hover_tracker(chip_id))
            .min_w(px(0.))
            .h(px(44.))
            .pl(px(16.))
            .pr(px(10.))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.))
            .rounded_full()
            .bg(ui::mix_rgb(p.card, p.accent_soft, chosen))
            .border_1()
            .border_color(ui::mix_rgb(p.border, p.accent, chosen))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .child(text(word, 14., FontWeight::MEDIUM, p.text).whitespace_nowrap().text_ellipsis()),
            );
        if selecting && !self.deleting {
            chip = chip.cursor_pointer().on_click(toggle);
        } else if !selecting && !self.deleting {
            return chip.child(marker.cursor_pointer().on_click(toggle));
        }
        chip.child(marker)
    }
}

/// Fit the widest digit in the localized label, with at least three places.
fn selection_count_width(lang: Lang, total: usize) -> f32 {
    let digits = total.max(100).ilog10() as usize + 1;
    ('0'..='9')
        .map(|digit| {
            let sample = digit.to_string().repeat(digits);
            ui::text_width(&fmt::dictionary_selected(lang, &sample), COUNT_SIZE, FontWeight::NORMAL)
        })
        .fold(0., f32::max)
        .ceil()
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
            "remove" => self.remove_words(vec![arg.to_owned()], cx),
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
