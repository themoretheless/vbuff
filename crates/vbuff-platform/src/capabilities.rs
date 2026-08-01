//! Capability-honest security posture and strict-mode decisions.

use serde::Serialize;
use vbuff_types::{SecurityPostureLevel, SecurityPostureSummary};

#[cfg(test)]
use crate::SYSTEM_CLIPBOARD_BACKEND;
use crate::lifecycle::{DisplayServer, SessionContext};
use crate::wayland::{WaylandCapabilities, WaylandFeatureState};

/// The capability vocabulary is owned by `vbuff-types`, which every surface
/// (GUI, doctor JSON, future IPC clients) already depends on. Platform code
/// keeps its historical names as aliases so there is exactly one set of
/// variants to keep in sync: none.
pub use vbuff_types::CapabilityView as FeatureCapability;
pub use vbuff_types::CapabilityViewLevel as CapabilityLevel;
pub use vbuff_types::CapabilityViewSeverity as CapabilitySeverity;

const fn satisfies_strict(level: CapabilityLevel) -> bool {
    matches!(
        level,
        CapabilityLevel::Active | CapabilityLevel::NotApplicable
    )
}

const fn is_required(severity: CapabilitySeverity) -> bool {
    matches!(severity, CapabilitySeverity::RequiredForCapture)
}

