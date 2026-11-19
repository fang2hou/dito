use std::time::{Duration, Instant};

use gpui_kit::SharedString;

pub(super) const HANDOFF: Duration = Duration::from_millis(120);
const TRAVEL: f32 = 0.18;
const FADE: f32 = 0.15;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Geometry {
    pub center: f32,
    pub bottom: f32,
    pub width: f32,
}

impl Geometry {
    fn values(self) -> [f32; 3] {
        [self.center, self.bottom, self.width]
    }
}

struct Travel {
    from: [f32; 3],
    to: [f32; 3],
    velocity: [f32; 3],
    start: Instant,
}

impl Travel {
    fn sample(&self, now: Instant) -> (Geometry, [f32; 3]) {
        let t = (now.duration_since(self.start).as_secs_f32() / TRAVEL).min(1.);
        let t2 = t * t;
        let t3 = t2 * t;
        let values = std::array::from_fn(|i| {
            (2. * t3 - 3. * t2 + 1.) * self.from[i]
                + (t3 - 2. * t2 + t) * TRAVEL * self.velocity[i]
                + (-2. * t3 + 3. * t2) * self.to[i]
        });
        let velocity = std::array::from_fn(|i| {
            (6. * t2 - 6. * t) / TRAVEL * (self.from[i] - self.to[i]) + (3. * t2 - 4. * t + 1.) * self.velocity[i]
        });
        let [center, bottom, width] = values;
        (Geometry { center, bottom, width: width.max(22.) }, velocity)
    }
}

pub(super) struct Label {
    pub text: SharedString,
    from: f32,
    to: f32,
}

pub(super) struct TooltipMotion {
    travel: Travel,
    labels: Vec<Label>,
    label_start: Instant,
    visibility: (f32, f32, Instant),
    pub last_seen: Instant,
    left_at: Option<Instant>,
}

fn ease(t: f32) -> f32 {
    crate::ui::motion::ease(t.clamp(0., 1.))
}

impl TooltipMotion {
    pub fn new(geometry: Geometry, text: SharedString, now: Instant) -> Self {
        Self {
            travel: Travel { from: geometry.values(), to: geometry.values(), velocity: [0.; 3], start: now },
            labels: vec![Label { text, from: 1., to: 1. }],
            label_start: now,
            visibility: (0., 1., now),
            last_seen: now,
            left_at: None,
        }
    }

    pub fn geometry(&self, now: Instant) -> Geometry {
        self.travel.sample(now).0
    }

    pub fn alpha(&self, now: Instant) -> f32 {
        let (from, to, start) = self.visibility;
        from + (to - from) * ease(now.duration_since(start).as_secs_f32() / FADE)
    }

    pub fn labels(&self, now: Instant) -> impl Iterator<Item = (&SharedString, f32)> {
        let t = ease(now.duration_since(self.label_start).as_secs_f32() / TRAVEL);
        self.labels.iter().map(move |label| (&label.text, label.from + (label.to - label.from) * t))
    }

    pub fn target(&mut self, geometry: Geometry, text: SharedString, now: Instant, reduced: bool) {
        self.last_seen = now;
        self.left_at = None;
        if self.travel.to != geometry.values() || (reduced && self.travel.sample(now).0.values() != geometry.values()) {
            let (current, velocity) = self.travel.sample(now);
            let from = if reduced { geometry.values() } else { current.values() };
            self.travel =
                Travel { from, to: geometry.values(), velocity: if reduced { [0.; 3] } else { velocity }, start: now };
        }
        if !self.labels.iter().any(|label| label.text == text && label.to == 1.) {
            let t = ease(now.duration_since(self.label_start).as_secs_f32() / TRAVEL);
            for label in &mut self.labels {
                label.from += (label.to - label.from) * t;
                label.to = f32::from(label.text == text);
            }
            self.labels.retain(|label| label.from > 0.005 || label.to > 0.);
            if !self.labels.iter().any(|label| label.text == text) {
                // Bound paint work when the pointer skims many different triggers in one transfer.
                if self.labels.len() > 3 {
                    self.labels.sort_by(|a, b| b.from.total_cmp(&a.from));
                    self.labels.truncate(3);
                    let total: f32 = self.labels.iter().map(|label| label.from).sum();
                    for label in &mut self.labels {
                        label.from /= total.max(0.001);
                    }
                }
                self.labels.push(Label { text, from: 0., to: 1. });
            }
            self.label_start = now;
        }
        if self.visibility.1 != 1. {
            self.visibility = (self.alpha(now), 1., now);
        }
    }

