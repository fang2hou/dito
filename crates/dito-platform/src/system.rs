//! Small OS integrations: frontmost app, keychain, sounds, audio playback, login item.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Mutex;

use dito_core::controller::{AppInfo, Frontmost};
use dito_core::secrets::SecretStore;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{NSApplication, NSImage, NSStatusWindowLevel, NSView, NSWindowLevel, NSWorkspace};
use objc2_foundation::{NSBundle, NSData};
use objc2_foundation::{NSLocale, ns_string};

pub struct MacFrontmost;

impl Frontmost for MacFrontmost {
    fn frontmost(&self) -> AppInfo {
        let app = NSWorkspace::sharedWorkspace().frontmostApplication();
        match app {
            Some(app) => AppInfo {
                name: app.localizedName().map(|s| s.to_string()).unwrap_or_default(),
                bundle_id: app.bundleIdentifier().map(|s| s.to_string()).unwrap_or_default(),
            },
            None => AppInfo::default(),
        }
    }
}

/// API keys in the login keychain (generic passwords under one service name).
pub struct KeychainSecrets {
    service: String,
}

impl KeychainSecrets {
    pub fn new() -> Self {
        Self::with_service("com.fang2hou.dito")
    }

    pub fn with_service(service: &str) -> Self {
        Self { service: service.into() }
    }
}

