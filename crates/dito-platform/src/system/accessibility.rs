use objc2_app_kit::NSWorkspace;

/// "Reduce motion" in System Settings, Accessibility: Liquid Glass drops its elastic behaviour then, so do we.
pub fn reduce_motion() -> bool {
    NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}
