//! The vertical layout of the HUD window, shared by rendering and hit testing.

use crate::hud::capsule::CAPSULE_H;
use crate::hud::window::{WIN_H, WIN_PAD};

pub(super) const BOTTOM_INSET: f32 = 35.;
pub(super) const ZONE_GAP: f32 = 6.;
pub(super) const ERROR_BUBBLE_H: f32 = 30.5;
pub(super) const CAPSULE_TOP: f32 = WIN_H + 2. * WIN_PAD - (BOTTOM_INSET + WIN_PAD) - CAPSULE_H;
/// Command windows sit this far above the capsule, and their action buttons this far above the window.
pub(super) const COMMAND_GAP: f32 = 10.;
/// From the window's bottom edge to the bottom of a command window.
pub(super) const COMMAND_BOTTOM: f32 = BOTTOM_INSET + WIN_PAD + CAPSULE_H + COMMAND_GAP;
/// The tallest command window, which scrolls its text beyond this.
pub(super) const COMMAND_MAX_H: f32 = 250.;
