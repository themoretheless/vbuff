//! Commands shared by the popup, tray, hotkey, and app wiring.

use vbuff_gui::{StarterPack, UiAction};
use vbuff_types::ClipId;

/// One vocabulary for every user-facing command surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AppCommand {
    Show,
    Paste(ClipId),
    #[cfg(feature = "tray")]
    CopyLatest,
    SetPinned(ClipId, bool),
    Delete(ClipId),
    #[cfg(feature = "tray")]
    RequestClearHistory,
    ClearHistory,
    TogglePause,
    RecoverSkipped,
    InstallStarterPack(StarterPack),
    #[cfg(feature = "tray")]
    ToggleAutostart,
    DismissNotice,
    Hide,
    #[cfg(feature = "tray")]
    Quit,
}

impl From<UiAction> for AppCommand {
    fn from(action: UiAction) -> Self {
        match action {
            UiAction::Paste(id) => Self::Paste(id),
            UiAction::SetPinned(id, pinned) => Self::SetPinned(id, pinned),
            UiAction::Delete(id) => Self::Delete(id),
            UiAction::ClearHistory => Self::ClearHistory,
            UiAction::TogglePause => Self::TogglePause,
            UiAction::RecoverSkipped => Self::RecoverSkipped,
            UiAction::InstallStarterPack(pack) => Self::InstallStarterPack(pack),
            UiAction::DismissNotice => Self::DismissNotice,
            UiAction::Hide => Self::Hide,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gui_clear_maps_to_shared_clear_history_command() {
        assert_eq!(
            AppCommand::from(UiAction::ClearHistory),
            AppCommand::ClearHistory
        );
    }

    #[test]
    fn notice_dismissal_stays_a_high_level_command() {
        assert_eq!(
            AppCommand::from(UiAction::DismissNotice),
            AppCommand::DismissNotice
        );
    }

    #[test]
    fn skipped_capture_recovery_stays_a_high_level_command() {
        assert_eq!(
            AppCommand::from(UiAction::RecoverSkipped),
            AppCommand::RecoverSkipped
        );
    }
}
