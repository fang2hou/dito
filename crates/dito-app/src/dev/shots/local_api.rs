//! The Local API page against `design:tools/local-api` and `design:tools/mcp`, in both appearances. Opt-in:
//! `DITO_SHOTS_ONLY=localapi`.

use std::path::Path;
use std::sync::Arc;

use dito_core::config::{Appearance, DisplayLanguage};
use gpui_kit::*;

use super::capture::{close_window, resize_tools_window, theme_tag, tools_page_shot};
use super::input::on_tools_window;
use crate::services;
use crate::state::AppState;
use crate::tools::{self, LocalApiPose, ToolsPage};

pub async fn local_api_shots(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let original = st.cfg.get();
    for appearance in [Appearance::Light, Appearance::Dark] {
        let tag = theme_tag(appearance);
        services::save_config(st, |c| {
            c.general.appearance = appearance;
            c.general.display_language = DisplayLanguage::Ja;
            c.local_api.enabled = false;
        });
        cx.update(|cx| {
            tools::open(st.clone(), cx);
            resize_tools_window(cx, 1120., 680.);
        });
        on_tools_window(cx, |_, cx, view| view.update(cx, |t, cx| t.select_page_for_shot(ToolsPage::LocalApi, cx)));
        tools_page_shot(cx, dir, &format!("local-api-off-{tag}.png")).await;
        services::save_config(st, |c| c.local_api.enabled = true);
        for (name, pose) in [
            ("api", LocalApiPose::Api),
            ("settings", LocalApiPose::Settings),
            ("reference-chat", LocalApiPose::Reference(1)),
            ("hover-first", LocalApiPose::Hover(0)),
            ("hover-last", LocalApiPose::Hover(2)),
            ("mcp", LocalApiPose::Mcp),
            ("reference-generate", LocalApiPose::Reference(3)),
        ] {
            on_tools_window(cx, |w, cx, view| view.update(cx, |t, cx| t.pose_local_api(pose, w, cx)));
            tools_page_shot(cx, dir, &format!("local-api-{name}-{tag}.png")).await;
        }
        services::save_config(st, |c| c.local_api.api_any_token = true);
        on_tools_window(cx, |w, cx, view| view.update(cx, |t, cx| t.pose_local_api(LocalApiPose::Api, w, cx)));
        tools_page_shot(cx, dir, &format!("local-api-any-token-{tag}.png")).await;
        services::save_config(st, |c| c.local_api.api_any_token = false);
        if let Some((window, _)) = cx.update(|cx| tools::handle(cx)) {
            close_window(cx, window);
        }
    }
    services::save_config(st, |c| {
        c.general.appearance = original.general.appearance;
        c.general.display_language = original.general.display_language;
        c.local_api = original.local_api.clone();
    });
}
