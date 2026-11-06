use std::time::Duration;

use dito_core::insert::{Outcome, outcome};

use super::focus::{read_cursor, read_field_value};

const VERIFY_TRIES: usize = 8;
const VERIFY_POLL: Duration = Duration::from_millis(40);

/// Poll the target's selection until it moved by `expected` UTF-16 units, or verification runs
/// out. Returns the verdict and the last selection location seen (the next baseline).
///
/// `text` is exactly what this attempt inserts. Editors rewrite text as it arrives (smart
/// quotes, `...` to `…`, markdown spacing), so the cursor can move by fewer units than posted
/// even though everything landed. When the cursor verdict is a refute, the field's content gets
/// a second look before declaring failure.
pub(super) fn verify(expected: usize, before: Option<usize>, text: &str) -> (Outcome, Option<usize>) {
    // Without a baseline the delta can never be observed, so the verdict is Unknown whatever
    // happens (key-window targets, secure fields); the poll loop would only add latency.
    let Some(base) = before else { return (Outcome::Unknown, None) };
    let mut last = None;
    for _ in 0..VERIFY_TRIES {
        std::thread::sleep(VERIFY_POLL);
        let cur = read_cursor();
        if cur.is_some() {
            last = cur;
        }
        if let Some(a) = cur
            && a >= base
            && a - base >= expected
        {
            return (Outcome::Confirmed, cur);
        }
    }
    let verdict = outcome(expected, Some(base), last);
    if matches!(verdict, Outcome::Refuted { .. }) && landed_by_content(read_field_value().as_deref(), text) {
        tracing::info!("insert: cursor disagrees but the text is in the field; counting it as confirmed");
        return (Outcome::Confirmed, last);
    }
    (verdict, last)
}

/// Comparison fold for content verification: editors rewrite punctuation and whitespace as text
/// arrives, so only the word characters (letters, digits, CJK) carry reliable evidence.
fn fold_content(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// The field's current value ends with (a rewritten form of) `text`, proof the insertion
/// landed even when the cursor moved less than expected. Short texts carry too little evidence
/// and stay with the cursor verdict.
fn landed_by_content(value: Option<&str>, text: &str) -> bool {
    let Some(value) = value else { return false };
    let needle = fold_content(text);
    needle.chars().count() >= 4 && fold_content(value).ends_with(&needle)
}

#[cfg(test)]
mod tests {
    use super::landed_by_content;

    #[test]
    fn content_check_accepts_rewritten_punctuation() {
        // Feishu-style rewrites: smart quotes, ellipsis collapse, spacing around CJK.
        let value = "他说“你好”…世界, 你好 世界abc";
        assert!(landed_by_content(Some(value), "他说\"你好\"...世界, 你好 世界abc"));
        assert!(landed_by_content(Some(value), "你好 世界abc"));
    }

    #[test]
    fn content_check_rejects_missing_tail_absent_value_and_short_text() {
        assert!(!landed_by_content(Some("他说你好世界"), "你好世界再见"));
        assert!(!landed_by_content(None, "你好世界很长"));
        // Short needles prove nothing.
        assert!(!landed_by_content(Some("嗯abc"), "嗯?"));
        assert!(!landed_by_content(Some("abc"), "abc"));
    }
}
