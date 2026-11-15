//! No CJK string literal may live outside `messages.toml` or the exemption list below. The compiler already rejects
//! unknown message IDs, missing languages and placeholder drift, so only text that never went through `Tr` is left to
//! catch. Exemptions name the exact file, relative to `src`.

use std::path::PathBuf;

const EXEMPT: &[(&str, &str)] = &[
    // Demo/seed history entries in full (user content, not UI copy).
    ("dev/seed.rs", "*"),
    // The mock selection (sample user content) and the keywords the mock router listens for in a spoken or typed
    // instruction; neither is UI copy.
    ("dev/mock_selection.rs", "*"),
    ("dev/mock_router.rs", "*"),
    // The text the command HUD shots show in the answer, confirm and typing windows (user content).
    ("dev/shots/hud.rs", "*"),
    // Configuration names the polish form shots type into the form (user content, not UI copy).
    ("dev/shots/text_model_form.rs", "*"),
    // List separator chosen per language in code (the ideographic comma, for Japanese and Chinese).
    ("settings/models/page.rs", "、"),
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

fn workspace_rust_files() -> Vec<PathBuf> {
    let crates_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
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
    walk(&crates_dir, &mut v);
    v
}

fn has_cjk(s: &str) -> bool {
    s.chars().any(|c| {
        ('\u{0400}'..='\u{052f}').contains(&c)
            || ('\u{1100}'..='\u{11ff}').contains(&c)
            || ('\u{3000}'..='\u{30ff}').contains(&c)
            || ('\u{4e00}'..='\u{9fff}').contains(&c)
            || ('\u{ac00}'..='\u{d7af}').contains(&c)
    })
}

#[test]
fn no_cjk_in_rust_comments() {
    let crates_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
    let mut violations = Vec::new();
    for f in workspace_rust_files() {
        let rel = f.strip_prefix(&crates_dir).unwrap().to_str().unwrap().to_string();
        for (line_no, line) in std::fs::read_to_string(&f).unwrap().lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") && has_cjk(trimmed) {
                violations.push(format!("crates/{rel}:{}: comment contains CJK: {trimmed}", line_no + 1));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "comments must use standard English without CJK literals (reference Tr variants or design:<id> instead):\n{}",
        violations.join("\n")
    );
}

#[test]
fn no_hard_coded_cjk_outside_messages_toml() {
    let src = src_dir();
    let mut stray = Vec::new();
    for f in rust_files() {
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
