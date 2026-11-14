//! The nav sidebar both windows render: brand corner, sliding selection plate, section headers and
//! rows, the collapsed icon rail and the hover reveal that names a rail icon.

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::health;
use super::nav::{
    NAV_COL_Y, NAV_GAP, NAV_H, NAV_SECTION_H, NavHost, NavPage, RAIL_MS, RAIL_PADDING, RAIL_WIDTH, nav_y,
    sidebar_top_height,
};
use crate::assets::Ic;
use crate::settings::Ctx;
use crate::theme::Palette;
use crate::ui::{self, text};
use dito_core::i18n::Tr;

pub(crate) fn sidebar<H: NavHost>(host: &H, c: &Ctx, cx: &mut Context<H>, rail: bool) -> Div {
    let p = &c.p;
    let (plate, contents) = nav_layers(host, c, rail);
    div()
        .w(px(if rail { RAIL_WIDTH } else { 232. }))
        .h_full()
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(4.))
        .px(px(if rail { RAIL_PADDING } else { 16. }))
        .pb(px(20.))
        .bg(p.side)
        .border_r_1()
        .border_color(p.border)
        .child(ui::drag_area("sidebar-top").h(px(sidebar_top_height(rail))).flex_none())
        .child(brand(host, c, rail))
        .child(
            div()
                .relative()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(NAV_GAP))
                .child(plate)
                .children(H::Page::sections().iter().flat_map(|(label, pages)| {
                    let mut children = vec![nav_section(c, rail, *label)];
                    children.extend(pages.iter().map(|page| nav_item(host, c, cx, rail, *page)));
                    children
                }))
                .child(contents),
        )
        .child(ui::drag_area("sidebar-drag").flex_1())
}

/// One tween fades the card in as a whole, so background and text arrive together. An invisible hit pill keeps the
/// hover (and forwards clicks) once the pointer travels past the rail edge onto the label.
pub(crate) fn rail_hover<H: NavHost>(host: &H, c: &Ctx, cx: &mut Context<H>) -> AnyElement {
    let p = c.p;
    let page = host.nav_page();
    let mut layer = div().absolute().size_full();
    for pg in H::Page::all() {
        let nav_id = ElementId::from(("nav", pg.index()));
        let pill_id = ElementId::from(("rail-pill", pg.index()));
        let hovered = ui::is_hovered(&nav_id) || ui::is_hovered(&pill_id);
        let y = NAV_COL_Y + nav_y(pg);
        if hovered {
            layer = layer.child(rail_hit_pill(pill_id, y, cx.listener(move |this, _, _, cx| this.select_page(pg, cx))));
        }
        let label = SharedString::from(c.lang.tr(pg.label()));
        let tween_id = ElementId::from(("rail-reveal", pg.index()));
        // Clicking a row selects it: the reveal hides at once (no fade) and the sliding plate
        // alone carries the transition — two moving layers would read as out of step.
        let selected = page == pg;
        let target = if hovered && !selected { 1. } else { 0. };
        let ms = if selected { 0 } else { RAIL_MS };
        layer = layer.child(ui::tweened(tween_id, target, ms, move |hv| {
            if hv <= 0.001 {
                return div().into_any_element();
            }
            rail_reveal_card(&p, pg.icon(), label.clone(), y, hv).into_any_element()
        }));
    }
    layer.into_any_element()
}

/// The wordmark itself never expands; the status card uses the measured dot bounds.
fn logo_mark<H: NavHost>(host: &H, c: &Ctx) -> AnyElement {
    let p = &c.p;
    let rows = health::rows(host.chrome().st(), c.lang);
    let healthy = rows.iter().all(|row| row.status.as_ref().is_none_or(|s| !s.is_problem()));
    let color = if healthy { p.success } else { p.danger };
    ui::drag_area("sidebar-brand-rail")
        .w_full()
        .h(px(51.))
        .relative()
        .child(
            div()
                .absolute()
                .left(px(8.))
                .top(px(-1.5))
                .child(ui::text("D", 48., FontWeight::SEMIBOLD, p.text).line_height(relative(1.))),
        )
        .child(div().absolute().left(px(29.)).top(px(28.)).child(health::dot(host.chrome(), px(13.5), color)))
        .into_any_element()
}

