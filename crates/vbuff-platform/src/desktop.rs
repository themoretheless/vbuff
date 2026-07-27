//! Native desktop-shell status, tray fallback, and paste-permission contracts.

use vbuff_types::{CaptureHealth, CapturePauseReason};

use crate::lifecycle::{DisplayServer, SessionContext};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopShell {
    MacMenuBar,
    WindowsNotificationArea,
    LinuxStatusNotifier,
    LinuxLegacyTray,
    PopupCommand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuickMenuLabels {
    pub open: &'static str,
    pub copy_latest: &'static str,
    pub clear_history: &'static str,
    pub pause: &'static str,
    pub resume: &'static str,
    pub autostart_on: &'static str,
    pub autostart_off: &'static str,
    pub quit: &'static str,
}

impl QuickMenuLabels {
    pub const fn for_shell(shell: DesktopShell) -> Self {
        match shell {
            DesktopShell::WindowsNotificationArea => Self {
                open: "Open vbuff",
                copy_latest: "Copy latest item",
                clear_history: "Clear history...",
                pause: "Pause monitoring",
                resume: "Resume monitoring",
                autostart_on: "Launch at sign-in",
                autostart_off: "Don't launch at sign-in",
                quit: "Exit vbuff",
            },
            DesktopShell::MacMenuBar => Self {
                open: "Open vbuff",
                copy_latest: "Copy latest clip",
                clear_history: "Clear history...",
                pause: "Pause capture",
                resume: "Resume capture",
                autostart_on: "Open at Login",
                autostart_off: "Don't Open at Login",
                quit: "Quit vbuff",
            },
            DesktopShell::LinuxStatusNotifier
            | DesktopShell::LinuxLegacyTray
            | DesktopShell::PopupCommand => Self {
                open: "Show vbuff",
                copy_latest: "Copy latest clip",
                clear_history: "Clear history...",
                pause: "Pause capture",
                resume: "Resume capture",
                autostart_on: "Start at login",
                autostart_off: "Don't start at login",
                quit: "Quit vbuff",
            },
        }
    }
}

pub const fn current_desktop_shell() -> DesktopShell {
    if cfg!(target_os = "macos") {
        DesktopShell::MacMenuBar
    } else if cfg!(target_os = "windows") {
        DesktopShell::WindowsNotificationArea
    } else {
        DesktopShell::LinuxStatusNotifier
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResidentStatus {
    #[default]
    Active,
    Paused,
    Degraded,
    Locked,
    PasteConfirmed,
}

impl ResidentStatus {
    pub const fn from_runtime(
        paused: bool,
        pause_reason: Option<CapturePauseReason>,
        health: CaptureHealth,
        paste_confirmed: bool,
    ) -> Self {
        if matches!(
            pause_reason,
            Some(CapturePauseReason::ScreenLocked | CapturePauseReason::SecurityPolicy)
        ) {
            return Self::Locked;
        }
        if paste_confirmed {
            return Self::PasteConfirmed;
        }
        if paused {
            return Self::Paused;
        }
        if matches!(health, CaptureHealth::Starting | CaptureHealth::Watching) {
            Self::Active
        } else {
            Self::Degraded
        }
    }

    pub const fn tooltip(self) -> &'static str {
        match self {
            Self::Active => "vbuff - capture active",
            Self::Paused => "vbuff - capture paused",
            Self::Degraded => "vbuff - capture issue",
            Self::Locked => "vbuff - capture locked",
            Self::PasteConfirmed => "vbuff - paste complete",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinuxTrayFallback {
    StatusNotifier,
    LegacyTray,
    PopupCommand,
}

impl LinuxTrayFallback {
    pub const fn choose(status_notifier: bool, legacy_tray: bool) -> Self {
        if status_notifier {
            Self::StatusNotifier
        } else if legacy_tray {
            Self::LegacyTray
        } else {
            Self::PopupCommand
        }
    }

    pub const fn shell(self) -> DesktopShell {
        match self {
            Self::StatusNotifier => DesktopShell::LinuxStatusNotifier,
            Self::LegacyTray => DesktopShell::LinuxLegacyTray,
            Self::PopupCommand => DesktopShell::PopupCommand,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PastePermissionLevel {
    Automatic,
    CopyOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PastePermissionSelfCheck {
    pub level: PastePermissionLevel,
    pub detail: &'static str,
    pub settings_uri: Option<&'static str>,
}

impl PastePermissionSelfCheck {
    pub const fn evaluate_session(session: &SessionContext, backend_available: bool) -> Self {
        if session.remote {
            return Self {
                level: PastePermissionLevel::CopyOnly,
                detail: "automatic paste is disabled in a remote session",
                settings_uri: None,
            };
        }
        if !session.input_injection_allowed {
            return Self {
                level: PastePermissionLevel::CopyOnly,
                detail: match session.display_server {
                    DisplayServer::Headless | DisplayServer::Unknown => {
                        "desktop input injection is unavailable"
                    }
                    _ => "automatic paste is disabled by the current session policy",
                },
                settings_uri: None,
            };
        }
        Self::evaluate(session.display_server, backend_available)
    }

    pub const fn evaluate(display: DisplayServer, backend_available: bool) -> Self {
        let backend_proven = backend_available
            && !matches!(
                display,
                DisplayServer::Wayland | DisplayServer::Headless | DisplayServer::Unknown
            );
        if !backend_proven {
            return Self {
                level: PastePermissionLevel::CopyOnly,
                detail: match display {
                    DisplayServer::MacOs => {
                        "Accessibility or the native paste backend is unavailable"
                    }
                    DisplayServer::Windows => "SendInput backend is unavailable",
                    DisplayServer::Wayland => {
                        "no proven Wayland input-injection protocol is available"
                    }
                    DisplayServer::X11 => "X11 input-injection backend is unavailable",
                    DisplayServer::Headless | DisplayServer::Unknown => {
                        "desktop input injection is unavailable"
                    }
                },
                settings_uri: match display {
                    DisplayServer::MacOs => Some(
                        "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
                    ),
                    DisplayServer::Windows => None,
                    _ => None,
                },
            };
        }
        Self {
            level: PastePermissionLevel::Automatic,
            detail: match display {
                DisplayServer::MacOs => "Accessibility permission verified by the native backend",
                DisplayServer::Windows => {
                    "SendInput ready; elevated targets can still deny lower-integrity input"
                }
                DisplayServer::Wayland => "Wayland input-injection backend initialized",
                DisplayServer::X11 => "X11 input-injection backend initialized",
                DisplayServer::Headless | DisplayServer::Unknown => {
                    "input-injection backend initialized without a proven desktop session"
                }
            },
            settings_uri: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resident_status_has_stable_precedence() {
        assert_eq!(
            ResidentStatus::from_runtime(
                true,
                Some(CapturePauseReason::ScreenLocked),
                CaptureHealth::StorageError,
                true,
            ),
            ResidentStatus::Locked
        );
        assert_eq!(
            ResidentStatus::from_runtime(false, None, CaptureHealth::StorageError, true),
            ResidentStatus::PasteConfirmed
        );
        assert_eq!(ResidentStatus::Locked.tooltip(), "vbuff - capture locked");
    }

    #[test]
    fn linux_fallback_always_leaves_a_popup_command() {
        assert_eq!(
            LinuxTrayFallback::choose(false, false),
            LinuxTrayFallback::PopupCommand
        );
        assert_eq!(
            LinuxTrayFallback::choose(true, true),
            LinuxTrayFallback::StatusNotifier
        );
    }

    #[test]
    fn failed_permission_check_is_immediately_copy_only() {
        let check = PastePermissionSelfCheck::evaluate(DisplayServer::MacOs, false);
        assert_eq!(check.level, PastePermissionLevel::CopyOnly);
        assert!(check.settings_uri.unwrap().contains("Accessibility"));
    }

    #[test]
    fn generic_wayland_backend_never_claims_an_unproven_protocol() {
        let check = PastePermissionSelfCheck::evaluate(DisplayServer::Wayland, true);
        assert_eq!(check.level, PastePermissionLevel::CopyOnly);
        assert!(check.detail.contains("proven"));
    }

    #[test]
    fn remote_session_reports_the_session_boundary_not_a_permission_guess() {
        let session = SessionContext {
            display_server: DisplayServer::MacOs,
            remote: true,
            seat: None,
            input_injection_allowed: false,
        };
        let check = PastePermissionSelfCheck::evaluate_session(&session, false);
        assert_eq!(check.level, PastePermissionLevel::CopyOnly);
        assert!(check.detail.contains("remote session"));
        assert!(!check.detail.contains("Accessibility"));
        assert_eq!(check.settings_uri, None);
    }

    #[test]
    fn windows_menu_uses_notification_area_conventions() {
        let labels = QuickMenuLabels::for_shell(DesktopShell::WindowsNotificationArea);
        assert_eq!(labels.pause, "Pause monitoring");
        assert_eq!(labels.quit, "Exit vbuff");
    }
}
