use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{NSApplication, NSImage};
use objc2_foundation::{NSBundle, NSData, ns_string};

/// When Info.plist names an icon (`CFBundleIconName`), the system draws the Liquid Glass icon itself, in every
/// appearance.
pub fn has_bundle_icon() -> bool {
    let bundle = NSBundle::mainBundle();
    bundle.bundlePath().to_string().ends_with(".app")
        && bundle.objectForInfoDictionaryKey(ns_string!("CFBundleIconName")).is_some()
}

/// For an unbundled run (`cargo run`), where there is no Info.plist to name an icon. A bundled app must never set
/// one: an image set at runtime replaces the system's Liquid Glass rendering of the icon with a flat picture (no
/// lighting, no light/dark/clear/tinted variants). So it does nothing then.
pub fn set_app_icon(png: &[u8]) {
    if has_bundle_icon() {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else { return };
    let data = NSData::with_bytes(png);
    if let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) {
        // SAFETY: main thread (checked above) and a valid image.
        unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image)) };
    }
}