// Items are transparent hit areas (with the hover tint); icons and labels live in one layer above the
// plate, so their brightness follows the plate as it slides under them.
fn nav_item<H: NavHost>(host: &H, c: &Ctx, cx: &mut Context<H>, rail: bool, page: H::Page) -> AnyElement {
    let p = &c.p;
    let on = host.nav_page() == page;
    let id = ElementId::from(("nav", page.index()));
    let hit = div()
        .id(id.clone())
        .h(px(NAV_H))
        .relative()
        .rounded(px(12.))
        .cursor_pointer()
        .on_hover(ui::hover_tracker(id.clone()))
        .on_click(cx.listener(move |this, _, _, cx| this.select_page(page, cx)))
        .child(ui::hover_fill_if(&id, Some(12.), ui::hex_alpha(p.nav_on, 0.7), !on));
    // Rail mode centers a square 44×44 pill (the expanded row plate is the same shape); the
    // wrapper keeps the full-width hit strip of the expanded sidebar.
    if rail {
        div().w_full().h(px(NAV_H)).flex().justify_center().child(hit.w(px(NAV_H))).into_any_element()
    } else {
        hit.w_full().into_any_element()
    }
}

fn nav_layers<H: NavHost>(host: &H, c: &Ctx, rail: bool) -> (AnyElement, AnyElement) {
    let p = &c.p;
    let page = host.nav_page();
    let plate_y = nav_y(page);
    let shell_scope = ui::motion_scope();
    let (nav_on, text_c, muted_c, selected) = (p.nav_on, p.text, p.muted, page);
    let motion = ui::plate_state(ElementId::from("nav-plate"), (plate_y, plate_y + NAV_H));
    let plate = motion.animate("plate", move |top, bottom, energy| {
        let lit = ui::mix(nav_on, gpui_kit::hsla(0., 0., 1., 1.), 0.12 * energy);
        let plate = div().absolute().top(px(top)).h(px(bottom - top)).rounded(px(12.)).bg(lit);
        if rail {
            // Square pill over the icon, same left edge the hover reveal grows from. The nav
            // column already sits inside the rail's symmetric padding, so offsets are column-relative.
            plate.left(px(4.)).w(px(NAV_H))
        } else {
            plate.left(px(0.)).w_full()
        }
    });
    let entries: Vec<(H::Page, SharedString, bool, Option<Hsla>)> = H::Page::all()
        .into_iter()
        .map(|pg| {
            let hovered = ui::is_hovered(&ElementId::from(("nav", pg.index()))) && pg != selected;
            let badge = if rail { None } else { host.nav_badge(pg, p) };
            (pg, SharedString::from(c.lang.tr(pg.label())), hovered, badge)
        })
        .collect();
    let contents = motion.animate("content", move |top, bottom, _| {
        let mut layer = div().absolute().top(px(0.)).left(px(0.)).size_full();
        for (pg, label, hovered, badge) in entries.iter() {
            let y = nav_y(*pg);
            let cover = ui::coverage(top, bottom, y, y + NAV_H);
            let weight = if cover >= 0.5 { FontWeight::MEDIUM } else { FontWeight::NORMAL };
            let (ic, label) = (pg.icon(), label.clone());
            let hover_id = ElementId::from(("nav-hover", pg.index()));
            let dot = *badge;
            let row = ui::tweened_at(
                shell_scope.clone(),
                hover_id,
                if *hovered { 0.55 } else { 0. },
                ui::HOVER_MS,
                move |hv| {
                    let color = ui::mix(muted_c, text_c, hv.max(cover));
                    let mut row = div().w_full().flex().items_center().child(ui::icon(ic, 18., color));
                    if !rail {
                        let label = text(label.clone(), 14., weight, color);
                        // The badge sits at the far end of the row, so the label takes the slack.
                        let label =
                            if dot.is_some() { div().flex_1().min_w(px(0.)).child(label) } else { div().child(label) };
                        row = row.gap(px(12.)).child(label).children(dot.map(badge_dot));
                    }
                    row
                },
            );
            let slot = if rail {
                // Icon centered on the rail's axis (column-relative); the label lives in the
                // floating reveal instead.
                div().absolute().left(px(17.)).top(px(y)).h(px(NAV_H)).flex().items_center()
            } else {
                div().absolute().left(px(0.)).top(px(y)).w_full().h(px(NAV_H)).px(px(14.)).flex().items_center()
            };
            layer = layer.child(slot.child(row));
        }
        layer
    });
    (plate.into_any_element(), contents.into_any_element())
}

