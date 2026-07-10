//! Commands shared by the popup, tray, hotkey, and app wiring.

use vbuff_gui::UiAction;
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
    #[cfg(feature = "tray")]
    ToggleAutostart,
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
}
