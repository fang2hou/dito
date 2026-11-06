use dito_core::i18n::Lang;
use objc2_foundation::NSLocale;

/// The first language in the user's macOS preference list that Dito has a UI for; English otherwise.
pub fn system_language() -> Lang {
    NSLocale::preferredLanguages().iter().find_map(|tag| Lang::from_tag(&tag.to_string())).unwrap_or(Lang::En)
}
