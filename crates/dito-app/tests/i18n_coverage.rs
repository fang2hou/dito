//! i18n guard for the template-ID world. The compiler already rejects unknown template IDs,
//! missing languages, and placeholder drift (build.rs validates `messages.toml`), so what is
//! left to guard here:
//!
//! 1. No call site may regress to a string-keyed `t("…")` — every string literal argument to
//!    `t`/`c.t` is a failure; copy must go through `Tr` variants.
//! 2. No CJK string literal may live outside `messages.toml` (or the exemption list below) —
//!    this catches hard-coded Japanese that never went through `Tr` at all.
//!
//! Adding UI copy: add the message to `crates/dito-core/src/i18n/messages.toml` and reference
//! the generated `Tr` variant. Exemptions are for text that is intentionally *not* translated
//! (language self-names, demo/seed data) — keep the list short and justified.

use std::path::PathBuf;

const EXEMPT: &[(&str, &str)] = &[
    // Language self-names are always shown in their own language (pickers, stats legend).
    ("view.rs", "日本語"),
    ("view.rs", "中文"),
    ("general.rs", "日本語"),
    ("general.rs", "中文"),
    ("stats.rs", "日本語"),
    ("stats.rs", "中文"),
    ("prompts.rs", "日本語"),
    ("prompts.rs", "中文"),
    // Demo/seed history entries in full (user content, not UI copy).
    ("fixtures.rs", "*"),
    // List separator chosen per language in code (、 only for Japanese).
    ("models.rs", "、"),
    // Test fixture inside #[cfg(test)] (letter-spacing assertions).
    ("drawing.rs", "识别中"),
];

fn rust_files() -> Vec<PathBuf> {
    fn walk(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    let mut v = Vec::new();
    walk(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src"), &mut v);
    v
}

fn has_cjk(s: &str) -> bool {
    s.chars().any(|c| ('\u{3000}'..='\u{30ff}').contains(&c) || ('\u{4e00}'..='\u{9fff}').contains(&c))
}

/// The Japanese copy defined in `messages.toml` — every CJK literal in app sources must be one
/// of these (UI copy) or an exempted exception.
fn japanese_copy() -> Vec<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../dito-core/src/i18n/messages.toml");
    let raw = std::fs::read_to_string(&path).unwrap();
    let table: toml::Table = toml::from_str(&raw).unwrap();
    fn collect(node: &toml::Table, out: &mut Vec<String>) {
        for v in node.values() {
            if let Some(t) = v.as_table() {
                if let Some(ja) = t.get("ja").and_then(|s| s.as_str()) {
                    out.push(ja.to_string());
                } else {
                    collect(t, out);
                }
            }
        }
    }
    let mut out = Vec::new();
    collect(&table, &mut out);
    out
}

/// Literal string arguments of `t("…")` / `c.t("…")` / `t(c.lang, "…")` / `t(lang, "…")`.
fn t_call_literals(text: &str) -> Vec<&str> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(p) = text[from..].find("t(") {
        let at = from + p;
        from = at + 2;
        // Identifier boundary: not part of a longer name like `start(` or `fmt(`.
        if at > 0 {
            let prev = b[at - 1];
            if prev.is_ascii_alphabetic() || prev == b'_' {
                continue;
            }
        }
        let mut j = at + 2;
        while j < b.len() && b[j] == b' ' {
            j += 1;
        }
        // Optional lang argument: `c.lang,` or `lang,`.
        for arg in ["c.lang", "lang"] {
            if text[j..].starts_with(arg) {
                let end = text[j..].find(',').unwrap() + j;
                j = end + 1;
                while j < b.len() && b[j] == b' ' {
                    j += 1;
                }
                break;
            }
        }
        if j < b.len()
            && b[j] == b'"'
            && let Some(close) = text[j + 1..].find('"')
        {
            out.push(&text[j + 1..j + 1 + close]);
        }
    }
    out
}

#[test]
fn no_string_keyed_translation_calls_remain() {
    let mut found = Vec::new();
    for f in rust_files() {
        let text = std::fs::read_to_string(&f).unwrap();
        let rel = f.file_name().unwrap().to_str().unwrap();
        for lit in t_call_literals(&text) {
            found.push(format!("{rel}: t({lit:?}) — copy must reference a Tr variant, not a string key"));
        }
    }
    assert!(
        found.is_empty(),
        "string-keyed translations are gone; use `lang.tr(Tr::…)` / `c.t(Tr::…)`:\n{}",
        found.join("\n")
    );
}

#[test]
fn no_hard_coded_cjk_outside_messages_toml() {
    let copy = japanese_copy();
    let mut stray = Vec::new();
    for f in rust_files() {
        // Comment lines (like the prose examples in models.rs) are not UI copy.
        let text = std::fs::read_to_string(&f)
            .unwrap()
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let text = &text;
        let rel = f.file_name().unwrap().to_str().unwrap().to_string();
        let b = text.as_bytes();
        let mut from = 0;
        while let Some(p) = text[from..].find('"') {
            let at = from + p + 1;
            let mut end = at;
            let mut probe = at;
            while let Some(n) = text[probe..].find('"').map(|i| probe + i) {
                if b[n - 1] == b'\\' {
                    // Escaped quote inside the literal: skip past it.
                    probe = n + 1;
                    continue;
                }
                end = n;
                break;
            }
            let lit = &text[at..end];
            from = end + 1;
            if has_cjk(lit) && !copy.iter().any(|c| c == lit) {
                let exempt = EXEMPT.iter().any(|(file, s)| *file == rel && (*s == lit || *s == "*"));
                if !exempt {
                    stray.push(format!("{rel}: {lit:?} is CJK copy outside messages.toml"));
                }
            }
        }
    }
    assert!(
        stray.is_empty(),
        "hard-coded CJK copy (add it to crates/dito-core/src/i18n/messages.toml and use its Tr variant):\n{}",
        stray.join("\n")
    );
}
