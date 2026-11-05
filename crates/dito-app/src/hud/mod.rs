//! The floating HUD: a capsule near the bottom of the screen that mirrors the dictation state.
//! Geometry below is authored in design points and drawn at [`HUD_SCALE`].

mod drawing;
mod motion;
mod view;
pub(crate) mod waveform;
mod window;

pub(crate) use drawing::{
    Btn, CAPSULE_H, CAPSULE_W, Capsule, OUTCOME_END, OUTCOME_FADE_S, OUTCOME_ICON_DELAY, Wave, eased_width, fill_at,
    legend_button, pop_shape, rest_width, static_button,
};
#[cfg(test)]
pub(crate) use drawing::{NARROW_W, WIDEN_S};
#[cfg(feature = "screenshots")]
pub use view::HudView;
pub(crate) use view::POP_MS;
#[cfg(feature = "screenshots")]
pub use window::WIN_H;
pub use window::{WIN_PAD, WIN_W, run};

/// The canonical FigCraft geometry is authored at 1×: a 105 × 33 capsule in whole design points.
pub const HUD_SCALE: f32 = 1.;

/// The scale a capsule is drawn at: the HUD window and every preview (setup wizard's usage demo,
/// settings style preview) all draw at the same canonical size.
#[derive(Clone, Copy)]
pub(crate) struct Scale(f32);

impl Scale {
    pub(crate) const HUD: Self = Self(HUD_SCALE);
    pub(crate) const DESIGN: Self = Self(1.);

    /// A design-point length, in pixels.
    pub(crate) fn px(self, v: f32) -> gpui_kit::Pixels {
        gpui_kit::px(self.tx(v))
    }

    /// Design-point sizes for the ui helpers (`text`, `hud_icon`), which take raw points and do not pass
    /// through `px`: same scale and snapping, so type and geometry grow together.
    pub(crate) fn tx(self, v: f32) -> f32 {
        v * self.0
    }
}

/// Module-local `px` applying [`HUD_SCALE`] to every dimension of the HUD window's own layout; shadows the glob import.
fn px(v: f32) -> gpui_kit::Pixels {
    Scale::HUD.px(v)
}

/// [`px`] for the ui helpers (`text`, `hud_icon`), which take raw points.
fn tx(v: f32) -> f32 {
    Scale::HUD.tx(v)
}
