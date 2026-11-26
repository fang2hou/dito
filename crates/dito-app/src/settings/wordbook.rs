//! The Wordbook page: words with optional spoken forms and a language scope, added in one form, listed as chips that
//! open the Edit word dialog, and searched by term or spoken form (`design:settings/wordbook/spoken-forms`).

use std::cell::Cell;
use std::collections::BTreeSet;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use dito_core::config::Source;
use dito_core::i18n::{Lang, Tr, fmt};
use dito_core::store::{WordbookEntry, parse_spoken_list};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::downloads_dir::downloads_dir;
use super::page_layout::page_scroll;
use super::page_query::{PageQuery, QueryPage};
use super::{Ctx, content_width};
use crate::assets::Ic;
use crate::services::{self, WordbookWords};
use crate::state::AppState;
use crate::theme::Palette;
use crate::timefmt::now;
use crate::ui::{self, BtnKind, InputEvent, InputState, cbk, cbk_idx, text};

/// One at a time: the newest failure replaces the last.
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
            Self::Add => Tr::WordbookAddFailed,
            Self::Delete => Tr::WordbookDeleteFailed,
            Self::Import => Tr::WordbookImportFailed,
            Self::Export => Tr::WordbookExportFailed,
        }
    }
}

mod find_words;

pub(super) struct ModalChanged;

// Dim both labels a little at the midpoint so the shared baseline stays legible.
const COUNT_FADE_POWER: f32 = 1.4;
const COUNT_SIZE: f32 = 13.;

const CHIP_H: f32 = 48.;
const GRID_GAP: f32 = 10.;
const ROW_PITCH: f32 = CHIP_H + GRID_GAP;
/// Rows built beyond the window on each side. The grid's top is known from the frame before, and nothing above the
/// grid grows or shrinks by this much in one frame.
const OVERSCAN_ROWS: usize = 4;
/// Two, so that a chip the change moved out of the built rows while hovered is sampled once more after its hover
/// is dropped.
const RESYNC_RENDERS: u8 = 2;

const ADD_LANG: &str = "wordbook-add-lang";
const EDIT_DIALOG: &str = "wordbook-edit";
const EDIT_LANG: &str = "wordbook-edit-lang";
/// The languages a spoken form can be scoped to, every language first.
const SCOPES: [Option<Lang>; 4] = [None, Some(Lang::Ja), Some(Lang::En), Some(Lang::Zh)];

/// A term and where the selection marker of its chip was painted, for the screenshot harness.
#[cfg(feature = "screenshots")]
type MarkerHit = (SharedString, Bounds<Pixels>);

/// What a grid that builds only the rows near the window remembers between renders.
struct GridState {
    /// Where the grid's top sits in the window with the page scrolled to its start, as last painted. `None` until
    /// a grid with unbuilt rows has been painted on this visit.
    top: Rc<Cell<Option<f32>>>,
    /// Renders left that sample the fades of every chip, built or not.
    resync: u8,
    /// The words the last two renders built.
    built: [Range<usize>; 2],
    /// The selection marker of the term the screenshot harness asked for, where it was last painted.
    #[cfg(feature = "screenshots")]
    marker: Rc<Cell<Option<MarkerHit>>>,
    #[cfg(feature = "screenshots")]
    watched: Rc<std::cell::RefCell<Option<SharedString>>>,
}

impl GridState {
    fn new() -> Self {
        Self {
            top: Rc::new(Cell::new(None)),
            resync: RESYNC_RENDERS,
            built: [0..0, 0..0],
            #[cfg(feature = "screenshots")]
            marker: Rc::new(Cell::new(None)),
            #[cfg(feature = "screenshots")]
            watched: Rc::new(std::cell::RefCell::new(None)),
        }
    }
}

struct GridRows {
    columns: u16,
    total: usize,
    built: Range<usize>,
}

impl GridRows {
    fn is_whole(&self) -> bool {
        self.built == (0..self.total)
    }

    fn words(&self, count: usize) -> Range<usize> {
        let columns = usize::from(self.columns);
        self.built.start * columns..(self.built.end * columns).min(count)
    }
}

struct ChipFades {
    marker: f32,
    ring: f32,
    chosen: f32,
}

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

pub struct WordbookPage {
    st: Arc<AppState>,
    add: Entity<InputState>,
    add_spoken: Entity<InputState>,
    /// The language scope of the word being added; `None` for every language.
    add_lang: Option<Lang>,
    /// Whether the language menu of the add form is open.
    add_menu_open: bool,
    search: Entity<InputState>,
    query_text: String,
    words: PageQuery<WordbookWords>,
    loaded: bool,
    placeholder_lang: Lang,
    failure: Option<Failure>,
    selection: Option<BTreeSet<String>>,
    exiting_count: usize,
    confirming_delete: bool,
    deleting: bool,
    delete_focus: FocusHandle,
    find: find_words::FindWords,
    edit_term: Entity<InputState>,
    edit_spoken: Entity<InputState>,
    /// The Edit word dialog, while it is open.
    edit: Option<EditWord>,
    /// The number given to the dialog opened last.
    edit_rev: u64,
    edit_focus: FocusHandle,
    motion: ui::MotionScope,
    scroll: ScrollHandle,
    grid: GridState,
}

