use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::{NSActivityOptions, NSObjectProtocol, NSProcessInfo, NSString};

/// A held process activity (`beginActivity`, options `UserInteractive`). While one is alive macOS
/// keeps scheduling this process at interactive rates: without it a background app (the HUD is
/// never the key window, dictation runs while the user types elsewhere) gets App-Napped and the
/// window server throttles its frame commits, which showed as the HUD animating at a crawl.
/// Drop the guard to end the activity.
pub struct NapGuard {
    token: Option<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
}

impl NapGuard {
    pub fn new(reason: &str) -> Self {
        let token = NSProcessInfo::processInfo()
            .beginActivityWithOptions_reason(NSActivityOptions::UserInteractive, &NSString::from_str(reason));
        Self { token: Some(token) }
    }
}

impl Drop for NapGuard {
    fn drop(&mut self) {
        if let Some(token) = self.token.take() {
            // SAFETY: the token came from `beginActivityWithOptions:reason:` and is ended once.
            unsafe { NSProcessInfo::processInfo().endActivity(&token) };
        }
    }
}
