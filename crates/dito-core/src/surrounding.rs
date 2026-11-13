//! The text of the input field around a command's selection: reference for the tools, never the text they work on.
//!
//! It is read in full when the command starts and cut to a budget only when a tool is about to run, so a field of any
//! size costs one copy and a bounded prompt.

/// What the prompt of one command tool may take in tokens, all parts together.
///
/// The local chat runtime has no fixed window: it sizes the context to the prompt plus a reply of up to 1.5 times the
/// prompt, at most 4096 tokens (`output_budget` in `dito-runtimes`). The prompt therefore decides the memory of the
/// context and the time spent reading it before the first token, and the reply budget grows with it, so context never
/// takes room from the output. Cloud models have far larger windows. The ceiling is chosen for latency, not for fit;
/// it is an estimate (see [`estimate_tokens`]) that was not tuned against a measured prefill speed.
const PROMPT_TOKENS: usize = 3_000;
/// The system prompt with its three demonstration turns (about 1 200 tokens at most), the instruction and the tags
/// around each block.
const FIXED_TOKENS: usize = 1_300;
/// The most surrounding text a short selection gets: about one page of prose. More would slow every command for
/// text that rarely changes the answer.
const CONTEXT_TOKENS: usize = 1_500;
/// A leading or trailing mark where the text was cut.
const CUT: &str = "…";
/// Cost of one character in quarter tokens: ASCII is cheap, every other character counts as a whole token. This
/// over-estimates Japanese and Chinese a little and is close for English; core has no tokenizer.
const ASCII_WEIGHT: usize = 1;
const WIDE_WEIGHT: usize = 4;
const WEIGHT_PER_TOKEN: usize = 4;

fn weight(c: char) -> usize {
    if c.is_ascii() { ASCII_WEIGHT } else { WIDE_WEIGHT }
}

fn weight_of(text: &str) -> usize {
    text.chars().map(weight).sum()
}

/// A rough token count of `text`: a quarter token per ASCII character, one per other character.
pub fn estimate_tokens(text: &str) -> usize {
    weight_of(text).div_ceil(WEIGHT_PER_TOKEN)
}

/// How many tokens of surrounding text a tool run may carry next to `selected`, the text it works on. The selection
/// has priority: a long one leaves nothing.
pub fn context_tokens(selected: &str) -> usize {
    CONTEXT_TOKENS.min(PROMPT_TOKENS.saturating_sub(FIXED_TOKENS + estimate_tokens(selected)))
}

/// The text of the focused input field apart from the selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Surrounding {
    /// The text before and after the selection, or the caret when nothing is selected. Either part may be empty.
    Around { before: String, after: String },
    /// The whole value, because the selection could not be placed in it.
    Whole(String),
}

impl Surrounding {
    /// Splits the field's `value` around the selection. `range` is the selection's UTF-16 location and length as
    /// Accessibility reports them; it is trusted only when it reads back `selected`. Without a usable range the first
    /// occurrence of a non-empty `selected` is taken, and without that the whole value stands as one block. `None`
    /// when the value holds nothing but the selection or only whitespace.
    pub fn locate(value: &str, selected: &str, range: Option<(usize, usize)>) -> Option<Self> {
        if value.trim().is_empty() {
            return None;
        }
        let placed = range
            .and_then(|(location, length)| {
                let start = byte_offset(value, location)?;
                let end = byte_offset(value, location.checked_add(length)?)?;
                (value.get(start..end) == Some(selected)).then_some(start..end)
            })
            .or_else(|| {
                value.find(selected).filter(|_| !selected.is_empty()).map(|start| start..start + selected.len())
            });
        let Some(placed) = placed else { return Some(Self::Whole(value.to_owned())) };
        let (before, after) = (&value[..placed.start], &value[placed.end..]);
        (!(before.trim().is_empty() && after.trim().is_empty()))
            .then(|| Self::Around { before: before.to_owned(), after: after.to_owned() })
    }

    /// The characters held, both parts together.
    pub fn chars(&self) -> usize {
        match self {
            Self::Around { before, after } => before.chars().count() + after.chars().count(),
            Self::Whole(text) => text.chars().count(),
        }
    }

    /// What goes to a tool that works on `selected`: the text nearest the selection on both sides, within
    /// [`context_tokens`], cut at a line or sentence end where one is near and marked with `…` where cut. `None`
    /// when nothing useful is left.
    pub fn fit(&self, selected: &str) -> Option<Self> {
        self.fit_within(context_tokens(selected) * WEIGHT_PER_TOKEN)
    }

