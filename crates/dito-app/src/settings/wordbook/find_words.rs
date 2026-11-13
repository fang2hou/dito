//! The find words dialog (`design:settings/wordbook/find-words`): pasted text and one chosen file go to the polish
//! model, and the words it finds are offered for the wordbook.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use dito_core::config::Source;
use dito_core::i18n::{Tr, fmt};
use dito_core::word_finder::{self, Candidate, Progress};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{ModalChanged, WordbookPage, chip_surface, off_main, selection_ring};
use crate::services::{self, WordSearchError};
use crate::settings::Ctx;
use crate::settings::edge_fade::{Edge, edge_fade};
use crate::settings::page_query::QueryPage;
use crate::timefmt::now;
use crate::ui::{self, BtnKind, TextAreaStyle, TextareaState, cbk, text};

const DIALOG: &str = "wordbook-find-words";
const TEXT_H: f32 = 148.;
const CHIP_H: f32 = 44.;
const CHIP_GAP: f32 = 8.;
const CANDIDATES_H: f32 = 230.;
const FADE: f32 = 20.;

struct ChosenFile {
    path: PathBuf,
    name: SharedString,
    icon: Option<Arc<Image>>,
}

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
    },
}

pub(super) struct FindWords {
    text: Entity<TextareaState>,
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
        cx.observe(&text, |_, _, cx| cx.notify()).detach();
        Self {
            text,
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
        let placeholder = self.st.cfg.get().general.lang().tr(Tr::WordbookFindTextHint);
        self.find.text.update(cx, |s, cx| s.set_placeholder(placeholder, window, cx));
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
        self.find.text.update(cx, |s, cx| s.set_value("", window, cx));
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

    /// The cloud model's name while the polish model is a cloud one.
    fn find_cloud_model(&self, c: &Ctx) -> Option<String> {
        let cfg = self.st.cfg.get();
        let engine = &cfg.polish.engine;
        if engine.source != Source::Provider {
            return None;
        }
        let configurations = engine.polish_configurations(c.lang);
        let active = configurations.active();
        let model = active.map_or("", |active| active.target.model());
        Some(if model.is_empty() { active.map_or("—", |active| active.name.as_str()) } else { model }.to_owned())
    }

    fn find_ready(&self, cloud: bool, cx: &App) -> bool {
        let has_source = self.find.file.is_some() || !self.find.text.read(cx).value().trim().is_empty();
        has_source && (!cloud || self.find.cloud_consent)
    }

    fn run_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cloud = self.st.cfg.get().polish.engine.source == Source::Provider;
        if !matches!(self.find.stage, Stage::Input) || !self.find_ready(cloud, cx) {
            return;
        }
        let text = self.find.text.read(cx).value().to_string();
        let file = self.find.file.as_ref().map(|file| file.path.clone());
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
                (Stage::Results { candidates, selected }, None)
            }
            Err(WordSearchError::File) => (Stage::Input, Some(Tr::WordbookFindFileFailed)),
            Err(WordSearchError::Model) => (Stage::Input, Some(Tr::WordbookFindFailed)),
        };
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
        let Stage::Results { candidates, selected } = &self.find.stage else { return };
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
        let cancel = |label: Tr, cx: &mut Context<Self>| ui::DialogAnswer {
            label: c.t(label),
            on_click: Some(cbk(cx, Self::close_find)),
        };
        let (title, content, cancel, confirm) = match &self.find.stage {
            Stage::Closed | Stage::Input => {
                let cloud = self.find_cloud_model(&c);
                let ready = self.find_ready(cloud.is_some(), cx);
                let run = ui::DialogAnswer {
                    label: c.t(Tr::WordbookFindRun),
                    on_click: ready.then(|| cbk(cx, Self::run_find)),
                };
                let mut content = vec![self.find_description(&c)];
                content.extend(self.find.error.map(|error| ui::banner(&c.p, c.t(error), None).into_any_element()));
                content.push(self.find_sources(&c, window, cx, true));
                content.extend(cloud.map(|model| self.find_consent(&c, &model, cx)));
                (c.t(Tr::WordbookFindTitle), content, cancel(Tr::AsrEditorCancel, cx), Some(run))
            }
            Stage::Searching { progress, .. } => {
                let stop = ui::DialogAnswer {
                    label: c.t(Tr::WordbookFindStop),
                    on_click: Some(cbk(cx, |this: &mut Self, _, cx| this.stop_find(cx))),
                };
                let content = vec![
                    self.find_description(&c),
                    self.find_sources(&c, window, cx, false),
                    find_progress(&c, *progress),
                ];
                (c.t(Tr::WordbookFindTitle), content, stop, None)
            }
            Stage::Results { candidates, selected } => {
                let add = ui::DialogAnswer {
                    label: fmt::wordbook_find_add(c.lang, &selected.len().to_string()).into(),
                    on_click: (!selected.is_empty()).then(|| cbk(cx, Self::add_candidates)),
                };
                let content = vec![
                    text(c.t(Tr::WordbookFindChoose), 13., FontWeight::NORMAL, c.p.muted)
                        .line_height(px(20.))
                        .into_any_element(),
                    self.find_candidates(&c, candidates, selected, window, cx),
                ];
                let title = fmt::wordbook_find_found(c.lang, &candidates.len().to_string()).into();
                (title, content, cancel(Tr::AsrEditorCancel, cx), Some(add))
            }
        };
        ui::with_motion_scope(self.motion.handle(), || {
            ui::confirm_dialog(
                window,
                &c.p,
                DIALOG,
                &self.find.focus,
                title,
                content,
                cancel,
                confirm,
                ui::ConfirmStyle::Standard,
            )
        })
    }

    fn find_description(&self, c: &Ctx) -> AnyElement {
        text(c.t(Tr::WordbookFindDescription), 13., FontWeight::NORMAL, c.p.muted)
            .line_height(px(20.))
            .into_any_element()
    }

    /// The text area and the file row. While a search runs they keep their content, dimmed and out of reach.
    fn find_sources(&self, c: &Ctx, window: &Window, cx: &mut Context<Self>, enabled: bool) -> AnyElement {
        let p = c.p;
        let focused = self.find.text.read(cx).focus_handle(cx).contains_focused(window, cx);
        let area = div().w_full().h(px(TEXT_H)).rounded(px(12.)).child(ui::text_area(
            &self.find.text,
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
        let area = ui::with_ring(area, "wordbook-find-text", focused, 12., p.border_strong, p.accent);

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
        let file_field = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(file_slot)
            .child(ui::muted12(&p, c.t(Tr::WordbookFindFileTypes)).line_height(px(18.)));

        let sources = div()
            .id("wordbook-find-sources")
            .relative()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(ui::field(&p, c.t(Tr::WordbookFindText), area))
            .child(ui::field(&p, c.t(Tr::WordbookFindFile), file_field));
        if enabled {
            sources
                .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                    if let Some(path) = paths.paths().first() {
                        this.set_find_file(path.clone(), cx);
                    }
                }))
                .into_any_element()
        } else {
            sources.opacity(0.45).child(div().absolute().inset_0().occlude()).into_any_element()
        }
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
