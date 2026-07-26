//! Durable persistence for the update verifier's security state.
//!
//! Without persistence the anti-replay watermark (`highest_accepted_sequence`)
//! and the keyring (including revocations) lived only in memory: every process
//! restart re-opened a downgrade/replay window and forgot rotated or revoked
//! keys. `UpdateVerifier::load`/`store` close that window. Loading fails
//! closed: a corrupt or unreadable state file is an error, never a silent
//! reset to sequence 0.

use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::manifest::{UpdateKeyring, UpdateVerifier};
use crate::{Result, UpdateError};

/// Serializable snapshot of everything the verifier must survive a restart.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifierState {
    pub keyring: UpdateKeyring,
    pub highest_accepted_sequence: u64,
}

impl UpdateVerifier {
    pub fn from_state(state: VerifierState) -> Self {
        Self::new(state.keyring, state.highest_accepted_sequence)
    }

    pub fn state(&self) -> VerifierState {
        VerifierState {
            keyring: self.keyring().clone(),
            highest_accepted_sequence: self.highest_accepted_sequence(),
        }
    }

    /// Load the verifier state from `path`.
    ///
    /// * Missing file: `bootstrap()` builds the initial keyring and the
    ///   watermark starts at 0 (first-run provisioning).
    /// * Corrupt or unreadable file: returns an error. There is deliberately
    ///   no silent fallback to a fresh state, because resetting the watermark
    ///   to 0 would reopen the downgrade/replay window and dropping the
    ///   keyring would forget revocations.
    pub fn load(path: impl AsRef<Path>, bootstrap: impl FnOnce() -> UpdateKeyring) -> Result<Self> {
        let path = path.as_ref();
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::new(bootstrap(), 0));
            }
            Err(error) => return Err(UpdateError::Io(error.to_string())),
        };
        let state: VerifierState = serde_json::from_slice(&bytes)
            .map_err(|error| UpdateError::Serialization(error.to_string()))?;
        Ok(Self::from_state(state))
    }

    /// Persist the full verifier state to `path` as one atomic write
    /// (write-temp-then-rename plus fsync, the same pattern the store uses).
    ///
    /// Callers must invoke this after every successful `verify()`: the
    /// watermark and any keyring changes (rotations, revocations) are only
    /// durable once stored. Skipping it degrades security to the old
    /// in-memory behavior — the next restart reopens the replay window.
    pub fn store(&self, path: impl AsRef<Path>) -> Result<()> {
        let bytes = serde_json::to_vec(&self.state())
            .map_err(|error| UpdateError::Serialization(error.to_string()))?;
        let mut file = atomic_write_file::AtomicWriteFile::open(path)
            .map_err(|error| UpdateError::Io(error.to_string()))?;
        file.write_all(&bytes)
            .map_err(|error| UpdateError::Io(error.to_string()))?;
        file.as_file()
            .sync_all()
            .map_err(|error| UpdateError::Io(error.to_string()))?;
        file.commit()
            .map_err(|error| UpdateError::Io(error.to_string()))?;
        Ok(())
    }
}
