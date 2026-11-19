use dito_core::controller::{AppInfo, Frontmost};
use objc2_app_kit::NSWorkspace;

use crate::with_autorelease_pool;

pub struct MacFrontmost;

impl Frontmost for MacFrontmost {
    fn frontmost(&self) -> AppInfo {
        with_autorelease_pool(|| match NSWorkspace::sharedWorkspace().frontmostApplication() {
            Some(app) => AppInfo {
                name: app.localizedName().map(|s| s.to_string()).unwrap_or_default(),
                bundle_id: app.bundleIdentifier().map(|s| s.to_string()).unwrap_or_default(),
            },
            None => AppInfo::default(),
        })
    }
}
