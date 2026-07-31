//! Commands shared by the popup, tray, hotkey, and app wiring.

use vbuff_gui::UiAction;

/// One vocabulary for every user-facing command surface.
///
/// The popup's own vocabulary is [`UiAction`], carried here verbatim rather
/// than mirrored variant by variant: the shell only *adds* entries the GUI
/// cannot produce (a hotkey/tray summon and the tray-only items). Containment
/// is what keeps the redaction contract single-sourced — `Debug` is derived on
/// both types, and the only impls that redact live on the content-carrying
/// newtypes (`vbuff_gui::ClipText`, `vbuff_gui::RestoredClip`), so there is no
/// second impl here to forget when a variant is added.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AppCommand {
    /// Anything the popup itself can ask for.
    Ui(UiAction),
    /// Summon and focus the popup (hotkey, tray, or a duplicate launch).
    Show,
    /// Copy the most recent clip without opening the popup.
    #[cfg(feature = "tray")]
    CopyLatest,
    /// Ask the popup to raise its clear-history confirmation.
    #[cfg(feature = "tray")]
    RequestClearHistory,
    /// Flip launch-at-login from the tray menu.
    #[cfg(feature = "tray")]
    ToggleAutostart,
}

impl From<UiAction> for AppCommand {
    fn from(action: UiAction) -> Self {
        Self::Ui(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gui_actions_reach_the_wiring_unchanged() {
        assert_eq!(
            AppCommand::from(UiAction::ClearHistory),
            AppCommand::Ui(UiAction::ClearHistory)
        );
        assert_eq!(
            AppCommand::from(UiAction::DismissNotice),
            AppCommand::Ui(UiAction::DismissNotice)
        );
        assert_eq!(
            AppCommand::from(UiAction::RecoverSkipped),
            AppCommand::Ui(UiAction::RecoverSkipped)
        );
    }

    #[test]
    fn composed_text_is_redacted_from_command_debug() {
        let command = AppCommand::from(UiAction::PasteText {
            text: vbuff_gui::ClipText::new("private draft"),
            sensitive: true,
        });

        assert!(!format!("{command:?}").contains("private draft"));
        assert!(format!("{command:?}").contains("sensitive: true"));
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
        let command = AppCommand::from(UiAction::RestoreClip(vbuff_gui::RestoredClip::new(
            Box::new(clip),
        )));

        assert!(!format!("{command:?}").contains("private restored value"));
    }

    /// The shell-only variants exist precisely because the GUI cannot produce
    /// them; if one ever becomes expressible as a [`UiAction`] it belongs in
    /// the GUI crate instead of here.
    #[test]
    fn shell_only_commands_stay_outside_the_gui_vocabulary() {
        assert_ne!(AppCommand::Show, AppCommand::from(UiAction::Hide));
        #[cfg(feature = "tray")]
        assert_ne!(
            AppCommand::RequestClearHistory,
            AppCommand::from(UiAction::ClearHistory)
        );
    }
}
