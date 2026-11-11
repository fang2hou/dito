use std::time::Duration;

use gpui_kit::*;

use crate::theme::Palette;
use crate::ui::text;

/// Keep one measured reveal mounted throughout animation and rest. Wrapping it in `tweened` would change its ancestor
/// identity at settle, discard its measured height and briefly lay it out at zero height.
#[derive(IntoElement)]
pub(super) struct OriginalDisclosure {
    pub(super) id: i64,
    pub(super) raw: String,
    pub(super) original: SharedString,
    pub(super) p: Palette,
    pub(super) open: bool,
}

impl RenderOnce for OriginalDisclosure {
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
        gpui_kit::base::Collapsible::new().reveal(("history-content", self.id as usize), progress).content(
            div().pt(px(10.)).child(
                div()
                    .w_full()
                    .px(px(16.))
                    .py(px(12.))
                    .flex()
                    .items_start()
                    .gap(px(12.))
                    .rounded(px(12.))
                    .bg(self.p.sub)
                    .opacity(progress)
                    .child(text(self.original, 12., FontWeight::MEDIUM, self.p.muted))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .child(text(self.raw, 12., FontWeight::NORMAL, self.p.muted).line_height(relative(1.6))),
                    ),
            ),
        )
    }
}
