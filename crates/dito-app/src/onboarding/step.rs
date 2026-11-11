#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::onboarding) enum Step {
    Welcome,
    Permissions,
    Usage,
    Shortcut,
    Engine,
    Prepare,
    Try,
}

impl Step {
    pub(in crate::onboarding) fn number(self) -> usize {
        match self {
            Self::Welcome => 0,
            Self::Permissions => 1,
            Self::Usage => 2,
            Self::Shortcut => 3,
            Self::Engine => 4,
            Self::Prepare => 5,
            Self::Try => 6,
        }
    }

    pub(in crate::onboarding) fn next(self) -> Option<Step> {
        match self {
            Self::Welcome => Some(Self::Permissions),
            Self::Permissions => Some(Self::Usage),
            Self::Usage => Some(Self::Shortcut),
            Self::Shortcut => Some(Self::Engine),
            Self::Engine => Some(Self::Prepare),
            Self::Prepare => Some(Self::Try),
            Self::Try => None,
        }
    }

    pub(in crate::onboarding) fn prev(self) -> Option<Step> {
        match self {
            Self::Welcome => None,
            Self::Permissions => Some(Self::Welcome),
            Self::Usage => Some(Self::Permissions),
            Self::Shortcut => Some(Self::Usage),
            Self::Engine => Some(Self::Shortcut),
            Self::Prepare => Some(Self::Engine),
            Self::Try => Some(Self::Prepare),
        }
    }
}

pub(in crate::onboarding) fn can_continue(
    step: Step,
    local: bool,
    local_models_installed: bool,
    connection_ok: bool,
    tried_ok: bool,
) -> bool {
    match step {
        Step::Prepare if local => local_models_installed,
        Step::Prepare => connection_ok,
        Step::Try => tried_ok,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_walk_in_order_and_back() {
        let order =
            [Step::Welcome, Step::Permissions, Step::Usage, Step::Shortcut, Step::Engine, Step::Prepare, Step::Try];

        let mut walked = vec![Step::Welcome];
        let mut cur = Step::Welcome;
        while let Some(n) = cur.next() {
            assert_eq!(n.number(), cur.number() + 1);
            walked.push(n);
            cur = n;
        }
        assert_eq!(walked, order);
        assert_eq!(cur.next(), None);

        let mut back = vec![cur];
        while let Some(p) = cur.prev() {
            assert_eq!(p.number() + 1, cur.number());
            back.push(p);
            cur = p;
        }
        back.reverse();
        assert_eq!(back, order);
        assert_eq!(Step::Welcome.prev(), None);
        assert_eq!(Step::Welcome.number(), 0);
    }

    #[test]
    fn gating_rule() {
        // Prepare with a local engine: only the installed models matter.
        assert!(can_continue(Step::Prepare, true, true, false, false));
        assert!(!can_continue(Step::Prepare, true, false, true, true));
        // Prepare with a cloud engine: only the connection test matters.
        assert!(can_continue(Step::Prepare, false, false, true, false));
        assert!(!can_continue(Step::Prepare, false, true, false, true));
        // Try: only the try-out matters.
        assert!(can_continue(Step::Try, true, false, false, true));
        assert!(!can_continue(Step::Try, true, true, true, false));
        // Every other step is open regardless of the inputs.
        for step in [Step::Welcome, Step::Permissions, Step::Usage, Step::Shortcut, Step::Engine] {
            assert!(can_continue(step, true, false, false, false));
            assert!(can_continue(step, false, false, false, false));
        }
    }
}
