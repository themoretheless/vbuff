//! Permission-loss watchdog independent from native probe implementations.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionKind {
    Accessibility,
    WaylandPortal,
    GlobalHotkey,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PermissionState {
    #[default]
    Unknown,
    Granted,
    Revoked,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionEvent {
    None,
    Granted,
    Lost,
    Restored,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionWatchdog {
    pub kind: PermissionKind,
    state: PermissionState,
    consecutive_failures: u8,
    failure_threshold: u8,
}

impl PermissionWatchdog {
    pub fn new(kind: PermissionKind, failure_threshold: u8) -> Self {
        Self {
            kind,
            state: PermissionState::Unknown,
            consecutive_failures: 0,
            failure_threshold: failure_threshold.max(1),
        }
    }

    pub fn observe(&mut self, state: PermissionState) -> PermissionEvent {
        match state {
            PermissionState::Granted => {
                self.consecutive_failures = 0;
                let event = match self.state {
                    PermissionState::Revoked | PermissionState::Unavailable => {
                        PermissionEvent::Restored
                    }
                    PermissionState::Unknown => PermissionEvent::Granted,
                    PermissionState::Granted => PermissionEvent::None,
                };
                self.state = PermissionState::Granted;
                event
            }
            PermissionState::Revoked => {
                self.consecutive_failures = self.consecutive_failures.saturating_add(1);
                if self.consecutive_failures < self.failure_threshold {
                    return PermissionEvent::None;
                }
                let event = if self.state != PermissionState::Revoked {
                    PermissionEvent::Lost
                } else {
                    PermissionEvent::None
                };
                self.state = PermissionState::Revoked;
                event
            }
            PermissionState::Unavailable => {
                let changed = self.state != PermissionState::Unavailable;
                self.state = PermissionState::Unavailable;
                if changed {
                    PermissionEvent::Unavailable
                } else {
                    PermissionEvent::None
                }
            }
            PermissionState::Unknown => PermissionEvent::None,
        }
    }

    pub fn state(&self) -> PermissionState {
        self.state
    }

    pub fn settings_deep_link(&self) -> Option<&'static str> {
        match self.kind {
            PermissionKind::Accessibility => Some(
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
            ),
            PermissionKind::WaylandPortal => Some("settings://applications/vbuff/permissions"),
            PermissionKind::GlobalHotkey => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_probe_failure_does_not_claim_permission_loss() {
        let mut watchdog = PermissionWatchdog::new(PermissionKind::Accessibility, 2);
        assert_eq!(
            watchdog.observe(PermissionState::Granted),
            PermissionEvent::Granted
        );
        assert_eq!(
            watchdog.observe(PermissionState::Revoked),
            PermissionEvent::None
        );
        assert_eq!(
            watchdog.observe(PermissionState::Revoked),
            PermissionEvent::Lost
        );
        assert_eq!(
            watchdog.observe(PermissionState::Granted),
            PermissionEvent::Restored
        );
        assert!(
            watchdog
                .settings_deep_link()
                .unwrap()
                .contains("Accessibility")
        );
    }
}
