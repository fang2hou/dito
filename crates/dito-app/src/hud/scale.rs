//! The scale a HUD capsule is drawn at.

use gpui_kit::Pixels;

/// The canonical FigCraft geometry is authored at 1x, in whole design points.
pub const HUD_SCALE: f32 = 1.;

/// The scale a capsule is drawn at: the HUD window and every preview (setup wizard's usage demo,
/// settings style preview) all draw at the same canonical size.
#[derive(Clone, Copy)]
pub(crate) struct Scale(pub(super) f32);

impl Scale {
    pub(crate) const HUD: Self = Self(HUD_SCALE);
    pub(crate) const DESIGN: Self = Self(1.);

    /// A design-point length, in pixels.
    pub(crate) fn px(self, v: f32) -> Pixels {
        gpui_kit::px(self.tx(v))
    }

    /// Design-point sizes for the ui helpers (`text`, `hud_icon`), which take raw points and do not pass
    /// through `px`: same scale and snapping, so type and geometry grow together.
    pub(crate) fn tx(self, v: f32) -> f32 {
        v * self.0
    }
}

/// [`Scale::px`] at the HUD window's own scale, for every dimension of its layout.
pub(super) fn px(v: f32) -> Pixels {
    Scale::HUD.px(v)
}

/// [`Scale::tx`] at the HUD window's own scale.
pub(super) fn tx(v: f32) -> f32 {
    Scale::HUD.tx(v)
}
