//! Presentation-only audio envelope and listening ocean. Never changes recorded audio.

use gpui_kit::*;

use super::Scale;
use super::buttons::BTN;
use super::capsule::CAPSULE_PAD;
use super::motion::{EntryPose, smooth};
use crate::ui::{ease, lerp};

#[derive(Clone, Copy)]
struct OceanPreset {
    frequency: f32,
    depth_lag: f32,
    curvature: f32,
    overlap_lift: f32,
    sparkle: f32,
    middle_speed: f32,
    quick_speed: f32,
}

// `design:hud/listening-ocean`. Swap the default or tune a field here without changing the renderer.
const PRESETS: [OceanPreset; 5] = [
    OceanPreset {
        frequency: 0.29,
        depth_lag: 1.4,
        curvature: 0.9,
        overlap_lift: 0.35,
        sparkle: 0.25,
        middle_speed: 1.040,
        quick_speed: 1.760,
    },
    OceanPreset {
        frequency: 0.34,
        depth_lag: 5.2,
        curvature: 1.6,
        overlap_lift: 0.38,
        sparkle: 0.27,
        middle_speed: 1.095,
        quick_speed: 1.825,
    },
    OceanPreset {
        frequency: 0.31,
        depth_lag: 3.2,
        curvature: 2.0,
        overlap_lift: 1.08,
        sparkle: 0.27,
        middle_speed: 1.150,
        quick_speed: 1.890,
    },
    OceanPreset {
        frequency: 0.31,
        depth_lag: 2.5,
        curvature: 8.5,
        overlap_lift: 0.64,
        sparkle: 0.25,
        middle_speed: 1.205,
        quick_speed: 1.955,
    },
    OceanPreset {
        frequency: 0.36,
        depth_lag: 3.1,
        curvature: 3.2,
        overlap_lift: 0.54,
        sparkle: 0.48,
        middle_speed: 1.260,
        quick_speed: 2.020,
    },
];
const SPARKLE_CURRENT: OceanPreset = PRESETS[4];

/// The water's own gentle motion while the microphone level cannot be trusted yet: no buffer has arrived, or the noise
/// floor is still being measured (`design:hud/listening-ocean`). It is drawn over the model and never read back by it.
struct StartMotion {
    /// The level the water is lifted to, well below quiet speech.
    swell: f32,
    /// How far a ripple pushes the water back and forth, in units of the ocean's phase.
    sway: f32,
    /// Ripple speed in radians per second.
    speed: f32,
    /// Columns of dots one ripple spans.
    ripple_columns: f32,
    /// How far the front row trails the back row, in radians.
    row_lag: f32,
    /// Seconds the motion takes to fade once the real level drives the water.
    blend_out_s: f32,
    /// It fades after this many seconds even without audio, so an input that stays silent does not look alive.
    longest_s: f32,
}

