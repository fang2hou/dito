use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::config::{Appearance, DisplayLanguage};
use dito_core::i18n::Lang;
use dito_core::store::WordbookEntry;
use gpui_kit::*;

use super::capture::{page_shot, resize_settings_window, theme_tag};
use super::input::{click, on_settings_window};
use crate::services;
use crate::settings::{self, Page};
use crate::state::AppState;

/// The words with spoken forms the shots import, as a user would write them.
const SPOKEN_FORMS: &str =
    "MTG [ja] = meeting, ミーティング\n1on1 = ワンオンワン\nPR [ja] = プルリク, プルリクエスト\ngpui-kit\n";

/// A second process holds the write lock, so every write the app attempts fails with a busy error once SQLite stops
/// waiting.
struct DbLock(Child);

impl DbLock {
    async fn hold(db: &Path, cx: &mut AsyncApp) -> Self {
        let mut child = Command::new("sqlite3")
            .arg(db)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .expect("start sqlite3 to lock the database");
        child.stdin.as_mut().expect("sqlite3 stdin").write_all(b"BEGIN IMMEDIATE;\n").expect("lock the database");
        cx.background_executor().timer(Duration::from_millis(500)).await;
        Self(child)
    }
}

impl Drop for DbLock {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn probe(cx: &mut AsyncApp, case: &'static str, arg: &str) {
    on_settings_window(cx, |window, cx, view| view.update(cx, |s, cx| s.probe_wordbook(case, arg, window, cx)));
}

fn failure(cx: &mut AsyncApp) -> Option<&'static str> {
    cx.update(|cx| settings::handle(cx).and_then(|(_, view)| view.read(cx).wordbook_failure(cx)))
}

/// `None` expects the banner to be gone.
async fn expect_failure(cx: &mut AsyncApp, expected: Option<&str>, within: Duration) {
    let start = Instant::now();
    while failure(cx) != expected {
        assert!(start.elapsed() < within, "wordbook banner stayed {:?}, expected {expected:?}", failure(cx));
        cx.background_executor().timer(Duration::from_millis(50)).await;
    }
    println!("wordbook banner {expected:?} after {} ms", start.elapsed().as_millis());
}

fn open_wordbook(st: &Arc<AppState>, cx: &mut AsyncApp) {
    cx.update(|cx| {
        settings::open(st.clone(), Page::Wordbook, cx);
        resize_settings_window(cx, 1120., 860.);
    });
}

/// Opt-in: `DITO_SHOTS_ONLY=wordbook`, with `--mock --data-dir <dir>` so the database file is known.
pub async fn wordbook_failure_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let data_dir = st.opts.data_dir.clone().expect("wordbook failure shots need --data-dir");
    let db = data_dir.join("dito.db");
    let original = st.cfg.get();
    let missing: PathBuf = dir.join("no-such-file.txt");
    let unwritable: PathBuf = dir.join("no-such-dir").join("wordbook.txt");
    let exported: PathBuf = dir.join("wordbook-export.txt");
    let quick = Duration::from_secs(3);
    for appearance in [Appearance::Light, Appearance::Dark] {
        let tag = theme_tag(appearance);
        services::save_config(st, |c| {
            c.general.appearance = appearance;
            c.general.display_language = DisplayLanguage::Ja;
        });
        open_wordbook(st, cx);
        cx.background_executor().timer(Duration::from_millis(900)).await;
        expect_failure(cx, None, quick).await;

        probe(cx, "import", missing.to_str().expect("utf-8 path"));
        expect_failure(cx, Some("import"), quick).await;
        page_shot(cx, dir, &format!("wordbook-failure-{tag}.png")).await;
        for (language, name) in [(DisplayLanguage::En, "en"), (DisplayLanguage::Zh, "zh")] {
            services::save_config(st, |c| c.general.display_language = language);
            page_shot(cx, dir, &format!("wordbook-failure-{name}-{tag}.png")).await;
        }
        services::save_config(st, |c| c.general.display_language = DisplayLanguage::Ja);

        probe(cx, "export", unwritable.to_str().expect("utf-8 path"));
        expect_failure(cx, Some("export"), quick).await;
        probe(cx, "export", exported.to_str().expect("utf-8 path"));
        expect_failure(cx, None, quick).await;

        probe(cx, "import", missing.to_str().expect("utf-8 path"));
        expect_failure(cx, Some("import"), quick).await;
        probe(cx, "dismiss", "");
        expect_failure(cx, None, quick).await;

        let lock = DbLock::hold(&db, cx).await;
        let busy = Duration::from_secs(15);
        probe(cx, "add", "banner-probe-word");
        expect_failure(cx, Some("add"), busy).await;
        page_shot(cx, dir, &format!("wordbook-failure-add-{tag}.png")).await;
        probe(cx, "remove", "Acme");
        expect_failure(cx, Some("delete"), busy).await;
        page_shot(cx, dir, &format!("wordbook-failure-delete-{tag}.png")).await;
        drop(lock);

        probe(cx, "add", "banner-probe-word");
        expect_failure(cx, None, quick).await;
        probe(cx, "remove", "banner-probe-word");
        cx.background_executor().timer(Duration::from_millis(300)).await;
    }
    services::save_config(st, |c| {
        c.general.appearance = original.general.appearance;
        c.general.display_language = original.general.display_language;
    });
}

