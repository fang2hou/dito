//! Native icons of the apps in the application picker. Producing one takes about 100 ms, so each bundle id is encoded
//! at most once per process, off the main thread, and every later Settings window reuses the image.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use gpui_kit::{Image, ImageFormat};

type Slot = Arc<OnceLock<Option<Arc<Image>>>>;

static ICONS: Mutex<BTreeMap<String, Slot>> = Mutex::new(BTreeMap::new());

fn slot(bundle_id: &str) -> Slot {
    ICONS.lock().unwrap_or_else(PoisonError::into_inner).entry(bundle_id.to_owned()).or_default().clone()
}

/// Never blocks: `None` until [`load`] or [`remember`] has finished for this app.
pub(super) fn cached(bundle_id: &str) -> Option<Arc<Image>> {
    ICONS.lock().unwrap_or_else(PoisonError::into_inner).get(bundle_id)?.get()?.clone()
}

/// Blocks while the icon is encoded, so call it off the main thread. A second caller for the same app waits for the
/// first instead of encoding again.
pub(super) fn load(bundle_id: &str, path: &Path) -> Option<Arc<Image>> {
    slot(bundle_id)
        .get_or_init(|| {
            let png = dito_platform::system::file_icon_png(path).filter(|png| !png.is_empty())?;
            Some(Arc::new(Image::from_bytes(ImageFormat::Png, png)))
        })
        .clone()
}

pub(super) fn remember(bundle_id: &str, icon: Arc<Image>) {
    let _ = slot(bundle_id).set(Some(icon));
}
