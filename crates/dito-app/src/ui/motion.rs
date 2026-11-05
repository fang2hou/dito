use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::*;

static NEXT_SCOPE_ID: AtomicU64 = AtomicU64::new(1);

/// Uniquely allocated identity of one registry generation. Each [`MotionScope::new`] and
/// [`MotionScope::reset`] allocates a fresh id, so a handle can tell whether it still points at the
/// registry generation it was built under.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MotionScopeId(u64);

impl MotionScopeId {
    fn next() -> Self {
        Self(NEXT_SCOPE_ID.fetch_add(1, Ordering::Relaxed))
    }
}

/// The transient per-owner UI state, previously process-wide maps keyed by `(scope, id)`: tweens,
/// hover, sliding plates, flips, tooltip labels, select widths and the open-menu press marks. It
/// lives with its owner and dies with it; ids are unique only inside one registry.
pub(super) struct Registry {
    /// Per animated value: (from, to, start, key). See [`tween_to`].
    pub(super) tweens: HashMap<ElementId, (f32, f32, Instant, usize)>,
    pub(super) hovered: HashMap<ElementId, bool>,
    /// Per sliding plate.
    pub(super) plates: HashMap<ElementId, PlateMotion>,
    /// Last value and number of flips seen per element id: a flip starts a new animation (keyed by the count),
    /// so an element drawn for the first time does not animate.
    pub(super) flips: HashMap<ElementId, (bool, usize, Instant)>,
    /// Label per element id, captured by [`tip_anchor`] at build time for [`tip_above`].
    pub(super) tips: HashMap<ElementId, SharedString>,
    /// Resolved width of the last painted fill-width select button, read by its menu panel. The panel
    /// is built before the button paints, so on the frame a menu opens it sees the previous frame's
    /// measurement — which is already correct, because the button paints every frame.
    pub(super) select_w: HashMap<&'static str, f32>,
    /// Select buttons that were pressed while their menu was open (the outside-click already closed it).
    pub(super) pressed_open: HashSet<ElementId>,
    pub(super) id: MotionScopeId,
}

impl Registry {
    fn fresh() -> Self {
        Self {
            tweens: HashMap::new(),
            hovered: HashMap::new(),
            plates: HashMap::new(),
            flips: HashMap::new(),
            tips: HashMap::new(),
            select_w: HashMap::new(),
            pressed_open: HashSet::new(),
            id: MotionScopeId::next(),
        }
    }
}

/// The owner of one bubble of transient motion state: a window's shell, a settings page, a wizard
/// step. Everything its controls remember between frames — tween values, hover, plates, flips,
/// tooltip labels, select widths, open-menu press marks — lives in its [`Registry`].
///
/// [`reset`](Self::reset) swaps in a fresh registry, so every handle handed out before (captured by
/// event, paint and animation closures) dies: a control drawn again later never replays a value
/// remembered under the previous generation, and a late callback from it can neither read nor
/// reinsert state.
pub struct MotionScope {
    registry: Rc<RefCell<Registry>>,
}

impl MotionScope {
    pub fn new() -> Self {
        Self { registry: Rc::new(RefCell::new(Registry::fresh())) }
    }

    /// Forget everything remembered under this scope: the elements that follow start un-hovered and
    /// at their targets, and GPUI then reports the real pointer state on the first frame, so a
    /// pointer already over a control lights it up again. Old handles expire.
    pub fn reset(&mut self) {
        self.registry = Rc::new(RefCell::new(Registry::fresh()));
    }

    /// A weak handle to this scope's current registry generation, for `with_motion_scope` and for
    /// closures that run after the build (hover events, paint, animation frames). It never keeps the
    /// registry alive.
    pub fn handle(&self) -> ScopeHandle {
        ScopeHandle { registry: Rc::downgrade(&self.registry) }
    }

    /// Identity of the current registry generation (tests; production reads it via `Debug`).
    #[cfg(test)]
    pub fn id(&self) -> MotionScopeId {
        self.registry.borrow().id
    }
}

impl Default for MotionScope {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for MotionScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // try_borrow: formatting must never panic on an already-borrowed registry.
        let id = self.registry.try_borrow().ok().map(|reg| reg.id);
        f.debug_struct("MotionScope").field("id", &id).finish_non_exhaustive()
    }
}

