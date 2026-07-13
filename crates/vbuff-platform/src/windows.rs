//! Pure state machine for the Windows foreground-transfer handshake.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ForegroundHandshake {
    #[default]
    Idle,
    PermissionGranted,
    InputQueuesAttached,
    TargetConfirmed,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForegroundObservation {
    AllowSetForegroundWindowSucceeded,
    InputQueuesAttached,
    TargetIsForeground,
    Rejected,
}

impl ForegroundHandshake {
    pub const fn observe(self, observation: ForegroundObservation) -> Self {
        match (self, observation) {
            (_, ForegroundObservation::Rejected) => Self::Failed,
            (Self::Idle, ForegroundObservation::AllowSetForegroundWindowSucceeded) => {
                Self::PermissionGranted
            }
            (Self::PermissionGranted, ForegroundObservation::InputQueuesAttached) => {
                Self::InputQueuesAttached
            }
            (Self::InputQueuesAttached, ForegroundObservation::TargetIsForeground) => {
                Self::TargetConfirmed
            }
            (state, _) => state,
        }
    }

    pub const fn may_inject(self) -> bool {
        matches!(self, Self::TargetConfirmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injection_requires_confirmed_foreground_target() {
        let state = ForegroundHandshake::Idle
            .observe(ForegroundObservation::AllowSetForegroundWindowSucceeded)
            .observe(ForegroundObservation::InputQueuesAttached)
            .observe(ForegroundObservation::TargetIsForeground);
        assert!(state.may_inject());
        assert!(!ForegroundHandshake::PermissionGranted.may_inject());
    }
}