const START_MOTION: StartMotion = StartMotion {
    swell: 0.15,
    sway: 1.4,
    speed: 8.5,
    ripple_columns: 28.,
    row_lag: 1.6,
    blend_out_s: 0.3,
    longest_s: 1.2,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OceanFrame {
    level: f32,
    cadence: f32,
    phase: f32,
    /// How much of [`START_MOTION`] shows, from 1 down to 0, and the angle its ripple has reached.
    start: f32,
    start_turn: f32,
}

impl Default for OceanFrame {
    fn default() -> Self {
        Self { level: 0., cadence: 0., phase: 3.5, start: 0., start_turn: 0. }
    }
}

#[derive(Clone, Copy, Default)]
struct StartClock {
    age: f32,
    fading: f32,
}

/// Between one and two 120 Hz frames, so the ceiling is 60 redraws a second and a late frame callback cannot push
/// the next redraw to the refresh after it.
const MIN_REDRAW_S: f32 = 0.011;
/// Quiet water drifts this far in about 27 ms: every second refresh at 60 Hz, with the same room for a late callback.
const QUIET_PHASE_STEP: f32 = 0.036;

pub(crate) struct Waveform {
    calibration_left: f32,
    startup_min: f32,
    floor: f32,
    peak: f32,
    primed: bool,
    last_signal: f32,
    frame: OceanFrame,
    last_drawn: OceanFrame,
    since_drawn: f32,
    /// `None` once the start motion has faded out.
    start: Option<StartClock>,
}

impl Default for Waveform {
    fn default() -> Self {
        Self {
            frame: OceanFrame { start: 1., ..OceanFrame::default() },
            start: Some(StartClock::default()),
            ..Self::without_start_motion()
        }
    }
}

impl Waveform {
    fn without_start_motion() -> Self {
        Self {
            calibration_left: 0.12,
            startup_min: f32::INFINITY,
            floor: 0.00006,
            peak: 0.0002,
            primed: false,
            last_signal: 0.,
            frame: OceanFrame::default(),
            last_drawn: OceanFrame::default(),
            since_drawn: 0.,
            start: None,
        }
    }

    fn advance_start_motion(&mut self, dt: f32) {
        let Some(clock) = &mut self.start else { return };
        clock.age += dt;
        if self.calibration_left <= 0. || clock.age >= START_MOTION.longest_s {
            clock.fading += dt;
        }
        let strength = 1. - smooth(clock.fading / START_MOTION.blend_out_s);
        if strength > 0. {
            self.frame.start = strength;
            self.frame.start_turn = clock.age * START_MOTION.speed;
        } else {
            self.frame.start = 0.;
            self.frame.start_turn = 0.;
            self.start = None;
        }
    }

    pub(crate) fn advance(&mut self, rms: f32, elapsed: f32) {
        let dt = elapsed.clamp(0., 0.05);
        if dt == 0. {
            return;
        }
        self.since_drawn += dt;
        self.advance_start_motion(dt);
        let rms = if rms.is_finite() { rms.clamp(0., 1.) } else { 0. };
        if self.calibration_left > 0. {
            // Zero also means the recorder has yet to deliver its first buffer.
            if rms > 0. {
                self.calibration_left -= dt;
                // Let the first buffers settle before taking a baseline.
                if self.calibration_left <= 0.08 {
                    self.startup_min = self.startup_min.min(rms);
                }
                if self.calibration_left <= 0. {
                    // Bound the estimate so speech already underway cannot set the gate too high.
                    self.floor = self.startup_min.clamp(0.00003, 0.006);
                }
            }
            self.frame.phase += 1.35 * dt;
            return;
        }
        let floor_target = rms.max(0.00003);
        self.floor += (floor_target - self.floor) * response(dt, if floor_target < self.floor { 0.25 } else { 30. });
        let threshold = (self.floor * 1.6).max(0.00008);
        let signal = (rms - threshold).max(0.);
        let gate = (signal / (self.floor * 2. + 0.00006)).clamp(0., 1.);
        if gate > 0.1 && !self.primed {
            self.peak = rms.max(0.0002);
            self.primed = true;
        }
        self.peak += (rms - self.peak) * response(dt, if rms > self.peak { 0.045 } else { 3. });
        let reference = (self.peak - threshold).max(0.0001);
        let normalized = gate * (0.18 + 0.82 * (1. - (-2. * signal / reference).exp()));
        let rising = (normalized - self.last_signal).max(0.);
        self.last_signal = normalized;

        let frame = &mut self.frame;
        frame.cadence = (frame.cadence * (-dt / 0.34).exp() + 0.95 * rising).clamp(0., 1.);
        let tau = if normalized > frame.level { 0.042 } else { 0.22 };
        frame.level += (normalized - frame.level) * response(dt, tau);
        if normalized < 0.008 && frame.level < 0.008 {
            frame.level = 0.;
        }
        frame.phase += dt * (1.35 + frame.level.sqrt() * (10. + 19. * frame.level + 7. * frame.cadence));
    }

    pub(crate) fn frame(&self) -> OceanFrame {
        self.frame
    }

    pub(super) fn calibrated(&self) -> bool {
        self.calibration_left <= 0.
    }

    pub(super) fn needs_redraw(&mut self, reduced: bool) -> bool {
        // Speech moves the phase past the threshold on every display refresh, so high refresh rates need a ceiling.
        if self.since_drawn < MIN_REDRAW_S {
            return false;
        }
        let frame = self.frame;
        let last = self.last_drawn;
        let starting = frame.start > 0. || last.start > 0.;
        // Quiet drift needs about 30 frames per second. With reduced motion, redraw only for changes in ink.
        if (!reduced && (starting || frame.phase - last.phase >= QUIET_PHASE_STEP))
            || (frame.level - last.level).abs() >= 0.02
            || (frame.level > 0.2 && (frame.cadence - last.cadence).abs() >= 0.08)
        {
            self.last_drawn = frame;
            self.since_drawn = 0.;
            return true;
        }
        false
    }

    pub(crate) fn demo(t: f32) -> OceanFrame {
        let mut model = Self::without_start_motion();
        let end = t.max(0.) % 4.;
        let mut time = 0.;
        while time < end {
            let dt = (end - time).min(1. / 60.);
            model.advance(speech_sample(time, 0.025), dt);
            time += dt;
        }
        model.frame()
    }
}

fn response(dt: f32, tau: f32) -> f32 {
    1. - (-dt / tau).exp()
}

pub(crate) fn speech_sample(t: f32, gain: f32) -> f32 {
    let t = t % 4.;
    if !(0.35..2.8).contains(&t) {
        return 0.;
    }
    let syllable = (t * 13.).sin().max(0.).powi(2);
    let phrase = 0.5 + 0.5 * (t * 3.1).sin();
    gain * (0.08 + 0.92 * syllable) * (0.35 + 0.65 * phrase)
}

fn drift(t: f32, seed: u32) -> f32 {
    let cell = t.floor();
    let random = |i: i32| {
        let mut n = (i as u32).wrapping_add(seed).wrapping_mul(0x9e37_79b9);
        n ^= n >> 16;
        n = n.wrapping_mul(0x85eb_ca6b);
        (n >> 16) as f32 / 65535.
    };
    let f = t - cell;
    lerp(random(cell as i32), random(cell as i32 + 1), ease(f))
}

/// A dot squeezed below this by the capsule edge is too faint to draw.
const MIN_DOT_RADIUS: f32 = 0.2;

/// Appends the dot as a triangle fan. A convex hexagon needs no tessellator, which was the largest per-frame cost.
fn dot(path: &mut Option<Path<Pixels>>, center: Point<Pixels>, radius: Pixels) {
    const RING: [(f32, f32); 6] = [(1., 0.), (0.5, 0.866), (-0.5, 0.866), (-1., 0.), (-0.5, -0.866), (0.5, -0.866)];
    let corner = |i: usize| point(center.x + radius * RING[i].0, center.y + radius * RING[i].1);
    let solid = point(0., 1.);
    let path = path.get_or_insert_with(|| Path::new(corner(0)));
    for i in 1..RING.len() - 1 {
        path.push_triangle((corner(0), corner(i), corner(i + 1)), (solid, solid, solid));
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ocean(
    frame: OceanFrame,
    scale: Scale,
    dark: bool,
    // The dot ramp of a command run; `None` keeps gold.
    ramp: Option<[[f32; 3]; 3]>,
    reduced: bool,
    width: f32,
    frost: bool,
    entry: EntryPose,
) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let p = SPARKLE_CURRENT;
            let start = if reduced { 0. } else { frame.start };
            let sway = START_MOTION.sway * start;
            let energy = frame.level.max(START_MOTION.swell * start).powf(0.72);
            let amplitude = 0.23 + 0.77 * energy;
            let fast = energy.powf(2.2) * (0.72 + 0.28 * frame.cadence);
            let travel = if reduced { 3.5 } else { frame.phase };
            let w = f32::from(bounds.size.width) / scale.0;
            let h = f32::from(bounds.size.height) / scale.0;
            let half = h * 0.5;
            let stretch_x = w / width;
            let stretch_y = h / 33.;
            let shape_fast = if reduced { 0. } else { fast };
            let shape_amplitude = if reduced { 0.23 } else { amplitude };
            // Bucket the dots so hundreds of points are painted in only a handful of paths.
            const BANDS: usize = 6;
            const SHADES: usize = 5;
            let mut paths: [[Option<Path<Pixels>>; SHADES]; BANDS] = Default::default();
            for row in 0..16 {
                let depth = row as f32 / 15.;
                let distant = 0.08 + 0.92 * depth.powf(1.85);
                let plane_y = (depth - 0.5) * 13.;
                let bend = p.curvature * (depth - 0.60).powi(2) * 0.12;
                // Cull offscreen columns before the noise and trigonometry. The margin allows for crest displacement.
                let col_limit = width * 0.5 / lerp(3.56, 1.99, (depth + 0.04).min(1.)) + 1.2;
                for col in -29..=29 {
                    let u = col as f32 + (row % 2) as f32 * 0.21;
                    if u.abs() > col_limit {
                        continue;
                    }
                    let travel = if sway > 0. {
                        let ripple = frame.start_turn
                            - u * std::f32::consts::TAU / START_MOTION.ripple_columns
                            - depth * START_MOTION.row_lag;
                        travel + sway * ripple.sin()
                    } else {
                        travel
                    };
                    let slow = u - travel * 0.56;
                    let mid = u - travel * p.middle_speed;
                    let quick = u - travel * p.quick_speed;
                    let a = slow * p.frequency - plane_y * (0.19 + p.depth_lag * 0.018)
                        + bend
                        + 0.72 * (drift(slow * 0.077 + depth * 2.1, 73) - 0.5);
                    let b = mid * 0.49
                        + plane_y * (0.37 + p.depth_lag * 0.022)
                        + 1.34
                        + 0.48 * (drift(mid * 0.084 - depth * 2.7, 146) - 0.5);
                    let c = quick * 0.76 - plane_y * 0.42 + 0.44 * (drift(quick * 0.12 + depth, 147) - 0.5);
                    let (a_sin, first) = a.sin_cos();
                    let (b_sin, second) = b.sin_cos();
                    let (c_sin, third) = c.sin_cos();
                    let swell = (0.54 + 0.24 * drift(slow * 0.093 + depth * 3.4, 128)) * first
                        + (0.23 + 0.26 * drift(mid * 0.102 - depth * 4.1, 223)) * second
                        + (0.10 + 0.16 * drift(quick * 0.13 + depth * 4.7, 264)) * (1. - 0.35 * shape_fast) * third;
                    let overlap = (0.5 + 0.5 * first).powf(1.7) * (0.5 + 0.5 * second).powf(1.7);
                    let lift = p.overlap_lift
                        * (0.38 + 0.62 * drift(slow * 0.101 + depth * 5.7, 313))
                        * overlap
                        * (0.68 + 0.32 * third.max(0.));
                    let height = ((swell + lift * 0.78) * 1.20).tanh();
                    let compression = shape_amplitude * (1. + 0.19 * overlap - 0.17 * shape_fast);
                    let along = u - compression * (0.47 * a_sin + 0.27 * b_sin + 0.12 * c_sin);
                    let across =
                        (depth + 0.032 * compression * (0.44 * a_sin - 0.64 * b_sin + 0.30 * c_sin)).clamp(0., 1.);
                    let x = w * 0.5 + along * lerp(3.56, 1.99, across) * stretch_x;
                    let y = (34.
                        - 21.5 * across.powf(0.78)
                        - shape_amplitude * (4.9 * distant + 0.55 * depth * overlap) * height)
                        * stretch_y
                        - (1. - ((x - w * 0.5) / (w * 0.5)).powi(2)).max(0.) * (entry.height - 1.).max(0.) / 0.16 * 1.5;
                    let edge = (x - w * 0.5).abs() - (w * 0.5 - half);
                    // The canvas already sits inside the capsule's border, so its own edge is the clip.
                    let from_edge = half - edge.max(0.).hypot(y - half);
                    if from_edge < MIN_DOT_RADIUS {
                        continue;
                    }
                    let crest = (0.5 + 0.5 * height).clamp(0., 1.);
                    let glint = drift(quick * 0.56 + depth * 10.3, 464).powf(2.4) * (0.40 + 0.60 * crest);
                    let exposure =
                        (0.21 + 0.30 * energy + 0.17 * depth + 0.15 * crest + p.sparkle * glint + 0.10 * fast)
                            .clamp(0., 1.);
                    let alpha = (0.26
                        + 0.32 * depth
                        + 0.24 * energy
                        + 0.29 * glint
                        + 0.09 * fast
                        + if dark { 0. } else { 0.16 + 0.09 * exposure })
                    .clamp(0., 1.);
                    let radius = lerp(0.55, 0.39, depth)
                        + 0.16 * glint
                        + if reduced { 0. } else { 0.05 * energy }
                        + if dark { 0. } else { 0.085 };
                    let center = point(bounds.origin.x + scale.px(x), bounds.origin.y + scale.px(y));
                    let shade = (exposure * (SHADES - 1) as f32).round() as usize;
                    let mut paint_dot = |radius: f32, opacity: f32| {
                        let band = (opacity * (BANDS - 1) as f32).round() as usize;
                        if band > 0 {
                            // A dot the edge would cut shrinks to fit, so the sheet reaches the border.
                            dot(&mut paths[band][shade], center, scale.px(radius.min(from_edge)));
                        }
                    };
                    let button_radius = BTN * entry.button * 0.5;
                    if frost && (x < (CAPSULE_PAD + BTN) * entry.button || x > w - (CAPSULE_PAD + BTN) * entry.button) {
                        let center_x = CAPSULE_PAD * entry.button + button_radius;
                        let dx = x.min(w - x) - center_x;
                        let inside = button_radius - dx.hypot(y - half);
                        if inside > 0. {
                            // A small two-radius spread of the same dot approximates the local blur.
                            // Keep its support inside the control and reuse the existing draw buckets.
                            let soft = (inside / 2.).min(1.);
                            paint_dot(radius, alpha * (1. - soft));
                            paint_dot((radius + 0.48).min(inside), alpha * soft * 0.48);
                            paint_dot((radius + 1.05).min(inside), alpha * soft * 0.24);
                            continue;
                        }
                    }
                    paint_dot(radius, alpha);
                }
            }
            let palette = if let Some(ramp) = ramp {
                ramp
            } else if dark {
                [[161., 146., 107.], [224., 196., 125.], [255., 236., 189.]]
            } else {
                [[94., 81., 52.], [133., 104., 41.], [170., 136., 49.]]
            };
            for (band, row) in paths.into_iter().enumerate().skip(1) {
                for (shade, path) in row.into_iter().enumerate() {
                    if let Some(path) = path {
                        let t = shade as f32 / (SHADES - 1) as f32 * 2.;
                        let base = if t < 1. { 0 } else { 1 };
                        let [r, g, b] = std::array::from_fn(|i| {
                            lerp(palette[base][i], palette[base + 1][i], t - base as f32) / 255.
                        });
                        window.paint_path(path, Rgba { r, g, b, a: band as f32 / (BANDS - 1) as f32 });
                    }
                }
            }
        },
    )
    .absolute()
    .inset_0()
}

