include!(concat!(env!("OUT_DIR"), "/i18n_gen.rs"));

#[cfg(test)]
mod tests {
    use crate::i18n::Lang;

    use super::*;

    #[test]
    fn formatting_functions_interpolate_every_language() {
        for lang in Lang::ALL {
            assert!(fmt::using_model(lang, "Qwen").contains("Qwen"), "{:?} dropped the model", lang);
            assert!(fmt::about_seconds_left(lang, "9").contains('9'), "{:?} dropped n", lang);
        }
    }
}
