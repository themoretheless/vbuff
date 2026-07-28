use crate::{KeyCombo, Modifier};

const MAX_HOTKEY_ALTERNATIVES: usize = 5;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HotkeyResolution {
    pub requested_available: bool,
    pub alternatives: Vec<KeyCombo>,
}

/// Produce a small deterministic set of alternatives after native
/// registration reports a conflict. This function never claims a candidate is
/// available until the native backend successfully registers it.
pub fn resolve_hotkey_conflict(requested: &KeyCombo, unavailable: &[KeyCombo]) -> HotkeyResolution {
    if !unavailable.contains(requested) {
        return HotkeyResolution {
            requested_available: true,
            alternatives: Vec::new(),
        };
    }

    let mut alternatives = Vec::new();
    for key in ["Space", "C", "B", "H", "V"] {
        let candidate = KeyCombo {
            modifiers: requested.modifiers.clone(),
            key: key.to_owned(),
        };
        push_candidate(&mut alternatives, candidate, requested, unavailable);
    }

    let mut modifiers = requested.modifiers.clone();
    if !modifiers.contains(&Modifier::Alt) {
        modifiers.push(Modifier::Alt);
    }
    push_candidate(
        &mut alternatives,
        KeyCombo {
            modifiers,
            key: requested.key.clone(),
        },
        requested,
        unavailable,
    );

    HotkeyResolution {
        requested_available: false,
        alternatives,
    }
}

fn push_candidate(
    alternatives: &mut Vec<KeyCombo>,
    candidate: KeyCombo,
    requested: &KeyCombo,
    unavailable: &[KeyCombo],
) {
    if alternatives.len() < MAX_HOTKEY_ALTERNATIVES
        && candidate != *requested
        && !unavailable.contains(&candidate)
        && !alternatives.contains(&candidate)
    {
        alternatives.push(candidate);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutAwareAccelerator {
    pub modifiers: Vec<Modifier>,
    /// Stable hardware-oriented key identifier, such as `KeyV`.
    pub physical_key: String,
    /// Current layout label shown to the user, such as `V` or `М`.
    pub display_key: String,
}

impl LayoutAwareAccelerator {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.modifiers.is_empty() || self.modifiers.len() > 4 {
            return Err("invalid_modifiers");
        }
        if self.physical_key.is_empty()
            || self.physical_key.len() > 32
            || !self
                .physical_key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric())
        {
            return Err("invalid_physical_key");
        }
        if self.display_key.trim().is_empty()
            || self.display_key.len() > 16
            || self.display_key.chars().any(char::is_control)
        {
            return Err("invalid_display_key");
        }
        Ok(())
    }
}