fn capability(
    feature: &str,
    level: CapabilityLevel,
    detail: &str,
    severity: CapabilitySeverity,
) -> FeatureCapability {
    FeatureCapability {
        feature: feature.into(),
        level,
        detail: detail.into(),
        severity,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SecurityPosture {
    pub strict_mode: bool,
    pub capabilities: Vec<FeatureCapability>,
}

impl SecurityPosture {
    /// Derive the posture from the caller's session snapshot.
    ///
    /// The session is an argument, not a second detection: the posture the GUI
    /// badge shows and the session `doctor` prints are then two views of one
    /// [`SessionContext`], and cannot describe different display servers.
    ///
    /// # The clipboard rows are claims about the compiled backend
    ///
    /// `clipboard_*` and `foreground_identity` state what the backend linked
    /// into this build can observe. That is a *static* claim, and the trait
    /// deliberately no longer offers one: a backend reports what it observed
    /// per read, precisely so a standing declaration cannot drift from
    /// reality. These rows exist anyway because the trust surface has to say
    /// something before the first read happens.
    ///
    /// The drift is guarded rather than prevented: `clipboard_rows_describe_the_compiled_backend`
    /// fails the moment [`SYSTEM_CLIPBOARD_BACKEND`] changes, so a second
    /// backend cannot ship while these sentences still describe the first one.
    pub fn detect(
        session: &SessionContext,
        strict_mode: bool,
        core_dumps_blocked: bool,
        ptrace_blocked: bool,
    ) -> Self {
        let wayland_session = session.display_server == DisplayServer::Wayland;
        let sandbox = detect_sandbox();
        let foreground = if wayland_session {
            capability(
                "foreground_identity",
                CapabilityLevel::Unavailable,
                "Wayland session does not expose foreground identity to this backend",
                CapabilitySeverity::Informational,
            )
        } else {
            capability(
                "foreground_identity",
                CapabilityLevel::Degraded,
                "generic backend has no authoritative foreground-app probe",
                CapabilitySeverity::Informational,
            )
        };
        let mut capabilities = vec![
            capability(
                "encryption_at_rest",
                CapabilityLevel::Unavailable,
                "bundled SQLite is not SQLCipher",
                CapabilitySeverity::Informational,
            ),
            capability(
                "hardware_key_wrap",
                CapabilityLevel::Unavailable,
                "native hardware key backend is not installed",
                CapabilitySeverity::Informational,
            ),
            capability(
                "memory_lock",
                CapabilityLevel::Unavailable,
                "key material is zeroized but not mlock-backed",
                CapabilitySeverity::Informational,
            ),
            capability(
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
            capability(
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
            capability(
                "clipboard_privacy_markers",
                CapabilityLevel::Unavailable,
                "generic clipboard adapter cannot observe concealed-content markers",
                CapabilitySeverity::Informational,
            ),
            capability(
                "clipboard_provenance",
                CapabilityLevel::Unavailable,
                "generic clipboard adapter cannot prove the source application or window",
                CapabilitySeverity::Informational,
            ),
            capability(
                "clipboard_flavor_enumeration",
                CapabilityLevel::Degraded,
                "generic clipboard adapter reads one text or image representation",
                CapabilitySeverity::Informational,
            ),
            capability(
                "swap_protection",
                CapabilityLevel::Degraded,
                "swap and hibernation encryption cannot be proven by the app",
                CapabilitySeverity::Informational,
            ),
        ];
        if wayland_session {
            let report = WaylandCapabilities::default().probe_report();
            capabilities.extend([
                capability(
                    "wayland_global_hotkeys",
                    wayland_level(report.hotkeys),
                    "GlobalShortcuts portal was not proven by the generic backend",
                    CapabilitySeverity::Informational,
                ),
                capability(
                    "wayland_clipboard_capture",
                    wayland_level(report.capture),
                    "focused clipboard only; data-control protocol was not proven",
                    CapabilitySeverity::Informational,
                ),
                capability(
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
            .filter(|capability| is_required(capability.severity))
            .filter(|capability| !satisfies_strict(capability.level))
    }

    /// The single security verdict every surface reports: the GUI badge, the
    /// trust surface, and `doctor`'s health line all derive from this, so a
    /// change of policy can never leave one of them disagreeing.
    ///
    /// Only capture-gating capabilities decide the level; informational gaps
    /// stay visible in the counters of [`SecurityPosture::summary`] but never
    /// force `Partial` or `Blocked` on their own.
    pub fn level(&self) -> SecurityPostureLevel {
        if self.required_capabilities_satisfied() {
            SecurityPostureLevel::Protected
        } else if self.strict_mode {
            SecurityPostureLevel::Blocked
        } else {
            SecurityPostureLevel::Partial
        }
    }

    /// Content-free rollup for UI and IPC surfaces: the verdict plus honest
    /// counters over every capability, informational ones included.
    pub fn summary(&self) -> SecurityPostureSummary {
        let mut summary = SecurityPostureSummary {
            level: self.level(),
            strict_mode: self.strict_mode,
            ..SecurityPostureSummary::default()
        };
        for capability in &self.capabilities {
            let counter = match capability.level {
                CapabilityLevel::Active | CapabilityLevel::NotApplicable => &mut summary.active,
                CapabilityLevel::Degraded => &mut summary.degraded,
                CapabilityLevel::Unavailable => &mut summary.unavailable,
            };
            *counter = counter.saturating_add(1);
        }
        summary
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
    capability(
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

    /// Fixtures name the session explicitly so the assertions describe the
    /// session under test rather than whatever session the test host happens
    /// to run in.
    fn session(display_server: DisplayServer) -> SessionContext {
        SessionContext {
            display_server,
            remote: false,
            seat: None,
            input_injection_allowed: !matches!(
                display_server,
                DisplayServer::Headless | DisplayServer::Unknown
            ),
        }
    }

    fn detect(
        strict_mode: bool,
        core_dumps_blocked: bool,
        ptrace_blocked: bool,
    ) -> SecurityPosture {
        SecurityPosture::detect(
            &session(DisplayServer::X11),
            strict_mode,
            core_dumps_blocked,
            ptrace_blocked,
        )
    }

    /// The posture reads the session it is handed, so a Wayland session gets
    /// the Wayland capability rows on any host OS - and only then.
    #[test]
    fn wayland_capabilities_follow_the_session_argument() {
        let wayland = SecurityPosture::detect(&session(DisplayServer::Wayland), false, true, true);
        let features = |posture: &SecurityPosture| {
            posture
                .capabilities
                .iter()
                .map(|capability| capability.feature.clone())
                .collect::<Vec<_>>()
        };
        assert!(
            features(&wayland)
                .iter()
                .any(|f| f == "wayland_paste_injection")
        );
        assert_eq!(
            wayland
                .capabilities
                .iter()
                .find(|capability| capability.feature == "foreground_identity")
                .map(|capability| capability.level),
            Some(CapabilityLevel::Unavailable)
        );

        let x11 = detect(false, true, true);
        assert!(!features(&x11).iter().any(|f| f.starts_with("wayland_")));
        assert_eq!(
            x11.capabilities
                .iter()
                .find(|capability| capability.feature == "foreground_identity")
                .map(|capability| capability.level),
            Some(CapabilityLevel::Degraded)
        );
    }

    #[test]
    fn strict_mode_allows_capture_when_required_capabilities_are_active() {
        let posture = detect(true, true, true);
        assert!(posture.strict_allows_capture());
        assert!(posture.required_capabilities_satisfied());
        assert_eq!(posture.failing_required().count(), 0);
    }

    #[test]
    fn strict_mode_fails_closed_on_failing_required_capability() {
        let posture = detect(true, true, false);
        assert!(!posture.strict_allows_capture());
        assert!(!posture.required_capabilities_satisfied());
        let failing: Vec<_> = posture.failing_required().collect();
        assert_eq!(failing.len(), 1);
        assert_eq!(failing[0].feature, "ptrace");

        let posture = detect(true, false, true);
        assert!(!posture.strict_allows_capture());
        let failing: Vec<_> = posture.failing_required().collect();
        assert_eq!(failing.len(), 1);
        assert_eq!(failing[0].feature, "core_dumps");
    }

    #[test]
    fn informational_gaps_do_not_block_strict_capture() {
        let posture = detect(true, true, true);
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
        assert!(detect(false, false, false).strict_allows_capture());
        assert!(!detect(false, false, false).required_capabilities_satisfied());
    }

    /// The `clipboard_*` and `foreground_identity` details are sentences about
    /// one specific backend. Nothing else ties them to it, so a second backend
    /// would silently turn them into lies on the trust surface - and a backend
    /// that *can* prove the source application would be reported as unable to.
    /// Changing the compiled backend must therefore fail here first.
    #[test]
    fn clipboard_rows_describe_the_compiled_backend() {
        assert_eq!(
            SYSTEM_CLIPBOARD_BACKEND, "arboard",
            "the compiled clipboard backend changed: re-read every clipboard_* \
             and foreground_identity detail in SecurityPosture::detect before \
             updating this assertion, because each one asserts what that \
             backend cannot observe"
        );
    }

    #[test]
    fn posture_level_blocks_only_when_strict_required_capabilities_fail() {
        assert_eq!(
            detect(true, true, false).level(),
            SecurityPostureLevel::Blocked
        );
        assert_eq!(
            detect(false, true, false).level(),
            SecurityPostureLevel::Partial
        );
    }

    #[test]
    fn posture_level_protected_is_reachable_and_counters_stay_honest() {
        let summary = detect(true, true, true).summary();
        assert_eq!(summary.level, SecurityPostureLevel::Protected);
        assert!(summary.strict_mode);
        // Informational gaps never change the verdict, but they are still
        // counted so the trust surface can list them.
        assert!(summary.degraded > 0 || summary.unavailable > 0);
    }

    #[test]
    fn summary_counts_every_capability_exactly_once() {
        let posture = detect(false, false, false);
        let summary = posture.summary();
        assert_eq!(
            usize::from(summary.active + summary.degraded + summary.unavailable),
            posture.capabilities.len()
        );
    }

    #[test]
    fn only_process_hardening_capabilities_are_required() {
        let posture = detect(true, true, true);
        let required: Vec<&str> = posture
            .capabilities
            .iter()
            .filter(|capability| capability.severity == CapabilitySeverity::RequiredForCapture)
            .map(|capability| capability.feature.as_str())
            .collect();
        assert_eq!(required, ["core_dumps", "ptrace"]);
    }
}
