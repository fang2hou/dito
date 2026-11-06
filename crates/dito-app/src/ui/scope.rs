use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use gpui_kit::*;

use super::plate::PlateMotion;

static NEXT_SCOPE_ID: AtomicU64 = AtomicU64::new(1);

/// Identity of one registry generation, fresh for every [`MotionScope::new`] and [`MotionScope::reset`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MotionScopeId(u64);

impl MotionScopeId {
    fn next() -> Self {
        Self(NEXT_SCOPE_ID.fetch_add(1, Ordering::Relaxed))
    }
}

/// The transient per-owner UI state: tweens, hover, sliding plates, flips, tooltip labels, select widths and the
/// open-menu press marks. It lives and dies with its owner; ids are unique only inside one registry.
pub(super) struct Registry {
    /// Per animated value: (from, to, start, key).
    pub(super) tweens: HashMap<ElementId, (f32, f32, Instant, usize)>,
    pub(super) hovered: HashMap<ElementId, bool>,
    /// Per sliding plate.
    pub(super) plates: HashMap<ElementId, PlateMotion>,
    /// Last value and number of flips seen per element id; a flip starts a new animation keyed by the count.
    pub(super) flips: HashMap<ElementId, (bool, usize, Instant)>,
    /// Label per element id, captured at build time for the tooltip.
    pub(super) tips: HashMap<ElementId, SharedString>,
    /// Width of the last painted fill-width select button, read by its menu panel. The panel is built before the
    /// button paints, so it sees the previous frame's measurement, which stays correct because the button repaints
    /// every frame.
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

/// The owner of one bubble of transient motion state: a window's shell, a settings page, a wizard step. Everything
/// its controls remember between frames lives in its [`Registry`].
///
/// [`reset`](Self::reset) swaps in a fresh registry, so every handle handed out before dies: a control drawn again
/// later never replays a remembered value, and a late callback from the old generation can neither read nor
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

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{ElementId, MotionScope, PlateMotion, ScopeHandle, SharedString, motion_scope, with_motion_scope};
    use crate::ui::hover::{is_hovered, is_hovered_in, reset_hover};
    use crate::ui::plate::plate_state;
    use crate::ui::tween::{count_flips, tween_to_at};

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
    fn the_ambient_guard_restores_the_parent_scope_on_nesting() {
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
    }

    #[test]
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

    #[test]
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

    #[test]
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
