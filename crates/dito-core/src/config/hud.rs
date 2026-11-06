use serde::{Deserialize, Serialize};

use super::General;

/// Clearance in points at height level 0, and the distance between two levels.
const BASE_CLEARANCE: i32 = 8;
const LEVEL_STEP: i32 = 19;
const MIN_LEVEL: i8 = -2;
const MAX_LEVEL: i8 = 12;

pub(super) const DEFAULT_HUD_OFFSET: u32 = BASE_CLEARANCE as u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HudPosition {
    #[default]
    Centered,
    Pointer,
}

impl General {
    pub fn hud_height_level(&self) -> i8 {
        self.hud_height_level
            .unwrap_or_else(|| {
                ((f64::from(self.hud_offset) - f64::from(BASE_CLEARANCE)) / f64::from(LEVEL_STEP)).round() as i8
            })
            .clamp(MIN_LEVEL, MAX_LEVEL)
    }

    /// Signed clearance of the countdown's bottom from the usable screen edge.
    pub fn hud_clearance(&self) -> i32 {
        BASE_CLEARANCE + i32::from(self.hud_height_level()) * LEVEL_STEP
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hud_levels_preserve_old_height_and_allow_negative_clearance() {
        let mut general = General::default();
        assert_eq!((general.hud_height_level(), general.hud_clearance()), (0, 8));
        general.hud_offset = 46;
        assert_eq!((general.hud_height_level(), general.hud_clearance()), (2, 46));
        general.hud_height_level = Some(-2);
        assert_eq!(general.hud_clearance(), -30);
        general.hud_height_level = Some(100);
        assert_eq!(general.hud_height_level(), 12);
    }
}