/// A weak reference to one [`MotionScope`] generation. Cheap to clone and safe to capture in
/// closures that outlive the build; once the owner resets or drops the scope, the handle is
/// [`expired`](Self::expired) and every access through it yields the static default.
#[derive(Clone)]
pub struct ScopeHandle {
    registry: Weak<RefCell<Registry>>,
}

impl ScopeHandle {
    fn detached() -> Self {
        Self { registry: Weak::new() }
    }

    /// Whether the generation this handle was built under is gone (owner reset or dropped).
    pub fn expired(&self) -> bool {
        self.registry.strong_count() == 0
    }

    pub(super) fn registry(&self) -> Option<Rc<RefCell<Registry>>> {
        self.registry.upgrade()
    }
}

impl std::fmt::Debug for ScopeHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // try_borrow: formatting must never panic on an already-borrowed registry.
        let id = self.registry.upgrade().and_then(|reg| reg.try_borrow().ok().map(|reg| reg.id));
        f.debug_struct("ScopeHandle").field("id", &id).field("expired", &self.expired()).finish_non_exhaustive()
    }
}

thread_local! {
    /// The scope the currently building elements belong to. Set by [`with_motion_scope`] for the
    /// duration of a render (or one page of it); callbacks capture it at build time instead.
    static AMBIENT: RefCell<ScopeHandle> = RefCell::new(ScopeHandle::detached());
}

/// The ambient scope of the current build. Detached (never live) outside a
/// [`with_motion_scope`] region: reads then take their static defaults.
pub fn motion_scope() -> ScopeHandle {
    AMBIENT.with(|s| s.borrow().clone())
}

/// Run `f` with every motion registry access inside it keyed under `scope`'s owner. Restores the
/// previous ambient scope when `f` returns or unwinds, so nesting and early exits keep the parent's
/// scope.
pub fn with_motion_scope<R>(scope: ScopeHandle, f: impl FnOnce() -> R) -> R {
    struct Restore {
        prev: ScopeHandle,
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            AMBIENT.with(|cell| *cell.borrow_mut() = self.prev.clone());
        }
    }
    let prev = AMBIENT.with(|cell| std::mem::replace(&mut *cell.borrow_mut(), scope));
    let _guard = Restore { prev };
    f()
}

pub(super) fn ambient_registry() -> Option<Rc<RefCell<Registry>>> {
    motion_scope().registry()
}

/// Forget the hover remembered for one control, for when it stops being drawn (disabled) while others stay: it
/// must not come back looking hovered.
pub fn reset_hover(id: &ElementId) {
    let Some(reg) = ambient_registry() else { return };
    let mut reg = reg.borrow_mut();
    reg.hovered.remove(id);
    reg.tweens.remove(&ElementId::from((id.clone(), "hover-fill")));
}

pub const HOVER_MS: u64 = 160;

fn ease(t: f32) -> f32 {
    t * t * (3. - 2. * t)
}

/// Retarget the value animated under `id` to `target`. Returns `(from, to, active, key)`: the animation runs
/// from `from` (where the value was when the target changed, so an interrupted move continues smoothly)
/// to `to`; `active` is false once it has settled, which also keeps a control drawn again later (another page)
/// from replaying it. A handle whose scope has expired settles at once and reinserts nothing.
pub(super) fn tween_to_at(scope: &ScopeHandle, id: &ElementId, target: f32, dur: Duration) -> (f32, f32, bool, usize) {
    let Some(reg) = scope.registry() else { return (target, target, false, 0) };
    let mut m = reg.borrow_mut();
    let key = id.clone();
    let now = Instant::now();
    let e = m.tweens.entry(key).or_insert((target, target, now - dur, 0));
    if (e.1 - target).abs() > f32::EPSILON {
        let progress = (now.duration_since(e.2).as_secs_f32() / dur.as_secs_f32()).clamp(0., 1.);
        let current = e.0 + (e.1 - e.0) * ease(progress);
        *e = (current, target, now, e.3 + 1);
    }
    (e.0, e.1, now.duration_since(e.2) < dur, e.3)
}

