//! A reply's Markdown as blocks of styled spans, parsed once and extended as the reply streams.

use std::ops::Range;

use gpui_kit::{ElementId, SharedString};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use super::highlight::{Ink, ink_ranges};
use crate::ui::Script;

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
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS
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
            Event::TaskListMarker(done) => b.push_text(if done { "\u{2611} " } else { "\u{2610} " }, false),
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

/// One stretch of an [`Inline`] in one style. The typeface and the colors are left to the frame, because they follow
/// the UI language and the palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Run {
    pub len: usize,
    pub script: Script,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: bool,
}

/// Spans joined into the one string their text element shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Inline {
    pub id: ElementId,
    pub text: SharedString,
    pub runs: Vec<Run>,
    pub links: Vec<(Range<usize>, SharedString)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CodeLine {
    pub id: ElementId,
    pub text: SharedString,
    pub inks: Vec<(Range<usize>, Ink)>,
}

/// A [`Block`] with all that follows from its text and its place in the reply worked out once: the strings as
/// handles GPUI shares instead of copying, the style of each stretch, the code coloring and the element ids. The
/// page is redrawn many times a second while a reply streams, and a frame only clones these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Prepared {
    Paragraph(Inline),
    Heading { level: u8, text: Inline },
    List { start: Option<u64>, items: Vec<Vec<Prepared>> },
    Quote(Vec<Prepared>),
    Code { id: ElementId, lang: SharedString, text: SharedString, lines: Vec<CodeLine> },
    Table { head: Vec<Inline>, rows: Vec<Vec<Inline>> },
    Rule,
}

fn child(parent: &ElementId, tag: &str, n: usize) -> ElementId {
    ElementId::from((parent.clone(), SharedString::from(format!("{tag}{n}"))))
}

/// `first` is the place of the first block among its siblings, which the ids count from.
fn prepare(blocks: Vec<Block>, parent: &ElementId, first: usize) -> Vec<Prepared> {
    blocks.into_iter().enumerate().map(|(i, block)| prepare_block(block, child(parent, "b", first + i))).collect()
}

fn prepare_block(block: Block, id: ElementId) -> Prepared {
    match block {
        Block::Paragraph(spans) => Prepared::Paragraph(inline(&spans, id)),
        Block::Heading { level, spans } => Prepared::Heading { level, text: inline(&spans, id) },
        Block::List { start, items } => {
            let items = items.into_iter().enumerate().map(|(i, item)| prepare(item, &child(&id, "b", i), 0));
            Prepared::List { start, items: items.collect() }
        }
        Block::Quote(inner) => Prepared::Quote(prepare(inner, &id, 0)),
        Block::Code { lang, text } => {
            let lines = text
                .split('\n')
                .enumerate()
                .map(|(i, line)| CodeLine {
                    id: child(&id, "line", i),
                    // An empty line keeps its height with one invisible space.
                    text: if line.is_empty() { " ".into() } else { line.into() },
                    inks: ink_ranges(line),
                })
                .collect();
            Prepared::Code { id, lang: lang.into(), text: text.into(), lines }
        }
        Block::Table { head, rows } => {
            let head = head.iter().enumerate().map(|(c, spans)| inline(spans, child(&id, "head", c)));
            let rows = rows.iter().enumerate().map(|(r, row)| {
                row.iter().enumerate().map(|(c, spans)| inline(spans, child(&id, "cell", r * 64 + c))).collect()
            });
            Prepared::Table { head: head.collect(), rows: rows.collect() }
        }
        Block::Rule => Prepared::Rule,
    }
}

fn inline(spans: &[Span], id: ElementId) -> Inline {
    let mut text = String::new();
    let mut runs = Vec::new();
    let mut links = Vec::new();
    for span in spans.iter().filter(|span| !span.text.is_empty()) {
        let start = text.len();
        text.push_str(&span.text);
        let style = &span.style;
        runs.push(Run {
            len: span.text.len(),
            script: Script::of(&span.text),
            bold: style.bold,
            italic: style.italic,
            strike: style.strike,
            code: style.code,
            link: style.link.is_some(),
        });
        if let Some(url) = &style.link {
            links.push((start..text.len(), SharedString::from(url)));
        }
    }
    Inline { id, text: text.into(), runs, links }
}