impl Default for KeychainSecrets {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretStore for KeychainSecrets {
    fn get(&self, account: &str) -> Option<String> {
        security_framework::passwords::get_generic_password(&self.service, account)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
    }

    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        security_framework::passwords::set_generic_password(&self.service, account, value.as_bytes())
            .map_err(|e| e.to_string())
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        match security_framework::passwords::delete_generic_password(&self.service, account) {
            Ok(()) => Ok(()),
            // Deleting what is not there is fine.
            Err(e) if e.code() == -25300 => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// Plays one recording at a time (for 履歴 → 音声を再生).
#[derive(Default)]
pub struct Player(Mutex<Option<Child>>);

impl Player {
    pub fn play(&self, path: &Path) {
        let mut cur = self.0.lock().unwrap();
        if let Some(mut c) = cur.take() {
            let _ = c.kill();
        }
        *cur = Command::new("afplay").arg(path).spawn().ok();
    }

    pub fn stop(&self) {
        if let Some(mut c) = self.0.lock().unwrap().take() {
            let _ = c.kill();
        }
    }
}

const AGENT_LABEL: &str = "com.fang2hou.dito";

fn agent_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/LaunchAgents").join(format!("{AGENT_LABEL}.plist")))
}

/// Register or remove a LaunchAgent that starts this executable at login.
pub fn set_launch_at_login(enabled: bool) -> std::io::Result<()> {
    let Some(path) = agent_path() else { return Ok(()) };
    if !enabled {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    let exe = std::env::current_exe()?;
    let plist = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n  <key>Label</key><string>{AGENT_LABEL}</string>\n  <key>ProgramArguments</key><array><string>{}</string></array>\n  <key>RunAtLoad</key><true/>\n</dict>\n</plist>\n",
        exe.display()
    );
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, plist)
}

pub fn launch_at_login_enabled() -> bool {
    agent_path().is_some_and(|p| p.exists())
}

/// Installed RAM in bytes (0 when unknown).
/// The first language in the user's macOS preference list that Dito has a UI for; English otherwise.
pub fn system_language() -> dito_core::i18n::Lang {
    NSLocale::preferredLanguages()
        .iter()
        .find_map(|tag| dito_core::i18n::Lang::from_tag(&tag.to_string()))
        .unwrap_or(dito_core::i18n::Lang::En)
}

/// Whether this process runs from a `.app` whose Info.plist names an icon (`CFBundleIconName`, the Icon Composer
/// document compiled into `Assets.car`). Then the system draws the Liquid Glass icon itself, in every appearance.
pub fn has_bundle_icon() -> bool {
    let bundle = NSBundle::mainBundle();
    bundle.bundlePath().to_string().ends_with(".app")
        && bundle.objectForInfoDictionaryKey(ns_string!("CFBundleIconName")).is_some()
}

/// Use `png` as the Dock icon for an unbundled run (`cargo run`), where there is no Info.plist to name an icon.
///
/// A bundled app must never call this: an image set at runtime replaces the system's Liquid Glass rendering of the
/// icon with a flat picture (no lighting, no light/dark/clear/tinted variants). So it does nothing then.
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

/// Turn off the drop shadow macOS gives every window, so a transparent window shows only what is drawn in it.
///
/// # Safety
/// `ns_view` must point to a live `NSView` (from a raw window handle) and the call must be on the main thread.
pub unsafe fn remove_window_shadow(ns_view: std::ptr::NonNull<std::ffi::c_void>) {
    // SAFETY: guaranteed by the caller.
    let view: &NSView = unsafe { ns_view.cast::<NSView>().as_ref() };
    if let Some(window) = view.window() {
        window.setHasShadow(false);
        window.invalidateShadow();
    }
}

/// Raise the window to the macOS status level, above every application's normal windows and
/// floating panels but below menus and tooltips, so it stays visible while the user works in
/// other apps. Returns the level AppKit reports afterwards, or `None` if the view has no window.
///
/// gpui has no `WindowOptions` field for the level; its `WindowKind::Floating` also drops the
/// mouse tracking area it installs for normal windows, which an unfocused window needs for hover.
///
/// # Safety
/// `ns_view` must point to a live `NSView` (from a raw window handle) and the call must be on the main thread.
pub unsafe fn set_status_window_level(ns_view: std::ptr::NonNull<std::ffi::c_void>) -> Option<NSWindowLevel> {
    // SAFETY: guaranteed by the caller.
    let view: &NSView = unsafe { ns_view.cast::<NSView>().as_ref() };
    view.window().map(|window| {
        window.setLevel(NSStatusWindowLevel);
        window.level()
    })
}

/// Keep a non-activating HUD panel from becoming the key window when its controls are clicked.
///
/// # Safety
/// `ns_view` must be a live NSView belonging to an NSPanel, on the main thread.
pub unsafe fn configure_nonactivating_panel(ns_view: std::ptr::NonNull<std::ffi::c_void>) {
    use objc2::msg_send;
    let view: &NSView = unsafe { ns_view.cast::<NSView>().as_ref() };
    if let Some(window) = view.window() {
        // SAFETY: the caller ensures this view belongs to the HUD's NSPanel.
        unsafe {
            let _: () = msg_send![&*window, setBecomesKeyOnlyIfNeeded: true];
        }
    }
}

/// Pointer in view coordinates, even while the window ignores mouse events.
///
/// # Safety
/// The view must be live and this function must run on the main thread.
pub unsafe fn hud_pointer(ns_view: std::ptr::NonNull<std::ffi::c_void>) -> Option<(f32, f32, bool)> {
    use objc2::{class, msg_send};
    let view: &NSView = unsafe { ns_view.cast::<NSView>().as_ref() };
    let window = view.window()?;
    let p = window.mouseLocationOutsideOfEventStream();
    let bounds = view.bounds();
    let buttons: usize = unsafe { msg_send![class!(NSEvent), pressedMouseButtons] };
    Some((p.x as f32, (bounds.size.height - p.y) as f32, buttons != 0))
}

/// # Safety
/// The view must be live and this function must run on the main thread.
pub unsafe fn hud_set_click_through(ns_view: std::ptr::NonNull<std::ffi::c_void>, through: bool) {
    let view: &NSView = unsafe { ns_view.cast::<NSView>().as_ref() };
    if let Some(window) = view.window()
        && window.ignoresMouseEvents() != through
    {
        window.setIgnoresMouseEvents(through);
    }
}

/// State of the continuous pointer-follow spring, retained for the lifetime of the HUD.
#[derive(Default)]
pub struct HudFollowState {
    position: Option<(f64, f64)>,
    velocity: (f64, f64),
    cursor: Option<(f64, f64)>,
    last_tick: Option<std::time::Instant>,
    frozen: bool,
}

/// Exact underdamped spring step with a target held constant during this frame.
fn follow_axis(position: f64, velocity: f64, target: f64, dt: f64, reduced: bool) -> (f64, f64) {
    if dt <= 0. {
        return (position, velocity);
    }
    if dt > 0.25 {
        return (target, 0.);
    }
    if reduced {
        let remaining = (-24. * dt).exp();
        return (target + (position - target) * remaining, 0.);
    }
    let (omega, zeta) = (18.0_f64, 0.9_f64);
    let decay = zeta * omega;
    let wd = omega * (1. - zeta * zeta).sqrt();
    let d = position - target;
    let b = (velocity + decay * d) / wd;
    let (sin, cos) = (wd * dt).sin_cos();
    let e = (-decay * dt).exp();
    let wave = d * cos + b * sin;
    let pos = target + e * wave;
    let vel = e * (wd * (b * cos - d * sin) - decay * wave);
    (pos, vel)
}

fn freeze_for_interaction(frozen: bool, pressed: bool, distance: f64, movement_toward_capsule: f64) -> bool {
    if pressed || (!frozen && distance < 64. && movement_toward_capsule > 0.1) {
        true
    } else {
        frozen && distance <= 80.
    }
}

/// Returns the target and limits for the window origin in AppKit's global coordinates.
fn pointer_target(
    cursor: (f64, f64),
    window_width: f64,
    visible: (f64, f64, f64, f64),
) -> ((f64, f64), (f64, f64, f64, f64)) {
    let (vx, vy, vw, vh) = visible;
    let x_min = vx;
    let x_max = (vx + vw - window_width).max(x_min);
    let y_min = vy - 15.;
    let y_max = (vy + vh - 112.).max(y_min);
    let x = (cursor.0 - window_width / 2.).clamp(x_min, x_max);
    let above = cursor.1 + 5.; // countdown bottom = cursor + 20 pt
    let y = if above <= y_max { above } else { cursor.1 - 132. }.clamp(y_min, y_max);
    ((x, y), (x_min, x_max, y_min, y_max))
}

/// Move the HUD with a continuous spring while allowing the pointer to approach and operate it.
/// # Safety
/// The view must be live and this function must run on the main thread.
pub unsafe fn hud_follow_pointer(
    ns_view: std::ptr::NonNull<std::ffi::c_void>,
    state: &mut HudFollowState,
    paused: bool,
    lower_error_bubble: bool,
) {
    use objc2::{class, msg_send};
    use objc2_foundation::NSPoint;
    let view: &NSView = unsafe { ns_view.cast::<NSView>().as_ref() };
    let Some(window) = view.window() else { return };
    let cursor: NSPoint = unsafe { msg_send![class!(NSEvent), mouseLocation] };
    let frame = window.frame();
    let Some(mtm) = MainThreadMarker::new() else { return };
    let screen = objc2_app_kit::NSScreen::screens(mtm)
        .into_iter()
        .find(|screen| {
            let r = screen.frame();
            cursor.x >= r.origin.x
                && cursor.x < r.origin.x + r.size.width
                && cursor.y >= r.origin.y
                && cursor.y < r.origin.y + r.size.height
        })
        .or_else(|| window.screen());
    let Some(screen) = screen else { return };
    let mut visible = screen.visibleFrame();
    if lower_error_bubble {
        // Reserve 12 additional points beneath the existing countdown footprint for
        // the content-hugging error reason, its shadow and spring overshoot.
        visible.origin.y += 12.;
        visible.size.height = (visible.size.height - 12.).max(0.);
    }
    // The visible footprint is 15..112 pt from the window bottom, including the
    // reserved countdown and upper controls. Empty transparent window space may extend offscreen.
    let ((target_x, target_y), (x_min, x_max, y_min, y_max)) = pointer_target(
        (cursor.x, cursor.y),
        frame.size.width,
        (visible.origin.x, visible.origin.y, visible.size.width, visible.size.height),
    );
    let mut current = state.position.unwrap_or((frame.origin.x, frame.origin.y));
    if lower_error_bubble && current.1 < y_min {
        current.1 = y_min;
        state.position = Some(current);
        window.setFrameOrigin(NSPoint::new(current.0, current.1));
    }
    let capsule = (current.0 + frame.size.width / 2., current.1 + 61.5);
    let dx = ((cursor.x - capsule.0).abs() - 52.5).max(0.);
    let dy = ((cursor.y - capsule.1).abs() - 16.5).max(0.);
    let distance = dx.hypot(dy);
    let approach = state
        .cursor
        .map_or(0., |old| (cursor.x - old.0) * (capsule.0 - cursor.x) + (cursor.y - old.1) * (capsule.1 - cursor.y));
    state.frozen = freeze_for_interaction(state.frozen, paused, distance, approach);
    state.cursor = Some((cursor.x, cursor.y));
    let now = std::time::Instant::now();
    let dt = state.last_tick.replace(now).map_or(0., |last| now.duration_since(last).as_secs_f64());
    if state.frozen {
        state.velocity = (0., 0.);
        state.position = Some(current);
        return;
    }
    if state.position.is_none() {
        state.position = Some((target_x, target_y));
    }
    let position = state.position.unwrap();
    let (mut x, mut vx) = follow_axis(position.0, state.velocity.0, target_x, dt, reduce_motion());
    let (mut y, mut vy) = follow_axis(position.1, state.velocity.1, target_y, dt, reduce_motion());
    if (x - target_x).abs() < 0.02 && vx.abs() < 0.1 {
        x = target_x;
        vx = 0.;
    }
    if (y - target_y).abs() < 0.02 && vy.abs() < 0.1 {
        y = target_y;
        vy = 0.;
    }
    let next = (x.clamp(x_min, x_max), y.clamp(y_min, y_max));
    state.position = Some(next);
    state.velocity = (if x == next.0 { vx } else { 0. }, if y == next.1 { vy } else { 0. });
    if (frame.origin.x - next.0).abs() > 0.001 || (frame.origin.y - next.1).abs() > 0.001 {
        window.setFrameOrigin(NSPoint::new(next.0, next.1));
    }
}

/// # Safety
/// The view must be live and this function must run on the main thread.
pub unsafe fn hud_center_above_dock(ns_view: std::ptr::NonNull<std::ffi::c_void>, clearance: i32) {
    use objc2_foundation::NSPoint;
    let view: &NSView = unsafe { ns_view.cast::<NSView>().as_ref() };
    let Some(window) = view.window() else { return };
    let Some(screen) = window.screen() else { return };
    let screen_frame = screen.frame();
    let visible = screen.visibleFrame();
    let frame = window.frame();
    let x = screen_frame.origin.x + (screen_frame.size.width - frame.size.width) / 2.;
    let y = (visible.origin.y + f64::from(clearance) - 15.)
        .min(visible.origin.y + (visible.size.height - frame.size.height).max(0.));
    window.setFrameOrigin(NSPoint::new(x, y));
}

/// "Reduce motion" in System Settings → Accessibility: Liquid Glass drops its elastic behaviour then, so do we.
pub fn reduce_motion() -> bool {
    use objc2_app_kit::NSWorkspace;
    NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}

/// A held process activity (`beginActivity`, options `UserInteractive`). While one is alive macOS
/// keeps scheduling this process at interactive rates: without it a background app — the HUD is
/// never the key window, dictation runs while the user types elsewhere — gets App-Napped and the
/// window server throttles its frame commits, which showed as the HUD animating at a crawl.
/// Drop the guard to end the activity.
pub struct NapGuard {
    token: Option<objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2_foundation::NSObjectProtocol>>>,
}

impl NapGuard {
    pub fn new(reason: &str) -> Self {
        use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};
        let token = NSProcessInfo::processInfo()
            .beginActivityWithOptions_reason(NSActivityOptions::UserInteractive, &NSString::from_str(reason));
        Self { token: Some(token) }
    }
}

