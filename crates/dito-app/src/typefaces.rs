use dito_core::i18n::Lang;

pub fn set_ui_language(lang: Lang) {
    let (cjk, primary) = match lang {
        Lang::Zh => ("IBM Plex Sans SC", "IBM Plex Sans SC"),
        Lang::ZhHant => ("PingFang TC", "PingFang TC"),
        Lang::Ko => ("M PLUS 1p", "Apple SD Gothic Neo"),
        Lang::Mn => ("M PLUS 1p", "Helvetica Neue"),
        Lang::Ja | Lang::En | Lang::Fr => ("M PLUS 1p", "M PLUS 1p"),
    };
    super::typography::set_ui_families(cjk, primary);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_typefaces_match_the_design() {
        let cases = [
            (Lang::En, "Outfit", "Helvetica Neue", "Apple SD Gothic Neo", "M PLUS 1p", "M PLUS 1p"),
            (Lang::Ja, "Outfit", "Helvetica Neue", "Apple SD Gothic Neo", "M PLUS 1p", "M PLUS 1p"),
            (Lang::Zh, "Outfit", "Helvetica Neue", "Apple SD Gothic Neo", "IBM Plex Sans SC", "IBM Plex Sans SC"),
            (Lang::ZhHant, "Outfit", "Helvetica Neue", "Apple SD Gothic Neo", "PingFang TC", "PingFang TC"),
            (Lang::Ko, "Outfit", "Helvetica Neue", "Apple SD Gothic Neo", "M PLUS 1p", "Apple SD Gothic Neo"),
            (Lang::Mn, "Outfit", "Helvetica Neue", "Apple SD Gothic Neo", "M PLUS 1p", "Helvetica Neue"),
            (Lang::Fr, "Outfit", "Helvetica Neue", "Apple SD Gothic Neo", "M PLUS 1p", "M PLUS 1p"),
        ];

        for (lang, latin, cyrillic, hangul, cjk, primary) in cases {
            set_ui_language(lang);
            assert_eq!(crate::ui::family_for("Latin"), latin, "{lang:?}");
            assert_eq!(crate::ui::family_for("\u{041a}\u{0438}\u{0440}"), cyrillic, "{lang:?}");
            assert_eq!(crate::ui::family_for("\u{d55c}\u{ae00}"), hangul, "{lang:?}");
            assert_eq!(crate::ui::family_for("\u{6f22}\u{5b57}"), cjk, "{lang:?}");
            assert_eq!(crate::ui::cjk_family(), primary, "{lang:?}");
        }
    }
}