/// Where the text ends in a prefix whose blocks cannot change as more text arrives: just after a blank line that is
/// outside a code fence and is followed by a line that does not continue a list item. 0 when there is no such place.
///
/// `known` is that place in an earlier text that `src` extends, 0 when there is none, and the scan starts there
/// instead of at the top. It is the end of a blank line outside a fence, still waiting for the line that settles
/// it, and those are exactly the fence and the pending place a scan from the top has when it gets there.
fn stable_prefix_after(src: &str, known: usize) -> usize {
    let mut fence: Option<(char, usize)> = None;
    let mut offset = known;
    let mut boundary = known;
    let mut pending = (known > 0).then_some(known);
    for line in src[known..].split_inclusive('\n') {
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

/// A reply's blocks, ready to draw: the part before a stable boundary is parsed and prepared once, only the tail
/// again as text arrives.
#[derive(Clone)]
pub(super) struct Document {
    /// The element id of the reply, which the id of every block starts from.
    root: ElementId,
    seen: usize,
    stable_len: usize,
    stable: Vec<Prepared>,
    tail: Vec<Prepared>,
}

impl Document {
    pub fn new(root: ElementId) -> Self {
        Self { root, seen: 0, stable_len: 0, stable: Vec::new(), tail: Vec::new() }
    }

    pub fn of(root: ElementId, text: &str) -> Self {
        let mut doc = Self::new(root);
        doc.update(text);
        doc
    }

    /// `text` is the whole reply so far. Text that does not extend what was seen starts the document over.
    pub fn update(&mut self, text: &str) {
        if text.len() == self.seen {
            return;
        }
        if text.len() < self.stable_len {
            *self = Self::new(self.root.clone());
        }
        let boundary = stable_prefix_after(text, self.stable_len);
        if boundary > self.stable_len {
            let settled = parse(&text[self.stable_len..boundary]);
            self.stable.extend(prepare(settled, &self.root, self.stable.len()));
            self.stable_len = boundary;
        }
        self.tail = prepare(parse(&text[self.stable_len..]), &self.root, self.stable.len());
        self.seen = text.len();
    }

    pub fn blocks(&self) -> impl Iterator<Item = &Prepared> {
        self.stable.iter().chain(&self.tail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(spans: &[Span]) -> String {
        spans.iter().map(|s| s.text.as_str()).collect()
    }

    fn stable_prefix(src: &str) -> usize {
        stable_prefix_after(src, 0)
    }

    #[test]
    fn inline_styles_nest_and_cjk_emphasis_closes_before_a_particle() {
        let blocks = parse(
            "\u{4eca}\u{9031}\u{4e2d}\u{306b}**3\u{3064}\u{306e}\u{4f5c}\u{696d}**\u{3092}\u{7d42}\u{3048}\u{307e}\u{3059}\u{3002}`config.toml` \u{3068} [\u{624b}\u{9806}](https://x.test)\u{3002}",
        );
        let [Block::Paragraph(spans)] = blocks.as_slice() else { panic!("one paragraph: {blocks:?}") };
        assert!(spans.iter().any(|s| s.style.bold && s.text == "3\u{3064}\u{306e}\u{4f5c}\u{696d}"));
        assert!(spans.iter().any(|s| s.style.code && s.text == "config.toml"));
        assert!(
            spans.iter().any(|s| s.style.link.as_deref() == Some("https://x.test") && s.text == "\u{624b}\u{9806}")
        );
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
        let text = "# \u{898b}\u{51fa}\u{3057}\n\n\u{672c}\u{6587} **\u{5f37}\u{8abf}**\n\n- a\n- b\n\n```rs\nlet x = 1;\n\nlet y = 2;\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n\u{7d42}\u{308f}\u{308a}";
        let root = ElementId::from("reply");
        let mut doc = Document::new(root.clone());
        for end in text.char_indices().map(|(i, _)| i).chain([text.len()]) {
            doc.update(&text[..end]);
            let streamed: Vec<_> = doc.blocks().cloned().collect();
            assert_eq!(streamed, prepare(parse(&text[..end]), &root, 0), "at byte {end}");
        }
    }

    #[test]
    fn a_stable_prefix_read_from_the_last_one_is_the_same_as_read_from_the_start() {
        let text =
            "one\n\n\n\ntwo\n\n- a\n\n  still a\n\n```\nx\n\n\ny\n```\n \n\n~~~~\n```\n\n~~~~\n\n    code\n\nend\n\n";
        let mut known = 0;
        for end in 0..=text.len() {
            known = stable_prefix_after(&text[..end], known);
            assert_eq!(known, stable_prefix(&text[..end]), "at byte {end}");
        }
    }

    #[test]
    fn prepared_text_joins_its_spans_and_code_is_colored_line_by_line() {
        let root = ElementId::from("reply");
        let doc = Document::of(root, "a **b** [c](https://x.test)\n\n```rs\nlet x;\n\n-v\n```");
        let blocks: Vec<_> = doc.blocks().collect();
        let Prepared::Paragraph(text) = blocks[0] else { panic!("{blocks:?}") };
        assert_eq!(text.text, "a b c");
        assert_eq!(
            text.runs.iter().map(|r| (r.len, r.bold, r.link)).collect::<Vec<_>>(),
            [(2, false, false), (1, true, false), (1, false, false), (1, false, true)]
        );
        assert_eq!(text.links, [(4..5, SharedString::from("https://x.test"))]);
        let Prepared::Code { lang, text, lines, .. } = blocks[1] else { panic!("{blocks:?}") };
        assert_eq!((lang.as_ref(), text.as_ref()), ("rs", "let x;\n\n-v"));
        let shown: Vec<_> = lines.iter().map(|l| (l.text.as_ref(), l.inks.as_slice())).collect();
        assert_eq!(shown, [("let x;", &[(0..3, Ink::Accent)][..]), (" ", &[][..]), ("-v", &[(0..2, Ink::Muted)][..])]);
    }
}
