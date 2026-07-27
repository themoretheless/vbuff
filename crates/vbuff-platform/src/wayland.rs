//! Capability selection for Wayland clipboard, portal, and bridge backends.

use zeroize::Zeroize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaylandClipboardProtocol {
    ExtDataControlV1,
    WlrDataControlV1,
    FocusedClipboardOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaylandPasteMethod {
    LibeiPortal,
    VirtualKeyboard,
    Wtype,
    Ydotool,
    CopyOnly,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WaylandCapabilities {
    pub ext_data_control: bool,
    pub wlr_data_control: bool,
    pub libei_portal: bool,
    pub virtual_keyboard: bool,
    pub wtype: bool,
    pub ydotool: bool,
    pub global_shortcuts_portal: bool,
    pub foreground_identity: bool,
}

impl WaylandCapabilities {
    pub const fn clipboard_protocol(self) -> WaylandClipboardProtocol {
        if self.ext_data_control {
            WaylandClipboardProtocol::ExtDataControlV1
        } else if self.wlr_data_control {
            WaylandClipboardProtocol::WlrDataControlV1
        } else {
            WaylandClipboardProtocol::FocusedClipboardOnly
        }
    }

    pub const fn paste_method(self) -> WaylandPasteMethod {
        if self.libei_portal {
            WaylandPasteMethod::LibeiPortal
        } else if self.virtual_keyboard {
            WaylandPasteMethod::VirtualKeyboard
        } else if self.wtype {
            WaylandPasteMethod::Wtype
        } else if self.ydotool {
            WaylandPasteMethod::Ydotool
        } else {
            WaylandPasteMethod::CopyOnly
        }
    }

    pub const fn probe_report(self) -> WaylandProbeReport {
        WaylandProbeReport {
            hotkeys: if self.global_shortcuts_portal {
                WaylandFeatureState::Available
            } else {
                WaylandFeatureState::Unavailable
            },
            capture: match self.clipboard_protocol() {
                WaylandClipboardProtocol::ExtDataControlV1
                | WaylandClipboardProtocol::WlrDataControlV1 => WaylandFeatureState::Available,
                WaylandClipboardProtocol::FocusedClipboardOnly => WaylandFeatureState::Degraded,
            },
            paste: if matches!(self.paste_method(), WaylandPasteMethod::CopyOnly) {
                WaylandFeatureState::Unavailable
            } else {
                WaylandFeatureState::Available
            },
            foreground_identity: if self.foreground_identity {
                WaylandFeatureState::Available
            } else {
                WaylandFeatureState::Unavailable
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaylandFeatureState {
    Available,
    Degraded,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaylandProbeReport {
    pub hotkeys: WaylandFeatureState,
    pub capture: WaylandFeatureState,
    pub paste: WaylandFeatureState,
    pub foreground_identity: WaylandFeatureState,
}

pub struct PortalRestoreToken(String);

impl PortalRestoreToken {
    pub fn new(token: String) -> Option<Self> {
        (!token.is_empty() && token.len() <= 4_096 && !token.contains('\0')).then_some(Self(token))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for PortalRestoreToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("PortalRestoreToken([redacted])")
    }
}

impl Drop for PortalRestoreToken {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GnomeBridgeHello {
    pub protocol_version: u16,
    pub clipboard_events: bool,
    pub source_identity: bool,
    pub challenge: [u8; 16],
}

impl GnomeBridgeHello {
    pub const CURRENT_VERSION: u16 = 1;

    pub fn compatible(self, expected_challenge: [u8; 16]) -> bool {
        self.protocol_version == Self::CURRENT_VERSION
            && self.clipboard_events
            && self.challenge == expected_challenge
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_and_paste_ladders_choose_best_available_native_path() {
        let capabilities = WaylandCapabilities {
            ext_data_control: true,
            wlr_data_control: true,
            libei_portal: true,
            ydotool: true,
            ..WaylandCapabilities::default()
        };
        assert_eq!(
            capabilities.clipboard_protocol(),
            WaylandClipboardProtocol::ExtDataControlV1
        );
        assert_eq!(capabilities.paste_method(), WaylandPasteMethod::LibeiPortal);
        let report = capabilities.probe_report();
        assert_eq!(report.capture, WaylandFeatureState::Available);
        assert_eq!(report.paste, WaylandFeatureState::Available);
        assert_eq!(report.hotkeys, WaylandFeatureState::Unavailable);
    }

    #[test]
    fn portal_token_debug_is_redacted() {
        let token = PortalRestoreToken::new("secret-token".into()).unwrap();
        assert!(!format!("{token:?}").contains("secret-token"));
        assert!(PortalRestoreToken::new("x".repeat(4_097)).is_none());
    }

    #[test]
    fn gnome_bridge_requires_the_expected_challenge() {
        let hello = GnomeBridgeHello {
            protocol_version: GnomeBridgeHello::CURRENT_VERSION,
            clipboard_events: true,
            source_identity: false,
            challenge: [7; 16],
        };
        assert!(hello.compatible([7; 16]));
        assert!(!hello.compatible([8; 16]));
    }
}
