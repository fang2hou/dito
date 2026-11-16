//! Markdown blocks as elements, in the reply typography of `design:tools/chat` (23.05.4).

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::ChatPage;
use super::highlight::{Ink, ink_ranges};
use super::markdown::{Block, Span};
use crate::theme::Palette;
use crate::ui;

const BODY_SIZE: f32 = 14.;
const BODY_LINE: f32 = 24.;
const BLOCK_GAP: f32 = 12.;
const CODE_FONT: &str = "Menlo";
const CODE_SIZE: f32 = 12.;
const CODE_LINE: f32 = 20.;

fn sub(id: &ElementId, tag: &str, n: usize) -> ElementId {
    ElementId::from((id.clone(), SharedString::from(format!("{tag}{n}"))))
}

pub(super) fn blocks<'a>(
    blocks: impl Iterator<Item = &'a Block>,
    p: &Palette,
    id: &ElementId,
    cx: &mut Context<ChatPage>,
) -> Div {
    let mut column = div().w_full().flex().flex_col().gap(px(BLOCK_GAP));
    for (i, block) in blocks.enumerate() {
        column = column.child(block_view(block, p, &sub(id, "b", i), cx));
    }
    column
}

fn block_view(block: &Block, p: &Palette, id: &ElementId, cx: &mut Context<ChatPage>) -> AnyElement {
    match block {
        Block::Paragraph(spans) => inline(spans, p, id, BODY_SIZE, BODY_LINE, FontWeight::NORMAL).into_any_element(),
        Block::Heading { level, spans } => {
            let (size, line) = if *level == 1 { (17., 26.) } else { (15., 24.) };
            inline(spans, p, id, size, line, FontWeight::BOLD).into_any_element()
        }
        Block::List { start, items } => list(*start, items, p, id, cx).into_any_element(),
        Block::Quote(inner) => div()
            .w_full()
            .px(px(16.))
            .py(px(12.))
            .rounded(px(12.))
            .bg(p.accent_soft)
            .child(blocks(inner.iter(), p, id, cx))
            .into_any_element(),
        Block::Code { lang, text } => code_block(lang, text, p, id).into_any_element(),
        Block::Table { head, rows } => table(head, rows, p, id).into_any_element(),
        Block::Rule => div().w_full().h(px(1.)).bg(p.border).into_any_element(),
    }
}

fn list(start: Option<u64>, items: &[Vec<Block>], p: &Palette, id: &ElementId, cx: &mut Context<ChatPage>) -> Div {
    let mut column = div().w_full().flex().flex_col().gap(px(8.));
    for (i, item) in items.iter().enumerate() {
        let marker = match start {
            Some(first) => div().w(px(20.)).flex_none().child(
                ui::text(format!("{}.", first + i as u64), 13., FontWeight::MEDIUM, p.accent_text)
                    .line_height(px(BODY_LINE)),
            ),
            None => div()
                .w(px(20.))
                .h(px(BODY_LINE))
                .flex_none()
                .flex()
                .items_center()
                .child(div().size(px(5.)).rounded_full().bg(p.accent)),
        };
        column = column.child(
            div().w_full().flex().gap(px(4.)).child(marker).child(div().flex_1().min_w(px(0.)).child(blocks(
                item.iter(),
                p,
                &sub(id, "b", i),
                cx,
            ))),
        );
    }
    column
}