/// An element whose look follows an animated number: `build(v)` draws it for the value `v`, which eases to
/// `target` over `ms` whenever the target changes. Use it for visual layers, not for the interactive element
/// itself (animating re-creates the element, which would reset its hover/press state).
pub fn tweened<E: IntoElement + 'static>(
    id: ElementId,
    target: f32,
    ms: u64,
    build: impl Fn(f32) -> E + 'static,
) -> AnyElement {
    tweened_at(motion_scope(), id, target, ms, build)
}

/// [`tweened`] with the scope pinned: for `build` closures that run during animation frames (paint time),
/// when the ambient scope is no longer the one the element was built under. Every build — the settled
/// one, the first frame and the deferred animation frames — runs with `scope` restored, so nested scoped
/// helpers (hover reads, further tweens) keep hitting this scope's registry; the handle is weak, so it
/// never keeps the owner alive.
pub fn tweened_at<E: IntoElement + 'static>(
    scope: ScopeHandle,
    id: ElementId,
    target: f32,
    ms: u64,
    build: impl Fn(f32) -> E + 'static,
) -> AnyElement {
    let dur = Duration::from_millis(ms);
    let (from, to, active, key) = tween_to_at(&scope, &id, target, dur);
    let render = move |v: f32| with_motion_scope(scope.clone(), || build(v));
    if !active {
        return render(to).into_any_element();
    }
    render(from)
        .with_animation((id, SharedString::from(format!("tween-{key}"))), Animation::new(dur), move |_, t| {
            render(from + (to - from) * ease(t))
        })
        .into_any_element()
}

/// Duration of a sliding plate. Long enough for the spring to settle; the plate is visibly "alive" for about
/// a third of a second, the rest is the tail.
pub const GLASS_MS: u64 = 460;
const GLASS_STRETCH_DELAY: f32 = 0.14;

/// A critically-underdamped spring step (damping ratio 0.72) over normalised time: starts fast, overshoots by
/// about 4% and settles. With "Reduce motion" it is a plain ease with no overshoot, like Liquid Glass.
pub fn spring(t: f32, reduced: bool) -> f32 {
    let t = t.clamp(0., 1.);
    if reduced {
        return ease(t);
    }
    if t >= 1. {
        return 1.;
    }
    let (decay, wd) = (5.5_f32, 5.30_f32);
    1. - (-decay * t).exp() * ((wd * t).cos() + decay / wd * (wd * t).sin())
}

/// Both edges of a plate going from `from` to `to` (start edge, end edge). The edge on the side it is travelling
/// toward leads on the spring; the other follows a beat later, so the plate stretches while it moves and
/// snaps back into shape as it arrives, like a drop of liquid being pulled along.
fn glass_edges(from: (f32, f32), to: (f32, f32), t: f32, reduced: bool) -> (f32, f32) {
    let forward = (to.0 + to.1) >= (from.0 + from.1);
    let lead = spring(t, reduced);
    let trail = if reduced { lead } else { spring((t - GLASS_STRETCH_DELAY) / (1. - GLASS_STRETCH_DELAY), false) };
    let (ta, tb) = if forward { (trail, lead) } else { (lead, trail) };
    let a = from.0 + (to.0 - from.0) * ta;
    let b = from.1 + (to.1 - from.1) * tb;
    (a, b.max(a + 1.))
}

/// A transition retains its last rendered edges, rather than estimating them with a second
/// wall clock. Layout can delay the animation element's first frame; an interruption must
/// continue from what the user saw, even when that frame arrived late.
pub(super) struct PlateMotion {
    from: (f32, f32),
    to: (f32, f32),
    rendered: (f32, f32),
    progress: f32,
    key: usize,
}

impl PlateMotion {
    fn settled(target: (f32, f32)) -> Self {
        Self { from: target, to: target, rendered: target, progress: 1., key: 0 }
    }

    fn retarget(&mut self, target: (f32, f32)) {
        if (self.to.0 - target.0).abs() > 0.01 || (self.to.1 - target.1).abs() > 0.01 {
            self.from = self.rendered;
            self.to = target;
            self.progress = 0.;
            self.key += 1;
        }
    }

