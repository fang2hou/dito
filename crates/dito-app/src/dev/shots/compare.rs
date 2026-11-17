//! Shots compared with `design:settings/compare-models`: the entry in the speech recognition card and every
//! stage of the comparison dialog, then one round run for real against the mock cloud speech server.
//! Opt-in: `DITO_SHOTS_ONLY=compare`, with `--mock --data-dir <dir>`.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::config::{Appearance, CloudAsrConfigurations, DisplayLanguage, Source};
use dito_core::secrets::Role;
use gpui_kit::*;

use super::capture::{page_shot, resize_settings_window, theme_tag};
use super::input::{on_settings_window, redraw};
use crate::services;
use crate::settings::{self, Page};
use crate::state::AppState;

const STAND_IN: &str = "来週の打ち合わせは木曜の午後3時からに変更します。資料は前日までに共有してください。
来週の打ち合わせは木曜の午後3時からに変更します。資料は全日までに共有してください。
来週の打ち合わせ、木曜の午後三時から変更します。資料は前日までに共有して下さい。
来週の打ち合わせは木曜の午後3時からに変更します。資料は前日までに共有してください。
来週の打ち合わせは木曜の午後3時からに変更します。資料は前日までに共友してください。";

fn step(cx: &mut AsyncApp, case: &'static str, arg: &str) {
    on_settings_window(cx, |window, cx, view| {
        view.update(cx, |s, cx| s.probe_models(case, arg, STAND_IN, window, cx));
        redraw(window, cx);
    });
}

fn stage(cx: &mut AsyncApp) -> &'static str {
    let mut out = "closed";
    on_settings_window(cx, |window, cx, view| out = view.read(cx).models_probe(window, cx).compare);
    out
}

async fn wait_for(cx: &mut AsyncApp, wanted: &str, within: Duration) {
    let start = Instant::now();
    while stage(cx) != wanted {
        assert!(start.elapsed() < within, "comparison stayed {:?}, expected {wanted:?}", stage(cx));
        cx.background_executor().timer(Duration::from_millis(100)).await;
    }
}

pub async fn compare_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let original = st.cfg.get();
    for appearance in [Appearance::Light, Appearance::Dark] {
        let tag = theme_tag(appearance);
        services::save_config(st, |c| {
            c.general.appearance = appearance;
            c.general.display_language = DisplayLanguage::Ja;
            c.asr.source = Source::Local;
            let mut cloud = CloudAsrConfigurations::default();
            for configuration in &mut cloud.items {
                configuration.config["connection"]["model"] = "asr-model".into();
            }
            c.asr.cloud_asr = Some(cloud);
        });
        // Two configurations are set up; the last one has no key, so it shows why it cannot be chosen.
        let accounts: Vec<String> =
            st.cfg.get().asr.cloud_asr.iter().flat_map(|list| &list.items).map(|c| c.account()).collect();
        for (index, account) in accounts.iter().enumerate() {
            services::store_secret(st, account, if index + 1 < accounts.len() { "mock-key" } else { "" });
        }
        let first =
            st.models.catalog().iter().find(|spec| spec.kind == dito_local::ModelKind::Asr).map(|s| s.id.clone());
        if let Some(first) = first {
            services::save_config(st, |c| c.asr.local_model = first);
        }
        cx.update(|cx| {
            settings::open(st.clone(), Page::Models, cx);
            resize_settings_window(cx, 1118., 978.);
        });
        on_settings_window(cx, |window, cx, view| {
            view.update(cx, |s, cx| s.preview_models(Role::Asr, None, window, cx))
        });
        step(cx, "compare-installed", "4");
        page_shot(cx, dir, &format!("compare-entry-{tag}.png")).await;
        step(cx, "source", "cloud");
        page_shot(cx, dir, &format!("compare-entry-cloud-{tag}.png")).await;
        step(cx, "source", "local");

        step(cx, "compare-open", "");
        page_shot(cx, dir, &format!("compare-1-limit-{tag}.png")).await;
        step(cx, "compare-toggle", "5");
        step(cx, "compare-toggle", "3");
        page_shot(cx, dir, &format!("compare-2-choose-{tag}.png")).await;
        step(cx, "compare-toggle", "3");

        step(cx, "compare-begin", "");
        page_shot(cx, dir, &format!("compare-3-ready-{tag}.png")).await;
        step(cx, "compare-record", "");
        page_shot(cx, dir, &format!("compare-4-recording-{tag}.png")).await;
        step(cx, "compare-close", "");

        step(cx, "compare-open", "");
        step(cx, "compare-begin", "");
        step(cx, "compare-stand-in", "recognizing");
        page_shot(cx, dir, &format!("compare-5-recognizing-{tag}.png")).await;
        step(cx, "compare-stand-in", "rate");
        step(cx, "compare-rate", "0:good");
        step(cx, "compare-rate", "1:poor");
        step(cx, "compare-rate", "3:good");
        page_shot(cx, dir, &format!("compare-6-rate-{tag}.png")).await;
        step(cx, "compare-finish", "");
        page_shot(cx, dir, &format!("compare-7-ranking-{tag}.png")).await;
        step(cx, "compare-close", "");
    }

    // design:settings/compare-models/short-window: only the middle of the dialog scrolls in a short window.
    services::save_config(st, |c| c.general.appearance = Appearance::Light);
    cx.update(|cx| resize_settings_window(cx, 1118., 640.));
    step(cx, "compare-open", "");
    step(cx, "compare-begin", "");
    step(cx, "compare-stand-in", "rate");
    step(cx, "compare-rate", "0:good");
    page_shot(cx, dir, "compare-short-top.png").await;
    for (fraction, name) in [("0.5", "middle"), ("1", "bottom")] {
        step(cx, "compare-scroll", fraction);
        page_shot(cx, dir, &format!("compare-short-{name}.png")).await;
    }
    step(cx, "compare-finish", "");
    page_shot(cx, dir, "compare-short-ranking.png").await;
    step(cx, "compare-close", "");
    cx.update(|cx| resize_settings_window(cx, 1118., 978.));

    // A real round: the synthetic microphone records, the local models (not on disk in the mock build) fail, and the
    // two cloud configurations answer from the mock server.
    step(cx, "compare-open", "");
    step(cx, "compare-begin", "");
    step(cx, "compare-record", "");
    cx.background_executor().timer(Duration::from_millis(1500)).await;
    step(cx, "compare-stop", "");
    wait_for(cx, "rate", Duration::from_secs(30)).await;
    page_shot(cx, dir, "compare-real-round.png").await;
    step(cx, "compare-rate", "0:good");
    step(cx, "compare-finish", "");
    page_shot(cx, dir, "compare-real-ranking.png").await;
    step(cx, "compare-close", "");
    services::save_config(st, |c| *c = (*original).clone());
}
