//! Code coloring that stays inside the palette: URLs and keywords in the accent, flags muted, the rest plain.

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Ink {
    Accent,
    Muted,
}

const KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "case", "class", "const", "continue", "def", "do", "done", "elif", "else", "enum",
    "export", "fi", "false", "fn", "for", "from", "func", "function", "if", "impl", "import", "in", "let", "loop",
    "match", "mod", "mut", "nil", "none", "null", "pub", "return", "self", "static", "struct", "switch", "then", "this",
    "trait", "true", "try", "type", "use", "var", "while", "yield",
];

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Colored byte ranges of one line, in order and without overlap; uncolored text has no range.
pub(super) fn ink_ranges(line: &str) -> Vec<(Range<usize>, Ink)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < line.len() {
        let rest = &line[i..];
        let c = rest.chars().next().expect("not at the end");
        let boundary = line[..i].chars().next_back().is_none_or(|p| !is_word(p));
        if (rest.starts_with("http://") || rest.starts_with("https://")) && boundary {
            let len = rest.find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ')' | '>' | '\\')).unwrap_or(rest.len());
            out.push((i..i + len, Ink::Accent));
            i += len;
        } else if c == '-' && line[..i].chars().next_back().is_none_or(char::is_whitespace) {
            let len = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let flag = &rest[..len];
            if flag.len() > 1 && flag.trim_start_matches('-').starts_with(|c: char| c.is_alphabetic()) {
                out.push((i..i + len, Ink::Muted));
            }
            i += len.max(1);
        } else if is_word(c) {
            let len = rest.find(|c: char| !is_word(c)).unwrap_or(rest.len());
            if boundary && KEYWORDS.contains(&&rest[..len]) {
                out.push((i..i + len, Ink::Accent));
            }
            i += len;
        } else {
            i += c.len_utf8();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inked(line: &str) -> Vec<(&str, Ink)> {
        ink_ranges(line).into_iter().map(|(r, ink)| (&line[r], ink)).collect()
    }

    #[test]
    fn urls_and_keywords_take_the_accent_and_flags_stay_muted() {
        assert_eq!(inked("curl http://127.0.0.1:17841/v1/models \\"), vec![("http://127.0.0.1:17841/v1/models", Ink::Accent)]);
        assert_eq!(inked("  -H \"Authorization: Bearer $T\""), vec![("-H", Ink::Muted)]);
        assert_eq!(inked("let x = 1; // letter"), vec![("let", Ink::Accent)]);
        assert_eq!(inked("rm --force a-b"), vec![("--force", Ink::Muted)]);
    }

    #[test]
    fn words_that_merely_contain_a_keyword_stay_plain_and_multibyte_text_is_safe() {
        assert!(inked("format informal returned").is_empty());
        assert_eq!(inked("\u{65e5}\u{672c}\u{8a9e} if \u{65e5}\u{672c}"), vec![("if", Ink::Accent)]);
        assert!(inked("").is_empty());
    }
}
