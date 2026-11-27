//! Markdown blocks as elements, in the reply typography of `design:tools/chat` (23.05.4).

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::highlight::Ink;
use super::markdown::{CodeLine, Document, Inline, Prepared};
use crate::theme::Palette;
use crate::ui::{self, Palette as UiPalette};

const BODY_SIZE: f32 = 14.;
const BODY_LINE: f32 = 24.;
const BLOCK_GAP: f32 = 12.;
const CODE_FONT: &str = "Menlo";
const CODE_SIZE: f32 = 12.;
const CODE_LINE: f32 = 20.;

/// A reply's blocks in the reply typography, for every surface that shows a reply.
pub(crate) fn document(doc: &Document, p: &Palette) -> Div {
    blocks(doc.blocks(), p)
}

/// The same blocks on a surface that is itself `p.sub`, where inline code and table heads would vanish: they take
/// `p.border` instead, which stays visible in both themes.
pub(crate) fn document_on_sub(doc: &Document, p: &Palette) -> Div {
    blocks(doc.blocks(), &Palette { ui: UiPalette { sub: p.border, ..p.ui }, ..*p })
}

fn blocks<'a>(blocks: impl Iterator<Item = &'a Prepared>, p: &Palette) -> Div {
    let mut column = div().w_full().flex().flex_col().gap(px(BLOCK_GAP));
    for block in blocks {
        column = column.child(block_view(block, p));
    }
    column
}

fn block_view(block: &Prepared, p: &Palette) -> AnyElement {
    match block {
        Prepared::Paragraph(text) => inline(text, p, BODY_SIZE, BODY_LINE, FontWeight::NORMAL).into_any_element(),
        Prepared::Heading { level, text } => {
            let (size, line) = if *level == 1 { (17., 26.) } else { (15., 24.) };
            inline(text, p, size, line, FontWeight::BOLD).into_any_element()
        }
        Prepared::List { start, items } => list(*start, items, p).into_any_element(),
        Prepared::Quote(inner) => div()
            .w_full()
            .px(px(16.))
            .py(px(12.))
            .rounded(px(12.))
            .bg(p.accent_soft)
            .child(blocks(inner.iter(), p))
            .into_any_element(),
        Prepared::Code { id, lang, text, lines } => code_block(id, lang, text, lines, p).into_any_element(),
        Prepared::Table { head, rows } => table(head, rows, p).into_any_element(),
        Prepared::Rule => div().w_full().h(px(1.)).bg(p.border).into_any_element(),
    }
}

fn list(start: Option<u64>, items: &[Vec<Prepared>], p: &Palette) -> Div {
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
            div()
                .w_full()
                .flex()
                .gap(px(4.))
                .child(marker)
                .child(div().flex_1().min_w(px(0.)).child(blocks(item.iter(), p))),
        );
    }
    column
}

/// One run of text per styled span, so the whole paragraph stays one selection.
fn inline(text: &Inline, p: &Palette, size: f32, line: f32, weight: FontWeight) -> Div {
    let runs = text
        .runs
        .iter()
        .map(|run| {
            let mut face = font(if run.code { CODE_FONT } else { run.script.family() });
            face.weight = if run.bold { FontWeight::BOLD } else { weight };
            if run.italic {
                face.style = FontStyle::Italic;
            }
            let color = if run.link { p.accent_text } else { p.text };
            TextRun {
                len: run.len,
                font: face,
                color,
                background_color: run.code.then_some(p.sub),
                underline: run.link.then(|| UnderlineStyle { thickness: px(1.), color: Some(color), wavy: false }),
                strikethrough: run.strike.then(|| StrikethroughStyle { thickness: px(1.), color: Some(color) }),
            }
        })
        .collect();
    let base = div().w_full().text_size(px(size)).line_height(px(line)).text_color(p.text);
    if text.links.is_empty() {
        return base.child(ui::selectable_runs(p, text.id.clone(), text.text.clone(), runs));
    }
    let (ranges, urls): (Vec<_>, Vec<_>) = text.links.iter().cloned().unzip();
    base.child(
        InteractiveText::new(text.id.clone(), StyledText::new(text.text.clone()).with_runs(runs))
            .on_click(ranges, move |ix, _, cx| cx.open_url(&urls[ix])),
    )
}

fn code_block(id: &ElementId, lang: &SharedString, body: &SharedString, lines: &[CodeLine], p: &Palette) -> Div {
    let mut column = div().w_full().flex().flex_col();
    for line in lines {
        column = column.child(code_line(line, p));
    }
    ui::code_block_with(id.clone(), p, lang.clone(), body.clone(), column)
}

fn code_line(line: &CodeLine, p: &Palette) -> impl IntoElement {
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
    for (range, ink) in &line.inks {
        push(range.start, p.text);
        push(range.end, if *ink == Ink::Accent { p.accent_text } else { p.muted });
    }
    push(line.text.len(), p.text);
    div().w_full().text_size(px(CODE_SIZE)).line_height(px(CODE_LINE)).child(ui::selectable_runs(
        p,
        line.id.clone(),
        line.text.clone(),
        runs,
    ))
}

fn table(head: &[Inline], rows: &[Vec<Inline>], p: &Palette) -> Div {
    let cell = |text: &Inline, weight: FontWeight| {
        div().flex_1().min_w(px(0.)).px(px(12.)).py(px(8.)).child(inline(text, p, 13., 20., weight))
    };
    let mut column =
        div().w_full().flex().flex_col().rounded(px(8.)).overflow_hidden().border_1().border_color(p.border);
    let mut header = div().w_full().flex().bg(p.sub).border_b_1().border_color(p.border);
    for text in head {
        header = header.child(cell(text, FontWeight::MEDIUM));
    }
    column = column.child(header);
    for (r, row) in rows.iter().enumerate() {
        let mut line = div().w_full().flex();
        if r + 1 < rows.len() {
            line = line.border_b_1().border_color(p.border);
        }
        for text in row {
            line = line.child(cell(text, FontWeight::NORMAL));
        }
        column = column.child(line);
    }
    column
}
