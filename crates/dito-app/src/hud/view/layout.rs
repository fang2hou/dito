//! The vertical layout of the HUD window, shared by rendering and hit testing.

use crate::hud::capsule::CAPSULE_H;
use crate::hud::window::{WIN_H, WIN_PAD};

/// Gap between the capsule and the window's bottom padding.
pub(super) const BOTTOM_INSET: f32 = 35.;
/// Gap between the capsule, the controls above it and the error reason.
pub(super) const ZONE_GAP: f32 = 6.;
pub(super) const ERROR_BUBBLE_H: f32 = 30.5;
pub(super) const CAPSULE_TOP: f32 = WIN_H + 2. * WIN_PAD - (BOTTOM_INSET + WIN_PAD) - CAPSULE_H;
