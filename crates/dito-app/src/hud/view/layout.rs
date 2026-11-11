//! The vertical layout of the HUD window, shared by rendering and hit testing.

use crate::hud::capsule::CAPSULE_H;
use crate::hud::window::{WIN_H, WIN_PAD};

pub(super) const BOTTOM_INSET: f32 = 35.;
pub(super) const ZONE_GAP: f32 = 6.;
pub(super) const ERROR_BUBBLE_H: f32 = 30.5;
pub(super) const CAPSULE_TOP: f32 = WIN_H + 2. * WIN_PAD - (BOTTOM_INSET + WIN_PAD) - CAPSULE_H;
