//! The brand health dot and the card it opens: the live state of every configured engine, readable
//! from either window's brand corner.

use dito_core::config::{EngineConfig, Source};
use dito_core::health::{EngineStatus, Health};
use dito_core::i18n::{Lang, Tr};
use dito_core::secrets::Role;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::Chrome;
use crate::settings::{Ctx, role};
use crate::state::AppState;
use crate::theme::Palette;
use crate::ui::{self, Motion, text};

/// A role stays visible even when its engine is disabled; disabled rows have no live status.
pub(crate) struct HealthRow {
    pub(crate) role: &'static str,
    pub(crate) name: SharedString,
    pub(crate) status: Option<EngineStatus>,
}

const HEALTH_RADIUS: f32 = 23.;
const HEALTH_PADDING: f32 = 12.;
const HEALTH_ROW_HEIGHT: f32 = 22.;
const HEALTH_ROW_GAP: f32 = 13.;
const HEALTH_RIGHT_PADDING: f32 = 20.;
const ROLE_GAP: f32 = 12.;
const STATUS_GAP: f32 = 16.;
const STATUS_DOT: f32 = 6.;
const STATUS_DOT_GAP: f32 = 6.;
const NAME_SIZE: f32 = 13.;
const NAME_LINE_HEIGHT: f32 = 19.;
const STATUS_SIZE: f32 = 12.;
/// The card opens on the motion/glass spring, width leading and height trailing, and settles with a small rebound; it
/// closes on a short ease without one.
const HEALTH_OPEN_MS: u64 = 460;
const HEALTH_CLOSE_MS: u64 = 200;
/// The height starts this share of the opening later than the width (64 ms of 460).
const HEALTH_TRAIL: f32 = 0.14;
const HEALTH_MIN_WIDTH: f32 = 322.;
const DOT_HIT: &str = "health-dot-hit";
const CARD_HIT: &str = "health-card-hit";

pub(crate) fn name(lang: Lang, role: Role, e: &EngineConfig, models: &dito_local::ModelManager) -> SharedString {
    match e.source {
        Source::Provider => {
            let asr_configurations = role::cloud_asr_configurations(role, e);
            let text_configurations = e.text_configurations(lang);
            let name = match asr_configurations.as_deref() {
                Some(configurations) => configurations.active_name(),
                None => text_configurations.active().map(|configuration| configuration.name.as_str()),
            };
            SharedString::from(name.unwrap_or_else(|| lang.tr(Tr::ModelsProvider)).to_owned())
        }
        Source::Local => SharedString::from(
            models
                .catalog()
                .iter()
                .find(|m| m.id == e.local_model)
                .map(|m| m.name.clone())
                .unwrap_or_else(|| e.local_model.clone()),
        ),
    }
}

pub(crate) fn rows(st: &AppState, lang: Lang) -> Vec<HealthRow> {
    let cfg = st.cfg.get();
    let health = Health::compute(&cfg, Some(&*st.local));
    vec![
        HealthRow {
            role: lang.tr(Tr::ModelsDictation),
            name: name(lang, Role::Asr, &cfg.asr, &st.models),
            status: Some(health.asr),
        },
        HealthRow {
            role: lang.tr(Tr::ModelsTextModelShort),
            name: if cfg.polish.enabled {
                name(lang, Role::Text, &cfg.text_model, &st.models)
            } else {
                SharedString::new_static(lang.tr(Tr::ModelsPolishOff))
            },
            status: cfg.polish.enabled.then_some(health.polish),
        },
    ]
}

pub(crate) fn hovered(shell: &ui::ScopeHandle) -> bool {
    ui::is_hovered_in(shell, &ElementId::from(DOT_HIT)) || ui::is_hovered_in(shell, &ElementId::from(CARD_HIT))
}

fn status_label(p: Palette, lang: Lang, status: &EngineStatus) -> (Hsla, &'static str) {
    match status {
        EngineStatus::Remote => (p.muted, lang.tr(Tr::ModelsConfigured)),
        EngineStatus::Running => (p.success, lang.tr(Tr::ModelsRunning)),
        EngineStatus::Stopping => (p.warning_text, lang.tr(Tr::ModelsStopping)),
        EngineStatus::Loading => (p.warning_text, lang.tr(Tr::MenubarLoading)),
        EngineStatus::Failed(_) => (p.danger, lang.tr(Tr::ModelsFailed)),
        EngineStatus::Stopped => (p.muted, lang.tr(Tr::MenubarStopped)),
        EngineStatus::NotInstalled => (p.muted, lang.tr(Tr::ModelsNotInstalled)),
    }
}

fn chip(p: Palette, tone: Hsla, label: &'static str) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(STATUS_DOT_GAP))
        .flex_none()
        .child(text(label, STATUS_SIZE, FontWeight::MEDIUM, p.muted).line_height(px(16.)))
        .child(div().size(px(STATUS_DOT)).rounded_full().bg(tone))
}