fn brand<H: NavHost>(host: &H, c: &Ctx, rail: bool) -> AnyElement {
    let p = &c.p;
    let rows = health::rows(host.chrome().st(), c.lang);
    let healthy = rows.iter().all(|row| row.status.as_ref().is_none_or(|s| !s.is_problem()));
    let dot_color = if healthy { p.success } else { p.danger };
    if rail {
        logo_mark(host, c)
    } else {
        ui::drag_area("sidebar-brand")
            .px(px(12.))
            .pb(px(14.))
            .flex()
            .items_end()
            .child(
                // The tracking after the last letter is the space before the dot: O-to-dot equals T-to-O.
                ui::tracked("DITO", 34., FontWeight::SEMIBOLD, p.text, 0.14),
            )
            // mb keeps the dot's ink bottom on the wordmark's ink bottom (items_end
            // alone would align to the line box).
            .child(health::dot(host.chrome(), px(12.), dot_color).mb(px(6.)))
            .into_any_element()
    }
}

/// A live session: one 7 pt dot at the far end of its row.
fn badge_dot(color: Hsla) -> AnyElement {
    div().flex_none().size(px(7.)).rounded_full().bg(color).into_any_element()
}

fn nav_section(c: &Ctx, rail: bool, label: Tr) -> AnyElement {
    let p = &c.p;
    if rail {
        // Section headers reduce to a centered hairline; the 36 px slot stays so rows keep their y.
        ui::drag_area(label.id())
            .w_full()
            .h(px(NAV_SECTION_H))
            .flex()
            .items_center()
            .justify_center()
            .child(div().w(px(16.)).h(px(1.5)).rounded(px(0.75)).bg(p.hair))
            .into_any_element()
    } else {
        ui::drag_area(label.id())
            .w_full()
            .h(px(NAV_SECTION_H))
            .px(px(14.))
            .flex()
            .items_end()
            .pb(px(6.))
            .child(text(c.lang.tr(label), 11., FontWeight::MEDIUM, p.muted))
            .into_any_element()
    }
}

fn rail_hit_pill(
    id: ElementId,
    y: f32,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id.clone())
        .absolute()
        .left(px(RAIL_PADDING + 4.))
        .top(px(y))
        .w(px(200.))
        .h(px(NAV_H))
        .rounded(px(12.))
        .cursor_pointer()
        .on_hover(ui::hover_tracker(id))
        .on_click(on_click)
}

/// Fixed-size card that fades in as a whole: no width animation, so the label never wraps mid-reveal and the icon
/// never moves.
fn rail_reveal_card(p: &Palette, icon: Ic, label: SharedString, y: f32, opacity: f32) -> Div {
    div()
        .absolute()
        .left(px(RAIL_PADDING + 4.))
        .top(px(y))
        .w(px(200.))
        .h(px(NAV_H))
        .rounded(px(12.))
        .bg(p.card)
        .shadow(vec![BoxShadow {
            color: p.shadow,
            offset: point(px(0.), px(6.)),
            blur_radius: px(24.),
            spread_radius: px(0.),
            inset: false,
        }])
        .opacity(opacity)
        .child(div().absolute().left(px(13.)).top(px(13.)).child(ui::icon(icon, 18., p.text)))
        .child(div().absolute().left(px(43.)).top(px(11.)).child(text(label, 14., FontWeight::MEDIUM, p.text)))
}
