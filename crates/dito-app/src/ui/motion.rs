/// Follows the system's Reduce Motion setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Full,
    Reduced,
}

impl Motion {
    pub fn system() -> Self {
        if dito_platform::system::reduce_motion() { Self::Reduced } else { Self::Full }
    }

    pub fn is_reduced(self) -> bool {
        self == Self::Reduced
    }
}