    fn record_frame(&mut self, key: usize, progress: f32, edges: (f32, f32)) {
        // Another layer or an obsolete frame must not rewind the transition.
        if self.key == key && progress >= self.progress {
            self.rendered = edges;
            self.progress = progress;
        }
    }
}

/// The motion of a plate sliding to the edges `target` (start, end) like Liquid Glass: spring, stretching along
/// the way (see [`glass_edges`]), lit up while it moves. A new target mid-flight continues from where the plate
/// currently is. Everything that should move with the plate (the plate itself, the labels whose brightness
/// follows it) animates from one [`PlateState`], so they share one clock.
pub struct PlateState {
    id: ElementId,
    from: (f32, f32),
    to: (f32, f32),
    active: bool,
    key: usize,
    reduced: bool,
    /// The scope the plate was built under: `animate`'s build closures run during deferred
    /// animation frames, when the ambient scope is gone; each build restores this weak handle.
    scope: ScopeHandle,
}

pub fn plate_state(id: ElementId, target: (f32, f32)) -> PlateState {
    let scope = motion_scope();
    let Some(reg) = scope.registry() else {
        // No live scope (none set, or the owner was reset): draw statically at the target.
        return PlateState { id, from: target, to: target, active: false, key: 0, reduced: false, scope };
    };
    let reduced = dito_platform::system::reduce_motion();
    let (from, to, active, key) = {
        let mut m = reg.borrow_mut();
        let e = m.plates.entry(id.clone()).or_insert_with(|| PlateMotion::settled(target));
        e.retarget(target);
        (e.from, e.to, e.progress < 1., e.key)
    };
    PlateState { id, from, to, active, key, reduced, scope }
}

impl PlateState {
    /// Draw a layer for the plate's current edges: `build(start, end, energy)`, where `energy` is 0 at rest and
    /// peaks mid-flight. `layer` names the layer; layers of one plate run on the same clock. Each build —
    /// settled, first frame and deferred animation frames — restores the plate's own weak scope, so
    /// nested scoped helpers inside `build` keep reading this scope's registry.
    pub fn animate<E: IntoElement + 'static>(
        &self,
        layer: &str,
        build: impl Fn(f32, f32, f32) -> E + 'static,
    ) -> AnyElement {
        let scope = self.scope.clone();
        let render = move |a: f32, b: f32, energy: f32| with_motion_scope(scope.clone(), || build(a, b, energy));
        if !self.active {
            return render(self.to.0, self.to.1, 0.).into_any_element();
        }
        let (from, to, reduced) = (self.from, self.to, self.reduced);
        let (scope, id, key) = (self.scope.clone(), self.id.clone(), self.key);
        render(from.0, from.1, 0.)
            .with_animation(
                (self.id.clone(), SharedString::from(format!("{layer}-{}", self.key))),
                Animation::new(Duration::from_millis(GLASS_MS)),
                move |_, t| {
                    let (a, b) = glass_edges(from, to, t, reduced);
                    if let Some(reg) = scope.registry()
                        && let Some(plate) = reg.borrow_mut().plates.get_mut(&id)
                    {
                        plate.record_frame(key, t, (a, b));
                    }
                    let energy = if reduced { 0. } else { (4. * t * (1. - t)).clamp(0., 1.) };
                    render(a, b, energy)
                },
            )
            .into_any_element()
    }
}

/// How much of `[lo, hi]` the plate `[a, b]` covers, 0..1. Labels use it for their brightness, so text lights up as
/// the plate arrives under it and dims as the plate leaves, instead of switching when the click lands.
pub fn coverage(a: f32, b: f32, lo: f32, hi: f32) -> f32 {
    ((b.min(hi) - a.max(lo)) / (hi - lo)).clamp(0., 1.)
}

/// Put on an interactive element to make [`is_hovered`] / [`hover_fill`] work for `id`.
pub fn hover_tracker(id: ElementId) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
    // Captured at build time: hover events fire outside the render that scoped the element.
    let (scope, key) = (motion_scope(), id);
    move |over, window, _| {
        let Some(reg) = scope.registry() else { return };
        let changed = reg.borrow_mut().hovered.insert(key.clone(), *over).unwrap_or(false) != *over;
        if changed {
            window.refresh();
        }
    }
}

pub fn is_hovered(id: &ElementId) -> bool {
    is_hovered_in(&motion_scope(), id)
}

