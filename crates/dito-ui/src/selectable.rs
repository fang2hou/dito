use std::ops::Range;

use gpui_kit::base::{SelectableText, TextSelectionHandle, TextSelectionRegistration, TextSelectionRun};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::primitives::hex_alpha;
use super::typography::family_for;
use crate::Palette;

/// The window must be rooted in `Root`, which paints the selection and handles the copy.
pub fn selectable_text(
    p: &Palette,
    id: impl Into<ElementId>,
    s: impl Into<SharedString>,
    size: f32,
    weight: FontWeight,
    color: Hsla,
) -> Div {
    let s: SharedString = s.into();
    div()
        .font_family(family_for(&s))
        .text_size(px(size))
        .font_weight(weight)
        .text_color(color)
        .child(SelectableText::new(id, s).selection_color(hex_alpha(p.accent, 0.3)))
}

/// Selectable text in one run, some spans of which have their own color, for text that is one selection but not one
/// color. The font, size, line height and base color come from the parent. `spans` are byte ranges in order, without
/// overlap. Same contract as [`selectable_text`]: the window must be rooted in `Root`.
pub fn selectable_spans(
    p: &Palette,
    id: impl Into<ElementId>,
    s: impl Into<SharedString>,
    spans: impl IntoIterator<Item = (Range<usize>, Hsla)>,
) -> impl IntoElement {
    let s: SharedString = s.into();
    let highlights =
        spans.into_iter().map(|(range, color)| (range, HighlightStyle { color: Some(color), ..Default::default() }));
    Spans {
        id: id.into(),
        styled: StyledText::new(s.clone()).with_highlights(highlights),
        text: s,
        selection: hex_alpha(p.accent, 0.3),
    }
}

/// Selectable text whose runs carry their own fonts, for text that mixes faces (a reply's inline code). `runs` cover
/// `s` end to end. Same contract as [`selectable_text`]: the window must be rooted in `Root`.
pub fn selectable_runs(
    p: &Palette,
    id: impl Into<ElementId>,
    s: impl Into<SharedString>,
    runs: Vec<TextRun>,
) -> impl IntoElement {
    let s: SharedString = s.into();
    Spans {
        id: id.into(),
        styled: StyledText::new(s.clone()).with_runs(runs),
        text: s,
        selection: hex_alpha(p.accent, 0.3),
    }
}

/// [`SelectableText`] with highlights: the same registration and painting, which the base type does not expose.
struct Spans {
    id: ElementId,
    text: SharedString,
    styled: StyledText,
    selection: Hsla,
}

impl IntoElement for Spans {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Spans {
    type RequestLayoutState = TextSelectionHandle;
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let handle = window.with_element_state(
            global_id.expect("the element has an id"),
            |retained: Option<TextSelectionHandle>, _| {
                let handle = retained.unwrap_or_else(|| TextSelectionHandle::new(self.text.clone(), cx));
                (handle.clone(), handle)
            },
        );
        let (layout_id, ()) = self.styled.request_layout(global_id, inspector_id, window, cx);
        (layout_id, handle)
    }

    fn prepaint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        handle: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.styled.prepaint(global_id, inspector_id, bounds, &mut (), window, cx);
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let registration = TextSelectionRegistration::new(hitbox.clone(), bounds)
            .with_text_bounds(vec![bounds])
            .with_rendered_element(handle, window, cx);
        handle.register(registration, window, cx);
        hitbox
    }

    fn paint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        handle: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let layout = self.styled.layout().clone();
        let projection = handle.update_runs(&[TextSelectionRun::new(self.text.clone(), layout.clone(), bounds)], cx);
        for range in projection.ranges().iter().flatten().cloned() {
            paint_selection(&layout, range, self.selection, window);
        }
        self.styled.paint(global_id, inspector_id, bounds, &mut (), &mut (), window, cx);
    }
}

fn paint_selection(layout: &TextLayout, range: Range<usize>, color: Hsla, window: &mut Window) {
    let (Some(start), Some(end)) = (layout.position_for_index(range.start), layout.position_for_index(range.end))
    else {
        return;
    };
    let line_height = layout.line_height();
    let bounds = layout.bounds();
    let mut quads = Vec::with_capacity(3);
    if start.y == end.y {
        quads.push(Bounds::from_corners(start, point(end.x, end.y + line_height)));
    } else {
        quads.push(Bounds::from_corners(start, point(bounds.right(), start.y + line_height)));
        if end.y > start.y + line_height {
            quads.push(Bounds::from_corners(point(bounds.left(), start.y + line_height), point(bounds.right(), end.y)));
        }
        quads.push(Bounds::from_corners(point(bounds.left(), end.y), point(end.x, end.y + line_height)));
    }
    for quad in quads {
        window.paint_quad(fill(quad, color));
    }
}
