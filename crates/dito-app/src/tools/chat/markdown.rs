//! A reply's Markdown as blocks of styled spans, parsed once and extended as the reply streams.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct SpanStyle {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Span {
    pub text: String,
    pub style: SpanStyle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Block {
    Paragraph(Vec<Span>),
    Heading { level: u8, spans: Vec<Span> },
    List { start: Option<u64>, items: Vec<Vec<Block>> },
    Quote(Vec<Block>),
    Code { lang: String, text: String },
    Table { head: Vec<Vec<Span>>, rows: Vec<Vec<Vec<Span>>> },
    Rule,
}

fn options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_CJK_FRIENDLY_EMPHASIS
}

enum Frame {
    Blocks(Vec<Block>),
    Spans(Vec<Span>),
    Items { start: Option<u64>, items: Vec<Vec<Block>> },
    Code { lang: String, text: String },
    Table { head: Vec<Vec<Span>>, rows: Vec<Vec<Vec<Span>>>, row: Vec<Vec<Span>>, in_head: bool },
}

struct Builder {
    stack: Vec<Frame>,
    style: SpanStyle,
}

impl Builder {
    fn push_text(&mut self, text: &str, code: bool) {
        let mut style = self.style.clone();
        style.code = code;
        match self.stack.last_mut() {
            Some(Frame::Spans(spans)) => merge(spans, text, style),
            Some(Frame::Code { text: body, .. }) => body.push_str(text),
            Some(Frame::Blocks(_)) => {
                // Tight list items carry their words without a paragraph around them.
                let mut spans = Vec::new();
                merge(&mut spans, text, style);
                self.stack.push(Frame::Spans(spans));
            }
            _ => {}
        }
    }

    /// Closes an implicit paragraph that a block-level event interrupts.
    fn flush_implicit(&mut self) {
        if let Some(Frame::Spans(_)) = self.stack.last()
            && matches!(self.stack.get(self.stack.len().wrapping_sub(2)), Some(Frame::Blocks(_)))
        {
            self.close_paragraph();
        }
    }

    fn close_paragraph(&mut self) {
        if let Some(Frame::Spans(spans)) = self.stack.pop() {
            self.push_block(Block::Paragraph(spans));
        }
    }

    fn push_block(&mut self, block: Block) {
        if let Some(Frame::Blocks(blocks)) = self.stack.last_mut() {
            blocks.push(block);
        }
    }

    fn pop_spans(&mut self) -> Vec<Span> {
        match self.stack.pop() {
            Some(Frame::Spans(spans)) => spans,
            _ => Vec::new(),
        }
    }

    fn pop_blocks(&mut self) -> Vec<Block> {
        match self.stack.pop() {
            Some(Frame::Blocks(blocks)) => blocks,
            _ => Vec::new(),
        }
    }
}

fn merge(spans: &mut Vec<Span>, text: &str, style: SpanStyle) {
    match spans.last_mut() {
        Some(last) if last.style == style => last.text.push_str(text),
        _ => spans.push(Span { text: text.to_owned(), style }),
    }
}

pub(super) fn parse(src: &str) -> Vec<Block> {
    let mut b = Builder { stack: vec![Frame::Blocks(Vec::new())], style: SpanStyle::default() };
    for event in Parser::new_ext(src, options()) {
        match event {
            Event::Start(tag) => start(&mut b, tag),
            Event::End(end) => finish(&mut b, end),
            Event::Text(text) => b.push_text(&text, false),
            Event::Code(text) => b.push_text(&text, true),
            Event::SoftBreak | Event::HardBreak => b.push_text("\n", false),
            Event::Rule => {
                b.flush_implicit();
                b.push_block(Block::Rule);
            }
            Event::TaskListMarker(done) => b.push_text(if done { "☑ " } else { "☐ " }, false),
            Event::Html(text) | Event::InlineHtml(text) => b.push_text(&text, false),
            _ => {}
        }
    }
    b.flush_implicit();
    b.pop_blocks()
}

