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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FeatureCapability {
    pub feature: String,
    pub level: CapabilityLevel,
    pub detail: String,
}

impl FeatureCapability {
    fn new(feature: &str, level: CapabilityLevel, detail: &str) -> Self {
        Self {
            feature: feature.into(),
            level,
            detail: detail.into(),
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
            )
        } else {
            FeatureCapability::new(
                "foreground_identity",
                CapabilityLevel::Degraded,
                "generic backend has no authoritative foreground-app probe",
            )
        };
        let mut capabilities = vec![
            FeatureCapability::new(
                "encryption_at_rest",
                CapabilityLevel::Unavailable,
                "bundled SQLite is not SQLCipher",
            ),
            FeatureCapability::new(
                "hardware_key_wrap",
                CapabilityLevel::Unavailable,
                "native hardware key backend is not installed",
            ),
            FeatureCapability::new(
                "memory_lock",
                CapabilityLevel::Unavailable,
                "key material is zeroized but not mlock-backed",
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
            ),
            sandbox,
            foreground,
            FeatureCapability::new(
                "clipboard_privacy_markers",
                CapabilityLevel::Unavailable,
                "generic clipboard adapter cannot observe concealed-content markers",
            ),
            FeatureCapability::new(
                "clipboard_provenance",
                CapabilityLevel::Unavailable,
                "generic clipboard adapter cannot prove the source application or window",
            ),
            FeatureCapability::new(
                "clipboard_flavor_enumeration",
                CapabilityLevel::Degraded,
                "generic clipboard adapter reads one text or image representation",
            ),
            FeatureCapability::new(
                "swap_protection",
                CapabilityLevel::Degraded,
                "swap and hibernation encryption cannot be proven by the app",
            ),
        ];
        if wayland_session {
            let report = WaylandCapabilities::default().probe_report();
            capabilities.extend([
                FeatureCapability::new(
                    "wayland_global_hotkeys",
                    wayland_level(report.hotkeys),
                    "GlobalShortcuts portal was not proven by the generic backend",
                ),
                FeatureCapability::new(
                    "wayland_clipboard_capture",
                    wayland_level(report.capture),
                    "focused clipboard only; data-control protocol was not proven",
                ),
                FeatureCapability::new(
                    "wayland_paste_injection",
                    wayland_level(report.paste),
                    "libei or virtual-keyboard capability was not proven",
                ),
            ]);
        }
        Self {
            strict_mode,
            capabilities,
        }
    }

    pub fn strict_allows_capture(&self) -> bool {
        !self.strict_mode
            || self
                .capabilities
                .iter()
                .all(|capability| capability.level.satisfies_strict())
    }

    pub fn is_fully_protected(&self) -> bool {
        self.capabilities
            .iter()
            .all(|capability| capability.level.satisfies_strict())
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
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_mode_fails_closed_on_missing_encryption() {
        let posture = SecurityPosture::detect(true, true, true);
        assert!(!posture.strict_allows_capture());
        assert!(!posture.is_fully_protected());
        assert!(SecurityPosture::detect(false, false, false).strict_allows_capture());
    }
}
