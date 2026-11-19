use dito_core::i18n::Tr;
use gpui_kit::SharedString;

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

pub fn error_line(lang: dito_core::i18n::Lang, line: &str) -> String {
    match dito_core::error::ErrorKind::decode(line) {
        Some((kind, detail)) => format!("{} ({detail})", error_summary(lang, kind)),
        None => line.to_string(),
    }
}

fn other_label(lang: dito_core::i18n::Lang) -> SharedString {
    SharedString::new_static(lang.tr(dito_core::i18n::Tr::PromptsOther))
}

/// Entries recorded before the app was configured, or under another bundle id, keep only the recorded name as their
/// link to the app.
fn configured_name<'a>(prompts: &'a dito_core::prompts::Prompts, app: &str) -> Option<&'a String> {
    if app.is_empty() {
        return None;
    }
    prompts.applications.values().find(|name| name.as_str() == app)
}

/// An app no longer in the list reads as Other; adding it back restores the name without touching history.
pub fn history_target_label(
    lang: dito_core::i18n::Lang,
    prompts: &dito_core::prompts::Prompts,
    target: &dito_core::prompts::AppTarget,
    app: &str,
) -> SharedString {
    use dito_core::prompts::AppTarget;
    match target {
        AppTarget::Builtin(_) if prompts.target_enabled(target) => SharedString::from(target.label().to_owned()),
        AppTarget::Builtin(_) => other_label(lang),
        AppTarget::Application(id) => match prompts.applications.get(id) {
            Some(configured) if app.is_empty() => configured.clone().into(),
            Some(_) => app.to_owned().into(),
            None => configured_name(prompts, app).map(|name| name.clone().into()).unwrap_or_else(|| other_label(lang)),
        },
        AppTarget::General => {
            configured_name(prompts, app).map(|name| name.clone().into()).unwrap_or_else(|| other_label(lang))
        }
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