type WordbookState = (Option<usize>, Option<(String, Option<&'static str>)>);

fn wordbook_state(cx: &mut AsyncApp) -> WordbookState {
    cx.update(|cx| settings::handle(cx).map(|(_, view)| view.read(cx).wordbook_state(cx))).expect("settings window")
}

/// Has the chip of `term` painted, and where its selection marker is.
async fn marker_of(cx: &mut AsyncApp, term: &'static str) -> Bounds<Pixels> {
    on_settings_window(cx, |_, cx, view| view.update(cx, |settings, cx| settings.wordbook_watch_marker(term, cx)));
    let start = Instant::now();
    loop {
        let marker = cx.update(|cx| settings::handle(cx).and_then(|(_, view)| view.read(cx).wordbook_marker(cx)));
        if let Some(marker) = marker {
            return marker;
        }
        assert!(start.elapsed() < Duration::from_secs(3), "the chip of {term} was never painted");
        cx.background_executor().timer(Duration::from_millis(50)).await;
    }
}

/// Waits until the Edit word dialog reports `error` for the entry it still edits.
async fn expect_edit_error(cx: &mut AsyncApp, original: &str, error: &'static str, within: Duration) {
    let start = Instant::now();
    while wordbook_state(cx).1 != Some((original.to_string(), Some(error))) {
        assert!(start.elapsed() < within, "edit state {:?}, expected the error {error:?}", wordbook_state(cx).1);
        cx.background_executor().timer(Duration::from_millis(50)).await;
    }
}

/// Opt-in: `DITO_SHOTS_ONLY=wordbookspoken`, with `--mock --data-dir <dir>`. The chips with spoken forms, the marker
/// that selects a word without opening the Edit word dialog, the dialog, and its banner for a term another entry
/// already has.
pub async fn wordbook_spoken_form_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let source: PathBuf = dir.join("wordbook-spoken-forms.txt");
    std::fs::write(&source, SPOKEN_FORMS).expect("write the sample word list");
    let original = st.cfg.get();
    let quick = Duration::from_secs(3);
    for appearance in [Appearance::Light, Appearance::Dark] {
        let tag = theme_tag(appearance);
        services::save_config(st, |c| {
            c.general.appearance = appearance;
            c.general.display_language = DisplayLanguage::Ja;
        });
        open_wordbook(st, cx);
        cx.background_executor().timer(Duration::from_millis(900)).await;
        probe(cx, "import", source.to_str().expect("utf-8 path"));
        page_shot(cx, dir, &format!("wordbook-spoken-forms-{tag}.png")).await;

        let marker = marker_of(cx, "MTG").await;
        assert!(marker.size.width > px(0.) && marker.size.height > px(0.), "the marker has a size");
        let body = point(marker.left() - px(40.), marker.center().y);
        on_settings_window(cx, |w, cx, _| click(w, cx, marker.center()));
        cx.background_executor().timer(Duration::from_millis(250)).await;
        let (selected, editing) = wordbook_state(cx);
        assert_eq!((selected, editing.is_none()), (Some(1), true), "the marker selects and never opens the dialog");
        page_shot(cx, dir, &format!("wordbook-spoken-forms-selected-{tag}.png")).await;

        on_settings_window(cx, |w, cx, _| click(w, cx, marker.center()));
        cx.background_executor().timer(Duration::from_millis(250)).await;
        assert_eq!(wordbook_state(cx).0, None, "deselecting the last word leaves selection mode");
        on_settings_window(cx, |w, cx, _| click(w, cx, body));
        cx.background_executor().timer(Duration::from_millis(250)).await;
        assert_eq!(
            wordbook_state(cx).1.map(|(original, _)| original),
            Some("MTG".to_string()),
            "the chip body opens the dialog"
        );
        page_shot(cx, dir, &format!("wordbook-spoken-forms-edit-{tag}.png")).await;

        probe(cx, "edit-term", "1on1");
        cx.background_executor().timer(Duration::from_millis(100)).await;
        probe(cx, "edit-save", "");
        expect_edit_error(cx, "MTG", "wordbook.duplicate_term", quick).await;
        assert!(st.store.words(None).unwrap().contains(&"MTG".to_string()), "a refused rename writes nothing");
        page_shot(cx, dir, &format!("wordbook-spoken-forms-duplicate-{tag}.png")).await;

        probe(cx, "edit-term", "Stand-up");
        cx.background_executor().timer(Duration::from_millis(100)).await;
        probe(cx, "edit-save", "");
        let start = Instant::now();
        while wordbook_state(cx).1.is_some() {
            assert!(start.elapsed() < quick, "the dialog stayed open after a save");
            cx.background_executor().timer(Duration::from_millis(50)).await;
        }
        let forms = vec!["meeting".to_string(), "ミーティング".to_string()];
        assert!(
            st.store.wordbook_entries(None).unwrap().contains(&WordbookEntry {
                term: "Stand-up".to_string(),
                spoken: forms,
                lang: Some(Lang::Ja),
            }),
            "the rename kept the spoken forms and their language"
        );
        page_shot(cx, dir, &format!("wordbook-spoken-forms-saved-{tag}.png")).await;

        // The next pass imports the list again, so the renamed entry goes first.
        probe(cx, "remove", "Stand-up");
        cx.background_executor().timer(Duration::from_millis(300)).await;
    }
    services::save_config(st, |c| {
        c.general.appearance = original.general.appearance;
        c.general.display_language = original.general.display_language;
    });
}
