//! Keyboard-layout lookup: which character a physical key prints on the current layout (JIS, ANSI, Dvorak, ...).

use std::ffi::c_void;

use core_foundation::base::{CFType, TCFType};
use core_foundation::data::{CFData, CFDataRef};
use core_foundation::string::CFStringRef;

use dito_core::i18n::Lang;

use crate::hotkey::Shortcut;

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    safe fn TISCopyCurrentKeyboardLayoutInputSource() -> *const c_void;
    fn TISGetInputSourceProperty(source: *const c_void, key: CFStringRef) -> *const c_void;
    safe static kTISPropertyUnicodeKeyLayoutData: CFStringRef;
    safe fn LMGetKbdType() -> u8;
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
}

const KEY_ACTION_DISPLAY: u16 = 3;
const NO_DEAD_KEYS: u32 = 1;

fn current_layout_source() -> Option<CFType> {
    let source = TISCopyCurrentKeyboardLayoutInputSource();
    // SAFETY: the Copy function returns a +1 retained input source or null; ownership passes to the wrapper.
    (!source.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(source) })
}

fn layout_data(source: &CFType) -> Option<CFData> {
    // SAFETY: `source` is a live input source and the key constant is a valid CFString.
    let data = unsafe { TISGetInputSourceProperty(source.as_CFTypeRef(), kTISPropertyUnicodeKeyLayoutData) };
    // SAFETY: the property is owned by `source` (get rule); retaining it keeps it valid on its own.
    (!data.is_null()).then(|| unsafe { CFData::wrap_under_get_rule(data as CFDataRef) })
}

fn translate(layout: &CFData, keycode: u16) -> Option<String> {
    let mut dead = 0u32;
    let mut len = 0usize;
    let mut buf = [0u16; 4];
    // SAFETY: `layout` holds a `UCKeyboardLayout` that stays alive for the call; `dead`, `len` and `buf` are valid
    // for the lengths passed (`buf.len()` UTF-16 units of output).
    let status = unsafe {
        UCKeyTranslate(
            layout.bytes().as_ptr(),
            keycode,
            KEY_ACTION_DISPLAY,
            0,
            u32::from(LMGetKbdType()),
            NO_DEAD_KEYS,
            &mut dead,
            buf.len(),
            &mut len,
            buf.as_mut_ptr(),
        )
    };
    (status == 0 && len > 0).then(|| String::from_utf16_lossy(&buf[..len]))
}

/// The character `keycode` prints with no modifiers on the current layout. Call from the main thread only
/// (the Text Input Sources API asserts on it).
pub fn layout_char(keycode: u16) -> Option<String> {
    let source = current_layout_source()?;
    let printed = translate(&layout_data(&source)?, keycode)?;
    (!printed.trim().is_empty() && !printed.chars().any(char::is_control)).then_some(printed)
}

/// Shortcut label using the real keyboard layout for character keys.
pub fn label(shortcut: &Shortcut, lang: Lang) -> String {
    shortcut.label_with(lang, layout_char)
}
