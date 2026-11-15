//! The find words dialog (`design:settings/wordbook/find-words`, `design:settings/wordbook/find-words-other-ai`).
//! Pasted text or one chosen file goes to the text model; for a file Dito cannot read, the user copies a prompt
//! for another AI and pastes its reply. Either way the words found are offered for the wordbook.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use dito_core::config::Source;
use dito_core::i18n::{Tr, fmt};
use dito_core::word_finder::{self, Candidate, Progress, ReplyError};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{ModalChanged, WordbookPage, chip_surface, off_main, selection_ring};
use crate::assets::Ic;
use crate::services::{self, WordSearchError};
use crate::settings::Ctx;
use crate::settings::edge_fade::{Edge, edge_fade};
use crate::settings::page_query::QueryPage;
use crate::timefmt::now;
use crate::ui::{self, BtnKind, TextAreaStyle, TextareaState, cbk, cbk_idx, text};

const DIALOG: &str = "wordbook-find-words";
const TEXT_H: f32 = 200.;
const REPLY_H: f32 = 148.;
/// How long the copy button shows that the prompt was copied.
const COPIED_FOR: Duration = Duration::from_millis(1500);
const CHIP_H: f32 = 44.;
const CHIP_GAP: f32 = 8.;
const CANDIDATES_H: f32 = 230.;
const FADE: f32 = 20.;

struct ChosenFile {
    path: PathBuf,
    name: SharedString,
    icon: Option<Arc<Image>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WordSource {
    Text,
    File,
    OtherAi,
}

impl WordSource {
    fn title(self) -> Tr {
        match self {
            Self::Text => Tr::WordbookFindTitleText,
            Self::File => Tr::WordbookFindTitleFile,
            Self::OtherAi => Tr::WordbookFindTitleAi,
        }
    }

    fn description(self) -> Tr {
        match self {
            Self::Text => Tr::WordbookFindDescText,
            Self::File => Tr::WordbookFindDescFile,
            Self::OtherAi => Tr::WordbookFindDescAi,
        }
    }
}

const SOURCES: [WordSource; 3] = [WordSource::Text, WordSource::File, WordSource::OtherAi];

enum Stage {
    Closed,
    Input,
    /// Dropping the tasks stops the search.
    Searching {
        progress: Progress,
        _tasks: [Task<()>; 2],
    },
    Results {
        candidates: Vec<Candidate>,
        selected: BTreeSet<usize>,
        /// Entries of another AI's reply that were not terms.
        skipped: usize,
    },
}

pub(super) struct FindWords {
    source: WordSource,
    text: Entity<TextareaState>,
    reply: Entity<TextareaState>,
    prompt_copied: Option<Task<()>>,
    focus: FocusHandle,
    scroll: ScrollHandle,
    file: Option<ChosenFile>,
    /// Given anew every time the dialog opens.
    cloud_consent: bool,
    error: Option<Tr>,
    stage: Stage,
}

impl FindWords {
    pub(super) fn new(window: &mut Window, cx: &mut Context<WordbookPage>) -> Self {
        let text = cx.new(|cx| TextareaState::new(window, cx));
        let reply = cx.new(|cx| TextareaState::new(window, cx));
        cx.observe(&text, |_, _, cx| cx.notify()).detach();
        cx.observe(&reply, |_, _, cx| cx.notify()).detach();
        Self {
            source: WordSource::Text,
            text,
            reply,
            prompt_copied: None,
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            file: None,
            cloud_consent: false,
            error: None,
            stage: Stage::Closed,
        }
    }

    pub(super) fn is_open(&self) -> bool {
        !matches!(self.stage, Stage::Closed)
    }
}

impl WordbookPage {
    pub(super) fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let lang = self.st.cfg.get().general.lang();
        self.find.text.update(cx, |s, cx| s.set_placeholder(lang.tr(Tr::WordbookFindTextHint), window, cx));
        self.find.reply.update(cx, |s, cx| s.set_placeholder(lang.tr(Tr::WordbookFindAiHint), window, cx));
        self.find.source = WordSource::Text;
        self.find.cloud_consent = false;
        self.find.error = None;
        self.find.stage = Stage::Input;
        self.find.text.read(cx).focus_handle(cx).focus(window, cx);
        ui::dismiss_tips(&self.motion.handle());
        cx.emit(ModalChanged);
    }