/// One run of text per styled span, so the whole paragraph stays one selection.
fn inline(spans: &[Span], p: &Palette, id: &ElementId, size: f32, line: f32, weight: FontWeight) -> Div {
    let mut text = String::new();
    let mut runs = Vec::new();
    let mut links: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for span in spans {
        if span.text.is_empty() {
            continue;
        }
        let start = text.len();
        text.push_str(&span.text);
        let style = &span.style;
        let mut face = font(if style.code { CODE_FONT } else { ui::family_for(&span.text) });
        face.weight = if style.bold { FontWeight::BOLD } else { weight };
        if style.italic {
            face.style = FontStyle::Italic;
        }
        let color = if style.link.is_some() { p.accent_text } else { p.text };
        runs.push(TextRun {
            len: span.text.len(),
            font: face,
            color,
            background_color: style.code.then_some(p.sub),
            underline: style.link.is_some().then(|| UnderlineStyle {
                thickness: px(1.),
                color: Some(color),
                wavy: false,
            }),
            strikethrough: style.strike.then(|| StrikethroughStyle { thickness: px(1.), color: Some(color) }),
        });
        if let Some(url) = &style.link {
            links.push((start..text.len(), url.clone()));
        }
    }
    let base = div().w_full().text_size(px(size)).line_height(px(line)).text_color(p.text);
    if links.is_empty() {
        return base.child(ui::selectable_runs(p, id.clone(), text, runs));
    }
    let (ranges, urls): (Vec<_>, Vec<_>) = links.into_iter().unzip();
    base.child(
        InteractiveText::new(id.clone(), StyledText::new(text).with_runs(runs))
            .on_click(ranges, move |ix, _, cx| cx.open_url(&urls[ix])),
    )
}

fn code_block(lang: &str, body: &str, p: &Palette, id: &ElementId) -> Div {
    let mut lines = div().w_full().flex().flex_col();
    for (i, line) in body.split('\n').enumerate() {
        lines = lines.child(code_line(line, p, &sub(id, "line", i)));
    }
    ui::code_block_with(id.clone(), p, lang.to_owned(), body.to_owned(), lines)
}

fn code_line(line: &str, p: &Palette, id: &ElementId) -> impl IntoElement {
    let mut runs = Vec::new();
    let mut at = 0;
    let mut push = |end: usize, color: Hsla| {
        if end > at {
            let mut face = font(CODE_FONT);
            face.weight = FontWeight::NORMAL;
            runs.push(TextRun {
                len: end - at,
                font: face,
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            });
            at = end;
        }
    };
    for (range, ink) in ink_ranges(line) {
        push(range.start, p.text);
        push(range.end, if ink == Ink::Accent { p.accent_text } else { p.muted });
    }
    push(line.len(), p.text);
    // An empty line keeps its height with one invisible space.
    let (shown, runs) = if line.is_empty() {
        let mut face = font(CODE_FONT);
        face.weight = FontWeight::NORMAL;
        (
            " ".to_owned(),
            vec![TextRun {
                len: 1,
                font: face,
                color: p.text,
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
        )
    } else {
        (line.to_owned(), runs)
    };
    div().w_full().text_size(px(CODE_SIZE)).line_height(px(CODE_LINE)).child(ui::selectable_runs(
        p,
        id.clone(),
        shown,
        runs,
    ))
}

fn table(head: &[Vec<Span>], rows: &[Vec<Vec<Span>>], p: &Palette, id: &ElementId) -> Div {
    let cell = |spans: &[Span], weight: FontWeight, id: ElementId| {
        div().flex_1().min_w(px(0.)).px(px(12.)).py(px(8.)).child(inline(spans, p, &id, 13., 20., weight))
    };
    let mut column =
        div().w_full().flex().flex_col().rounded(px(8.)).overflow_hidden().border_1().border_color(p.border);
    let mut header = div().w_full().flex().bg(p.sub).border_b_1().border_color(p.border);
    for (c, spans) in head.iter().enumerate() {
        header = header.child(cell(spans, FontWeight::MEDIUM, sub(id, "head", c)));
    }
    column = column.child(header);
    for (r, row) in rows.iter().enumerate() {
        let mut line = div().w_full().flex();
        if r + 1 < rows.len() {
            line = line.border_b_1().border_color(p.border);
        }
        for (c, spans) in row.iter().enumerate() {
            line = line.child(cell(spans, FontWeight::NORMAL, sub(id, "cell", r * 64 + c)));
        }
        column = column.child(line);
    }
    column
}
