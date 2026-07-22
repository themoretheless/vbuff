//! Native desktop-shell status, tray fallback, and paste-permission contracts.

use vbuff_types::{CaptureHealth, CapturePauseReason};

use crate::lifecycle::{DisplayServer, SessionContext};

/// Read the desktop's reduced-motion preference without treating an unknown
/// shell or failed probe as an affirmative accessibility setting.
pub fn reduced_motion_preference() -> Option<bool> {
    #[cfg(target_os = "macos")]
    {
        command_bool(
            "defaults",
            &["read", "com.apple.universalaccess", "reduceMotion"],
        )
        .or_else(|| {
            command_bool(
                "defaults",
                &["read", "-g", "NSAutomaticWindowAnimationsEnabled"],
            )
            .map(|animations_enabled| !animations_enabled)
        })
    }
    #[cfg(target_os = "linux")]
    {
        command_bool(
            "gsettings",
            &["get", "org.gnome.desktop.interface", "enable-animations"],
        )
        .map(|animations_enabled| !animations_enabled)
    }
    #[cfg(target_os = "windows")]
    {
        windows_reduced_motion_preference()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn command_bool(program: &str, arguments: &[&str]) -> Option<bool> {
    let output = std::process::Command::new(program)
        .args(arguments)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| parse_desktop_bool(&String::from_utf8_lossy(&output.stdout)))?
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn parse_desktop_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" => Some(true),
        "0" | "false" | "no" => Some(false),
        _ => None,
    }
}

#[cfg(target_os = "windows")]
fn windows_reduced_motion_preference() -> Option<bool> {
    use std::ffi::c_void;

    const SPI_GETCLIENTAREAANIMATION: u32 = 0x1042;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SystemParametersInfoW(
            action: u32,
            parameter: u32,
            value: *mut c_void,
            update: u32,
        ) -> i32;
    }

    let mut animations_enabled = 1_i32;
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL to the valid pointer.
    let succeeded = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            (&mut animations_enabled as *mut i32).cast(),
            0,
        )
    } != 0;
    succeeded.then_some(animations_enabled == 0)
}

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
    PasteSent,
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
            return Self::PasteSent;
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

    pub const fn title(self) -> &'static str {
        match self {
            Self::Active => "",
            Self::Paused => "Paused",
            Self::Degraded => "Issue",
            Self::Locked => "Locked",
            Self::PasteSent => "Sent",
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
    pub const fn evaluate_session(
        session: &SessionContext,
        confirmed_target_backend: bool,
    ) -> Self {
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
        Self::evaluate(session.display_server, confirmed_target_backend)
    }

    pub const fn evaluate(display: DisplayServer, confirmed_target_backend: bool) -> Self {
        let backend_proven = confirmed_target_backend
            && !matches!(
                display,
                DisplayServer::Wayland | DisplayServer::Headless | DisplayServer::Unknown
            );
        if !backend_proven {
            return Self {
                level: PastePermissionLevel::CopyOnly,
                detail: match display {
                    DisplayServer::MacOs => {
                        "target confirmation or the Accessibility paste backend is unavailable"
                    }
                    DisplayServer::Windows => {
                        "confirmed foreground target or SendInput backend is unavailable"
                    }
                    DisplayServer::Wayland => {
                        "no proven Wayland input-injection protocol is available"
                    }
                    DisplayServer::X11 => {
                        "confirmed foreground target or X11 injection backend is unavailable"
                    }
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
                DisplayServer::MacOs => {
                    "paste target and Accessibility permission verified by the native backend"
                }
                DisplayServer::Windows => {
                    "foreground target confirmed; elevated targets can still deny lower-integrity input"
                }
                DisplayServer::Wayland => "Wayland input-injection backend initialized",
                DisplayServer::X11 => "foreground target and X11 injection backend confirmed",
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

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn desktop_boolean_parser_is_strict() {
        assert_eq!(parse_desktop_bool("true\n"), Some(true));
        assert_eq!(parse_desktop_bool("0"), Some(false));
        assert_eq!(parse_desktop_bool("enabled"), None);
    }

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
            ResidentStatus::PasteSent
        );
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
