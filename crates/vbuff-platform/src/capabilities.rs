//! Capability-honest security posture and strict-mode decisions.

use serde::Serialize;

use crate::wayland::{WaylandCapabilities, WaylandFeatureState};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityLevel {
    Active,
    Degraded,
    Unavailable,
    NotApplicable,
}

impl CapabilityLevel {
    const fn satisfies_strict(self) -> bool {
        matches!(self, Self::Active | Self::NotApplicable)
    }
}

/// Whether a capability gates strict-mode capture or is reported for
/// transparency only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySeverity {
    /// Strict mode refuses capture until this capability is satisfied.
    RequiredForCapture,
    /// Informational evidence; never blocks capture, doctor health, or the
    /// protected posture level.
    #[default]
    Informational,
}

impl CapabilitySeverity {
    const fn is_required(self) -> bool {
        matches!(self, Self::RequiredForCapture)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FeatureCapability {
    pub feature: String,
    pub level: CapabilityLevel,
    pub detail: String,
    pub severity: CapabilitySeverity,
}

impl FeatureCapability {
    fn new(
        feature: &str,
        level: CapabilityLevel,
        detail: &str,
        severity: CapabilitySeverity,
    ) -> Self {
        Self {
            feature: feature.into(),
            level,
            detail: detail.into(),
            severity,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SecurityPosture {
    pub strict_mode: bool,
    pub capabilities: Vec<FeatureCapability>,
}

impl SecurityPosture {
    pub fn detect(strict_mode: bool, core_dumps_blocked: bool, ptrace_blocked: bool) -> Self {
        let wayland_session = std::env::var_os("WAYLAND_DISPLAY").is_some();
        let sandbox = detect_sandbox();
        let foreground = if wayland_session {
            FeatureCapability::new(
                "foreground_identity",
                CapabilityLevel::Unavailable,
                "Wayland session does not expose foreground identity to this backend",
                CapabilitySeverity::Informational,
            )
        } else {
            FeatureCapability::new(
                "foreground_identity",
                CapabilityLevel::Degraded,
                "generic backend has no authoritative foreground-app probe",
                CapabilitySeverity::Informational,
            )
        };
        let mut capabilities = vec![
            FeatureCapability::new(
                "encryption_at_rest",
                CapabilityLevel::Unavailable,
                "bundled SQLite is not SQLCipher",
                CapabilitySeverity::Informational,
            ),
            FeatureCapability::new(
                "hardware_key_wrap",
                CapabilityLevel::Unavailable,
                "native hardware key backend is not installed",
                CapabilitySeverity::Informational,
            ),
            FeatureCapability::new(
                "memory_lock",
                CapabilityLevel::Unavailable,
                "key material is zeroized but not mlock-backed",
                CapabilitySeverity::Informational,
            ),
            FeatureCapability::new(
                "core_dumps",
                if core_dumps_blocked {
                    CapabilityLevel::Active
                } else {
                    CapabilityLevel::Unavailable
                },
                if core_dumps_blocked {
                    "process core-dump limit is zero"
                } else {
                    "process core-dump suppression unavailable"
                },
                CapabilitySeverity::RequiredForCapture,
            ),
            FeatureCapability::new(
                "ptrace",
                if ptrace_blocked {
                    CapabilityLevel::Active
                } else {
                    CapabilityLevel::Degraded
                },
                if ptrace_blocked {
                    "process is non-dumpable to peer processes"
                } else {
                    "platform-specific anti-ptrace policy is not active"
                },
                CapabilitySeverity::RequiredForCapture,
            ),
            sandbox,
            foreground,
            FeatureCapability::new(
                "clipboard_privacy_markers",
                CapabilityLevel::Unavailable,
                "generic clipboard adapter cannot observe concealed-content markers",
                CapabilitySeverity::Informational,
            ),
            FeatureCapability::new(
                "clipboard_provenance",
                CapabilityLevel::Unavailable,
                "generic clipboard adapter cannot prove the source application or window",
                CapabilitySeverity::Informational,
            ),
            FeatureCapability::new(
                "clipboard_flavor_enumeration",
                CapabilityLevel::Degraded,
                "generic clipboard adapter reads one text or image representation",
                CapabilitySeverity::Informational,
            ),
            FeatureCapability::new(
                "swap_protection",
                CapabilityLevel::Degraded,
                "swap and hibernation encryption cannot be proven by the app",
                CapabilitySeverity::Informational,
            ),
        ];
        if wayland_session {
            let report = WaylandCapabilities::default().probe_report();
            capabilities.extend([
                FeatureCapability::new(
                    "wayland_global_hotkeys",
                    wayland_level(report.hotkeys),
                    "GlobalShortcuts portal was not proven by the generic backend",
                    CapabilitySeverity::Informational,
                ),
                FeatureCapability::new(
                    "wayland_clipboard_capture",
                    wayland_level(report.capture),
                    "focused clipboard only; data-control protocol was not proven",
                    CapabilitySeverity::Informational,
                ),
                FeatureCapability::new(
                    "wayland_paste_injection",
                    wayland_level(report.paste),
                    "libei or virtual-keyboard capability was not proven",
                    CapabilitySeverity::Informational,
                ),
            ]);
        }
        Self {
            strict_mode,
            capabilities,
        }
    }

    /// Strict mode fails closed only while a capture-gating capability is
    /// unsatisfied; informational gaps never block capture.
    pub fn strict_allows_capture(&self) -> bool {
        !self.strict_mode || self.required_capabilities_satisfied()
    }

    /// All capture-gating capabilities satisfy the strict bar (`Active` or
    /// `NotApplicable`). Informational capabilities are ignored.
    pub fn required_capabilities_satisfied(&self) -> bool {
        self.failing_required().next().is_none()
    }

    /// Capture-gating capabilities that currently fail the strict bar.
    pub fn failing_required(&self) -> impl Iterator<Item = &FeatureCapability> {
        self.capabilities
            .iter()
            .filter(|capability| capability.severity.is_required())
            .filter(|capability| !capability.level.satisfies_strict())
    }
}

const fn wayland_level(state: WaylandFeatureState) -> CapabilityLevel {
    match state {
        WaylandFeatureState::Available => CapabilityLevel::Active,
        WaylandFeatureState::Degraded => CapabilityLevel::Degraded,
        WaylandFeatureState::Unavailable => CapabilityLevel::Unavailable,
    }
}

fn detect_sandbox() -> FeatureCapability {
    let package_marker = std::env::var_os("FLATPAK_ID").is_some()
        || std::env::var_os("SNAP").is_some()
        || std::env::var_os("APP_SANDBOX_CONTAINER_ID").is_some();
    FeatureCapability::new(
        "process_sandbox",
        CapabilityLevel::Degraded,
        if package_marker {
            "package sandbox marker found, but active confinement was not verified"
        } else {
            "no package sandbox detected; use hardened service/package profile"
        },
        CapabilitySeverity::Informational,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_mode_allows_capture_when_required_capabilities_are_active() {
        let posture = SecurityPosture::detect(true, true, true);
        assert!(posture.strict_allows_capture());
        assert!(posture.required_capabilities_satisfied());
        assert_eq!(posture.failing_required().count(), 0);
    }

    #[test]
    fn strict_mode_fails_closed_on_failing_required_capability() {
        let posture = SecurityPosture::detect(true, true, false);
        assert!(!posture.strict_allows_capture());
        assert!(!posture.required_capabilities_satisfied());
        let failing: Vec<_> = posture.failing_required().collect();
        assert_eq!(failing.len(), 1);
        assert_eq!(failing[0].feature, "ptrace");

        let posture = SecurityPosture::detect(true, false, true);
        assert!(!posture.strict_allows_capture());
        let failing: Vec<_> = posture.failing_required().collect();
        assert_eq!(failing.len(), 1);
        assert_eq!(failing[0].feature, "core_dumps");
    }

    #[test]
    fn informational_gaps_do_not_block_strict_capture() {
        let posture = SecurityPosture::detect(true, true, true);
        let informational_gaps = posture
            .capabilities
            .iter()
            .filter(|capability| capability.severity == CapabilitySeverity::Informational)
            .filter(|capability| {
                matches!(
                    capability.level,
                    CapabilityLevel::Degraded | CapabilityLevel::Unavailable
                )
            })
            .count();
        assert!(informational_gaps > 0);
        assert!(posture.strict_allows_capture());
    }

    #[test]
    fn non_strict_mode_always_allows_capture() {
        assert!(SecurityPosture::detect(false, false, false).strict_allows_capture());
        assert!(!SecurityPosture::detect(false, false, false).required_capabilities_satisfied());
    }

    #[test]
    fn only_process_hardening_capabilities_are_required() {
        let posture = SecurityPosture::detect(true, true, true);
        let required: Vec<&str> = posture
            .capabilities
            .iter()
            .filter(|capability| capability.severity == CapabilitySeverity::RequiredForCapture)
            .map(|capability| capability.feature.as_str())
            .collect();
        assert_eq!(required, ["core_dumps", "ptrace"]);
    }
}
