use std::time::Duration;

use gpui_kit::*;

use crate::theme::Palette;
use crate::ui::{text, text_width};

/// Keep one measured reveal mounted throughout animation and rest. Wrapping it in `tweened` would change its ancestor
/// identity at settle, discard its measured height and briefly lay it out at zero height.
///
/// Each line is a muted label and its value: the original text of a dictation, or the instruction and source text of a
/// command.
#[derive(IntoElement)]
pub(super) struct Disclosure {
    pub(super) id: i64,
    pub(super) lines: Vec<(SharedString, SharedString)>,
    /// The narrowest label column; a command's two lines share one wider column.
    pub(super) min_label_width: f32,
    pub(super) p: Palette,
    pub(super) open: bool,
}

impl RenderOnce for Disclosure {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let progress = gpui_kit::base::transition(
            ElementId::from(("history-original-progress", self.id as usize)),
            if self.open { 1_f32 } else { 0_f32 },
            gpui_kit::base::Transition::new(Duration::from_millis(if dito_platform::system::reduce_motion() {
                0
            } else {
                200
            }))
            .ease(|t| t * t * (3. - 2. * t)),
            window,
            cx,
        );
        // Labels share one column so the values start at the same x.
        let label_width = self
            .lines
            .iter()
            .map(|(label, _)| text_width(label, 12., FontWeight::MEDIUM))
            .fold(self.min_label_width, f32::max);
        let p = self.p;
        let lines = self.lines.into_iter().map(|(label, value)| {
            div()
                .w_full()
                .flex()
                .items_start()
                .gap(px(12.))
                .child(div().w(px(label_width)).flex_none().whitespace_nowrap().child(text(
                    label,
                    12.,
                    FontWeight::MEDIUM,
                    p.muted,
                )))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .child(text(value, 12., FontWeight::NORMAL, p.muted).line_height(relative(1.6))),
                )
        });
        gpui_kit::base::Collapsible::new().reveal(("history-content", self.id as usize), progress).content(
            div().pt(px(10.)).child(
                div()
                    .w_full()
                    .px(px(16.))
                    .py(px(12.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .rounded(px(12.))
                    .bg(p.sub)
                    .opacity(progress)
                    .children(lines),
            ),
        )
    }
}
