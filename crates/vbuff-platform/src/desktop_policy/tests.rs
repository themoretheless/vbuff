use std::path::PathBuf;

use super::*;
use crate::{KeyCombo, Modifier};

fn combo(key: &str) -> KeyCombo {
    KeyCombo {
        modifiers: vec![Modifier::Control, Modifier::Shift],
        key: key.to_owned(),
    }
}

#[test]
fn conflict_resolution_is_bounded_and_never_claims_registration() {
    let requested = combo("V");
    let occupied = vec![requested.clone(), combo("Space"), combo("C")];
    let resolution = resolve_hotkey_conflict(&requested, &occupied);
    assert!(!resolution.requested_available);
    assert!(!resolution.alternatives.is_empty());
    assert!(resolution.alternatives.len() <= 5);
    assert!(
        resolution
            .alternatives
            .iter()
            .all(|candidate| !occupied.contains(candidate))
    );
}

#[test]
fn resident_modes_always_leave_one_recovery_surface() {
    for mode in [
        ResidentAccessMode::HotkeyAndMenu,
        ResidentAccessMode::HotkeyOnly,
        ResidentAccessMode::MenuOnly,
    ] {
        assert!(mode.hotkey_enabled() || mode.menu_enabled());
    }
}

#[test]
fn layout_accelerator_separates_physical_key_from_label() {
    let accelerator = LayoutAwareAccelerator {
        modifiers: vec![Modifier::Control, Modifier::Shift],
        physical_key: "KeyV".into(),
        display_key: "М".into(),
    };
    assert!(accelerator.validate().is_ok());
    assert_ne!(accelerator.physical_key, accelerator.display_key);
}

#[test]
fn managed_policy_can_disable_portable_and_hotkey_paths() {
    let policy = ManagedInstallPolicy {
        forced_mode: Some(ResidentAccessMode::MenuOnly),
        portable_profile_allowed: false,
        locked_hotkey: Some(combo("B")),
    };
    let effective = policy.apply(
        ResidentAccessMode::HotkeyOnly,
        ProfileLocation::Portable {
            root: PathBuf::from("/media/vbuff"),
        },
        combo("V"),
    );
    assert_eq!(effective.mode, ResidentAccessMode::MenuOnly);
    assert_eq!(effective.profile, ProfileLocation::Standard);
    assert_eq!(effective.hotkey, None);
}

#[test]
fn theme_revision_changes_only_for_native_state_changes() {
    let mut state = NativeThemeState::new(NativeTheme::Light);
    assert!(!state.observe(NativeTheme::Light));
    assert!(state.observe(NativeTheme::HighContrastDark));
    assert_eq!(state.current(), NativeTheme::HighContrastDark);
    assert_eq!(state.revision(), 1);
}
