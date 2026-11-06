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
}