fn start(b: &mut Builder, tag: Tag<'_>) {
    match tag {
        Tag::Paragraph => b.stack.push(Frame::Spans(Vec::new())),
        Tag::Heading { .. } => {
            b.flush_implicit();
            b.stack.push(Frame::Spans(Vec::new()));
        }
        Tag::BlockQuote(_) => {
            b.flush_implicit();
            b.stack.push(Frame::Blocks(Vec::new()));
        }
        Tag::CodeBlock(kind) => {
            b.flush_implicit();
            let lang = match kind {
                CodeBlockKind::Fenced(info) => info.split_whitespace().next().unwrap_or("").to_owned(),
                CodeBlockKind::Indented => String::new(),
            };
            b.stack.push(Frame::Code { lang, text: String::new() });
        }
        Tag::List(start) => {
            b.flush_implicit();
            b.stack.push(Frame::Items { start, items: Vec::new() });
        }
        Tag::Item => b.stack.push(Frame::Blocks(Vec::new())),
        Tag::Table(_) => {
            b.flush_implicit();
            b.stack.push(Frame::Table { head: Vec::new(), rows: Vec::new(), row: Vec::new(), in_head: false });
        }
        Tag::TableHead => {
            if let Some(Frame::Table { in_head, .. }) = b.stack.last_mut() {
                *in_head = true;
            }
        }
        Tag::TableRow => {}
        Tag::TableCell => b.stack.push(Frame::Spans(Vec::new())),
        Tag::Emphasis => b.style.italic = true,
        Tag::Strong => b.style.bold = true,
        Tag::Strikethrough => b.style.strike = true,
        Tag::Link { dest_url, .. } => b.style.link = Some(dest_url.into_string()),
        _ => {}
    }
}

fn finish(b: &mut Builder, end: TagEnd) {
    match end {
        TagEnd::Paragraph => b.close_paragraph(),
        TagEnd::Heading(level) => {
            let spans = b.pop_spans();
            b.push_block(Block::Heading { level: heading_depth(level), spans });
        }
        TagEnd::BlockQuote(_) => {
            b.flush_implicit();
            let blocks = b.pop_blocks();
            b.push_block(Block::Quote(blocks));
        }
        TagEnd::CodeBlock => {
            if let Some(Frame::Code { lang, text }) = b.stack.pop() {
                b.push_block(Block::Code { lang, text: text.trim_end_matches('\n').to_owned() });
            }
        }
        TagEnd::List(_) => {
            if let Some(Frame::Items { start, items }) = b.stack.pop() {
                b.push_block(Block::List { start, items });
            }
        }
        TagEnd::Item => {
            b.flush_implicit();
            let blocks = b.pop_blocks();
            if let Some(Frame::Items { items, .. }) = b.stack.last_mut() {
                items.push(blocks);
            }
        }
        TagEnd::Table => {
            if let Some(Frame::Table { head, rows, .. }) = b.stack.pop() {
                b.push_block(Block::Table { head, rows });
            }
        }
        TagEnd::TableHead => {
            if let Some(Frame::Table { head, row, in_head, .. }) = b.stack.last_mut() {
                *head = std::mem::take(row);
                *in_head = false;
            }
        }
        TagEnd::TableRow => {
            if let Some(Frame::Table { rows, row, .. }) = b.stack.last_mut() {
                rows.push(std::mem::take(row));
            }
        }
        TagEnd::TableCell => {
            let spans = b.pop_spans();
            if let Some(Frame::Table { row, .. }) = b.stack.last_mut() {
                row.push(spans);
            }
        }
        TagEnd::Emphasis => b.style.italic = false,
        TagEnd::Strong => b.style.bold = false,
        TagEnd::Strikethrough => b.style.strike = false,
        TagEnd::Link => b.style.link = None,
        _ => {}
    }
}

fn heading_depth(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Where the text ends in a prefix whose blocks cannot change as more text arrives: just after a blank line that is
/// outside a code fence and is followed by a line that does not continue a list item. 0 when there is no such place.
pub(super) fn stable_prefix(src: &str) -> usize {
    let mut fence: Option<(char, usize)> = None;
    let mut offset = 0;
    let mut boundary = 0;
    let mut pending: Option<usize> = None;
    for line in src.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        let body = trimmed.trim_end();
        if let Some((mark, len)) = fence {
            let run = body.chars().take_while(|c| *c == mark).count();
            if indent < 4 && run >= len && body.chars().all(|c| c == mark) {
                fence = None;
            }
        } else if indent < 4
            && let Some(mark) = body.chars().next().filter(|c| matches!(c, '`' | '~'))
            && body.chars().take_while(|c| *c == mark).count() >= 3
        {
            fence = Some((mark, body.chars().take_while(|c| *c == mark).count()));
        }
        if body.is_empty() && fence.is_none() {
            pending.get_or_insert(offset + line.len());
        } else if let Some(at) = pending.take()
            && indent < 2
        {
            boundary = at;
        }
        offset += line.len();
    }
    boundary
}

