//! Commands shared by the popup, tray, hotkey, and app wiring.

use std::fmt;

use vbuff_gui::{StarterPack, UiAction};
use vbuff_types::{Clip, ClipId};

/// One vocabulary for every user-facing command surface.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum AppCommand {
    Show,
    Paste(ClipId),
    PasteText(String),
    #[cfg(feature = "tray")]
    CopyLatest,
    SetPinned(ClipId, bool),
    Delete(ClipId),
    RestoreClip(Box<Clip>),
    #[cfg(feature = "tray")]
    RequestClearHistory,
    ClearHistory,
    TogglePause,
    RecoverSkipped,
    InstallStarterPack(StarterPack),
    #[cfg(feature = "tray")]
    ToggleAutostart,
    DismissNotice,
    DismissHotkeyCoachmark,
    Hide,
    #[cfg(feature = "tray")]
    Quit,
}

impl fmt::Debug for AppCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Show => formatter.write_str("Show"),
            Self::Paste(id) => formatter.debug_tuple("Paste").field(id).finish(),
            Self::PasteText(text) => formatter
                .debug_struct("PasteText")
                .field("text", &format_args!("[redacted; {} bytes]", text.len()))
                .finish(),
            #[cfg(feature = "tray")]
            Self::CopyLatest => formatter.write_str("CopyLatest"),
            Self::SetPinned(id, pinned) => formatter
                .debug_tuple("SetPinned")
                .field(id)
                .field(pinned)
                .finish(),
            Self::Delete(id) => formatter.debug_tuple("Delete").field(id).finish(),
            Self::RestoreClip(clip) => formatter
                .debug_struct("RestoreClip")
                .field("id", &clip.id)
                .field("kind", &clip.meta.kind)
                .field("bytes", &clip.meta.byte_size)
                .finish(),
            #[cfg(feature = "tray")]
            Self::RequestClearHistory => formatter.write_str("RequestClearHistory"),
            Self::ClearHistory => formatter.write_str("ClearHistory"),
            Self::TogglePause => formatter.write_str("TogglePause"),
            Self::RecoverSkipped => formatter.write_str("RecoverSkipped"),
            Self::InstallStarterPack(pack) => formatter
                .debug_tuple("InstallStarterPack")
                .field(pack)
                .finish(),
            #[cfg(feature = "tray")]
            Self::ToggleAutostart => formatter.write_str("ToggleAutostart"),
            Self::DismissNotice => formatter.write_str("DismissNotice"),
            Self::DismissHotkeyCoachmark => formatter.write_str("DismissHotkeyCoachmark"),
            Self::Hide => formatter.write_str("Hide"),
            #[cfg(feature = "tray")]
            Self::Quit => formatter.write_str("Quit"),
        }
    }
}

impl From<UiAction> for AppCommand {
    fn from(action: UiAction) -> Self {
        match action {
            UiAction::Paste(id) => Self::Paste(id),
            UiAction::PasteText(text) => Self::PasteText(text),
            UiAction::SetPinned(id, pinned) => Self::SetPinned(id, pinned),
            UiAction::Delete(id) => Self::Delete(id),
            UiAction::RestoreClip(clip) => Self::RestoreClip(clip),
            UiAction::ClearHistory => Self::ClearHistory,
            UiAction::TogglePause => Self::TogglePause,
            UiAction::RecoverSkipped => Self::RecoverSkipped,
            UiAction::InstallStarterPack(pack) => Self::InstallStarterPack(pack),
            UiAction::DismissNotice => Self::DismissNotice,
            UiAction::DismissHotkeyCoachmark => Self::DismissHotkeyCoachmark,
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

    #[test]
    fn composed_text_is_redacted_from_command_debug() {
        let command = AppCommand::from(UiAction::PasteText("private draft".into()));
        assert!(!format!("{command:?}").contains("private draft"));
    }

    #[test]
    fn restored_clip_is_redacted_from_command_debug() {
        let flavors = vec![vbuff_types::Flavor::inline(
            "text/plain",
            b"private restored value".to_vec(),
        )];
        let clip = vbuff_types::Clip {
            id: vbuff_types::ClipId::new(),
            content_hash: vbuff_core::content_hash_from_flavors(&flavors),
            meta: vbuff_types::ClipMeta::now(vbuff_types::ContentKind::Text, 22, None),
            flavors,
            pinned: false,
            favorite: false,
        };
        let command = AppCommand::from(UiAction::RestoreClip(Box::new(clip)));

        assert!(!format!("{command:?}").contains("private restored value"));
    }
}
