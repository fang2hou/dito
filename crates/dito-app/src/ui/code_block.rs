//! The code block of `design:tools/chat` (23.05.4): a muted header label with a ghost Copy over Menlo 12 text, on the
//! sub surface with a hairline border at radius 12.

use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::button::CircleSize;
use super::copy_button::ghost_copy_button;
use super::typography::text;
use crate::theme::Palette;

const HEADER_H: f32 = 32.;
const PAD_X: f32 = 14.;
const CODE_PAD_Y: f32 = 10.;
const LINE_H: f32 = 20.;

/// `label` names what the code is (a language, a transport, a request); Copy puts `body` on the clipboard. `id` must
/// be unique on its page, because the Copy confirmation is remembered under it.
pub fn code_block(
    id: impl Into<ElementId>,
    p: &Palette,
    label: impl Into<SharedString>,
    body: impl Into<SharedString>,
) -> Div {
    let id: ElementId = id.into();
    let body: SharedString = body.into();
    let copied = body.clone();
    let copy = ghost_copy_button(
        ElementId::from((id, "copy")),
        p,
        CircleSize::Compact,
        Rc::new(move |_, cx: &mut App| {
            cx.write_to_clipboard(ClipboardItem::new_string(copied.to_string()));
            true
        }),
    );
    let header = div()
        .w_full()
        .h(px(HEADER_H + 1.))
        .px(px(PAD_X))
        .flex()
        .items_center()
        .gap(px(6.))
        .border_b_1()
        .border_color(p.border)
        .child(div().flex_1().min_w(px(0.)).child(text(label, 12., FontWeight::NORMAL, p.muted).line_height(px(18.))))
        .child(copy);
    let code = div()
        .w_full()
        .px(px(PAD_X))
        .py(px(CODE_PAD_Y))
        .font_family("Menlo")
        .text_size(px(12.))
        .line_height(px(LINE_H))
        .text_color(p.text)
        .child(body);
    div()
        .w_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(px(12.))
        .bg(p.sub)
        .border_1()
        .border_color(p.border)
        .child(header)
        .child(code)
}
