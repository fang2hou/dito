//! macOS privacy permissions used by Dito.

use std::process::Command;

use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    Microphone,
    Accessibility,
    SystemAudio,
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
        Permission::SystemAudio => tcc::status(),
    }
}

/// - Microphone: the first request registers the app and shows the system prompt; once it has been answered the
///   only way left is the Settings pane.
/// - Accessibility: the system call registers the app in the list (switched off) and shows its prompt; the
///   Settings pane is opened as well, so the switch is one click away.
/// - System audio: TCC's request shows the recording prompt while the app is undecided; a decided app, or a build
///   whose TCC framework no longer exports the call, is sent to the Settings pane.
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
        Permission::SystemAudio => {
            let asked = status(p) == Status::NotDetermined && tcc::request();
            if !asked {
                open_settings(p);
            }
        }
    }
}

pub fn open_settings(p: Permission) {
    let pane = match p {
        Permission::Microphone => "Privacy_Microphone",
        Permission::Accessibility => "Privacy_Accessibility",
        Permission::SystemAudio => "Privacy_AudioCapture",
    };
    let _ = Command::new("open").arg(format!("x-apple.systempreferences:com.apple.preference.security?{pane}")).spawn();
}

/// System Audio Recording has no public API, so it is read and requested through TCC's private SPI (the approach
/// insidegui/AudioCap takes). Both entry points are resolved lazily from the framework; a missing framework or
/// symbol means the permission cannot be observed, which reads as not determined and sends the user to Settings.
mod tcc {
    use std::ffi::{CStr, c_char, c_int, c_void};
    use std::sync::LazyLock;

    use block2::{Block, RcBlock};
    use core_foundation::base::TCFType;
    use core_foundation::dictionary::CFDictionaryRef;
    use core_foundation::string::{CFString, CFStringRef};
    use objc2::runtime::Bool;

    use super::Status;

    const FRAMEWORK: &CStr = c"/System/Library/PrivateFrameworks/TCC.framework/Versions/A/TCC";
    const SERVICE: &str = "kTCCServiceAudioCapture";
    const GRANTED: c_int = 0;
    const DENIED: c_int = 1;

    type Preflight = unsafe extern "C" fn(service: CFStringRef, options: CFDictionaryRef) -> c_int;
    type Request =
        unsafe extern "C" fn(service: CFStringRef, options: CFDictionaryRef, completion: &Block<dyn Fn(Bool)>);

    struct Spi {
        preflight: Option<Preflight>,
        request: Option<Request>,
    }

    static SPI: LazyLock<Spi> = LazyLock::new(|| {
        // SAFETY: the handle stays open for the process; a null handle or symbol just leaves that call out.
        unsafe {
            let handle = libc::dlopen(FRAMEWORK.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
            if handle.is_null() {
                return Spi { preflight: None, request: None };
            }
            Spi {
                preflight: resolve::<Preflight>(handle, c"TCCAccessPreflight"),
                request: resolve::<Request>(handle, c"TCCAccessRequest"),
            }
        }
    });

    /// # Safety
    /// The symbol at `name` must have the layout of `T`.
    unsafe fn resolve<T: Copy>(handle: *mut c_void, name: &CStr) -> Option<T> {
        // SAFETY: `handle` came from a successful dlopen and `name` is NUL-terminated.
        let symbol = unsafe { libc::dlsym(handle, name.as_ptr() as *const c_char) };
        // SAFETY: the caller guarantees the symbol has the function layout `T`.
        (!symbol.is_null()).then(|| unsafe { std::mem::transmute_copy(&symbol) })
    }

    pub(super) fn status() -> Status {
        let Some(preflight) = SPI.preflight else {
            return Status::NotDetermined;
        };
        let service = CFString::from_static_string(SERVICE);
        // SAFETY: the service is a valid CFString and NULL options are documented as accepted.
        let answer = unsafe { preflight(service.as_concrete_TypeRef(), std::ptr::null()) };
        match answer {
            GRANTED => Status::Granted,
            DENIED => Status::Denied,
            // An undecided app, or a service this macOS build does not know.
            _ => Status::NotDetermined,
        }
    }

    /// Returns whether the prompt could be asked for; the answer arrives through the block, not as a return value.
    pub(super) fn request() -> bool {
        let Some(request) = SPI.request else {
            return false;
        };
        let service = CFString::from_static_string(SERVICE);
        let handler = RcBlock::new(|_granted: Bool| {});
        // SAFETY: the system copies the block, and NULL options are documented as accepted.
        unsafe { request(service.as_concrete_TypeRef(), std::ptr::null(), &handler) };
        true
    }
}
