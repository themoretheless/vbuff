//! Process hardening and native key/user-presence contracts.

use serde::Serialize;
use zeroize::Zeroize;

use crate::Result;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ProcessHardeningReport {
    pub core_dumps_blocked: bool,
    pub ptrace_blocked: bool,
}

/// Apply portable hardening that is safe before worker threads start.
pub fn harden_current_process() -> ProcessHardeningReport {
    #[cfg(unix)]
    let core_dumps_blocked = rlimit::setrlimit(rlimit::Resource::CORE, 0, 0).is_ok();
    #[cfg(not(unix))]
    let core_dumps_blocked = false;

    #[cfg(target_os = "linux")]
    let ptrace_blocked = prctl::set_dumpable(false).is_ok();
    #[cfg(not(target_os = "linux"))]
    let ptrace_blocked = false;

    ProcessHardeningReport {
        core_dumps_blocked,
        ptrace_blocked,
    }
}

/// Wrapped key bytes are redacted in Debug and zeroized on drop.
pub struct WrappedKey(Vec<u8>);

impl WrappedKey {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub fn expose(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Debug for WrappedKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WrappedKey")
            .field("bytes", &"[redacted]")
            .finish()
    }
}

impl Drop for WrappedKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserPresenceReason {
    UnlockHistory,
    RevealSensitiveClip,
    ExportVault,
}

pub trait HardwareKeyBackend: Send {
    fn is_hardware_bound(&self) -> bool;
    fn wrap_key(&mut self, plaintext_key: &[u8; 32]) -> Result<WrappedKey>;
    fn unwrap_key(&mut self, wrapped: &WrappedKey) -> Result<[u8; 32]>;
}

pub trait UserPresenceBackend: Send {
    fn verify(&mut self, reason: UserPresenceReason) -> Result<bool>;
}

/// Deliberately excludes destructive wipe behavior from duress handling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnlockProfile {
    PrimaryVault,
    IsolatedDecoyVault,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapped_key_debug_is_redacted() {
        let key = WrappedKey::new(vec![1, 2, 3]);
        let debug = format!("{key:?}");
        assert!(debug.contains("redacted"));
        assert!(!debug.contains("1, 2, 3"));
    }
}
