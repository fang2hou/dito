//! Resolve installed macOS applications and their native icons.

use std::path::{Path, PathBuf};

use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSWorkspace};
use objc2_foundation::{NSBundle, NSDictionary, NSSize, NSString, ns_string};

#[derive(Clone)]
pub struct InstalledApplication {
    pub bundle_id: String,
    pub name: String,
    pub path: PathBuf,
    pub icon_png: Vec<u8>,
}

pub fn installed_application(bundle_id: &str) -> Option<InstalledApplication> {
    let url = NSWorkspace::sharedWorkspace().URLForApplicationWithBundleIdentifier(&NSString::from_str(bundle_id))?;
    application_at(Path::new(&url.path()?.to_string()))
}

pub fn application_at(path: &Path) -> Option<InstalledApplication> {
    if path.extension()?.to_str()? != "app" {
        return None;
    }
    let path_string = NSString::from_str(path.to_str()?);
    let bundle = NSBundle::bundleWithPath(&path_string)?;
    let bundle_id = bundle.bundleIdentifier()?.to_string();
    if bundle_id.is_empty() {
        return None;
    }
    let name = [ns_string!("CFBundleDisplayName"), ns_string!("CFBundleName")]
        .into_iter()
        .find_map(|key| bundle.objectForInfoDictionaryKey(key)?.downcast::<NSString>().ok().map(|s| s.to_string()))
        .unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().into_owned());
    let icon = NSWorkspace::sharedWorkspace().iconForFile(&path_string);
    icon.setSize(NSSize::new(64., 64.));
    let icon_png = icon
        .TIFFRepresentation()
        .and_then(|tiff| NSBitmapImageRep::imageRepWithData(&tiff))
        .and_then(|bitmap| {
            // SAFETY: an empty property dictionary has no incorrectly typed values.
            unsafe { bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) }
        })
        .map(|data| data.to_vec())
        .unwrap_or_default();
    Some(InstalledApplication { bundle_id, name, path: path.to_owned(), icon_png })
}