    /// Whatever was entered or found goes with the dialog.
    pub(super) fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.find.is_open() {
            return;
        }
        self.find.stage = Stage::Closed;
        self.find.file = None;
        self.find.prompt_copied = None;
        self.find.text.update(cx, |s, cx| s.set_value("", window, cx));
        self.find.reply.update(cx, |s, cx| s.set_value("", window, cx));
        window.blur(cx);
        cx.emit(ModalChanged);
    }

    fn choose_find_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result =
            cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: None });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = result.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, cx| this.set_find_file(path, cx));
            }
        })
        .detach();
    }

    fn set_find_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let supported = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| word_finder::EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
        if supported {
            let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned().into();
            let icon = dito_platform::system::file_icon_png(&path)
                .map(|png| Arc::new(Image::from_bytes(ImageFormat::Png, png)));
            self.find.file = Some(ChosenFile { path, name, icon });
            self.find.error = None;
        } else {
            self.find.error = Some(Tr::WordbookFindFileFailed);
        }
        cx.notify();
    }

    fn toggle_find_consent(&mut self, cx: &mut Context<Self>) {
        self.find.cloud_consent = !self.find.cloud_consent;
        cx.notify();
    }

    /// The cloud model's name while the text model is a cloud one.
    fn find_cloud_model(&self, c: &Ctx) -> Option<String> {
        let cfg = self.st.cfg.get();
        let engine = &cfg.text_model;
        if engine.source != Source::Provider {
            return None;
        }
        let configurations = engine.text_configurations(c.lang);
        let active = configurations.active();
        let model = active.map_or("", |active| active.target.model());
        Some(if model.is_empty() { active.map_or("—", |active| active.name.as_str()) } else { model }.to_owned())
    }

    fn find_ready(&self, cloud: bool, cx: &App) -> bool {
        match self.find.source {
            WordSource::Text => {
                !self.find.text.read(cx).value().trim().is_empty() && (!cloud || self.find.cloud_consent)
            }
            WordSource::File => self.find.file.is_some() && (!cloud || self.find.cloud_consent),
            WordSource::OtherAi => !self.find.reply.read(cx).value().trim().is_empty(),
        }
    }

    fn set_find_source(&mut self, index: usize, cx: &mut Context<Self>) {
        if matches!(self.find.stage, Stage::Input) && SOURCES[index] != self.find.source {
            self.find.source = SOURCES[index];
            self.find.error = None;
            cx.notify();
        }
    }

    fn run_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cloud = self.st.cfg.get().text_model.source == Source::Provider;
        if !matches!(self.find.stage, Stage::Input) || !self.find_ready(cloud, cx) {
            return;
        }
        let (text, file) = match self.find.source {
            WordSource::Text => (self.find.text.read(cx).value().to_string(), None),
            WordSource::File => (String::new(), self.find.file.as_ref().map(|file| file.path.clone())),
            WordSource::OtherAi => return self.show_reply(cx),
        };
        let mut search = services::find_words(self.st.clone(), text, file);
        let mut progress = search.progress.clone();
        let watching = cx.spawn(async move |this, cx| {
            while progress.changed().await.is_ok() {
                let now = *progress.borrow_and_update();
                let shown = this.update(cx, |this, cx| {
                    if let Stage::Searching { progress, .. } = &mut this.find.stage {
                        *progress = now;
                        cx.notify();
                    }
                });
                if shown.is_err() {
                    break;
                }
            }
        });
        let finishing = cx.spawn(async move |this, cx| {
            let found = search.finished().await;
            let _ = this.update(cx, |this, cx| this.find_finished(found, cx));
        });
        self.find.error = None;
        self.find.stage = Stage::Searching { progress: Progress::default(), _tasks: [watching, finishing] };
        self.find.focus.focus(window, cx);
        cx.notify();
    }

    fn find_finished(&mut self, found: Result<Vec<Candidate>, WordSearchError>, cx: &mut Context<Self>) {
        if !matches!(self.find.stage, Stage::Searching { .. }) {
            return;
        }
        (self.find.stage, self.find.error) = match found {
            Ok(candidates) if candidates.is_empty() => (Stage::Input, Some(Tr::WordbookFindNone)),
            Ok(candidates) => {
                let selected = candidates.iter().enumerate().filter(|(_, c)| c.likely).map(|(i, _)| i).collect();
                self.find.scroll = ScrollHandle::new();
                (Stage::Results { candidates, selected, skipped: 0 }, None)
            }
            Err(WordSearchError::File) => (Stage::Input, Some(Tr::WordbookFindFileFailed)),
            Err(WordSearchError::Model) => (Stage::Input, Some(Tr::WordbookFindFailed)),
        };
        cx.notify();
    }

    /// Reads the pasted reply of another AI. Nothing leaves the app on this path.
    fn show_reply(&mut self, cx: &mut Context<Self>) {
        let reply = self.find.reply.read(cx).value().to_string();
        let st = self.st.clone();
        cx.spawn(async move |this, cx| {
            let store = st.store.clone();
            let read = off_main(&st, move || Ok(word_finder::read_reply(&reply, &store.words(None)?))).await;
            let _ = this.update(cx, |this, cx| {
                if !matches!(this.find.stage, Stage::Input) || this.find.source != WordSource::OtherAi {
                    return;
                }
                (this.find.stage, this.find.error) = match read {
                    Ok(Ok(reply)) if reply.candidates.is_empty() => (Stage::Input, Some(Tr::WordbookFindNone)),
                    Ok(Ok(reply)) => {
                        let selected =
                            reply.candidates.iter().enumerate().filter(|(_, c)| c.likely).map(|(i, _)| i).collect();
                        this.find.scroll = ScrollHandle::new();
                        (Stage::Results { candidates: reply.candidates, selected, skipped: reply.skipped }, None)
                    }
                    Ok(Err(ReplyError::Unreadable)) => (Stage::Input, Some(Tr::WordbookFindReplyUnreadable)),
                    Ok(Err(ReplyError::WrongShape)) => (Stage::Input, Some(Tr::WordbookFindReplyWrongShape)),
                    Err(e) => {
                        tracing::warn!("word finder: the wordbook could not be read: {e:#}");
                        (Stage::Input, Some(Tr::WordbookFindFailed))
                    }
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn copy_find_prompt(&mut self, cx: &mut Context<Self>) {
        if let Err(e) = dito_platform::insert::copy_to_clipboard(word_finder::OTHER_AI_PROMPT) {
            tracing::warn!("word finder: the prompt could not be copied: {e}");
            return;
        }
        self.find.prompt_copied = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPIED_FOR).await;
            let _ = this.update(cx, |this, cx| {
                this.find.prompt_copied = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn stop_find(&mut self, cx: &mut Context<Self>) {
        if matches!(self.find.stage, Stage::Searching { .. }) {
            self.find.stage = Stage::Input;
            cx.notify();
        }
    }

    fn toggle_candidate(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Stage::Results { selected, .. } = &mut self.find.stage {
            if !selected.remove(&index) {
                selected.insert(index);
            }
            cx.notify();
        }
    }

    fn add_candidates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Stage::Results { candidates, selected, .. } = &self.find.stage else { return };
        let words: Vec<&str> = selected.iter().filter_map(|i| candidates.get(*i)).map(|c| c.term.as_str()).collect();
        if words.is_empty() {
            return;
        }
        let words = words.join("\n");
        self.close_find(window, cx);
        let st = self.st.clone();
        cx.spawn(async move |this, cx| {
            let store = st.store.clone();
            let added = off_main(&st, move || Ok(store.import_words(&words, now())?)).await;
            let _ = this.update(cx, |this, cx| {
                match added {
                    Ok(_) => this.succeeded(cx),
                    Err(e) => this.failed(super::Failure::Add, &e, cx),
                }
                // A failed batch may still have added the words before the error.
                this.load(cx);
            });
        })
        .detach();
    }

    pub(super) fn find_overlay(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        let cancel = |cx: &mut Context<Self>| ui::DialogAnswer {
            label: c.t(Tr::AsrEditorCancel),
            on_click: Some(cbk(cx, Self::close_find)),
        };
        let source = self.find.source;
        let (title, switch_on, content, cancel, confirm) = match &self.find.stage {
            Stage::Closed | Stage::Input => {
                let cloud = (source != WordSource::OtherAi).then(|| self.find_cloud_model(&c)).flatten();
                let ready = self.find_ready(cloud.is_some(), cx);
                let run = ui::DialogAnswer {
                    label: c.t(if source == WordSource::OtherAi {
                        Tr::WordbookFindAiShow
                    } else {
                        Tr::WordbookFindRun
                    }),
                    on_click: ready.then(|| cbk(cx, Self::run_find)),
                };
                let mut content = vec![self.find_description(&c)];
                content.extend(self.find.error.map(|error| ui::banner(&p, c.t(error), None).into_any_element()));
                match source {
                    WordSource::Text => content.push(self.find_text(&c, window, cx)),
                    WordSource::File => content.push(self.find_file(&c, cx, true)),
                    WordSource::OtherAi => content.extend(self.find_other_ai(&c, window, cx)),
                }
                content.extend(cloud.map(|model| self.find_consent(&c, &model, cx)));
                (c.t(source.title()), Some(true), content, cancel(cx), Some(run))
            }
            Stage::Searching { progress, .. } => {
                let stop = ui::DialogAnswer {
                    label: c.t(Tr::WordbookFindStop),
                    on_click: Some(cbk(cx, |this: &mut Self, _, cx| this.stop_find(cx))),
                };
                let shown = match source {
                    WordSource::File => self.find_file(&c, cx, false),
                    _ => self.find_text(&c, window, cx),
                };
                let dimmed =
                    div().relative().w_full().opacity(0.45).child(shown).child(div().absolute().inset_0().occlude());
                let content = vec![self.find_description(&c), dimmed.into_any_element(), find_progress(&c, *progress)];
                (c.t(source.title()), Some(false), content, stop, None)
            }
            Stage::Results { candidates, selected, skipped } => {
                let add = ui::DialogAnswer {
                    label: fmt::wordbook_find_add(c.lang, &selected.len().to_string()).into(),
                    on_click: (!selected.is_empty()).then(|| cbk(cx, Self::add_candidates)),
                };
                let mut content = vec![
                    text(c.t(Tr::WordbookFindChoose), 13., FontWeight::NORMAL, p.muted)
                        .line_height(px(20.))
                        .into_any_element(),
                ];
                if *skipped > 0 {
                    let note = fmt::wordbook_find_skipped(c.lang, &skipped.to_string());
                    content.push(ui::muted12(&p, note).line_height(px(18.)).into_any_element());
                }
                content.push(self.find_candidates(&c, candidates, selected, window, cx));
                let title = fmt::wordbook_find_found(c.lang, &candidates.len().to_string()).into();
                (title, None, content, cancel(cx), Some(add))
            }
        };
        let title = text(title, 20., FontWeight::BOLD, p.text).flex_1().min_w(px(0.)).truncate();
        let header = div().w_full().flex().items_center().justify_between().gap(px(16.)).child(title).children(
            switch_on.map(|enabled| {
                let labels = [c.t(Tr::WordbookFindSegText), c.t(Tr::WordbookFindSegFile), c.t(Tr::WordbookFindSegAi)];
                let selected = SOURCES.iter().position(|s| *s == source).unwrap_or(0);
                let switch = ui::seg(
                    "wordbook-find-source",
                    &p,
                    &labels,
                    selected,
                    cbk_idx(cx, |this: &mut Self, index, _, cx| this.set_find_source(index, cx)),
                );
                if enabled {
                    div().child(switch)
                } else {
                    div().relative().opacity(0.45).child(switch).child(div().absolute().inset_0().occlude())
                }
            }),
        );
        ui::render_with_scope(self.motion.handle(), || {
            ui::confirm_dialog_with_header(
                window,
                &p,
                DIALOG,
                &self.find.focus,
                header.into_any_element(),
                content,
                cancel,
                confirm,
                ui::ConfirmStyle::Standard,
            )
        })
    }

    fn find_description(&self, c: &Ctx) -> AnyElement {
        text(c.t(self.find.source.description()), 13., FontWeight::NORMAL, c.p.muted)
            .line_height(px(20.))
            .into_any_element()
    }

    fn find_area(
        &self,
        c: &Ctx,
        state: &Entity<TextareaState>,
        id: &'static str,
        height: f32,
        window: &Window,
        cx: &App,
    ) -> Div {
        let p = c.p;
        let focused = state.read(cx).focus_handle(cx).contains_focused(window, cx);
        let area = div().w_full().h(px(height)).rounded(px(12.)).child(ui::text_area(
            state,
            TextAreaStyle {
                bg: p.input_bg,
                edge: None,
                radius: 12.,
                text_size: 13.,
                line_height: 20.,
                pad: [14., 10., 14., 10.],
                fill: true,
                reserve_bottom: 0.,
            },
        ));
        ui::with_ring(area, id, focused, 12., p.border_strong, p.accent)
    }

    fn find_text(&self, c: &Ctx, window: &Window, cx: &App) -> AnyElement {
        self.find_area(c, &self.find.text, "wordbook-find-text", TEXT_H, window, cx).into_any_element()
    }

    /// The file row; a file dropped on it is taken as the chosen file.
    fn find_file(&self, c: &Ctx, cx: &mut Context<Self>, enabled: bool) -> AnyElement {
        let p = c.p;
        let identity = match &self.find.file {
            Some(file) => div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .items_center()
                .gap(px(16.))
                .children(file.icon.clone().map(|icon| img(icon).size(px(40.)).flex_none()))
                .child(
                    text(file.name.clone(), 25., FontWeight::MEDIUM, p.text)
                        .flex_1()
                        .min_w(px(0.))
                        .line_height(px(40.))
                        .truncate(),
                ),
            None => text(c.t(Tr::PromptsUnselected), 14., FontWeight::NORMAL, p.muted).flex_1(),
        };
        let choose_label =
            c.t(if self.find.file.is_some() { Tr::PromptsChangeApplication } else { Tr::WordbookFindChooseFile });
        let file_slot = div()
            .w_full()
            .h(px(72.))
            .px(px(16.))
            .rounded(px(12.))
            .border_1()
            .border_color(p.border_strong)
            .bg(p.input_bg)
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.))
            .child(identity)
            .child(ui::button_with(
                "wordbook-find-choose-file",
                &p,
                &choose_label,
                BtnKind::Primary,
                ui::BtnSize::Large,
                None,
                None,
                cbk(cx, Self::choose_find_file),
            ));
        let field = div()
            .id("wordbook-find-file")
            .w_full()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(file_slot)
            .child(ui::muted12(&p, c.t(Tr::WordbookFindFileTypes)).line_height(px(18.)));
        if !enabled {
            return field.into_any_element();
        }
        field
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                if let Some(path) = paths.paths().first() {
                    this.set_find_file(path.clone(), cx);
                }
            }))
            .into_any_element()
    }

    /// The two steps of the other AI source: copy the prompt, paste the reply.
    fn find_other_ai(&self, c: &Ctx, window: &Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = c.p;
        let copied = self.find.prompt_copied.is_some();
        let copy = div().flex().child(ui::button(
            "wordbook-find-copy-prompt",
            &p,
            &c.t(Tr::WordbookFindAiCopy),
            BtnKind::Secondary,
            Some(if copied { Ic::Check } else { Ic::Copy }),
            cbk(cx, |this: &mut Self, _, cx| this.copy_find_prompt(cx)),
        ));
        let reply = self.find_area(c, &self.find.reply, "wordbook-find-reply", REPLY_H, window, cx);
        vec![
            ui::field(&p, c.t(Tr::WordbookFindAiStep1), copy).into_any_element(),
            ui::field(&p, c.t(Tr::WordbookFindAiStep2), reply).into_any_element(),
        ]
    }

    fn find_consent(&self, c: &Ctx, model: &str, cx: &mut Context<Self>) -> AnyElement {
        let p = c.p;
        let ring_id = ElementId::from("wordbook-find-consent-ring");
        let hover = if ui::is_hovered(&ring_id) && !self.find.cloud_consent { 1. } else { 0. };
        let chosen = if self.find.cloud_consent { 1. } else { 0. };
        div()
            .id("wordbook-find-consent")
            .w_full()
            .px(px(14.))
            .py(px(12.))
            .rounded(px(12.))
            .bg(p.sub)
            .flex()
            .items_start()
            .gap(px(10.))
            .cursor_pointer()
            .on_click(ui::click_cb(cbk(cx, |this: &mut Self, _, cx| this.toggle_find_consent(cx))))
            .child(div().pt(px(2.)).child(selection_ring(&p, ring_id, hover, chosen)))
            .child(
                text(fmt::wordbook_find_consent(c.lang, model), 13., FontWeight::NORMAL, p.text)
                    .flex_1()
                    .min_w(px(0.))
                    .line_height(px(20.)),
            )
            .into_any_element()
    }

    fn find_candidates(
        &self,
        c: &Ctx,
        candidates: &[Candidate],
        selected: &BTreeSet<usize>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = c.p;
        let on_scroll = cx.listener(|_, _: &ScrollEvent, _, cx| cx.notify());
        let chips = candidates.iter().enumerate().map(|(index, candidate)| {
            let chip_id = ElementId::from(("wordbook-find-candidate", index));
            let ring_id = ElementId::from((chip_id.clone(), "ring"));
            let on = selected.contains(&index);
            let hover = if ui::is_hovered(&chip_id) && !on { 1. } else { 0. };
            let (chosen, fading) =
                ui::fade_value(&ElementId::from((chip_id.clone(), "chosen")), f32::from(on), ui::HOVER_MS);
            if fading {
                window.request_animation_frame();
            }
            chip_surface(&p, chip_id, candidate.term.clone().into(), chosen)
                .cursor_pointer()
                .on_click(ui::click_cb(cbk(cx, move |this: &mut Self, _, cx| this.toggle_candidate(index, cx))))
                .child(
                    div()
                        .size(px(24.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(selection_ring(&p, ring_id, hover, chosen)),
                )
        });
        let rows = candidates.len().div_ceil(2) as f32;
        let content_h = rows * (CHIP_H + CHIP_GAP) - CHIP_GAP;
        let viewport_h = content_h.min(CANDIDATES_H);
        let max_scroll = content_h - viewport_h;
        let scrolled = (-f32::from(self.find.scroll.offset().y)).clamp(0., max_scroll);
        let top = (scrolled / FADE).clamp(0., 1.);
        let bottom = ((max_scroll - scrolled) / FADE).clamp(0., 1.);
        let list = div()
            .id("wordbook-find-candidates")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.find.scroll)
            .on_scroll(on_scroll)
            .child(div().w_full().grid().grid_cols(2).gap(px(CHIP_GAP)).children(chips));
        div()
            .relative()
            .flex_none()
            .w_full()
            .h(px(viewport_h))
            .child(list)
            .when(top > 0.001, |d| d.child(edge_fade(Edge::Top, p.card, FADE, 0.).opacity(top)))
            .when(bottom > 0.001, |d| d.child(edge_fade(Edge::Bottom, p.card, FADE, 0.).opacity(bottom)))
            .into_any_element()
    }
}

fn find_progress(c: &Ctx, progress: Progress) -> AnyElement {
    let p = c.p;
    let fraction = if progress.total == 0 { 0. } else { progress.done as f32 / progress.total as f32 };
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
                .justify_between()
                .child(text(c.t(Tr::WordbookFindRunning), 13., FontWeight::MEDIUM, p.text).line_height(px(20.)))
                .when(progress.total > 0, |row| {
                    row.child(
                        text(format!("{} / {}", progress.done, progress.total), 13., FontWeight::NORMAL, p.muted)
                            .line_height(px(20.)),
                    )
                }),
        )
        .child(ui::progress_bar(&p, fraction))
        .into_any_element()
}
