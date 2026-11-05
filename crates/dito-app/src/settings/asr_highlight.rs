//! Small JSON lexer for the connection editor; edits remain owned by GPUI's editor.
use gpui_kit::{
    base::input::{EditorState, FoldRange, HighlightStyleResolver, InputEdit, InputHighlighter, Rope},
    *,
};
use std::ops::Range;
pub struct JsonHighlight {
    dark: bool,
    tokens: Vec<(Range<usize>, u8)>,
}
impl JsonHighlight {
    pub fn new(dark: bool) -> Self {
        Self { dark, tokens: Vec::new() }
    }
}
impl InputHighlighter for JsonHighlight {
    fn language(&self) -> SharedString {
        "json".into()
    }
    fn update(&mut self, _: Option<InputEdit>, text: &Rope, _: bool, _: &mut Window, _: &mut Context<EditorState>) {
        self.tokens.clear();
        let source = text.to_string();
        let b = source.as_bytes();
        let mut i = 0;
        while i < b.len() {
            let start = i;
            match b[i] {
                b'"' => {
                    i += 1;
                    while i < b.len() {
                        if b[i] == b'\\' {
                            i = (i + 2).min(b.len());
                        } else if b[i] == b'"' {
                            i += 1;
                            break;
                        } else {
                            i += 1;
                        }
                    }
                    let mut next = i;
                    while next < b.len() && b[next].is_ascii_whitespace() {
                        next += 1;
                    }
                    self.tokens.push((start..i, if next < b.len() && b[next] == b':' { 1 } else { 2 }));
                }
                b'-' | b'0'..=b'9' | b't' | b'f' | b'n' => {
                    i += 1;
                    while i < b.len() && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'.' | b'+' | b'-')) {
                        i += 1;
                    }
                    self.tokens.push((start..i, 3));
                }
                _ => i += 1,
            }
        }
    }
    fn styles(&self, range: &Range<usize>, _: &dyn HighlightStyleResolver) -> Vec<(Range<usize>, HighlightStyle)> {
        let mut styles = Vec::new();
        let mut offset = range.start;
        for (token, kind) in &self.tokens {
            let start = token.start.max(range.start);
            let end = token.end.min(range.end);
            if end <= start {
                continue;
            }
            if offset < start {
                styles.push((offset..start, HighlightStyle::default()));
            }
            let color = match (self.dark, kind) {
                (false, 1) => "#91631E",
                (false, 2) => "#25705D",
                (false, _) => "#685AA5",
                (true, 1) => "#E0C47D",
                (true, 2) => "#8BC9B3",
                (true, _) => "#BEB1E3",
            };
            styles.push((start..end, HighlightStyle { color: Some(crate::theme::hex(color)), ..Default::default() }));
            offset = end;
        }
        if offset < range.end {
            styles.push((offset..range.end, HighlightStyle::default()));
        }
        styles
    }
    fn fold_ranges(&self, _: &Rope) -> Vec<FoldRange> {
        Vec::new()
    }
}