/// [`is_hovered`] against one explicit scope, for readers outside the build (e.g. the screenshot
/// harness probing the shell).
pub fn is_hovered_in(scope: &ScopeHandle, id: &ElementId) -> bool {
    scope.registry().is_some_and(|r| r.borrow().hovered.get(id).copied().unwrap_or(false))
}

/// Pretend the pointer is over `id` (screenshots), in the scope the id actually renders under.
#[cfg(feature = "screenshots")]
pub fn force_hover(scope: &ScopeHandle, id: ElementId, over: bool) {
    if let Some(reg) = scope.registry() {
        reg.borrow_mut().hovered.insert(id, over);
    }
}

/// The motion/tip hover pop (design 12.01) shared by the HUD's in-flow hover content (the language chip, the stop
/// pill): the content rises 6 pt and fades in over 150 ms ease-out when it appears, and is
/// simply not drawn once the pointer leaves (instant out, per the spec). The value lives in the
/// motion registry like every other tween — so unmounting on leave still settles it to 0 and
/// every re-appearance pops, from wherever the fade-out had got to — but unlike
/// [`tweened_at`] the content is built once per render and only the wrapper is restyled each
/// frame, which keeps interactive state (seg hover, presses) intact.
pub fn hover_pop<E: IntoElement + 'static>(id: ElementId, on: bool, build: impl FnOnce() -> E) -> Option<AnyElement> {
    const MS: u64 = 150;
    const RISE: f32 = 6.;
    let dur = Duration::from_millis(MS);
    let scope = motion_scope();
    let (from, to, active, bump) = tween_to_at(&scope, &id, if on { 1. } else { 0. }, dur);
    if !on {
        return None;
    }
    let el = build();
    if !active {
        return Some(el.into_any_element());
    }
    let reduced = dito_platform::system::reduce_motion();
    Some(
        div()
            .child(el)
            .with_animation((id, SharedString::from(format!("pop-{bump}"))), Animation::new(dur), move |el, t| {
                let v = from + (to - from) * (1. - (1. - t.clamp(0., 1.)).powi(3));
                let rise = if reduced { 0. } else { (1. - v) * RISE };
                el.mt(px(rise)).opacity(v)
            })
            .into_any_element(),
    )
}

/// [`hover_pop`] that also grows the content out of its bottom edge, from `from` to full size — the
/// motion/tip pop (design 12.01, HUD 21.07): 150 ms ease-out, rising 6 pt and fading in. GPUI has no
/// transform for a plain element, so `build(k)` redraws the content at scale `k` on every frame of the
/// pop and at `k = 1` once settled. The rise is a relative offset, so the layout never moves.
pub fn hover_pop_scaled<E: IntoElement + 'static>(
    id: ElementId,
    on: bool,
    from: f32,
    build: impl Fn(f32) -> E + 'static,
) -> Option<AnyElement> {
    const MS: u64 = 150;
    const RISE: f32 = 6.;
    let scope = motion_scope();
    if !on {
        tween_to_at(&scope, &id, 0., Duration::from_millis(MS));
        return None;
    }
    let reduced = dito_platform::system::reduce_motion();
    Some(tweened_at(scope, id, 1., MS, move |v| {
        let e = 1. - (1. - v).powi(3);
        let (k, rise) = if reduced { (1., 0.) } else { (from + (1. - from) * e, (1. - e) * RISE) };
        div().relative().top(px(rise)).opacity(e).child(build(k))
    }))
}

/// Number of flips of `value` seen for `id`, but only while the latest one is younger than `animation`; 0 once
/// it has settled. A control that is drawn again later (another page, another window) must not replay a
/// transition that already happened: GPUI restarts an animation whenever its element reappears.
pub(super) fn count_flips(id: &ElementId, value: bool, animation: Duration) -> usize {
    let Some(reg) = ambient_registry() else { return 0 };
    let mut m = reg.borrow_mut();
    let entry = m.flips.entry(id.clone()).or_insert((value, 0, Instant::now()));
    if entry.0 != value {
        *entry = (value, entry.1 + 1, Instant::now());
    }
    if entry.2.elapsed() < animation { entry.1 } else { 0 }
}