#[cfg(test)]
mod tests {
    use super::{START_MOTION, Waveform, speech_sample};

    #[test]
    fn delayed_microphone_start_keeps_room_noise_quiet() {
        let mut ocean = Waveform::default();
        for _ in 0..24 {
            ocean.advance(0., 1. / 60.);
        }
        for i in 0..90 {
            let noise = match i {
                0 => 0.0001,
                1 | 2 => 0.007,
                _ => 0.002 + (i % 5) as f32 * 0.00015,
            };
            ocean.advance(noise, 1. / 60.);
            assert_eq!(ocean.frame().level, 0.);
        }
        for _ in 0..8 {
            ocean.advance(0.018, 1. / 60.);
        }
        assert!(ocean.frame().level > 0.5);
    }

    #[test]
    fn speech_already_underway_can_still_drive_the_ocean() {
        let mut ocean = Waveform::default();
        for _ in 0..16 {
            ocean.advance(0.025, 1. / 60.);
        }
        assert!(ocean.frame().level > 0.5);
    }

    #[test]
    fn quiet_drifts_and_speech_accelerates_with_a_smooth_release() {
        for gain in [0.0006, 0.006, 0.06, 0.6] {
            let mut ocean = Waveform::default();
            for _ in 0..60 {
                ocean.advance(0.00002, 1. / 60.);
            }
            let quiet = ocean.frame();
            assert_eq!(quiet.level, 0.);
            assert!(quiet.phase > 3.5);
            let mut peak = 0_f32;
            for i in 0..130 {
                ocean.advance(speech_sample(i as f32 / 60., gain), 1. / 60.);
                peak = peak.max(ocean.frame().level);
            }
            assert!(peak > 0.35);
            assert!(ocean.frame().phase > quiet.phase + 6.);
            for _ in 0..120 {
                ocean.advance(0., 1. / 60.);
            }
            assert_eq!(ocean.frame().level, 0.);
        }
    }

