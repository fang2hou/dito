use std::time::Duration;

use dito_core::insert::{Outcome, outcome};

use super::focus::{FocusTarget, read_focus};

const VERIFY_TRIES: usize = 8;
const VERIFY_POLL: Duration = Duration::from_millis(40);

/// Returns the verdict and the last selection location seen (the next baseline).
///
/// Editors rewrite text as it arrives (smart quotes, `...` to `…`, markdown spacing), so the cursor can move by fewer
/// units than posted even though everything landed. When the cursor verdict is a refute, the field's content gets a
/// second look before declaring failure.
pub(super) fn verify(
    target: &FocusTarget,
    expected: usize,
    before: Option<usize>,
    text: &str,
) -> (Outcome, Option<usize>) {
    // Without a baseline the delta can never be observed, so the verdict is Unknown whatever
    // happens (key-window targets, secure fields); the poll loop would only add latency.
    let Some(base) = before else { return (Outcome::Unknown, None) };
    let mut last = None;
    for _ in 0..VERIFY_TRIES {
        std::thread::sleep(VERIFY_POLL);
        let cur = match read_focus() {
            Some(focused) if target.same_identity(&focused) => focused.cursor,
            Some(_) => return (Outcome::Unknown, last),
            None => continue,
        };
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
    if matches!(verdict, Outcome::Refuted { .. })
        && let Some(caret) = last.filter(|&caret| caret > base)
    {
        let length = expected.saturating_mul(2).saturating_add(8).min(caret);
        let nearby = target.element.text_for_range(caret - length, length);
        let value = nearby.is_none().then(|| target.element.string("AXValue")).flatten();
        let before_caret =
            nearby.as_deref().or_else(|| value.as_deref().and_then(|value| prefix_at_utf16(value, caret)));
        if landed_by_content(before_caret, text) {
            tracing::info!("insert: cursor disagrees but the text is before the caret; counting it as confirmed");
            return (Outcome::Confirmed, last);
        }
    }
    (verdict, last)
}

fn prefix_at_utf16(value: &str, caret: usize) -> Option<&str> {
    let mut offset = 0;
    for (index, ch) in value.char_indices() {
        if offset >= caret {
            return (offset == caret).then(|| &value[..index]);
        }
        offset += ch.len_utf16();
    }
    (offset == caret).then_some(value)
}

/// Comparison fold for content verification: editors rewrite punctuation and whitespace as text
/// arrives, so only the word characters (letters, digits, CJK) carry reliable evidence.
fn fold_content(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Proof the insertion landed even when the cursor moved less than expected. Short texts carry too little evidence and
/// stay with the cursor verdict.
fn landed_by_content(value: Option<&str>, text: &str) -> bool {
    let Some(value) = value else { return false };
    let needle = fold_content(text);
    needle.chars().count() >= 4 && fold_content(value).ends_with(&needle)
}

#[cfg(test)]
mod tests {
    use super::{landed_by_content, prefix_at_utf16};

    #[test]
    fn verification_uses_the_caret_in_the_middle_of_the_field() {
        let value = "🙂你好 世界 untouched suffix";
        assert!(landed_by_content(prefix_at_utf16(value, 7), "你好世界"));
        assert!(prefix_at_utf16(value, 1).is_none());
    }

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
        assert!(!landed_by_content(Some("嗯abc"), "嗯?"));
        assert!(!landed_by_content(Some("abc"), "abc"));
    }
}