    fn fit_within(&self, budget: usize) -> Option<Self> {
        if budget == 0 {
            return None;
        }
        let fitted = match self {
            Self::Around { before, after } => {
                let (before_budget, after_budget) = split_budget(weight_of(before), weight_of(after), budget);
                Self::Around { before: tail(before, before_budget), after: head(after, after_budget) }
            }
            Self::Whole(text) => Self::Whole(head(text, budget)),
        };
        let useful = |text: &String| text.chars().any(|c| !c.is_whitespace() && c != '…');
        let has_text = match &fitted {
            Self::Around { before, after } => useful(before) || useful(after),
            Self::Whole(text) => useful(text),
        };
        has_text.then_some(fitted)
    }
}

/// Both sides get half; a side that needs less leaves the rest to the other.
fn split_budget(before: usize, after: usize, budget: usize) -> (usize, usize) {
    let half = budget / 2;
    if before + after <= budget {
        (before, after)
    } else if before <= half {
        (before, budget - before)
    } else if after <= half {
        (budget - after, after)
    } else {
        (half, budget - half)
    }
}

/// The byte offset of a UTF-16 offset, `None` when it falls inside a surrogate pair or past the end.
fn byte_offset(text: &str, utf16: usize) -> Option<usize> {
    let mut units = 0;
    for (index, c) in text.char_indices() {
        if units == utf16 {
            return Some(index);
        }
        units += c.len_utf16();
        if units > utf16 {
            return None;
        }
    }
    (units == utf16).then_some(text.len())
}

fn ends_unit(c: char, next: Option<char>) -> bool {
    matches!(c, '\n' | '。' | '！' | '？' | '．')
        || (matches!(c, '.' | '!' | '?') && next.is_some_and(char::is_whitespace))
}

/// The end of the last `budget` weight of `text`, starting after a line or sentence end when one lies within the
/// first quarter of what is kept.
fn tail(text: &str, budget: usize) -> String {
    let (mut spent, mut start) = (0, text.len());
    for (index, c) in text.char_indices().rev() {
        spent += weight(c);
        if spent > budget {
            break;
        }
        start = index;
    }
    if start == 0 {
        return text.to_owned();
    }
    let limit = start + (text.len() - start) / 4;
    for (offset, c) in text[start..].char_indices() {
        let end = start + offset + c.len_utf8();
        if start + offset > limit {
            break;
        }
        if end < text.len() && ends_unit(c, text[end..].chars().next()) {
            start = end;
            break;
        }
    }
    let kept = text[start..].trim_start_matches([' ', '\t']);
    if kept.is_empty() { String::new() } else { format!("{CUT}{kept}") }
}

