//! Cross-platform paste-keystroke backend built on `enigo`.
//!
//! After the clip is written to the system clipboard and the popup is hidden,
//! the app calls [`PasteBackend::paste`] to synthesize the platform paste combo
//! (Cmd+V on macOS, Ctrl+V elsewhere) into the previously focused application.
//!
//! On macOS this requires the Accessibility permission to be granted to the
//! app, otherwise the keystroke is silently dropped by the OS.

use enigo::{Direction, Enigo, Key, Keyboard, Settings};

use crate::traits::PasteBackend;
use crate::{Modifier, PlatformError, Result, paste_modifier};

/// An `enigo`-backed paste simulator.
pub struct EnigoPaste {
    enigo: Enigo,
}

impl EnigoPaste {
    /// Construct a paste backend.
    pub fn new() -> Result<Self> {
        let enigo =
            Enigo::new(&Settings::default()).map_err(|e| PlatformError::Paste(e.to_string()))?;
        Ok(EnigoPaste { enigo })
    }
}

impl PasteBackend for EnigoPaste {
    fn paste(&mut self) -> Result<()> {
        let modifier_key = match paste_modifier() {
            Modifier::Meta => Key::Meta,
            Modifier::Control => Key::Control,
            Modifier::Alt => Key::Alt,
            Modifier::Shift => Key::Shift,
        };

        self.enigo
            .key(modifier_key, Direction::Press)
            .map_err(|e| PlatformError::Paste(e.to_string()))?;
        let click = self.enigo.key(Key::Unicode('v'), Direction::Click);
        // Always release the modifier even if the 'v' click failed.
        let release = self.enigo.key(modifier_key, Direction::Release);

        click.map_err(|e| PlatformError::Paste(e.to_string()))?;
        release.map_err(|e| PlatformError::Paste(e.to_string()))?;
        Ok(())
    }
}