impl EventEmitter<ModalChanged> for WordbookPage {}

/// The Edit word dialog while it is open: the entry it edits and what is not one of its inputs.
struct EditWord {
    /// Tells this opening of the dialog from the next: a save that finishes after it was closed, or reopened for
    /// another entry, still wrote to the wordbook and leaves the dialog alone.
    rev: u64,
    /// The term as stored, which the save addresses.
    original: String,
    lang: Option<Lang>,
    /// Whether the language menu is open.
    menu_open: bool,
    /// Whether a save is running, so Enter and Save wait for it.
    saving: bool,
    /// Why the last save wrote nothing, until the term is edited again.
    error: Option<Tr>,
}

impl QueryPage for WordbookPage {
    type Data = WordbookWords;

    fn page_query(&mut self) -> &mut PageQuery<WordbookWords> {
        &mut self.words
    }

    fn fetch(&self) -> impl Future<Output = anyhow::Result<WordbookWords>> + 'static {
        services::wordbook_words(self.st.clone(), self.query_text.clone())
    }

    fn applied(&mut self, cx: &mut Context<Self>) {
        self.grid.resync = RESYNC_RENDERS;
        if !self.loaded {
            self.loaded = true;
            cx.notify();
        }
    }
}

impl WordbookPage {
    /// Leaving drops any result still in flight, and whatever dialog was open.
    pub fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        self.words.set_active(active);
        if active {
            // Each visit starts at the top.
            self.scroll = ScrollHandle::new();
            self.grid.top.set(None);
            self.load(cx);
        } else {
            self.edit = None;
            self.add_menu_open = false;
            self.selection = None;
            self.grid.resync = RESYNC_RENDERS;
            self.confirming_delete = false;
        }
    }

    pub fn new(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        services::watch_config(&st, cx);
        let lang = st.cfg.get().general.lang();
        let add = cx.new(|cx| InputState::new(window, cx).placeholder(lang.tr(Tr::WordbookExampleHint)));
        let add_spoken = cx.new(|cx| InputState::new(window, cx).placeholder(lang.tr(Tr::WordbookSpokenExample)));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(lang.tr(Tr::WordbookSearchWords)));
        let edit_term = cx.new(|cx| InputState::new(window, cx));
        let edit_spoken = cx.new(|cx| InputState::new(window, cx));
        for input in [&add, &add_spoken, &search, &edit_term, &edit_spoken] {
            ui::notify_on_focus(input, window, cx);
        }
        for input in [&add, &add_spoken] {
            cx.subscribe_in(input, window, |this: &mut Self, _, ev: &InputEvent, window, cx| {
                if matches!(ev, InputEvent::PressEnter { .. }) {
                    this.add_word(window, cx);
                }
            })
            .detach();
        }
        cx.subscribe_in(&edit_term, window, |this: &mut Self, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::PressEnter { .. } => this.save_edit(window, cx),
            // Save follows the term, and a complaint about the term goes with the term.
            InputEvent::Change => {
                if let Some(edit) = &mut this.edit {
                    edit.error = None;
                }
                cx.notify();
            }
            _ => {}
        })
        .detach();
        cx.subscribe_in(&edit_spoken, window, |this: &mut Self, _, ev: &InputEvent, window, cx| {
            if matches!(ev, InputEvent::PressEnter { .. }) {
                this.save_edit(window, cx);
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
            add_spoken,
            add_lang: None,
            add_menu_open: false,
            search,
            query_text: String::new(),
            words: PageQuery::new("wordbook", WordbookWords::default()),
            loaded: false,
            placeholder_lang: lang,
            failure: None,
            selection: None,
            exiting_count: 0,
            confirming_delete: false,
            deleting: false,
            delete_focus: cx.focus_handle(),
            find: find_words::FindWords::new(window, cx),
            edit_term,
            edit_spoken,
            edit: None,
            edit_rev: 0,
            edit_focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            grid: GridState::new(),
        }
    }

    fn add_word(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let word = self.add.read(cx).value().to_string();
        let spoken = parse_spoken_list(&self.add_spoken.read(cx).value());
        let lang = self.add_lang;
        let st = self.st.clone();
        cx.spawn_in(window, async move |this, cx| {
            let store = st.store.clone();
            let added = off_main(&st, move || Ok(store.add_word_entry(&word, &spoken, lang, now())?)).await;
            let _ = this.update_in(cx, |this, window, cx| match added {
                Ok(true) => {
                    this.add.update(cx, |s, cx| s.set_value("", window, cx));
                    this.add_spoken.update(cx, |s, cx| s.set_value("", window, cx));
                    this.add_lang = None;
                    this.add_menu_open = false;
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

    /// Opens the entry in the Edit word dialog. Nothing is written until it is saved.
    fn open_edit(&mut self, entry: &WordbookEntry, window: &mut Window, cx: &mut Context<Self>) {
        self.edit_term.update(cx, |s, cx| s.set_value(entry.term.clone(), window, cx));
        self.edit_spoken.update(cx, |s, cx| s.set_value(entry.spoken.join(", "), window, cx));
        self.edit_rev += 1;
        self.edit = Some(EditWord {
            rev: self.edit_rev,
            original: entry.term.clone(),
            lang: entry.lang,
            menu_open: false,
            saving: false,
            error: None,
        });
        self.edit_term.update(cx, |s, cx| s.focus(window, cx));
        ui::dismiss_tips(&self.motion.handle());
        cx.emit(ModalChanged);
    }

    /// Writes the dialog's fields. A term another entry already has keeps the dialog open and says why.
    fn save_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let term = self.edit_term.read(cx).value().to_string();
        if term.trim().is_empty() {
            return;
        }
        let Some(edit) = self.edit.as_mut().filter(|edit| !edit.saving) else { return };
        let (rev, original, lang) = (edit.rev, edit.original.clone(), edit.lang);
        edit.saving = true;
        let spoken = parse_spoken_list(&self.edit_spoken.read(cx).value());
        let st = self.st.clone();
        cx.spawn_in(window, async move |this, cx| {
            let store = st.store.clone();
            let saved =
                off_main(&st, move || Ok(store.update_word_entry(&original, &term, &spoken, lang, now())?)).await;
            let _ = this.update_in(cx, |this, window, cx| {
                let open = this.edit.as_mut().is_some_and(|edit| {
                    edit.saving = false;
                    edit.rev == rev
                });
                if matches!(saved, Ok(true)) {
                    // The wordbook was written, whatever became of the dialog while the save ran.
                    this.succeeded(cx);
                    this.load(cx);
                    if open {
                        this.close_edit(window, cx);
                    }
                } else if open {
                    // Only the dialog that is still open for this save hears why it wrote nothing.
                    let failure = match &saved {
                        Err(e) => {
                            tracing::warn!("wordbook: edit failed: {e:#}");
                            Tr::WordbookUpdateFailed
                        }
                        Ok(_) => Tr::WordbookDuplicateTerm,
                    };
                    this.edit_failed(failure, cx);
                }
            });
        })
        .detach();
    }

    /// Enter on the dialog itself saves, which is where the keyboard is after a click on its padding or a pick from
    /// the language menu. Inside an input, the input's own Enter saves.
    fn enter_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.edit.as_ref().is_some_and(|edit| !edit.menu_open) {
            self.save_edit(window, cx);
        }
    }

    fn close_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.edit = None;
        window.blur(cx);
        cx.emit(ModalChanged);
    }

    /// Escape closes the language menu first, then the dialog.
    fn escape_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.edit.as_mut().is_some_and(|edit| std::mem::take(&mut edit.menu_open)) {
            cx.notify();
            return;
        }
        self.close_edit(window, cx);
    }

    fn edit_failed(&mut self, error: Tr, cx: &mut Context<Self>) {
        if let Some(edit) = &mut self.edit {
            edit.error = Some(error);
            cx.notify();
        }
    }

    fn toggle_word(&mut self, word: &str, cx: &mut Context<Self>) {
        if self.deleting {
            return;
        }
        if self.selection.is_none() {
            self.grid.resync = RESYNC_RENDERS;
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
            self.grid.resync = RESYNC_RENDERS;
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
                        let removed: BTreeSet<_> = words.into_iter().collect();
                        this.words.data.matching.retain(|entry| !removed.contains(&entry.term));
                        this.grid.resync = RESYNC_RENDERS;
                        this.changed(cx);
                    }
                    Err(e) => this.failed(Failure::Delete, &e, cx),
                }
            });
        })
        .detach();
    }

    pub(super) fn overlay(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.find.is_open() {
            return Some(self.find_overlay(window, cx));
        }
        if self.edit.is_some() {
            return Some(self.edit_overlay(window, cx));
        }
        if !self.confirming_delete {
            return None;
        }
        let count = self.selection.as_ref()?.len().to_string();
        let c = Ctx::new(&self.st, cx);
        let body = text(c.t(Tr::WordbookDeleteDescription), 14., FontWeight::NORMAL, c.p.text).into_any_element();
        Some(ui::with_motion_scope(self.motion.handle(), || {
            ui::confirm_dialog(
                window,
                &c.p,
                "wordbook-delete-confirmation",
                &self.delete_focus,
                fmt::wordbook_delete_title(c.lang, &count),
                vec![body],
                ui::DialogAnswer { label: c.t(Tr::AsrEditorCancel), on_click: Some(cbk(cx, Self::close_delete)) },
                Some(ui::DialogAnswer { label: c.t(Tr::ModelsDelete), on_click: Some(cbk(cx, Self::confirm_delete)) }),
                ui::ConfirmStyle::CompactDestructive,
            )
        }))
    }

    /// The Edit word dialog: the term, how it is said, and the language that is listened for.
    fn edit_overlay(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        let edit = self.edit.as_ref().expect("the edit dialog is open");
        let blank = self.edit_term.read(cx).value().trim().is_empty();
        let language = ui::select(
            EDIT_LANG,
            &p,
            &scope_options(c.lang),
            scope_index(edit.lang),
            edit.menu_open,
            Some(180.),
            cbk(cx, |this: &mut Self, _, cx| {
                if let Some(edit) = &mut this.edit {
                    edit.menu_open = !edit.menu_open;
                }
                cx.notify();
            }),
            cbk_idx(cx, |this: &mut Self, index, _, cx| {
                if let Some(edit) = &mut this.edit {
                    edit.lang = SCOPES[index];
                }
                cx.notify();
            }),
        );
        let mut content = vec![
            text(c.t(Tr::WordbookEditDesc), 13., FontWeight::NORMAL, p.muted).line_height(px(20.)).into_any_element(),
            ui::field(&p, c.t(Tr::WordbookWord), ui::text_input(&p, &self.edit_term, None)).into_any_element(),
            described(
                &p,
                c.t(Tr::WordbookSpokenForms),
                ui::text_input(&p, &self.edit_spoken, None),
                c.t(Tr::WordbookEditSpokenHint),
            )
            .into_any_element(),
            described(&p, c.t(Tr::WordbookTargetLanguage), language, c.t(Tr::WordbookEditLangHint)).into_any_element(),
        ];
        content.extend(edit.error.map(|error| ui::banner(&p, c.t(error), None).into_any_element()));
        let actions = vec![
            ui::dialog_button(
                &p,
                ElementId::from((ElementId::from(EDIT_DIALOG), "cancel")),
                BtnKind::Secondary,
                None,
                ui::DialogAnswer { label: c.t(Tr::AsrEditorCancel), on_click: Some(cbk(cx, Self::close_edit)) },
            )
            .into_any_element(),
            ui::dialog_button(
                &p,
                ElementId::from((ElementId::from(EDIT_DIALOG), "save")),
                BtnKind::Primary,
                None,
                ui::DialogAnswer {
                    label: c.t(Tr::WordbookSave),
                    on_click: (!blank && !edit.saving).then(|| cbk(cx, Self::save_edit)),
                },
            )
            .into_any_element(),
        ];
        let dialog = ui::dialog_with_actions(
            window,
            &p,
            EDIT_DIALOG,
            &self.edit_focus,
            text(c.t(Tr::WordbookEditTitle), 20., FontWeight::BOLD, p.text).into_any_element(),
            content,
            actions,
            Some(cbk(cx, Self::escape_edit)),
            Some(cbk(cx, Self::enter_edit)),
        );
        ui::render_with_scope(self.motion.handle(), || dialog)
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
            let _ = this.update(cx, |this, cx| match imported {
                Ok(_) => {
                    this.succeeded(cx);
                    this.load(cx);
                }
                Err(e) => this.failed(Failure::Import, &e, cx),
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

    fn succeeded(&mut self, cx: &mut Context<Self>) {
        if self.failure.take().is_some() {
            cx.notify();
        }
    }

    fn failed(&mut self, failure: Failure, error: &anyhow::Error, cx: &mut Context<Self>) {
        tracing::warn!("wordbook: {failure:?} failed: {error:#}");
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

impl WordbookPage {
    pub(super) fn reset_motion(&mut self) {
        self.motion.reset();
        self.grid.resync = RESYNC_RENDERS;
    }

    #[cfg(feature = "screenshots")]
    pub(super) fn motion_scope(&self) -> ui::ScopeHandle {
        self.motion.handle()
    }
}

impl Render for WordbookPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Own scope: after a page switch the controls mount statically instead of sliding to values remembered from an
        // earlier visit.
        ui::render_with_scope(self.motion.handle(), || self.view(window, cx))
    }
}

impl WordbookPage {
    fn view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        if self.placeholder_lang != c.lang {
            self.placeholder_lang = c.lang;
            self.add.update(cx, |s, cx| s.set_placeholder(c.lang.tr(Tr::WordbookExampleHint), window, cx));
            self.add_spoken.update(cx, |s, cx| s.set_placeholder(c.lang.tr(Tr::WordbookSpokenExample), window, cx));
            self.search.update(cx, |s, cx| s.set_placeholder(c.lang.tr(Tr::WordbookSearchWords), window, cx));
        }
        let total = self.loaded.then_some(self.words.data.total);

        let fades = SelectionFades::new();
        let rows = self.grid_rows(window);
        self.sample_unbuilt_fades(&rows, &fades);
        let page = page_scroll(
            p,
            "wordbook",
            c.t(Tr::NavWordbook),
            Some(self.actions(&c, cx)),
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(20.))
                .child(div().child(text(c.t(Tr::WordbookWordsPassedToAsr), 14., FontWeight::NORMAL, p.muted)))
                .children(self.failure_banner(&c, cx))
                .child(self.add_card(&c, cx))
                .child(self.polish_option(&c))
                .child(self.toolbar(&c, total, cx, &fades))
                .child(if self.loaded { self.word_grid(&c, cx, &rows, &fades) } else { placeholder_grid(&p, window) }),
            Some(&self.scroll),
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
                "find-words",
                &p,
                &c.t(Tr::WordbookFindWords),
                BtnKind::Secondary,
                Some(Ic::BookSearch),
                cbk(cx, Self::open_find),
            ))
            .child(ui::button(
                "import",
                &p,
                &c.t(Tr::WordbookImport),
                BtnKind::Secondary,
                Some(Ic::Upload),
                cbk(cx, |this: &mut Self, _, cx| this.import(cx)),
            ))
            .child(ui::button(
                "export",
                &p,
                &c.t(Tr::WordbookExport),
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
            "wordbook-failure-close",
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
        let language = ui::select(
            ADD_LANG,
            &p,
            &scope_options(c.lang),
            scope_index(self.add_lang),
            self.add_menu_open,
            Some(132.),
            cbk(cx, |this: &mut Self, _, cx| {
                this.add_menu_open = !this.add_menu_open;
                cx.notify();
            }),
            cbk_idx(cx, |this: &mut Self, index, _, cx| {
                this.add_lang = SCOPES[index];
                cx.notify();
            }),
        );
        let field = |label, control| div().flex_1().min_w(px(0.)).child(ui::field(&p, label, control));
        let mut add_card = ui::card(&p)
            .bg(p.sub)
            .px(px(26.))
            .py(px(18.))
            .gap(px(12.))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_end()
                    .gap(px(12.))
                    .child(field(c.t(Tr::WordbookWord), ui::text_input(&p, &self.add, None)))
                    .child(ui::button(
                        "add-word",
                        &p,
                        &c.t(Tr::WordbookAdd),
                        BtnKind::Primary,
                        Some(Ic::Plus),
                        cbk(cx, |this: &mut Self, w, cx| this.add_word(w, cx)),
                    )),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_end()
                    .gap(px(12.))
                    .child(field(c.t(Tr::WordbookSpokenForms), ui::text_input(&p, &self.add_spoken, None)))
                    .child(div().flex_none().child(ui::field(&p, c.t(Tr::WordbookTargetLanguage), language))),
            )
            .child(ui::muted12(&p, c.t(Tr::WordbookSpokenHint)).line_height(px(18.)));
        if hotwords_unsupported {
            add_card = add_card.child(
                div().w_full().flex().items_start().gap(px(10.)).child(ui::icon(Ic::Alert, 16., p.accent_text)).child(
                    div().flex_1().min_w(px(0.)).child(text(
                        c.t(Tr::WordbookNoHotwordSupport),
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
                    .child(text(c.t(Tr::WordbookSameSpelling), 14., FontWeight::MEDIUM, p.text))
                    .child(ui::muted12(&p, c.t(Tr::WordbookWordsSentToPolish))),
            )
            .child(ui::switch(
                "wordbook-polish",
                &p,
                cfg.wordbook_for_polish,
                Rc::new(move |on, _, _| {
                    services::save_config(&st, |c| c.wordbook_for_polish = on);
                }),
            ))
    }

    fn toolbar(&self, c: &Ctx, total: Option<usize>, cx: &mut Context<Self>, fades: &SelectionFades) -> Div {
        let p = &c.p;
        let selecting = self.selection.is_some();
        let opacity = fades.sample("wordbook-selection-toolbar".into(), selecting);
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
        let exit = ElementId::from("wordbook-exit-selection");
        let delete = ElementId::from("wordbook-delete-selected");
        let (exit_button, delete_button) = if !selecting {
            let appearance = || ui::CircleAppearance {
                background: Some(p.input_bg),
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
            ui::tip_label(exit.clone(), c.t(Tr::WordbookExitSelection));
            ui::tip_label(delete.clone(), c.t(Tr::WordbookDeleteSelected));
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
                ui::muted12(p, fmt::wordbook_selected(c.lang, &count))
                    .text_size(px(COUNT_SIZE))
                    .w(px(selection_count_width(c.lang, total)))
                    .flex_none()
                    .whitespace_nowrap()
                    .opacity(opacity.powf(COUNT_FADE_POWER)),
            )
            .child(exit_button.opacity(opacity))
            .child(delete_button.opacity(opacity))
    }

    fn grid_rows(&mut self, window: &Window) -> GridRows {
        let columns = word_columns(window);
        let total = self.words.data.matching.len().div_ceil(usize::from(columns));
        let scrolled = -f32::from(self.scroll.offset().y);
        let built = rows_to_build(self.grid.top.get(), scrolled, f32::from(window.viewport_size().height), total);
        let rows = GridRows { columns, total, built };
        if rows.is_whole() {
            self.grid.top.set(None);
        }
        rows
    }

    /// Chips outside the built rows keep the fades they would have if every chip were sampled on every render.
    /// That holds as long as a chip is sampled on each render where its targets can differ from the render before:
    /// its hover can only change within two renders of being built, and everything else the targets depend on
    /// (the word list, the selection, the motion scope) asks for a resync when it changes.
    fn sample_unbuilt_fades(&mut self, rows: &GridRows, fades: &SelectionFades) {
        let words = &self.words.data.matching;
        let built = rows.words(words.len());
        let [last, before] = std::mem::replace(&mut self.grid.built, [0..0, 0..0]);
        let stale = if self.grid.resync > 0 {
            self.grid.resync -= 1;
            [0..words.len(), 0..0]
        } else {
            [last.clone(), before]
        };
        for index in stale.into_iter().flatten().filter(|index| !built.contains(index)) {
            let Some(entry) = words.get(index) else { continue };
            let chip_id = chip_id(entry.term.as_str());
            let ring_id = ElementId::from((chip_id.clone(), "ring"));
            // Nothing tells a chip that is not painted that the pointer left: its hover is dropped after this
            // build, and the next render has to come to fade it.
            if ui::is_hovered(&chip_id) || ui::is_hovered(&ring_id) {
                fades.active.set(true);
            }
            self.chip_fades(&chip_id, &ring_id, &entry.term, fades);
        }
        self.grid.built = [built, last];
    }

    fn chip_fades(&self, chip_id: &ElementId, ring_id: &ElementId, term: &str, fades: &SelectionFades) -> ChipFades {
        let selecting = self.selection.is_some();
        let selected = self.selection.as_ref().is_some_and(|terms| terms.contains(term));
        let card_hovered = ui::is_hovered(chip_id);
        let ring_hovered = ui::is_hovered(ring_id);
        ChipFades {
            marker: fades.sample((chip_id.clone(), "marker-opacity").into(), selecting || card_hovered),
            ring: fades.sample((chip_id.clone(), "ring-fill").into(), !selected && !self.deleting && ring_hovered),
            chosen: fades.sample((chip_id.clone(), "chosen").into(), selected),
        }
    }

    fn word_grid(&self, c: &Ctx, cx: &mut Context<Self>, rows: &GridRows, fades: &SelectionFades) -> Div {
        let p = c.p;
        let words = &self.words.data.matching;
        if words.is_empty() {
            return div()
                .w_full()
                .py(px(28.))
                .flex()
                .justify_center()
                .child(ui::muted12(&p, c.t(Tr::WordbookNoWordsYet)).text_size(px(13.)));
        }
        let grid = div()
            .w_full()
            .grid()
            .grid_cols(rows.columns)
            .gap(px(GRID_GAP))
            .children(words[rows.words(words.len())].iter().map(|entry| self.word_chip(c, cx, entry, fades)));
        if rows.is_whole() {
            return grid;
        }
        let spacer = |rows: usize| div().flex_none().h(px(rows as f32 * ROW_PITCH));
        div()
            .w_full()
            .flex()
            .flex_col()
            .child(spacer(rows.built.start))
            .child(grid)
            .child(spacer(rows.total - rows.built.end))
            .child(self.grid_probe(rows))
    }

    /// Records where the grid was painted for the next render, and asks for that render at once if this one left
    /// out a row the window shows.
    fn grid_probe(&self, rows: &GridRows) -> Canvas<()> {
        let (top, scroll) = (self.grid.top.clone(), self.scroll.clone());
        let (built, total) = (rows.built.clone(), rows.total);
        canvas(
            move |bounds, window, _| {
                debug_assert!(
                    (f32::from(bounds.size.height) - (total as f32 * ROW_PITCH - GRID_GAP)).abs() < 0.5,
                    "the wordbook grid rows are not {CHIP_H} pt chips {GRID_GAP} pt apart"
                );
                let painted_top = f32::from(bounds.top());
                top.set(Some(painted_top - f32::from(scroll.offset().y)));
                let shown = rows_in_window(painted_top, f32::from(window.viewport_size().height), total);
                if shown.start < built.start || shown.end > built.end {
                    window.request_animation_frame();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
    }

    fn word_chip(
        &self,
        c: &Ctx,
        cx: &mut Context<Self>,
        entry: &WordbookEntry,
        fades: &SelectionFades,
    ) -> Stateful<Div> {
        let p = &c.p;
        let term = SharedString::new(entry.term.as_str());
        let chip_id = chip_id(&entry.term);
        let hit_id = ElementId::from((chip_id.clone(), "hit"));
        let ring_id = ElementId::from((chip_id.clone(), "ring"));
        let selecting = self.selection.is_some();
        let ChipFades { marker: opacity, ring: hover, chosen } =
            self.chip_fades(&chip_id, &ring_id, &entry.term, fades);

        let toggle = ui::click_cb(cbk(cx, {
            let term = term.clone();
            move |this: &mut Self, _, cx| this.toggle_word(&term, cx)
        }));
        let ring = ui::selection_ring(p, ring_id, hover, chosen).opacity(opacity);
        let marker = div().id(hit_id).size(px(24.)).flex_none().flex().items_center().justify_center().child(ring);
        #[cfg(feature = "screenshots")]
        let marker = marker.child(self.marker_probe(&entry.term));
        let lang = entry.lang.map(|lang| SharedString::new_static(lang.name(c.lang)));
        let spoken = (!entry.spoken.is_empty()).then(|| SharedString::from(entry.spoken.join(", ")));
        let mut chip = ui::wordbook_chip_surface(p, chip_id, term.clone(), lang, spoken, chosen);
        if selecting && !self.deleting {
            chip = chip.cursor_pointer().on_click(toggle);
        } else if !selecting && !self.deleting {
            // The chip body opens the Edit word dialog. The marker over it selects instead, and keeps the click
            // from reaching the chip.
            let edit = ui::click_cb(cbk(cx, {
                let term = term.clone();
                move |this: &mut Self, window, cx| {
                    let entry = this.words.data.matching.iter().find(|entry| entry.term.as_str() == term.as_ref());
                    if let Some(entry) = entry.cloned() {
                        this.open_edit(&entry, window, cx);
                    }
                }
            }));
            let select = move |event: &ClickEvent, window: &mut Window, cx: &mut App| {
                cx.stop_propagation();
                toggle(event, window, cx);
            };
            return chip.cursor_pointer().on_click(edit).child(marker.cursor_pointer().on_click(select));
        }
        chip.child(marker)
    }
}

/// The name of a language scope in the display language.
fn scope_label(scope: Option<Lang>, ui: Lang) -> &'static str {
    match scope {
        None => ui.tr(Tr::WordbookLangAll),
        Some(lang) => lang.name(ui),
    }
}

fn scope_options(ui: Lang) -> Vec<SharedString> {
    SCOPES.iter().map(|scope| SharedString::new_static(scope_label(*scope, ui))).collect()
}

fn scope_index(scope: Option<Lang>) -> usize {
    SCOPES.iter().position(|candidate| *candidate == scope).unwrap_or(0)
}

/// A labeled control of the Edit word dialog, with one line of help under it.
fn described(p: &Palette, label: SharedString, control: impl IntoElement, hint: SharedString) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(ui::field(p, label, control))
        .child(ui::muted12(p, hint).line_height(px(18.)))
}

fn chip_id(term: &str) -> ElementId {
    ElementId::from((ElementId::from("word"), SharedString::new(term)))
}

/// The rows of a grid with its top at `top` in the window that a window `window_h` tall can show.
fn rows_in_window(top: f32, window_h: f32, total: usize) -> Range<usize> {
    let row_at = |y: f32| (y / ROW_PITCH).floor().max(0.) as usize;
    row_at(-top).min(total)..(row_at(window_h - top) + 1).min(total)
}

/// The rows to build. `top` is where the grid's top would sit in the window with the page scrolled to its start;
/// without it the grid can start anywhere at or below the top of the page. `scrolled` is the offset as read
/// before layout clamps it.
fn rows_to_build(top: Option<f32>, scrolled: f32, window_h: f32, total: usize) -> Range<usize> {
    let window_rows = (window_h / ROW_PITCH).ceil() as usize;
    if total <= window_rows + 2 * OVERSCAN_ROWS {
        return 0..total;
    }
    let scrolled = scrolled.max(0.);
    let shown = match top {
        Some(top) => rows_in_window(top - scrolled, window_h, total),
        None => 0..rows_in_window(-scrolled, window_h, total).end,
    };
    // An offset past the end is clamped to it by layout, which shows the last rows.
    let first = shown.start.saturating_sub(OVERSCAN_ROWS).min(total - window_rows - OVERSCAN_ROWS);
    first..(shown.end + OVERSCAN_ROWS).min(total)
}

/// Fit the widest digit in the localized label, with at least three places.
fn selection_count_width(lang: Lang, total: usize) -> f32 {
    let digits = total.max(100).ilog10() as usize + 1;
    ('0'..='9')
        .map(|digit| {
            let sample = digit.to_string().repeat(digits);
            ui::text_width(&fmt::wordbook_selected(lang, &sample), COUNT_SIZE, FontWeight::NORMAL)
        })
        .fold(0., f32::max)
        .ceil()
}

fn placeholder_bar(p: &Palette) -> AnyElement {
    div().w(px(44.)).h(px(16.)).rounded(px(8.)).bg(p.seg).into_any_element()
}

fn placeholder_grid(p: &Palette, window: &Window) -> Div {
    let cols = word_columns(window);
    let mut grid = div().w_full().grid().grid_cols(cols).gap(px(GRID_GAP));
    for _ in 0..cols * 2 {
        grid = grid.child(div().h(px(CHIP_H)).rounded_full().bg(p.seg));
    }
    grid
}

fn word_columns(window: &Window) -> u16 {
    if content_width(window) >= 720. { 4 } else { 3 }
}

/// The same handlers the controls call.
#[cfg(feature = "screenshots")]
impl WordbookPage {
    /// Where the selection marker of the term's chip is painted next; read it with [`Self::marker`](Self::marker)
    /// once a frame has run.
    pub(super) fn watch_marker(&mut self, term: &str, cx: &mut Context<Self>) {
        *self.grid.watched.borrow_mut() = Some(SharedString::new(term));
        cx.notify();
    }

    pub(super) fn marker(&self) -> Option<Bounds<Pixels>> {
        self.grid.marker.take().map(|(_, bounds)| bounds)
    }

    fn marker_probe(&self, term: &str) -> Canvas<()> {
        let marker = self.grid.marker.clone();
        let watched = self.grid.watched.clone();
        let term = SharedString::new(term);
        canvas(
            move |bounds, _, _| {
                if watched.borrow().as_ref() == Some(&term) {
                    marker.set(Some((term.clone(), bounds)));
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
    }

    /// How many words are selected; `None` outside selection mode.
    pub(super) fn selection(&self) -> Option<usize> {
        self.selection.as_ref().map(BTreeSet::len)
    }

    /// The entry the Edit word dialog edits, with the id of why its last save wrote nothing.
    pub(super) fn editing(&self) -> Option<(String, Option<&'static str>)> {
        self.edit.as_ref().map(|edit| (edit.original.clone(), edit.error.map(Tr::id)))
    }

    /// The terms the page lists, as loaded from the wordbook.
    pub(super) fn listed(&self) -> Vec<String> {
        self.words.data.matching.iter().map(|entry| entry.term.clone()).collect()
    }

    pub(super) fn probe(&mut self, case: &str, arg: &str, window: &mut Window, cx: &mut Context<Self>) {
        match case {
            "add" => {
                self.add.update(cx, |s, cx| s.set_value(arg, window, cx));
                self.add_word(window, cx);
            }
            "edit" => {
                let entry = self.words.data.matching.iter().find(|entry| entry.term == arg).cloned();
                if let Some(entry) = entry {
                    self.open_edit(&entry, window, cx);
                }
            }
            "edit-term" => self.edit_term.update(cx, |s, cx| s.set_value(arg, window, cx)),
            "edit-save" => self.save_edit(window, cx),
            "remove" => self.remove_words(vec![arg.to_owned()], cx),
            "import" => self.import_from(arg.into(), cx),
            "export" => self.export_to(arg.into(), cx),
            "dismiss" => self.dismiss_failure(cx),
            other => panic!("unknown wordbook probe {other:?}"),
        }
    }

    pub(super) fn failure_name(&self) -> Option<&'static str> {
        self.failure.map(|f| match f {
            Failure::Add => "add",
            Failure::Delete => "delete",
            Failure::Import => "import",
            Failure::Export => "export",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{GRID_GAP, OVERSCAN_ROWS, ROW_PITCH, Range, rows_in_window, rows_to_build};

    const WINDOW_H: f32 = 860.;
    /// The rows of the grid that a window `WINDOW_H` tall shows, whole or in part.
    const WINDOW_ROWS: usize = (WINDOW_H / ROW_PITCH).ceil() as usize;

    fn covers(built: &Range<usize>, shown: &Range<usize>) -> bool {
        built.start <= shown.start && shown.end <= built.end
    }

    #[test]
    fn a_grid_no_taller_than_the_window_and_its_overscan_is_built_whole() {
        let total = WINDOW_ROWS + 2 * OVERSCAN_ROWS;
        assert_eq!(rows_to_build(Some(437.), 0., WINDOW_H, total), 0..total);
        assert_eq!(rows_to_build(None, 300., WINDOW_H, total), 0..total);
        assert_ne!(rows_to_build(Some(437.), 0., WINDOW_H, total + 1), 0..total + 1);
    }

    #[test]
    fn the_built_rows_cover_the_window_at_every_offset() {
        let total = 500;
        let slack = OVERSCAN_ROWS as f32 * ROW_PITCH;
        for top in [0., 437., 700.] {
            let mut scrolled = 0.;
            while scrolled < total as f32 * ROW_PITCH + top {
                let shown = rows_in_window(top - scrolled, WINDOW_H, total);
                let built = rows_to_build(Some(top), scrolled, WINDOW_H, total);
                assert!(covers(&built, &shown), "top {top}, scrolled {scrolled}: {built:?} misses {shown:?}");
                assert!(
                    built.len() <= WINDOW_ROWS + 1 + 2 * OVERSCAN_ROWS,
                    "top {top}, scrolled {scrolled}: {built:?}"
                );
                for stale in [Some(top - slack), Some(top + slack), None] {
                    let built = rows_to_build(stale, scrolled, WINDOW_H, total);
                    assert!(covers(&built, &shown), "top {stale:?} for {top}, scrolled {scrolled}: {built:?}");
                }
                scrolled += 7.;
            }
        }
    }

    #[test]
    fn an_offset_from_before_the_grid_shrank_still_builds_its_last_rows() {
        let total = 60;
        for below in [0., 36.] {
            let top_at_the_end = WINDOW_H - below - (total as f32 * ROW_PITCH - GRID_GAP);
            let shown = rows_in_window(top_at_the_end, WINDOW_H, total);
            for top in [Some(437.), None] {
                let built = rows_to_build(top, 90_000., WINDOW_H, total);
                assert!(covers(&built, &shown), "top {top:?}, {below} below: {built:?} misses {shown:?}");
            }
        }
    }
}