/// The start of the first `budget` weight of `text`, ending at a line or sentence end when one lies within the last
/// quarter of what is kept.
fn head(text: &str, budget: usize) -> String {
    let (mut spent, mut end) = (0, 0);
    for (index, c) in text.char_indices() {
        spent += weight(c);
        if spent > budget {
            break;
        }
        end = index + c.len_utf8();
    }
    if end == text.len() {
        return text.to_owned();
    }
    let limit = end - end / 4;
    for (index, c) in text[..end].char_indices().rev() {
        let after = index + c.len_utf8();
        if after < limit {
            break;
        }
        if ends_unit(c, text[after..].chars().next()) {
            end = after;
            break;
        }
    }
    let kept = text[..end].trim_end();
    if kept.is_empty() { String::new() } else { format!("{kept}{CUT}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn around(before: &str, after: &str) -> Surrounding {
        Surrounding::Around { before: before.into(), after: after.into() }
    }

    #[test]
    fn the_text_nearest_the_selection_is_kept_on_both_sides() {
        let before = format!("{}近い前", "あ".repeat(5_000));
        let after = format!("近い後{}", "い".repeat(5_000));
        let Some(Surrounding::Around { before, after }) = around(&before, &after).fit("選択") else {
            panic!("a short selection leaves room for context");
        };
        assert!(before.starts_with(CUT) && before.ends_with("近い前"), "{before}");
        assert!(after.starts_with("近い後") && after.ends_with(CUT), "{after}");
        // Both sides get a share of the budget, and the budget is respected with the two marks.
        let (b, a) = (before.chars().count(), after.chars().count());
        assert!(b > 500 && a > 500, "{b} {a}");
        assert!(b + a <= context_tokens("選択") + 2, "{b} {a}");
    }

    #[test]
    fn a_side_that_needs_less_leaves_its_room_to_the_other() {
        let Some(Surrounding::Around { before, after }) = around("前", &"い".repeat(5_000)).fit("x") else {
            panic!("context expected");
        };
        assert_eq!(before, "前");
        assert!(after.chars().count() > context_tokens("x") - 10, "{}", after.chars().count());
    }

    #[test]
    fn a_cut_prefers_a_sentence_boundary_and_is_marked() {
        let context = around("Alpha one. Beta two. Gamma three. Delta four.", "Echo five. Foxtrot six. Golf seven.");
        let Some(Surrounding::Around { before, after }) = context.fit_within(60) else { panic!("context expected") };
        assert_eq!(before, "…Gamma three. Delta four.");
        assert_eq!(after, "Echo five. Foxtrot six.…");

        let lines = around("one\ntwo\nthree\nfour", "");
        assert_eq!(lines.fit_within(12), Some(around("…three\nfour", "")));
    }

    #[test]
    fn text_that_fits_is_untouched_and_a_long_selection_takes_everything() {
        let small = around("before. ", "after.");
        assert_eq!(small.fit("sel"), Some(small.clone()));
        assert_eq!(context_tokens(""), CONTEXT_TOKENS);
        let long = "あ".repeat(3_000);
        assert_eq!(context_tokens(&long), 0);
        assert_eq!(around(&"x".repeat(100), "").fit(&long), None);
    }

    #[test]
    fn a_field_of_a_hundred_thousand_characters_stays_in_budget() {
        let value = format!("{}[選択]{}", "ab あ".repeat(25_000), "いう cd".repeat(12_000));
        let Some(Surrounding::Around { before, after }) = Surrounding::locate(&value, "[選択]", None) else {
            panic!("the selection is in the value");
        };
        assert!(value.chars().count() >= 100_000);
        let fitted = around(&before, &after).fit("[選択]").expect("context expected");
        let weight = match &fitted {
            Surrounding::Around { before, after } => weight_of(before) + weight_of(after),
            Surrounding::Whole(text) => weight_of(text),
        };
        assert!(weight <= context_tokens("[選択]") * WEIGHT_PER_TOKEN + 2 * WIDE_WEIGHT, "{weight}");
    }

    #[test]
    fn the_whole_value_without_a_placed_selection_keeps_its_start() {
        let value = format!("始まり{}", "あ".repeat(5_000));
        let Some(Surrounding::Whole(text)) = Surrounding::locate(&value, "", None) else { panic!("whole expected") };
        let Some(Surrounding::Whole(fitted)) = Surrounding::Whole(text).fit("") else { panic!("context expected") };
        assert!(fitted.starts_with("始まり") && fitted.ends_with(CUT));
        assert!(fitted.chars().count() <= CONTEXT_TOKENS + 1);
    }

    #[test]
    fn the_range_places_the_selection_in_utf16_units() {
        // The emoji is two UTF-16 units, so the byte and UTF-16 offsets differ.
        let value = "😀 one two three";
        let range = Some((3, 3));
        assert_eq!(Surrounding::locate(value, "one", range), Some(around("😀 ", " two three")));
        // A caret: nothing selected, the range has no length.
        assert_eq!(Surrounding::locate(value, "", Some((3, 0))), Some(around("😀 ", "one two three")));
    }

    #[test]
    fn a_range_that_does_not_read_back_the_selection_falls_back_to_a_search() {
        let value = "x one y one";
        assert_eq!(Surrounding::locate(value, "one", Some((0, 3))), Some(around("x ", " y one")));
        assert_eq!(Surrounding::locate(value, "one", Some((99, 3))), Some(around("x ", " y one")));
        assert_eq!(Surrounding::locate(value, "gone", None), Some(Surrounding::Whole(value.into())));
        assert_eq!(Surrounding::locate(value, "", None), Some(Surrounding::Whole(value.into())));
        // The offset inside a surrogate pair is no boundary.
        assert_eq!(Surrounding::locate("😀 a", "", Some((1, 0))), Some(Surrounding::Whole("😀 a".into())));
    }

    #[test]
    fn a_value_with_nothing_but_the_selection_has_no_surrounding() {
        assert_eq!(Surrounding::locate("only this", "only this", Some((0, 9))), None);
        assert_eq!(Surrounding::locate("  \n ", "", None), None);
        assert_eq!(Surrounding::locate("", "", None), None);
    }
}
