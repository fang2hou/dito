use dito_core::config::{EngineConfig, Source};
use dito_core::health::{EngineStatus, Health};
use dito_core::i18n::{Lang, Tr};
use dito_core::secrets::Role;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::shell::Settings;
use super::{Ctx, role};
use crate::theme::Palette;
use crate::ui::{self, Motion, text};

/// A role stays visible even when its engine is disabled; disabled rows have no live status.
pub(super) struct HealthRow {
    pub(super) role: &'static str,
    pub(super) name: SharedString,
    pub(super) status: Option<EngineStatus>,
}

pub(super) const HEALTH_RADIUS: f32 = 23.;
pub(super) const HEALTH_PADDING: f32 = 12.;
pub(super) const HEALTH_ROW_HEIGHT: f32 = 22.;
pub(super) const HEALTH_ROW_GAP: f32 = 13.;
pub(super) const HEALTH_HEIGHT: f32 = HEALTH_PADDING * 2. + HEALTH_ROW_HEIGHT * 3. + HEALTH_ROW_GAP * 2.;
pub(super) const HEALTH_MS: u64 = 320;
const HEALTH_WIDTH: f32 = 322.;

impl Settings {
    /// The engine's display name shown in the health card.
    pub(super) fn health_name(
        lang: Lang,
        role: Role,
        e: &EngineConfig,
        models: &dito_local::ModelManager,
    ) -> SharedString {
        match e.source {
            Source::Provider => {
                let profiles = role::cloud_profiles(role, e);
                let configurations = e.polish_configurations(lang);
                let name = match profiles.as_deref() {
                    Some(profiles) => profiles.active_name(),
                    None => configurations.active().map(|configuration| configuration.name.as_str()),
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

    /// Role labels describe the engine's purpose, independently of its live status color.
    pub(super) fn health_rows(&self, lang: Lang) -> Vec<HealthRow> {
        let cfg = self.st.cfg.get();
        let health = Health::compute(&cfg, Some(&*self.st.local));
        vec![
            HealthRow {
                role: lang.tr(Tr::ModelsDictation),
                name: Self::health_name(lang, Role::Asr, &cfg.asr, &self.st.models),
                status: Some(health.asr),
            },
            HealthRow {
                role: lang.tr(Tr::ModelsPolish),
                name: if cfg.polish.enabled {
                    Self::health_name(lang, Role::Polish, &cfg.polish.engine, &self.st.models)
                } else {
                    SharedString::from(lang.tr(Tr::ModelsPolishOff))
                },
                status: cfg.polish.enabled.then_some(health.polish),
            },
        ]
    }

    pub(super) fn health_hovered(shell: &ui::ScopeHandle) -> bool {
        ui::is_hovered_in(shell, &ElementId::from("health-dot-hit"))
            || ui::is_hovered_in(shell, &ElementId::from("health-card-hit"))
    }

    /// A dot and unfilled text distinguish runtime status from the role labels in this popover.
    pub(super) fn health_chip(p: Palette, lang: Lang, status: &EngineStatus) -> Div {
        let (tone, label) = match status {
            EngineStatus::Remote => (p.muted, lang.tr(Tr::ModelsConfigured)),
            EngineStatus::Running => (p.success, lang.tr(Tr::ModelsRunning)),
            EngineStatus::Stopping => (p.warning_text, lang.tr(Tr::ModelsStopping)),
            EngineStatus::Loading => (p.warning_text, lang.tr(Tr::MenubarLoading)),
            EngineStatus::Failed(_) => (p.danger, lang.tr(Tr::ModelsFailed)),
            EngineStatus::Stopped => (p.muted, lang.tr(Tr::MenubarStopped)),
            EngineStatus::NotInstalled => (p.muted, lang.tr(Tr::ModelsNotInstalled)),
        };
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .flex_none()
            .child(text(label, 12., FontWeight::MEDIUM, p.muted).line_height(px(16.)))
            .child(div().size(px(6.)).rounded_full().bg(tone))
    }

    /// Fixed text geometry inside the growing surface: reveal text without scaling or reflow.
    pub(super) fn health_card(p: Palette, lang: Lang, rows: &[HealthRow], dot_size: Pixels, width: f32) -> Div {
        let label_left = HEALTH_RADIUS - f32::from(dot_size) / 2.;
        let role_width = 58.;
        let name_left = label_left + role_width + 12.;
        let mut card = div().absolute().left_0().top_0().w(px(width)).h(px(HEALTH_HEIGHT)).child(
            div()
                .absolute()
                .left(px(45.))
                .top(px(HEALTH_PADDING))
                .h(px(HEALTH_ROW_HEIGHT))
                .flex()
                .items_center()
                .child(text(lang.tr(Tr::ModelsState), 14., FontWeight::BOLD, p.text).line_height(px(19.))),
        );
        for (index, row) in rows.iter().enumerate() {
            let top = HEALTH_PADDING + (index + 1) as f32 * (HEALTH_ROW_HEIGHT + HEALTH_ROW_GAP);
            card = card
                .child(
                    div()
                        .absolute()
                        .left(px(label_left))
                        .top(px(top))
                        .w(px(role_width))
                        .h(px(22.))
                        .rounded_full()
                        .bg(if p.dark { p.bg } else { p.tile_idle })
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(text(row.role, 11., FontWeight::MEDIUM, p.muted).line_height(px(16.))),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(name_left))
                        .top(px(top))
                        .w(px(width - name_left - 20.))
                        .h(px(22.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(16.))
                        .child(
                            div().flex_1().min_w(px(0.)).child(
                                text(
                                    row.name.clone(),
                                    13.,
                                    FontWeight::MEDIUM,
                                    if row.status.is_some() { p.text } else { p.muted },
                                )
                                .line_height(px(19.))
                                .truncate(),
                            ),
                        )
                        .children(row.status.as_ref().map(|status| Self::health_chip(p, lang, status))),
                );
        }
        card
    }

    /// The glow is wider than the visible punctuation; the card adds a further 10 px outside it.
    pub(super) fn health_glow(size: Pixels, color: Hsla, opacity: f32) -> Div {
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
    pub(super) fn health_dot(&self, size: Pixels, color: Hsla) -> Div {
        let hit_id = ElementId::from("health-dot-hit");
        let hovered = Self::health_hovered(&self.motion.handle());
        let bounds = self.health_dot_bounds.clone();
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
                Self::health_glow(size, color, t)
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
    pub(super) fn health_overlay(&self, c: &Ctx) -> AnyElement {
        let hovered = Self::health_hovered(&self.motion.handle());
        let rows = self.health_rows(c.lang);
        let color = if rows.iter().any(|row| row.status.as_ref().is_some_and(EngineStatus::is_problem)) {
            c.p.danger
        } else {
            c.p.success
        };
        let bounds = self.health_dot_bounds.get().unwrap_or_default();
        let popup = HealthPopup { p: c.p, lang: c.lang, rows, dot_size: bounds.size.width, color };
        let radius = px(HEALTH_RADIUS);
        let motion = Motion::system();
        let hit_id = ElementId::from("health-card-hit");
        div()
            .id(hit_id.clone())
            .absolute()
            .left(bounds.center().x - radius)
            .top(bounds.center().y - radius)
            // The stable hit surface includes both the dot and the card. It never replaces the
            // animated element's handler mid-flight, and cannot intercept anything while closed.
            .w(px(if hovered { HEALTH_WIDTH } else { 0. }))
            .h(px(if hovered { HEALTH_HEIGHT } else { 0. }))
            .rounded(radius)
            .on_hover(ui::hover_tracker(hit_id))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(ui::tweened(
                ElementId::from("health-card"),
                if hovered { 1. } else { 0. },
                if motion.is_reduced() { 120 } else { HEALTH_MS },
                move |t| popup.at(t, motion),
            ))
            .into_any_element()
    }
}

/// What the open health card shows: the rows next to the dot it grows out of.
struct HealthPopup {
    p: Palette,
    lang: Lang,
    rows: Vec<HealthRow>,
    dot_size: Pixels,
    color: Hsla,
}

impl HealthPopup {
    /// The card `t` of the way open: it grows from the dot while its rows fade in behind it.
    fn at(&self, t: f32, motion: Motion) -> Div {
        if t <= 0.001 {
            return div();
        }
        let Self { p, lang, dot_size, color, .. } = *self;
        let radius = px(HEALTH_RADIUS);
        let reduced = motion.is_reduced();
        let progress = if reduced { 1. } else { t };
        let text_opacity = if reduced { t } else { ((t - 0.25) / 0.75).clamp(0., 1.) };
        div()
            .absolute()
            .left_0()
            .top_0()
            .w(px(46. + (HEALTH_WIDTH - 46.) * progress))
            .h(px(46. + (HEALTH_HEIGHT - 46.) * progress))
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
                div()
                    .absolute()
                    .inset_0()
                    .rounded(radius)
                    .overflow_hidden()
                    .child(Settings::health_card(p, lang, &self.rows, dot_size, HEALTH_WIDTH).opacity(text_opacity)),
            )
            .child(
                div()
                    .absolute()
                    .left(radius - dot_size / 2.)
                    .top(radius - dot_size / 2.)
                    .size(dot_size)
                    .child(Settings::health_glow(dot_size, color, 1.))
                    .child(div().absolute().inset_0().rounded_full().bg(color)),
            )
    }
}
