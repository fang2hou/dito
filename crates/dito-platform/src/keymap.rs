//! Keyboard-layout lookup: which character a physical key prints on the current layout (JIS, ANSI, Dvorak, ...).

use std::ffi::c_void;

use core_foundation::base::TCFType;
use core_foundation::data::CFData;
use core_foundation::string::CFStringRef;

use dito_core::i18n::Lang;

use crate::hotkey::Shortcut;

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn TISCopyCurrentKeyboardLayoutInputSource() -> *const c_void;
    fn TISGetInputSourceProperty(source: *const c_void, key: CFStringRef) -> *const c_void;
    static kTISPropertyUnicodeKeyLayoutData: CFStringRef;
    fn LMGetKbdType() -> u8;
    fn UCKeyTranslate(
        layout: *const u8,
        virtual_key: u16,
        action: u16,
        modifier_state: u32,
        keyboard_type: u32,
        options: u32,
        dead_key_state: *mut u32,
        max_len: usize,
        actual_len: *mut usize,
        out: *mut u16,
    ) -> i32;
    fn CFRelease(cf: *const c_void);
}

const KEY_ACTION_DISPLAY: u16 = 3;
const NO_DEAD_KEYS: u32 = 1;

/// The character `keycode` prints with no modifiers on the current layout. Call from the main thread only
/// (the Text Input Sources API asserts on it).
pub fn layout_char(keycode: u16) -> Option<String> {
    // SAFETY: the input source is released before returning; `data` borrows from it and is not used afterwards.
    unsafe {
        let source = TISCopyCurrentKeyboardLayoutInputSource();
        if source.is_null() {
            return None;
        }
        let result = (|| {
            let data_ref = TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData);
            if data_ref.is_null() {
                return None;
            }
            let data = CFData::wrap_under_get_rule(data_ref as _);
            let mut dead = 0u32;
            let mut len = 0usize;
            let mut buf = [0u16; 4];
            let status = UCKeyTranslate(
                data.bytes().as_ptr(),
                keycode,
                KEY_ACTION_DISPLAY,
                0,
                u32::from(LMGetKbdType()),
                NO_DEAD_KEYS,
                &mut dead,
                buf.len(),
                &mut len,
                buf.as_mut_ptr(),
            );
            (status == 0 && len > 0).then(|| String::from_utf16_lossy(&buf[..len]))
        })();
        CFRelease(source);
        result.filter(|s| !s.trim().is_empty() && !s.chars().any(char::is_control))
    }
}

/// Shortcut label using the real keyboard layout for character keys.
pub fn label(shortcut: &Shortcut, lang: Lang) -> String {
    shortcut.label_with(lang, layout_char)
}