/// Text-field chrome: border, radius 12, height 36. The caller supplies the inner input element.
pub(super) const RING_MS: u64 = 180;

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn the_spring_starts_fast_overshoots_a_little_and_lands_exactly() {
        assert_eq!(spring(0., false), 0.);
        assert_eq!(spring(1., false), 1.);
        let peak = (0..=100).map(|i| spring(i as f32 / 100., false)).fold(0., f32::max);
        assert!((1.01..1.06).contains(&peak), "overshoot should be a few percent, got {peak}");
        assert!(spring(0.2, false) > 0.45, "most of the distance is covered early");
        for i in 0..=100 {
            let r = spring(i as f32 / 100., true);
            assert!((0. ..=1.).contains(&r), "reduced motion never overshoots");
        }
    }

    #[::core::prelude::v1::test]
    fn rapid_navigation_continues_from_the_last_rendered_frame() {
        let mut plate = PlateMotion::settled((0., 44.));
        for _ in 0..100 {
            for row in [0, 1, 1, 0, 0, 1, 1, 0, 1, 2, 2, 2, 1] {
                let target = (row as f32 * 48., row as f32 * 48. + 44.);
                let (previous, key, same) = (plate.rendered, plate.key, plate.to == target);
                plate.retarget(target);
                if same {
                    assert_eq!(plate.key, key, "clicking the selected row preserves its animation");
                    assert_eq!(plate.rendered, previous);
                } else {
                    assert_eq!(plate.from, previous, "an interruption cannot jump ahead of the displayed frame");
                    assert_eq!(plate.progress, 0., "each new target gets a full transition");
                    assert_eq!(plate.key, key + 1);
                }
                let edges = glass_edges(plate.from, plate.to, 0.28, false);
                plate.record_frame(plate.key, 0.28, edges);
            }
        }
    }

    #[::core::prelude::v1::test]
    fn a_delayed_first_frame_and_obsolete_callbacks_cannot_advance_a_new_transition() {
        let mut plate = PlateMotion::settled((0., 44.));
        plate.retarget((48., 92.));
        // Until the renderer supplies a frame, layout delay cannot advance the plate.
        let old_key = plate.key;
        plate.retarget((96., 140.));
        assert_eq!(plate.from, (0., 44.));
        plate.record_frame(old_key, 1., (48., 92.));
        assert_eq!(plate.rendered, (0., 44.));
        let halfway = glass_edges(plate.from, plate.to, 0.5, false);
        plate.record_frame(plate.key, 0.5, halfway);
        plate.record_frame(plate.key, 0.1, (0., 44.));
        assert_eq!(plate.rendered, halfway, "layers cannot rewind one another");
        plate.record_frame(plate.key, 1., plate.to);
        assert_eq!(plate.rendered, plate.to);
        assert_eq!(plate.progress, 1.);
    }

    #[::core::prelude::v1::test]
    fn a_sliding_plate_stretches_in_flight_and_returns_to_its_size() {
        let (from, to) = ((10., 60.), (200., 250.));
        let widths: Vec<f32> = (0..=100)
            .map(|i| {
                let (a, b) = glass_edges(from, to, i as f32 / 100., false);
                b - a
            })
            .collect();
        assert_eq!(glass_edges(from, to, 0., false), from);
        assert_eq!(glass_edges(from, to, 1., false), to);
        assert!(widths.iter().cloned().fold(0., f32::max) > 50. * 1.5, "the leading edge pulls ahead");
        // Going back the other way stretches too, leading with the other edge.
        let (a, b) = glass_edges(to, from, 0.3, false);
        assert!(b - a > 50.);
        // Reduced motion keeps its shape.
        for i in 0..=100 {
            let (a, b) = glass_edges(from, to, i as f32 / 100., true);
            assert!((b - a - 50.).abs() < 0.01);
        }
    }

    #[::core::prelude::v1::test]
    fn a_settled_flip_is_not_replayed_when_the_control_is_drawn_again() {
        let scope = MotionScope::new();
        with_motion_scope(scope.handle(), || {
            let id = ElementId::from("flip-test");
            let window = Duration::from_millis(40);
            assert_eq!(count_flips(&id, false, window), 0, "first draw never animates");
            assert_eq!(count_flips(&id, true, window), 1, "a change animates");
            assert_eq!(count_flips(&id, true, window), 1, "...for as long as the animation runs");
            std::thread::sleep(Duration::from_millis(60));
            assert_eq!(count_flips(&id, true, window), 0, "later draws (another page, another window) are static");
            assert_eq!(count_flips(&id, false, window), 2, "the next change animates again");
        });
    }

    #[::core::prelude::v1::test]
    fn two_live_scopes_isolate_the_same_id_in_every_registry() {
        let (a, b) = (MotionScope::new(), MotionScope::new());
        let id = ElementId::from("shared-id");
        let (ra, rb) = (a.handle().registry().unwrap(), b.handle().registry().unwrap());
        ra.borrow_mut().hovered.insert(id.clone(), true);
        ra.borrow_mut().tweens.insert(id.clone(), (0., 1., Instant::now(), 3));
        let mut plate = PlateMotion::settled((0., 1.));
        plate.retarget((2., 3.));
        ra.borrow_mut().plates.insert(id.clone(), plate);
        ra.borrow_mut().flips.insert(id.clone(), (true, 2, Instant::now()));
        ra.borrow_mut().tips.insert(id.clone(), SharedString::from("tip"));
        ra.borrow_mut().select_w.insert("sel", 240.);
        ra.borrow_mut().pressed_open.insert(id.clone());
        let (ra, rb) = (ra.borrow(), rb.borrow());
        assert!(is_hovered_in(&a.handle(), &id) && !is_hovered_in(&b.handle(), &id), "hover");
        assert_eq!(ra.tweens.get(&id).unwrap().3, 3, "tween key");
        assert!(!rb.tweens.contains_key(&id));
        assert_eq!(ra.plates.get(&id).unwrap().key, 1, "plate generation");
        assert!(!rb.plates.contains_key(&id));
        assert_eq!(ra.flips.get(&id).unwrap().1, 2, "flip count");
        assert!(!rb.flips.contains_key(&id));
        assert_eq!(ra.tips.get(&id).map(|s| &**s), Some("tip"));
        assert!(!rb.tips.contains_key(&id));
        assert_eq!(ra.select_w.get("sel"), Some(&240.));
        assert_eq!(rb.select_w.get("sel"), None);
        assert!(ra.pressed_open.contains(&id) && !rb.pressed_open.contains(&id));
    }

    #[::core::prelude::v1::test]
    fn resetting_or_dropping_an_owner_expires_its_old_handles() {
        let mut scope = MotionScope::new();
        let old = scope.handle();
        let old_id = scope.id();
        assert!(!old.expired());
        scope.reset();
        assert!(old.expired(), "a reset kills the handles of the previous generation");
        assert_ne!(scope.id(), old_id, "a reset allocates a fresh id");
        let current = scope.handle();
        drop(scope);
        assert!(current.expired(), "dropping the owner expires its handles");
    }

    #[::core::prelude::v1::test]
    fn late_callbacks_from_an_expired_scope_neither_read_nor_reinsert_state() {
        let mut scope = MotionScope::new();
        let late = scope.handle();
        scope.reset();
        let fresh = scope.handle();
        // A late tween retarget settles at once and writes nothing anywhere.
        let id = ElementId::from("late");
        assert_eq!(tween_to_at(&late, &id, 7., Duration::from_millis(100)), (7., 7., false, 0));
        // A late hover read is false; the ambient variant through the expired handle too.
        with_motion_scope(late.clone(), || {
            assert!(!is_hovered(&id));
            assert_eq!(count_flips(&id, true, Duration::from_millis(100)), 0, "no flip to replay");
        });
        assert!(!(late.registry().is_some()), "the expired handle upgrades to nothing, so it cannot reinsert entries");
        assert!(fresh.registry().unwrap().borrow().tweens.is_empty(), "the new generation stays clean");
    }

    #[::core::prelude::v1::test]
    fn a_thousand_navigation_renewals_release_every_retired_registry() {
        let mut scope = MotionScope::new();
        let mut retired = Vec::with_capacity(1000);
        for _ in 0..1000 {
            retired.push(scope.handle());
            scope.reset();
        }
        assert!(retired.iter().all(|h| h.expired()), "every retired generation was released");
        assert!(!scope.handle().expired());
    }

    #[::core::prelude::v1::test]
    fn shell_and_leaving_scopes_survive_while_a_page_scope_renews() {
        let (shell, leaving, mut page) = (MotionScope::new(), MotionScope::new(), MotionScope::new());
        let nav = ElementId::from("nav");
        let out = ElementId::from("out");
        shell.handle().registry().unwrap().borrow_mut().hovered.insert(nav.clone(), true);
        leaving.handle().registry().unwrap().borrow_mut().tweens.insert(out.clone(), (0., 1., Instant::now(), 1));
        page.reset();
        assert!(is_hovered_in(&shell.handle(), &nav), "the shell keeps its plate and hover");
        assert!(
            leaving.handle().registry().unwrap().borrow().tweens.contains_key(&out),
            "the leaving step keeps its state until the transition ends"
        );
        assert!(!is_hovered_in(&page.handle(), &nav));
    }

    #[::core::prelude::v1::test]
    fn the_ambient_guard_restores_the_parent_scope_on_nesting_and_unwind() {
        let (outer, inner) = (MotionScope::new(), MotionScope::new());
        let probe = ElementId::from("probe");
        // Conflicting values for one id: only a correct restore can tell the scopes apart.
        outer.handle().registry().unwrap().borrow_mut().hovered.insert(probe.clone(), true);
        inner.handle().registry().unwrap().borrow_mut().hovered.insert(probe.clone(), false);
        with_motion_scope(outer.handle(), || {
            assert!(is_hovered(&probe), "the outer scope is ambient first");
            with_motion_scope(inner.handle(), || {
                assert!(!is_hovered(&probe), "the inner scope is ambient while it runs");
            });
            assert!(is_hovered(&probe), "the outer scope is restored after the inner returns");
        });
        assert!(!is_hovered(&probe), "after the outer returns the ambient scope is detached");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_motion_scope(outer.handle(), || panic!("unwind inside the scope"));
        }));
        assert!(result.is_err());
        assert!(!is_hovered(&probe), "an unwind restores the scope that was ambient before");
    }

    #[::core::prelude::v1::test]
    fn an_expired_ambient_scope_yields_static_defaults() {
        let detached = ScopeHandle::detached();
        with_motion_scope(detached, || {
            let id = ElementId::from("detached");
            assert!(!is_hovered(&id));
            assert_eq!(count_flips(&id, true, Duration::from_millis(100)), 0);
            let plate = plate_state(id.clone(), (10., 50.));
            assert!(!plate.active, "a plate with no scope rests at its target");
            assert_eq!(tween_to_at(&motion_scope(), &id, 3., Duration::from_millis(100)), (3., 3., false, 0));
            reset_hover(&id);
        });
    }

    #[::core::prelude::v1::test]
    fn a_deferred_build_restores_the_scope_it_was_built_under() {
        // Deferred work (RenderOnce elements, paint closures) runs after the building page's
        // guard has ended; without restoring the captured handle it would see no scope at all.
        let scope = MotionScope::new();
        let handle = scope.handle();
        let id = ElementId::from("deferred");
        with_motion_scope(handle.clone(), || {
            handle.registry().unwrap().borrow_mut().hovered.insert(id.clone(), true);
            assert!(is_hovered(&id), "inside the build the ambient scope is live");
        });
        assert!(!is_hovered(&id), "after the guard ends the ambient scope is detached");
        let deferred = with_motion_scope(handle, || is_hovered(&id));
        assert!(deferred, "restoring the captured handle reads the owner's registry");
    }

    #[::core::prelude::v1::test]
    fn invalidating_every_owner_scope_on_close_expires_all_handles() {
        let mut shell = MotionScope::new();
        let mut pages: Vec<MotionScope> = (0..7).map(|_| MotionScope::new()).collect();
        let handles: Vec<ScopeHandle> =
            std::iter::once(shell.handle()).chain(pages.iter().map(MotionScope::handle)).collect();
        shell.reset();
        for page in &mut pages {
            page.reset();
        }
        assert!(handles.iter().all(ScopeHandle::expired), "a close invalidates shell and pages alike");
    }
}
