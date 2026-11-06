use std::sync::atomic::{AtomicU8, Ordering};

use super::Lang;

const UNSET: u8 = u8::MAX;
static SYSTEM: AtomicU8 = AtomicU8::new(UNSET);

/// Tell the UI which language the operating system prefers (the platform layer calls this at startup).
pub fn set_system_language(lang: Lang) {
    SYSTEM.store(Lang::ALL.iter().position(|l| *l == lang).unwrap_or(1) as u8, Ordering::Relaxed);
}

/// The system language, English when it is unknown or unsupported.
pub fn system_language() -> Lang {
    match SYSTEM.load(Ordering::Relaxed) {
        UNSET => ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|k| std::env::var(k).ok())
            .find_map(|v| Lang::from_tag(&v))
            .unwrap_or(Lang::En),
        i => Lang::ALL[i as usize],
    }
}
