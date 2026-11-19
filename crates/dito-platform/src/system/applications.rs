//! Resolve installed macOS applications and their native icons.

use std::path::{Path, PathBuf};

use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSWorkspace};
use objc2_foundation::{NSBundle, NSDictionary, NSSize, NSString, ns_string};

use crate::with_autorelease_pool;

#[derive(Clone)]
pub struct InstalledApplication {
    pub bundle_id: String,
    pub name: String,
    pub path: PathBuf,
    pub icon_png: Vec<u8>,
}

impl InstalledApplication {
    /// The app with `bundle_id`, its `icon_png` left empty. Encoding an icon takes about 100 ms, so a caller that
    /// lists apps resolves them here and fetches each icon off the main thread with [`file_icon_png`].
    pub fn without_icon(bundle_id: &str) -> Option<Self> {
        without_icon_at(&bundle_path(bundle_id)?)
    }
}

pub fn installed_application(bundle_id: &str) -> Option<InstalledApplication> {
    application_at(&bundle_path(bundle_id)?)
}

pub fn application_at(path: &Path) -> Option<InstalledApplication> {
    let mut app = without_icon_at(path)?;
    app.icon_png = file_icon_png(path).unwrap_or_default();
    Some(app)
}

fn bundle_path(bundle_id: &str) -> Option<PathBuf> {
    let url = NSWorkspace::sharedWorkspace().URLForApplicationWithBundleIdentifier(&NSString::from_str(bundle_id))?;
    Some(PathBuf::from(url.path()?.to_string()))
}

fn without_icon_at(path: &Path) -> Option<InstalledApplication> {
    if path.extension()?.to_str()? != "app" {
        return None;
    }
    let bundle = NSBundle::bundleWithPath(&NSString::from_str(path.to_str()?))?;
    let bundle_id = bundle.bundleIdentifier()?.to_string();
    if bundle_id.is_empty() {
        return None;
    }
    let name = [ns_string!("CFBundleDisplayName"), ns_string!("CFBundleName")]
        .into_iter()
        .find_map(|key| bundle.objectForInfoDictionaryKey(key)?.downcast::<NSString>().ok().map(|s| s.to_string()))
        .unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().into_owned());
    Some(InstalledApplication { bundle_id, name, path: path.to_owned(), icon_png: Vec::new() })
}

/// The icon Finder shows for the file at `path`, as PNG.
pub fn file_icon_png(path: &Path) -> Option<Vec<u8>> {
    icon_png(&NSString::from_str(path.to_str()?))
}

fn icon_png(path: &NSString) -> Option<Vec<u8>> {
    // Also called on background queues, which drain their own pool at no set time. The TIFF holds every size of the
    // icon, about 70 MB, so free it before returning.
    with_autorelease_pool(|| {
        let icon = NSWorkspace::sharedWorkspace().iconForFile(path);
        icon.setSize(NSSize::new(64., 64.));
        let tiff = icon.TIFFRepresentation()?;
        let bitmap = NSBitmapImageRep::imageRepWithData(&tiff)?;
        // SAFETY: an empty property dictionary has no incorrectly typed values.
        let png =
            unsafe { bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) };
        png.map(|data| data.to_vec())
    })
}
