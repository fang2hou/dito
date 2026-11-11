//! macOS privacy permissions used by Dito.

use std::process::Command;

use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    Microphone,
    Accessibility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Granted,
    Denied,
    NotDetermined,
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    safe fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: core_foundation::dictionary::CFDictionaryRef) -> bool;
}

/// Lists the app under Privacy & Security, Accessibility (switched off) and shows the system's prompt. Returns
/// whether the app is already trusted.
fn prompt_accessibility() -> bool {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::string::CFString;
    let key = CFString::from_static_string("AXTrustedCheckOptionPrompt");
    let options = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), CFBoolean::true_value().as_CFType())]);
    // SAFETY: `options` is a valid CFDictionary that lives for the duration of the call.
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
}

pub fn status(p: Permission) -> Status {
    match p {
        Permission::Accessibility => {
            if AXIsProcessTrusted() {
                Status::Granted
            } else {
                Status::Denied
            }
        }
        Permission::Microphone => {
            // SAFETY: AVMediaTypeAudio is a valid static NSString constant.
            let st = unsafe {
                AVCaptureDevice::authorizationStatusForMediaType(AVMediaTypeAudio.expect("AVMediaTypeAudio"))
            };
            match st {
                AVAuthorizationStatus::Authorized => Status::Granted,
                AVAuthorizationStatus::NotDetermined => Status::NotDetermined,
                _ => Status::Denied,
            }
        }
    }
}

/// - Microphone: the first request registers the app and shows the system prompt; once it has been answered the
///   only way left is the Settings pane.
/// - Accessibility: the system call registers the app in the list (switched off) and shows its prompt; the
///   Settings pane is opened as well, so the switch is one click away.
///
/// The app is listed by its bundle (or, for an unbundled `cargo run` binary, by the terminal that launched it).
pub fn request(p: Permission) {
    if status(p) == Status::Granted {
        return;
    }
    match p {
        Permission::Microphone => {
            if status(p) == Status::NotDetermined {
                let handler = RcBlock::new(|_granted: Bool| {});
                // SAFETY: the block outlives the call (the system copies it) and the media type constant is valid.
                unsafe {
                    AVCaptureDevice::requestAccessForMediaType_completionHandler(
                        AVMediaTypeAudio.expect("AVMediaTypeAudio"),
                        &handler,
                    )
                };
            } else {
                open_settings(p);
            }
        }
        Permission::Accessibility => {
            prompt_accessibility();
            open_settings(p);
        }
    }
}

pub fn open_settings(p: Permission) {
    let pane = match p {
        Permission::Microphone => "Privacy_Microphone",
        Permission::Accessibility => "Privacy_Accessibility",
    };
    let _ = Command::new("open").arg(format!("x-apple.systempreferences:com.apple.preference.security?{pane}")).spawn();
}
