use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::config::{Appearance, DisplayLanguage};
use gpui_kit::*;

use super::capture::{page_shot, resize_settings_window, theme_tag};
use super::input::on_settings_window;
use crate::services;
use crate::settings::{self, Page};
use crate::state::AppState;

/// Holds the write lock of the database from a second process, so every write the app attempts fails with a busy
/// error once SQLite stops waiting for it.
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
    on_settings_window(cx, |window, cx, view| view.update(cx, |s, cx| s.probe_dictionary(case, arg, window, cx)));
}

fn failure(cx: &mut AsyncApp) -> Option<&'static str> {
    cx.update(|cx| settings::handle(cx).and_then(|(_, view)| view.read(cx).dictionary_failure(cx)))
}

/// Wait for an operation's result to reach the page: the banner names `expected`, or is gone for `None`.
async fn expect_failure(cx: &mut AsyncApp, expected: Option<&str>, within: Duration) {
    let start = Instant::now();
    while failure(cx) != expected {
        assert!(start.elapsed() < within, "dictionary banner stayed {:?}, expected {expected:?}", failure(cx));
        cx.background_executor().timer(Duration::from_millis(50)).await;
    }
    println!("dictionary banner {expected:?} after {} ms", start.elapsed().as_millis());
}

fn open_dictionary(st: &Arc<AppState>, cx: &mut AsyncApp) {
    cx.update(|cx| {
        settings::open(st.clone(), Page::Dictionary, cx);
        resize_settings_window(cx, 1120., 860.);
    });
}

/// Force each of the four operations to fail for real and check the banner: an unreadable import file, an
/// unwritable export path, and add and delete against a database another process holds locked. Opt-in:
/// `DITO_SHOTS_ONLY=dictionary`, with `--mock --data-dir <dir>` so the database file is known.
pub async fn dictionary_failure_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let data_dir = st.opts.data_dir.clone().expect("dictionary failure shots need --data-dir");
    let db = data_dir.join("dito.db");
    let original = st.cfg.get();
    let missing: PathBuf = dir.join("no-such-file.txt");
    let unwritable: PathBuf = dir.join("no-such-dir").join("dictionary.txt");
    let exported: PathBuf = dir.join("dictionary-export.txt");
    let quick = Duration::from_secs(3);
    for appearance in [Appearance::Light, Appearance::Dark] {
        let tag = theme_tag(appearance);
        services::save_config(st, |c| {
            c.general.appearance = appearance;
            c.general.display_language = DisplayLanguage::Ja;
        });
        open_dictionary(st, cx);
        cx.background_executor().timer(Duration::from_millis(900)).await;
        expect_failure(cx, None, quick).await;

        probe(cx, "import", missing.to_str().expect("utf-8 path"));
        expect_failure(cx, Some("import"), quick).await;
        page_shot(cx, dir, &format!("dictionary-failure-{tag}.png")).await;
        for (language, name) in [(DisplayLanguage::En, "en"), (DisplayLanguage::Zh, "zh")] {
            services::save_config(st, |c| c.general.display_language = language);
            page_shot(cx, dir, &format!("dictionary-failure-{name}-{tag}.png")).await;
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
        page_shot(cx, dir, &format!("dictionary-failure-add-{tag}.png")).await;
        probe(cx, "remove", "Acme");
        expect_failure(cx, Some("delete"), busy).await;
        page_shot(cx, dir, &format!("dictionary-failure-delete-{tag}.png")).await;
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