impl Drop for NapGuard {
    fn drop(&mut self) {
        use objc2_foundation::NSProcessInfo;
        if let Some(token) = self.token.take() {
            // SAFETY: the token came from `beginActivityWithOptions:reason:` and is ended once.
            unsafe { NSProcessInfo::processInfo().endActivity(&token) };
        }
    }
}

pub fn physical_memory_bytes() -> u64 {
    let mut mem: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    // SAFETY: the name is NUL-terminated and `mem`/`len` describe a valid u64 buffer.
    let rc = unsafe {
        libc::sysctlbyname(c"hw.memsize".as_ptr(), (&mut mem as *mut u64).cast(), &mut len, std::ptr::null_mut(), 0)
    };
    if rc == 0 { mem } else { 0 }
}

/// Seconds east of UTC for the local time zone, from `localtime_r` (safe to call from any thread).
pub fn local_utc_offset_secs() -> i32 {
    // SAFETY: `now` and `tm` are valid for the duration of the calls; localtime_r is re-entrant.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() { 0 } else { tm.tm_gmtoff as i32 }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn pointer_spring_is_frame_rate_independent_and_handles_sleep() {
        let once = super::follow_axis(0., 0., 100., 0.1, false);
        let first = super::follow_axis(0., 0., 100., 0.04, false);
        let split = super::follow_axis(first.0, first.1, 100., 0.06, false);
        assert!((once.0 - split.0).abs() < 0.0001);
        assert!((once.1 - split.1).abs() < 0.0001);
        assert!(once.0 > 0. && once.0 < 100.);
        assert_eq!(super::follow_axis(0., 0., 100., 1., false), (100., 0.));
        let reduced = super::follow_axis(0., 0., 100., 0.1, true);
        assert!(reduced.0 > 0. && reduced.0 < 100.);
    }

    #[test]
    fn approaching_controls_freezes_until_the_pointer_leaves() {
        use super::freeze_for_interaction as freeze;
        assert!(!freeze(false, false, 50., -1.));
        assert!(freeze(false, false, 50., 2.));
        assert!(freeze(true, false, 70., -2.));
        assert!(!freeze(true, false, 81., -2.));
        assert!(freeze(false, true, 200., 0.));
    }

    #[test]
    fn pointer_target_reserves_the_countdown_and_flips_at_top() {
        let visible = (0., 0., 1000., 1000.);
        let (above, _) = super::pointer_target((500., 400.), 293., visible);
        assert_eq!(above.1 + 15., 420.);
        let (below, _) = super::pointer_target((500., 950.), 293., visible);
        assert_eq!(below.1 + 112., 930.);
        let (edge, limits) = super::pointer_target((0., 400.), 293., visible);
        assert_eq!(edge.0, limits.0);
    }
    #[test]
    fn memory_size_is_reported() {
        assert!(super::physical_memory_bytes() > 1 << 30);
    }

    #[test]
    fn utc_offset_is_a_plausible_time_zone() {
        let off = super::local_utc_offset_secs();
        assert!((-14 * 3600..=14 * 3600).contains(&off), "{off}");
    }
}
