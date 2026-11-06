//! i18n guard. The compiler already rejects unknown message IDs, missing languages and
//! placeholder drift (`build.rs` validates `messages.toml`, and `Ctx::t` takes a `Tr`), so
//! what is left to guard here is Japanese that never went through `Tr` at all: no CJK string
//! literal may live outside `messages.toml` or the exemption list below.
//!
//! Adding UI copy: add the message to `crates/dito-core/src/i18n/messages.toml` and reference
//! the generated `Tr` variant. Exemptions are for text that is intentionally *not* translated
//! (language self-names, demo/seed data) and name the exact file, relative to `src`.

use std::path::PathBuf;

const EXEMPT: &[(&str, &str)] = &[
    // Language self-names are always shown in their own language (pickers, stats legend).
    ("hud/view/language_chip.rs", "日本語"),
    ("hud/view/language_chip.rs", "中文"),
    ("settings/general/audio.rs", "日本語"),
    ("settings/general/audio.rs", "中文"),
    ("settings/stats.rs", "日本語"),
    ("settings/stats.rs", "中文"),
    ("settings/prompts.rs", "日本語"),
    ("settings/prompts.rs", "中文"),
    // Demo/seed history entries in full (user content, not UI copy).
    ("dev/seed.rs", "*"),
    // List separator chosen per language in code (、 only for Japanese).
    ("settings/models/page.rs", "、"),
    // Test fixture inside #[cfg(test)] (letter-spacing assertions).
    ("ui/typography.rs", "認識中"),
];

fn src_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
}

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
    walk(&src_dir(), &mut v);
    v
}

fn has_cjk(s: &str) -> bool {
    s.chars().any(|c| ('\u{3000}'..='\u{30ff}').contains(&c) || ('\u{4e00}'..='\u{9fff}').contains(&c))
}

#[test]
fn no_hard_coded_cjk_outside_messages_toml() {
    let src = src_dir();
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
        let rel = f.strip_prefix(&src).unwrap().to_str().unwrap().to_string();
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
            let exempt = EXEMPT.iter().any(|(file, s)| *file == rel && (*s == lit || *s == "*"));
            if has_cjk(lit) && !exempt {
                stray.push(format!("{rel}: {lit:?} is CJK copy outside messages.toml"));
            }
        }
    }
    assert!(
        stray.is_empty(),
        "hard-coded CJK copy (add it to crates/dito-core/src/i18n/messages.toml and use its Tr variant):\n{}",
        stray.join("\n")
    );
}