/// A reply's blocks: the part before a stable boundary is parsed once, only the tail is parsed again as text arrives.
#[derive(Default)]
pub(super) struct Document {
    seen: usize,
    stable_len: usize,
    stable: Vec<Block>,
    tail: Vec<Block>,
}

impl Document {
    pub fn of(text: &str) -> Self {
        let mut doc = Self::default();
        doc.update(text);
        doc
    }

    /// `text` is the whole reply so far. Text that does not extend what was seen starts the document over.
    pub fn update(&mut self, text: &str) {
        if text.len() == self.seen {
            return;
        }
        if text.len() < self.stable_len {
            *self = Self::default();
        }
        let boundary = stable_prefix(text);
        if boundary > self.stable_len {
            self.stable.extend(parse(&text[self.stable_len..boundary]));
            self.stable_len = boundary;
        }
        self.tail = parse(&text[self.stable_len..]);
        self.seen = text.len();
    }

    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.stable.iter().chain(&self.tail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(spans: &[Span]) -> String {
        spans.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn inline_styles_nest_and_cjk_emphasis_closes_before_a_particle() {
        let blocks = parse("今週中に**3つの作業**を終えます。`config.toml` と [手順](https://x.test)。");
        let [Block::Paragraph(spans)] = blocks.as_slice() else { panic!("one paragraph: {blocks:?}") };
        assert!(spans.iter().any(|s| s.style.bold && s.text == "3つの作業"));
        assert!(spans.iter().any(|s| s.style.code && s.text == "config.toml"));
        assert!(spans.iter().any(|s| s.style.link.as_deref() == Some("https://x.test") && s.text == "手順"));
    }

    #[test]
    fn lists_keep_their_numbering_and_tight_items_become_paragraphs() {
        let blocks = parse("3. **a**\n   detail\n4. b\n\n- x\n- y");
        let Block::List { start, items } = &blocks[0] else { panic!("{blocks:?}") };
        assert_eq!((*start, items.len()), (Some(3), 2));
        let Block::Paragraph(first) = &items[0][0] else { panic!("{:?}", items[0]) };
        assert_eq!(plain(first), "a\ndetail");
        assert!(matches!(&blocks[1], Block::List { start: None, items } if items.len() == 2));
    }

    #[test]
    fn code_blocks_carry_their_language_and_tables_split_head_from_rows() {
        let blocks = parse("```bash\ncurl -s x\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n");
        assert_eq!(blocks[0], Block::Code { lang: "bash".into(), text: "curl -s x".into() });
        let Block::Table { head, rows } = &blocks[1] else { panic!("{blocks:?}") };
        assert_eq!((head.len(), rows.len(), plain(&rows[0][1])), (2, 1, "2".to_owned()));
    }

    #[test]
    fn quotes_hold_blocks() {
        let blocks = parse("> note\n>\n> more");
        let [Block::Quote(inner)] = blocks.as_slice() else { panic!("{blocks:?}") };
        assert_eq!(inner.len(), 2);
    }

    #[test]
    fn a_stable_prefix_ends_after_a_blank_line_outside_a_fence() {
        assert_eq!(stable_prefix("one\n\ntwo"), 5);
        assert_eq!(stable_prefix("one\n\n"), 0, "nothing follows the blank line yet");
        assert_eq!(stable_prefix("```\na\n\nb\n```\n\nc"), 14);
        assert_eq!(stable_prefix("```\na\n\nb"), 0, "an open fence holds everything after it");
        assert_eq!(stable_prefix("- a\n\n  still item a"), 0, "an indented line continues the list item");
        assert_eq!(stable_prefix("para\n\n~~~\ncode\n\n~~~\n\nend"), 21);
    }

    #[test]
    fn a_streamed_document_matches_parsing_the_whole_text_at_every_step() {
        let text = "# 見出し\n\n本文 **強調**\n\n- a\n- b\n\n```rs\nlet x = 1;\n\nlet y = 2;\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n終わり";
        let mut doc = Document::default();
        for end in text.char_indices().map(|(i, _)| i).chain([text.len()]) {
            doc.update(&text[..end]);
            let streamed: Vec<_> = doc.blocks().cloned().collect();
            assert_eq!(streamed, parse(&text[..end]), "at byte {end}");
        }
    }
}