    #[test]
    fn speech_redraws_at_most_sixty_times_a_second() {
        let mut ocean = Waveform::default();
        let frames = (0..240)
            .filter(|i| {
                ocean.advance(speech_sample(*i as f32 / 120., 0.025), 1. / 120.);
                ocean.needs_redraw(false)
            })
            .count();
        assert!((90..=120).contains(&frames), "{frames} frames in two seconds at 120 Hz");
    }

    #[test]
    fn late_frame_callbacks_do_not_skip_redraws() {
        for (rate, speech, expected) in [(60., true, 58..=60), (120., true, 58..=60), (60., false, 29..=30)] {
            let mut ocean = Waveform::without_start_motion();
            let ticks = rate as usize;
            for _ in 0..ticks {
                ocean.advance(if speech { 0.025 } else { 0. }, 1. / rate);
                ocean.needs_redraw(false);
            }
            let frames = (0..ticks)
                .filter(|i| {
                    let late = if i % 2 == 0 { 0.0015 } else { -0.0015 };
                    ocean.advance(if speech { 0.025 } else { 0. }, 1. / rate + late);
                    ocean.needs_redraw(false)
                })
                .count();
            assert!(expected.contains(&frames), "{frames} frames in a second at {rate} Hz, speech {speech}");
        }
    }

