//! One debug line per HUD opening: whether the first frames or the microphone made the ocean start late.

use std::time::{Duration, Instant};

const WATCHED: Duration = Duration::from_secs(1);

pub(super) struct Opening {
    built: Instant,
    first_tick: Option<Instant>,
    last_tick: Instant,
    longest_gap: Duration,
    first_level: Option<Duration>,
    calibrated: Option<Duration>,
}

impl Opening {
    /// `None` unless the line would be written, so a tick pays for nothing else.
    pub(super) fn new() -> Option<Self> {
        tracing::enabled!(tracing::Level::DEBUG).then(|| {
            let built = Instant::now();
            Self {
                built,
                first_tick: None,
                last_tick: built,
                longest_gap: Duration::ZERO,
                first_level: None,
                calibrated: None,
            }
        })
    }

    pub(super) fn heard(&mut self, level: f32, calibrated: bool) {
        let since_first_tick = self.first_tick.map_or(Duration::ZERO, |first| first.elapsed());
        if level > 0. {
            self.first_level.get_or_insert(since_first_tick);
        }
        if calibrated {
            self.calibrated.get_or_insert(since_first_tick);
        }
    }

    /// Returns true once the line is written.
    pub(super) fn tick(&mut self) -> bool {
        let now = Instant::now();
        let Some(first) = self.first_tick else {
            self.first_tick = Some(now);
            self.last_tick = now;
            return false;
        };
        if now.duration_since(first) <= WATCHED {
            self.longest_gap = self.longest_gap.max(now.duration_since(self.last_tick));
            self.last_tick = now;
            return false;
        }
        tracing::debug!(
            waited = ?first.duration_since(self.built),
            first_level = ?self.first_level,
            calibrated = ?self.calibrated,
            longest_gap = ?self.longest_gap,
            "hud opening: the level and the end of calibration after the first tick, and the longest gap in that second"
        );
        true
    }
}
