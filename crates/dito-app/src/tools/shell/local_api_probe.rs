//! What the screenshot harness steers on the Local API page.

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{Tools, ToolsPage};
use crate::app_shell::NavHost;
use crate::tools::local_api::LocalApiPose;

impl Tools {
    pub fn select_page_for_shot(&mut self, page: ToolsPage, cx: &mut Context<Self>) {
        self.select_page(page, cx);
    }

    /// Shows the Local API page in one of the states of its design boards.
    pub fn pose_local_api(&mut self, pose: LocalApiPose, window: &mut Window, cx: &mut Context<Self>) {
        self.select_page(ToolsPage::LocalApi, cx);
        self.local_api.update(cx, |page, cx| page.pose(pose, window, cx));
    }
}