    pub fn leave(&mut self, now: Instant) {
        let left_at = *self.left_at.get_or_insert(now);
        if now.duration_since(left_at) >= HANDOFF && self.visibility.1 != 0. {
            self.visibility = (self.alpha(now), 0., now);
        }
    }

    pub fn dismiss(&mut self, now: Instant) {
        self.left_at = Some(now - HANDOFF);
        self.leave(now);
    }

    pub fn moving(&self, now: Instant) -> bool {
        now.duration_since(self.travel.start).as_secs_f32() < TRAVEL
            || now.duration_since(self.label_start).as_secs_f32() < TRAVEL
            || (self.visibility.0 != self.visibility.1 && now.duration_since(self.visibility.2).as_secs_f32() < FADE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupted_transfer_continues_from_the_visible_position_width_and_labels() {
        let now = Instant::now();
        let mut motion = TooltipMotion::new(Geometry { center: 30., bottom: 50., width: 45. }, "A".into(), now);
        let now = now + Duration::from_secs(1);
        motion.target(Geometry { center: 100., bottom: 50., width: 93. }, "Long".into(), now, false);
        let middle = now + Duration::from_millis(60);
        let before = motion.travel.sample(middle);
        let weights: Vec<_> = motion.labels(middle).map(|(text, alpha)| (text.clone(), alpha)).collect();
        motion.target(Geometry { center: 65., bottom: 50., width: 57. }, "Next".into(), middle, false);
        let after = motion.travel.sample(middle);
        assert_eq!(before.0.values(), after.0.values());
        assert_eq!(before.1, after.1);
        for ((_, before), (_, after)) in weights.iter().zip(motion.labels(middle)) {
            assert!((before - after).abs() < 0.001);
        }
    }

    #[test]
    fn brief_gaps_keep_the_bubble_and_a_fading_bubble_can_be_reused() {
        let now = Instant::now();
        let geometry = Geometry { center: 50., bottom: 50., width: 45. };
        let mut motion = TooltipMotion::new(geometry, "A".into(), now);
        let seen = now + Duration::from_secs(1);
        motion.target(geometry, "A".into(), seen, false);
        motion.leave(seen);
        motion.leave(seen + Duration::from_millis(100));
        assert_eq!(motion.alpha(seen + Duration::from_millis(100)), 1.);
        motion.leave(seen + Duration::from_millis(120));
        let fading = seen + Duration::from_millis(180);
        let alpha = motion.alpha(fading);
        motion.target(geometry, "B".into(), fading, false);
        assert_eq!(motion.alpha(fading), alpha);
        assert_eq!(motion.alpha(fading + Duration::from_millis(150)), 1.);
    }

    #[test]
    fn dismissal_keeps_the_visible_bubble_until_its_exit_finishes() {
        let start = Instant::now();
        let geometry = Geometry { center: 50., bottom: 50., width: 45. };
        let mut motion = TooltipMotion::new(geometry, "Tip".into(), start);
        let now = start + Duration::from_secs(1);
        motion.dismiss(now);
        assert_eq!(motion.alpha(now), 1.);
        assert!(motion.alpha(now + Duration::from_millis(75)) > 0.);
        assert_eq!(motion.geometry(now + Duration::from_millis(75)).values(), geometry.values());
        assert_eq!(motion.alpha(now + Duration::from_millis(150)), 0.);
        let reenter = now + Duration::from_millis(90);
        let current = motion.alpha(reenter);
        motion.target(geometry, "Tip".into(), reenter, false);
        assert_eq!(motion.alpha(reenter), current);
    }
    #[test]
    fn rapid_skimming_bounds_text_layers_and_reduced_motion_stops_travel() {
        let start = Instant::now();
        let mut geometry = Geometry { center: 30., bottom: 50., width: 45. };
        let mut motion = TooltipMotion::new(geometry, "Start".into(), start);
        for i in 0..80 {
            let now = start + Duration::from_millis(30 * i);
            geometry.center += 1.;
            motion.target(geometry, format!("Label {i}").into(), now, false);
            assert!(motion.labels.len() <= 4);
            let alpha: f32 = motion.labels(now).map(|(_, alpha)| alpha).sum();
            assert!((alpha - 1.).abs() < 0.03);
        }
        let now = start + Duration::from_millis(2410);
        motion.target(geometry, "Reduced".into(), now, true);
        assert_eq!(motion.geometry(now).values(), geometry.values());
    }
}