/// The glow is wider than the visible punctuation; the card adds a further 10 px outside it.
pub(crate) fn glow(size: Pixels, color: Hsla, opacity: f32) -> Div {
    div()
        .absolute()
        .left(size / 2. - px(13.))
        .top(size / 2. - px(13.))
        .size(px(26.))
        .rounded_full()
        .bg(Hsla { a: 0.20 * opacity, ..color })
        .shadow(vec![BoxShadow {
            color: Hsla { a: 0.18 * opacity, ..color },
            offset: point(px(0.), px(0.)),
            blur_radius: px(6.),
            spread_radius: px(0.),
            inset: false,
        }])
}

/// Measure the real punctuation so the floating corner also follows font and rail changes.
pub(crate) fn dot(chrome: &Chrome, size: Pixels, color: Hsla) -> Div {
    let hit_id = ElementId::from(DOT_HIT);
    let hovered = hovered(&chrome.motion());
    let bounds = chrome.health_dot_cell();
    div()
        .relative()
        .flex_none()
        .size(size)
        .rounded_full()
        .bg(color)
        .child(
            canvas(
                move |actual, window, _| {
                    if bounds.replace(Some(actual)) != Some(actual) {
                        window.refresh();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        )
        .child(ui::tweened(ElementId::from("health-halo"), if hovered { 1. } else { 0. }, 150, move |t| {
            glow(size, color, t)
        }))
        .child(
            div()
                .id(hit_id.clone())
                .absolute()
                .left(size / 2. - px(15.))
                .top(size / 2. - px(15.))
                .size(px(30.))
                .rounded_full()
                .on_hover(ui::hover_tracker(hit_id)),
        )
}

/// The dot is both logo punctuation and the center of the card's top-left corner.
/// Prime the tween while closed, and keep it mounted to reverse smoothly on mouse-out.
pub(crate) fn overlay(chrome: &Chrome, c: &Ctx, window: &Window) -> AnyElement {
    let hovered = chrome.health_hovered();
    let rows = rows(chrome.st(), c.lang);
    let color = if rows.iter().any(|row| row.status.as_ref().is_some_and(EngineStatus::is_problem)) {
        c.p.danger
    } else {
        c.p.success
    };
    let bounds = chrome.health_dot_bounds().unwrap_or_default();
    let radius = px(HEALTH_RADIUS);
    let origin = bounds.center() - point(radius, radius);
    let viewport = window.viewport_size();
    let available =
        size(viewport.width - origin.x - px(HEALTH_PADDING), viewport.height - origin.y - px(HEALTH_PADDING));
    let popup = HealthPopup::new(c, rows, bounds.size.width, color, available);
    let motion = Motion::system();
    let hit_id = ElementId::from(CARD_HIT);
    div()
        .id(hit_id.clone())
        .absolute()
        .left(origin.x)
        .top(origin.y)
        // The stable hit surface includes both the dot and the card. It never replaces the
        // animated element's handler mid-flight, and cannot intercept anything while closed.
        .w(px(if hovered { popup.width } else { 0. }))
        .h(px(if hovered { popup.height } else { 0. }))
        .rounded(radius)
        .on_hover(ui::hover_tracker(hit_id))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(ui::tweened_linear(
            ElementId::from("health-card"),
            if hovered { 1. } else { 0. },
            match (motion.is_reduced(), hovered) {
                (true, _) => 120,
                (false, true) => HEALTH_OPEN_MS,
                (false, false) => HEALTH_CLOSE_MS,
            },
            move |t| popup.at(t, hovered, motion),
        ))
        .into_any_element()
}

struct CardRow {
    role: &'static str,
    name: SharedString,
    status: Option<(Hsla, &'static str)>,
    height: f32,
}

struct HealthPopup {
    p: Palette,
    lang: Lang,
    rows: Vec<CardRow>,
    dot_size: Pixels,
    color: Hsla,
    label_left: f32,
    role_width: f32,
    name_width: f32,
    status_width: f32,
    width: f32,
    height: f32,
    scrollable: bool,
}

impl HealthPopup {
    fn new(c: &Ctx, rows: Vec<HealthRow>, dot_size: Pixels, color: Hsla, available: Size<Pixels>) -> Self {
        let label_left = HEALTH_RADIUS - f32::from(dot_size) / 2.;
        let role_width = rows
            .iter()
            .map(|row| (ui::text_width(row.role, 11., FontWeight::MEDIUM) + 14.).ceil())
            .fold(58_f32, f32::max);
        let mut rows: Vec<_> = rows
            .into_iter()
            .map(|row| CardRow {
                role: row.role,
                name: row.name,
                status: row.status.as_ref().map(|status| status_label(c.p, c.lang, status)),
                height: HEALTH_ROW_HEIGHT,
            })
            .collect();
        let status_width = rows
            .iter()
            .filter_map(|row| row.status)
            .map(|(_, label)| ui::text_width(label, STATUS_SIZE, FontWeight::MEDIUM) + STATUS_DOT_GAP + STATUS_DOT)
            .fold(0_f32, f32::max)
            .ceil();
        let widest_name = rows
            .iter()
            .flat_map(|row| row.name.split('\n'))
            .map(|line| ui::text_width(line, NAME_SIZE, FontWeight::MEDIUM))
            .fold(0_f32, f32::max)
            .ceil();
        let fixed_width = label_left + role_width + ROLE_GAP + STATUS_GAP + status_width + HEALTH_RIGHT_PADDING;
        let width = (fixed_width + widest_name).max(HEALTH_MIN_WIDTH).min(f32::from(available.width));
        let name_width = (width - fixed_width).max(1.);
        for row in &mut rows {
            let lines = ui::text_lines(&row.name, ui::family_for(&row.name), NAME_SIZE, FontWeight::MEDIUM, name_width);
            row.height = (lines as f32 * NAME_LINE_HEIGHT).max(HEALTH_ROW_HEIGHT);
        }
        let content_height =
            HEALTH_PADDING * 2. + HEALTH_ROW_HEIGHT + rows.iter().map(|row| row.height + HEALTH_ROW_GAP).sum::<f32>();
        let height = content_height.min(f32::from(available.height));

        Self {
            p: c.p,
            lang: c.lang,
            rows,
            dot_size,
            color,
            label_left,
            role_width,
            name_width,
            status_width,
            width,
            height,
            scrollable: height < content_height,
        }
    }

    /// The text is laid out at the settled width throughout the reveal.
    fn card(&self) -> Div {
        let p = self.p;
        div()
            .w(px(self.width))
            .flex_none()
            .flex()
            .flex_col()
            .py(px(HEALTH_PADDING))
            .gap(px(HEALTH_ROW_GAP))
            .child(
                div()
                    .pl(px(45.))
                    .h(px(HEALTH_ROW_HEIGHT))
                    .flex()
                    .items_center()
                    .child(text(self.lang.tr(Tr::ModelsState), 14., FontWeight::BOLD, p.text).line_height(px(19.))),
            )
            .children(self.rows.iter().map(|row| {
                div()
                    .ml(px(self.label_left))
                    .h(px(row.height))
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .w(px(self.role_width))
                            .h(px(HEALTH_ROW_HEIGHT))
                            .mr(px(ROLE_GAP))
                            .flex_none()
                            .rounded_full()
                            .bg(if p.dark { p.bg } else { p.tile_idle })
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(text(row.role, 11., FontWeight::MEDIUM, p.muted).line_height(px(16.))),
                    )
                    .child(
                        text(
                            row.name.clone(),
                            NAME_SIZE,
                            FontWeight::MEDIUM,
                            if row.status.is_some() { p.text } else { p.muted },
                        )
                        .line_height(px(NAME_LINE_HEIGHT))
                        .whitespace_normal()
                        .w(px(self.name_width))
                        .flex_none(),
                    )
                    .children(row.status.map(|(tone, label)| {
                        div()
                            .ml(px(STATUS_GAP))
                            .w(px(self.status_width))
                            .flex_none()
                            .flex()
                            .justify_end()
                            .child(chip(p, tone, label))
                    }))
            }))
    }

    /// `t` is the linear progress toward open; `opening` picks the spring or the closing ease.
    fn at(&self, t: f32, opening: bool, motion: Motion) -> Div {
        if t <= 0.001 {
            return div();
        }
        let Self { p, dot_size, color, .. } = *self;
        let radius = px(HEALTH_RADIUS);
        let reduced = motion.is_reduced();
        let ease = |x: f32| x * x * (3. - 2. * x);
        let (wide, tall) = if reduced {
            (1., 1.)
        } else if opening {
            (ui::spring(t, motion), ui::spring(((t - HEALTH_TRAIL) / (1. - HEALTH_TRAIL)).max(0.), motion))
        } else {
            (ease(t), ease(t))
        };
        let text_opacity = if reduced { t } else { ((t - 0.25) / 0.75).clamp(0., 1.) };
        // The surface arrives in the first half of the opening and leaves on the closing ease.
        let t = if opening { (t * 2.).min(1.) } else { ease(t) };
        div()
            .absolute()
            .left_0()
            .top_0()
            .w(px(46. + (self.width - 46.) * wide))
            .h(px(46. + (self.height - 46.) * tall))
            .rounded(radius)
            .bg(p.menu_bg)
            .opacity(t)
            .shadow(vec![BoxShadow {
                color: Hsla { a: if p.dark { 0.28 } else { 0.12 }, ..p.shadow },
                offset: point(px(0.), px(4.)),
                blur_radius: px(18.),
                spread_radius: px(0.),
                inset: false,
            }])
            .child(div().absolute().inset_0().rounded(radius).border_1().border_color(p.border))
            .child(
                div().absolute().inset_0().rounded(radius).overflow_hidden().child(
                    div()
                        .id("health-card-content")
                        .absolute()
                        .left_0()
                        .top_0()
                        .w(px(self.width))
                        .h(px(self.height))
                        .when(self.scrollable, |el| el.overflow_y_scroll())
                        .opacity(text_opacity)
                        .child(self.card()),
                ),
            )
            .child(
                div()
                    .absolute()
                    .left(radius - dot_size / 2.)
                    .top(radius - dot_size / 2.)
                    .size(dot_size)
                    .child(glow(dot_size, color, 1.))
                    .child(div().absolute().inset_0().rounded_full().bg(color)),
            )
    }
}
