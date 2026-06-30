//! Cross-platform global-hotkey backend built on `global-hotkey`.
//!
//! `global-hotkey` covers macOS, Windows, and Linux/X11 (not Wayland). Events
//! are delivered through its process-global receiver, which the app polls inside
//! the eframe update loop; see `GlobalHotKeyEvent::receiver()`.

use std::collections::HashMap;

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::GlobalHotKeyManager;

use crate::traits::{HotkeyBackend, KeyCombo, Modifier};
use crate::{PlatformError, Result};

/// A `global-hotkey`-backed hotkey manager.
pub struct GlobalHotkeyBackend {
    manager: GlobalHotKeyManager,
    /// Map our returned id -> the registered HotKey (needed to unregister).
    registered: HashMap<u32, HotKey>,
}

impl GlobalHotkeyBackend {
    /// Construct the platform hotkey manager.
    pub fn new() -> Result<Self> {
        let manager =
            GlobalHotKeyManager::new().map_err(|e| PlatformError::Hotkey(e.to_string()))?;
        Ok(GlobalHotkeyBackend {
            manager,
            registered: HashMap::new(),
        })
    }
}

impl HotkeyBackend for GlobalHotkeyBackend {
    fn register(&mut self, combo: &KeyCombo) -> Result<u32> {
        let hotkey = combo_to_hotkey(combo)?;
        self.manager
            .register(hotkey)
            .map_err(|e| PlatformError::Hotkey(e.to_string()))?;
        let id = hotkey.id();
        self.registered.insert(id, hotkey);
        Ok(id)
    }

    fn unregister(&mut self, id: u32) -> Result<()> {
        if let Some(hotkey) = self.registered.remove(&id) {
            self.manager
                .unregister(hotkey)
                .map_err(|e| PlatformError::Hotkey(e.to_string()))?;
        }
        Ok(())
    }
}

/// Convert our [`KeyCombo`] into a `global-hotkey` [`HotKey`].
pub fn combo_to_hotkey(combo: &KeyCombo) -> Result<HotKey> {
    let mut mods = Modifiers::empty();
    for m in &combo.modifiers {
        mods |= match m {
            Modifier::Control => Modifiers::CONTROL,
            Modifier::Alt => Modifiers::ALT,
            Modifier::Shift => Modifiers::SHIFT,
            Modifier::Meta => Modifiers::META,
        };
    }
    let code = key_to_code(&combo.key)
        .ok_or_else(|| PlatformError::BadCombo(format!("unknown key {:?}", combo.key)))?;
    Ok(HotKey::new(Some(mods), code))
}

/// Map a key name/char to a `global-hotkey` [`Code`].
///
/// Supports the letters A-Z, digits 0-9, and a few named keys. This is enough
/// for the default hotkeys; it can be extended as needed.
fn key_to_code(key: &str) -> Option<Code> {
    let upper = key.to_ascii_uppercase();
    let code = match upper.as_str() {
        "A" => Code::KeyA,
        "B" => Code::KeyB,
        "C" => Code::KeyC,
        "D" => Code::KeyD,
        "E" => Code::KeyE,
        "F" => Code::KeyF,
        "G" => Code::KeyG,
        "H" => Code::KeyH,
        "I" => Code::KeyI,
        "J" => Code::KeyJ,
        "K" => Code::KeyK,
        "L" => Code::KeyL,
        "M" => Code::KeyM,
        "N" => Code::KeyN,
        "O" => Code::KeyO,
        "P" => Code::KeyP,
        "Q" => Code::KeyQ,
        "R" => Code::KeyR,
        "S" => Code::KeyS,
        "T" => Code::KeyT,
        "U" => Code::KeyU,
        "V" => Code::KeyV,
        "W" => Code::KeyW,
        "X" => Code::KeyX,
        "Y" => Code::KeyY,
        "Z" => Code::KeyZ,
        "0" => Code::Digit0,
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        "6" => Code::Digit6,
        "7" => Code::Digit7,
        "8" => Code::Digit8,
        "9" => Code::Digit9,
        "SPACE" => Code::Space,
        "ENTER" | "RETURN" => Code::Enter,
        "TAB" => Code::Tab,
        _ => return None,
    };
    Some(code)
}

/// Parse a human hotkey string like `"Cmd+Shift+V"` / `"Ctrl+Shift+V"`.
///
/// Recognized modifier tokens (case-insensitive): `ctrl`/`control`,
/// `alt`/`option`/`opt`, `shift`, `cmd`/`command`/`super`/`meta`/`win`. The
/// last token is the main key.
pub fn parse_combo(s: &str) -> Result<KeyCombo> {
    let mut modifiers = Vec::new();
    let mut key = None;
    for raw in s.split('+') {
        let token = raw.trim();
        if token.is_empty() {
            continue;
        }
        match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers.push(Modifier::Control),
            "alt" | "option" | "opt" => modifiers.push(Modifier::Alt),
            "shift" => modifiers.push(Modifier::Shift),
            "cmd" | "command" | "super" | "meta" | "win" | "windows" => {
                modifiers.push(Modifier::Meta)
            }
            _ => key = Some(token.to_string()),
        }
    }
    let key = key.ok_or_else(|| PlatformError::BadCombo(format!("no main key in {s:?}")))?;
    Ok(KeyCombo { modifiers, key })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_macos_default() {
        let combo = parse_combo("Cmd+Shift+V").unwrap();
        assert_eq!(combo.key, "V");
        assert!(combo.modifiers.contains(&Modifier::Meta));
        assert!(combo.modifiers.contains(&Modifier::Shift));
    }

    #[test]
    fn parses_other_default() {
        let combo = parse_combo("Ctrl+Shift+V").unwrap();
        assert_eq!(combo.key, "V");
        assert!(combo.modifiers.contains(&Modifier::Control));
    }

    #[test]
    fn maps_known_keys() {
        assert!(key_to_code("v").is_some());
        assert!(key_to_code("Space").is_some());
        assert!(key_to_code("?").is_none());
    }

    #[test]
    fn combo_without_key_is_error() {
        assert!(parse_combo("Ctrl+Shift").is_err());
    }
}