    #[test]
    fn start_motion_shows_at_once_and_leaves_the_frames_of_real_audio_untouched() {
        let ends_at = |input: fn(f32) -> f32| {
            let (mut ocean, mut plain) = (Waveform::default(), Waveform::without_start_motion());
            let (mut time, mut ended) = (0., None);
            while time < 2. {
                ocean.advance(input(time), 1. / 120.);
                plain.advance(input(time), 1. / 120.);
                time += 1. / 120.;
                let (frame, reference) = (ocean.frame(), plain.frame());
                assert_eq!(
                    (frame.level, frame.cadence, frame.phase),
                    (reference.level, reference.cadence, reference.phase)
                );
                if frame.start > 0. {
                    assert!(ended.is_none(), "the start motion came back at {time} s");
                } else {
                    ended.get_or_insert(time);
                    assert_eq!(frame, reference);
                }
            }
            ended.expect("the start motion ends")
        };
        let after_calibration = 0.12 + START_MOTION.blend_out_s;
        let without_audio = START_MOTION.longest_s + START_MOTION.blend_out_s;
        assert!((ends_at(|_| 0.0015) - after_calibration).abs() < 0.02);
        assert!((ends_at(|time| speech_sample(time + 0.35, 0.06)) - after_calibration).abs() < 0.02);
        assert!((ends_at(|_| 0.) - without_audio).abs() < 0.02);
        assert_eq!(Waveform::default().frame().start, 1.);
        for (reduced, expected) in [(false, 30), (true, 0)] {
            let mut ocean = Waveform::default();
            let frames = (0..30)
                .filter(|_| {
                    ocean.advance(0., 1. / 60.);
                    ocean.needs_redraw(reduced)
                })
                .count();
            assert_eq!(frames, expected, "frames of the start motion with reduced motion {reduced}");
        }
    }

    #[test]
    fn quiet_water_requests_fewer_frames_and_respects_reduced_motion() {
        for (reduced, expected) in [(false, 28..=31), (true, 0..=0)] {
            let mut ocean = Waveform::without_start_motion();
            let frames = (0..60)
                .filter(|_| {
                    ocean.advance(0., 1. / 60.);
                    ocean.needs_redraw(reduced)
                })
                .count();
            assert!(expected.contains(&frames), "{frames} quiet frames with reduced motion {reduced}");
        }
    }
}
