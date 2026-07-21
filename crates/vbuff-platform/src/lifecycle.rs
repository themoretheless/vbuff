//! Session lifecycle, reconnect, power, and coexistence policies.

use std::time::Duration;

use serde::Serialize;
pub use vbuff_types::CapturePauseReason as AutoPauseReason;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayServer {
    Wayland,
    X11,
    Windows,
    MacOs,
    Headless,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SessionContext {
    pub display_server: DisplayServer,
    pub remote: bool,
    pub seat: Option<String>,
    pub input_injection_allowed: bool,
}

impl SessionContext {
    pub fn detect() -> Self {
        let display_server = if cfg!(target_os = "windows") {
            DisplayServer::Windows
        } else if cfg!(target_os = "macos") {
            DisplayServer::MacOs
        } else if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            DisplayServer::Wayland
        } else if std::env::var_os("DISPLAY").is_some() {
            DisplayServer::X11
        } else if std::env::var_os("CI").is_some() {
            DisplayServer::Headless
        } else {
            DisplayServer::Unknown
        };
        let windows_remote = std::env::var("SESSIONNAME")
            .ok()
            .is_some_and(|name| name.to_ascii_uppercase().starts_with("RDP-"));
        let remote = windows_remote
            || std::env::var_os("SSH_CONNECTION").is_some()
            || std::env::var_os("VNCSESSION").is_some();
        let input_injection_allowed = !remote
            && !matches!(
                display_server,
                DisplayServer::Headless | DisplayServer::Unknown
            );
        Self {
            display_server,
            remote,
            seat: std::env::var("XDG_SEAT").ok(),
            input_injection_allowed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionState {
    locked: bool,
    foreground: bool,
    sleeping: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AutoPauseSignals {
    pub idle_for: Duration,
    pub screen_locked: bool,
    pub remote_control_active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutoPausePolicy {
    pub idle_after: Option<Duration>,
    pub pause_on_lock: bool,
    pub pause_on_remote_control: bool,
}

impl Default for AutoPausePolicy {
    fn default() -> Self {
        Self {
            idle_after: Some(Duration::from_secs(15 * 60)),
            pause_on_lock: true,
            pause_on_remote_control: true,
        }
    }
}

impl AutoPausePolicy {
    pub fn reason(self, signals: AutoPauseSignals) -> Option<AutoPauseReason> {
        if self.pause_on_lock && signals.screen_locked {
            Some(AutoPauseReason::ScreenLocked)
        } else if self.pause_on_remote_control && signals.remote_control_active {
            Some(AutoPauseReason::RemoteControl)
        } else if self
            .idle_after
            .is_some_and(|threshold| !threshold.is_zero() && signals.idle_for >= threshold)
        {
            Some(AutoPauseReason::Idle)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoPauseTransition {
    Pause(AutoPauseReason),
    Resume,
}

#[derive(Clone, Copy, Debug)]
pub struct AutoPauseController {
    policy: AutoPausePolicy,
    active_reason: Option<AutoPauseReason>,
}

impl AutoPauseController {
    pub const fn new(policy: AutoPausePolicy) -> Self {
        Self {
            policy,
            active_reason: None,
        }
    }

    pub fn observe(
        &mut self,
        signals: AutoPauseSignals,
        manually_paused: bool,
    ) -> Option<AutoPauseTransition> {
        let next = self.policy.reason(signals);
        if next == self.active_reason {
            return None;
        }
        let previous = self.active_reason;
        self.active_reason = next;
        match (previous, next) {
            (_, Some(reason)) => Some(AutoPauseTransition::Pause(reason)),
            (Some(_), None) if !manually_paused => Some(AutoPauseTransition::Resume),
            _ => None,
        }
    }

    pub const fn active_reason(self) -> Option<AutoPauseReason> {
        self.active_reason
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            locked: false,
            foreground: true,
            sleeping: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionEvent {
    Locked,
    Unlocked,
    BecameForeground,
    BecameBackground,
    SleepStarted,
    WakeCompleted,
}

impl SessionState {
    pub fn transition(mut self, event: SessionEvent) -> Self {
        match event {
            SessionEvent::Locked => self.locked = true,
            SessionEvent::Unlocked => self.locked = false,
            SessionEvent::BecameForeground => self.foreground = true,
            SessionEvent::BecameBackground => self.foreground = false,
            SessionEvent::SleepStarted => self.sleeping = true,
            SessionEvent::WakeCompleted => self.sleeping = false,
        }
        self
    }

    pub const fn permits_capture(self) -> bool {
        !self.locked && self.foreground && !self.sleeping
    }

    pub const fn permits_paste(self) -> bool {
        !self.locked && self.foreground && !self.sleeping
    }
}

#[derive(Clone, Debug)]
pub struct ReconnectBackoff {
    initial: Duration,
    maximum: Duration,
    current: Duration,
}

impl ReconnectBackoff {
    pub fn new(initial: Duration, maximum: Duration) -> Self {
        let initial = initial.max(Duration::from_millis(50));
        let maximum = maximum.max(initial);
        Self {
            initial,
            maximum,
            current: initial,
        }
    }

    pub fn disconnected(&mut self) -> Duration {
        let delay = self.current;
        self.current = self.current.saturating_mul(2).min(self.maximum);
        delay
    }

    pub fn connected(&mut self) {
        self.current = self.initial;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PowerSignals {
    pub low_power_mode: bool,
    pub display_asleep: bool,
    pub thermal_pressure: bool,
    pub copy_in_flight: bool,
}

pub fn power_aware_poll_interval(base: Duration, signals: PowerSignals) -> Duration {
    if signals.copy_in_flight {
        return base;
    }
    let multiplier = 1_u32
        + u32::from(signals.low_power_mode)
        + u32::from(signals.thermal_pressure)
        + 3 * u32::from(signals.display_asleep);
    base.saturating_mul(multiplier).min(Duration::from_secs(5))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoexistenceMode {
    Exclusive,
    Cooperative,
    ReadOnly,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoexistenceReport {
    pub detected_managers: Vec<String>,
    pub recommended_mode: CoexistenceMode,
}

pub fn detect_coexisting_managers(
    process_names: impl IntoIterator<Item = String>,
) -> CoexistenceReport {
    const KNOWN: [&str; 8] = [
        "copyq", "maccy", "ditto", "cliphist", "clipman", "klipper", "gpaste", "pastebot",
    ];
    let mut detected_managers = process_names
        .into_iter()
        .filter(|name| {
            let lower = name.to_ascii_lowercase();
            KNOWN.iter().any(|known| lower.contains(known))
        })
        .collect::<Vec<_>>();
    detected_managers.sort();
    detected_managers.dedup();
    CoexistenceReport {
        recommended_mode: if detected_managers.is_empty() {
            CoexistenceMode::Exclusive
        } else {
            CoexistenceMode::Cooperative
        },
        detected_managers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_and_background_sessions_fail_closed() {
        let state = SessionState::default().transition(SessionEvent::Locked);
        assert!(!state.permits_capture());
        assert!(state.transition(SessionEvent::Unlocked).permits_capture());
        let background_and_locked = SessionState::default()
            .transition(SessionEvent::BecameBackground)
            .transition(SessionEvent::Locked)
            .transition(SessionEvent::Unlocked);
        assert!(!background_and_locked.permits_capture());
    }

    #[test]
    fn reconnect_backoff_is_bounded_and_resets() {
        let mut backoff = ReconnectBackoff::new(Duration::from_millis(100), Duration::from_secs(1));
        assert_eq!(backoff.disconnected(), Duration::from_millis(100));
        assert_eq!(backoff.disconnected(), Duration::from_millis(200));
        backoff.connected();
        assert_eq!(backoff.disconnected(), Duration::from_millis(100));
    }

    #[test]
    fn power_signals_slow_idle_polling_only() {
        let base = Duration::from_millis(100);
        assert_eq!(
            power_aware_poll_interval(
                base,
                PowerSignals {
                    low_power_mode: true,
                    display_asleep: true,
                    ..PowerSignals::default()
                }
            ),
            Duration::from_millis(500)
        );
    }

    #[test]
    fn coexistence_is_detected_case_insensitively() {
        let report = detect_coexisting_managers(["CopyQ".into(), "terminal".into()]);
        assert_eq!(report.detected_managers, vec!["CopyQ"]);
        assert_eq!(report.recommended_mode, CoexistenceMode::Cooperative);
    }

    #[test]
    fn auto_pause_prioritizes_lock_and_never_resumes_a_manual_pause() {
        let policy = AutoPausePolicy {
            idle_after: Some(Duration::from_secs(60)),
            ..AutoPausePolicy::default()
        };
        let mut controller = AutoPauseController::new(policy);
        assert_eq!(
            controller.observe(
                AutoPauseSignals {
                    idle_for: Duration::from_secs(61),
                    ..AutoPauseSignals::default()
                },
                false,
            ),
            Some(AutoPauseTransition::Pause(AutoPauseReason::Idle))
        );
        assert_eq!(
            controller.observe(
                AutoPauseSignals {
                    screen_locked: true,
                    ..AutoPauseSignals::default()
                },
                false,
            ),
            Some(AutoPauseTransition::Pause(AutoPauseReason::ScreenLocked))
        );
        assert_eq!(controller.observe(AutoPauseSignals::default(), true), None);
        assert_eq!(controller.active_reason(), None);
    }

    #[test]
    fn clearing_an_automatic_reason_requests_resume() {
        let mut controller = AutoPauseController::new(AutoPausePolicy::default());
        assert_eq!(
            controller.observe(
                AutoPauseSignals {
                    remote_control_active: true,
                    ..AutoPauseSignals::default()
                },
                false,
            ),
            Some(AutoPauseTransition::Pause(AutoPauseReason::RemoteControl))
        );
        assert_eq!(
            controller.observe(AutoPauseSignals::default(), false),
            Some(AutoPauseTransition::Resume)
        );
    }
}
