//! The syntax of a message in `messages.toml`. `build.rs` includes this file with `#[path]`, so the rules are tested
//! where the build uses them.
//!
//! Copy under `fmt.` is a `format!` template: `{name}` is a placeholder and a literal brace is doubled. All other copy
//! is shown as written, so its braces are literal, except that a bare `{name}` is refused there.

pub fn placeholders(text: &str) -> Result<Vec<String>, String> {
    let mut found: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(['{', '}']) {
        let (brace, after) = (&rest[at..=at], &rest[at + 1..]);
        if after.starts_with(brace) {
            rest = &after[1..];
            continue;
        }
        if brace == "}" {
            return Err(format!("unmatched `}}` in {text:?}; write `}}}}` for a literal brace"));
        }
        let Some(close) = after.find('}') else {
            return Err(format!("unclosed `{{` in {text:?}; write `{{{{` for a literal brace"));
        };
        let name = &after[..close];
        if !is_identifier(name) {
            return Err(format!(
                "`{{{name}}}` in {text:?} is not a bare identifier (no positional `{{}}`, no format spec); write `{{{{` for a literal brace"
            ));
        }
        if !found.iter().any(|known| known == name) {
            found.push(name.to_owned());
        }
        rest = &after[close + 1..];
    }
    Ok(found)
}

pub fn plain(text: &str) -> Result<(), String> {
    let mut rest = text;
    while let Some(at) = rest.find('{') {
        let after = &rest[at + 1..];
        if let Some(close) = after.find('}')
            && is_identifier(&after[..close])
        {
            return Err(format!(
                "`{{{}}}` in {text:?} is a placeholder, so the message belongs under `fmt.`",
                &after[..close]
            ));
        }
        rest = after;
    }
    Ok(())
}

fn is_identifier(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(text: &str) -> Vec<String> {
        placeholders(text).unwrap()
    }

    #[test]
    fn placeholders_are_listed_once_in_order_of_appearance() {
        assert_eq!(names("{b} と {a}、{b}"), ["b", "a"]);
        assert_eq!(names("{line} 行目、{column} 列目：{detail}"), ["line", "column", "detail"]);
        assert!(names("no placeholders").is_empty());
    }

    #[test]
    fn a_doubled_brace_in_parameterized_copy_is_a_literal_one() {
        assert!(names(r#"{{"seed": 42}}"#).is_empty());
        assert_eq!(names(r#"{{"seed": {n}}}"#), ["n"]);
        assert_eq!(names("{{{name}}}"), ["name"]);
    }

    #[test]
    fn parameterized_copy_refuses_what_format_would_refuse() {
        for text in [r#"{"seed": 42}"#, "{}", "{count:>3}", "{ count }", "open {", "close }", "{{{x}}"] {
            assert!(placeholders(text).is_err(), "{text:?} was accepted");
        }
    }

    #[test]
    fn plain_copy_shows_its_braces_as_written() {
        for text in [r#"例：{"seed": 42}"#, r#"e.g. {"seed": 42}"#, "{}", "a { b", "a } b", "{ count }", "{count:>3}"]
        {
            assert_eq!(plain(text), Ok(()), "{text:?} was refused");
        }
    }

    #[test]
    fn plain_copy_refuses_a_bare_placeholder() {
        for text in ["{model}", "「{name}」を削除", r#"{"a": {b}}"#] {
            let error = plain(text).expect_err(text);
            assert!(error.contains("fmt."), "{error}");
        }
    }
}
