//! The floating HUD: a capsule near the bottom of the screen that mirrors the dictation state.
//! Geometry is authored in design points and drawn at [`HUD_SCALE`].

mod buttons;
mod capsule;
mod elevation;
mod motion;
mod outcome_icon;
mod progress;
mod scale;
mod shimmer;
mod style_preview;
mod timing;
mod view;
pub(crate) mod waveform;
mod window;

pub(crate) use buttons::{Btn, legend_button, static_button};
#[cfg(test)]
pub(crate) use capsule::NARROW_W;
pub(crate) use capsule::{CAPSULE_H, CAPSULE_W, Capsule, Wave, eased_width, pop_shape, rest_width};
pub use scale::HUD_SCALE;
pub(crate) use scale::Scale;
use scale::{px, tx};
pub(crate) use style_preview::StylePreview;
pub(crate) use timing::{OUTCOME_END, OUTCOME_FADE_S, OUTCOME_ICON_DELAY, fill_at};
pub(crate) use view::POP_MS;
#[cfg(feature = "screenshots")]
pub use view::{HudView, PreviewHover};
#[cfg(feature = "screenshots")]
pub use window::{WIN_H, WIN_W};
pub use window::{WIN_PAD, run};
