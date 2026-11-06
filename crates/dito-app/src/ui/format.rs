use dito_core::i18n::Tr;
use gpui_kit::SharedString;

/// Tiny helper for "left label, right value" meta lines.
/// Localised name of a polish style: the generic one is "一般", the others are app names.
/// One-line, user-language description of a failure class.
pub fn error_summary(lang: dito_core::i18n::Lang, kind: dito_core::error::ErrorKind) -> &'static str {
    use dito_core::error::ErrorKind as K;
    let message = match kind {
        K::Auth => Tr::ErrorsAuthFailed,
        K::NoSpeech => Tr::ErrorsNoSpeech,
        K::Mic => Tr::ErrorsMicUnavailable,
        K::Network => Tr::ErrorsConnectFailed,
        K::Timeout => Tr::ErrorsNoResponse,
        K::RateLimit => Tr::ErrorsTooManyRequests,
        K::ModelNotFound => Tr::ErrorsModelNotFound,
        K::BadResponse => Tr::ErrorsBadResponse,
        K::Unavailable => Tr::StatusLocalStopped,
        K::Other => Tr::ErrorsGeneric,
    };
    lang.tr(message)
}

/// A stored or returned `Kind: detail` error line, with the kind in the user's language and the technical detail kept.
pub fn error_line(lang: dito_core::i18n::Lang, line: &str) -> String {
    match dito_core::error::ErrorKind::decode(line) {
        Some((kind, detail)) => format!("{} ({detail})", error_summary(lang, kind)),
        None => line.to_string(),
    }
}

pub fn style_label(lang: dito_core::i18n::Lang, style: dito_core::prompts::Style) -> SharedString {
    match style {
        dito_core::prompts::Style::General | dito_core::prompts::Style::Application(_) => {
            SharedString::from(lang.tr(dito_core::i18n::Tr::PromptsOther))
        }
        other => SharedString::from(other.label().to_owned()),
    }
}

pub fn fmt_bytes(n: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let n = n as f64;
    if n >= 1000.0 * MB { format!("{:.1} GB", n / (1024.0 * MB)) } else { format!("{:.0} MB", n / MB) }
}

pub fn fmt_eta(lang: dito_core::i18n::Lang, secs: u64) -> String {
    if secs >= 90 {
        dito_core::i18n::fmt::about_minutes_left(lang, &secs.div_ceil(60).to_string())
    } else {
        dito_core::i18n::fmt::about_seconds_left(lang, &secs.max(1).to_string())
    }
}
