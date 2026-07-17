//! One behavioral keymap with native modifier rendering on every target.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum KeymapTarget {
    MacOs,
    Windows,
    LinuxX11,
    LinuxWayland,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CanonicalAction {
    Show,
    Next,
    Previous,
    Paste,
    Dismiss,
    Pin,
    Delete,
    QuickPick,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyBinding {
    pub action: CanonicalAction,
    pub physical_key: &'static str,
    pub modifiers: &'static [&'static str],
    pub display: &'static str,
}

pub fn canonical_keymap(target: KeymapTarget) -> [KeyBinding; 8] {
    let (show_modifiers, show_display) = match target {
        KeymapTarget::MacOs => (&["meta", "shift"][..], "Cmd+Shift+V"),
        KeymapTarget::Windows | KeymapTarget::LinuxX11 | KeymapTarget::LinuxWayland => {
            (&["ctrl", "shift"][..], "Ctrl+Shift+V")
        }
    };
    [
        KeyBinding {
            action: CanonicalAction::Show,
            physical_key: "key_v",
            modifiers: show_modifiers,
            display: show_display,
        },
        KeyBinding {
            action: CanonicalAction::Next,
            physical_key: "arrow_down",
            modifiers: &[],
            display: "Down",
        },
        KeyBinding {
            action: CanonicalAction::Previous,
            physical_key: "arrow_up",
            modifiers: &[],
            display: "Up",
        },
        KeyBinding {
            action: CanonicalAction::Paste,
            physical_key: "enter",
            modifiers: &[],
            display: "Enter",
        },
        KeyBinding {
            action: CanonicalAction::Dismiss,
            physical_key: "escape",
            modifiers: &[],
            display: "Esc",
        },
        KeyBinding {
            action: CanonicalAction::Pin,
            physical_key: "key_p",
            modifiers: &[],
            display: "P",
        },
        KeyBinding {
            action: CanonicalAction::Delete,
            physical_key: "delete",
            modifiers: &[],
            display: "Delete",
        },
        KeyBinding {
            action: CanonicalAction::QuickPick,
            physical_key: "digit_1_9",
            modifiers: if target == KeymapTarget::MacOs {
                &["meta"]
            } else {
                &["ctrl"]
            },
            display: if target == KeymapTarget::MacOs {
                "Cmd+1..9"
            } else {
                "Ctrl+1..9"
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn every_target_exposes_the_identical_action_set_once() {
        let expected = canonical_keymap(KeymapTarget::MacOs)
            .map(|binding| binding.action)
            .into_iter()
            .collect::<BTreeSet<_>>();
        for target in [
            KeymapTarget::MacOs,
            KeymapTarget::Windows,
            KeymapTarget::LinuxX11,
            KeymapTarget::LinuxWayland,
        ] {
            let bindings = canonical_keymap(target);
            let actions = bindings
                .map(|binding| binding.action)
                .into_iter()
                .collect::<BTreeSet<_>>();
            assert_eq!(actions, expected);
            assert_eq!(actions.len(), bindings.len());
        }
    }
}
