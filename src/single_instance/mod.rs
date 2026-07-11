//! Single-instance guard and minimal control-intent handoff.

use std::io::{self, Read, Write};
use std::sync::mpsc::Receiver;

use serde::Serialize;
use serde::de::DeserializeOwned;
use vbuff_types::ClientIntent;

const MAX_FRAME_BYTES: usize = 64 * 1024;

pub(crate) enum LaunchOutcome {
    Primary {
        guard: InstanceGuard,
        intents: Receiver<ClientIntent>,
    },
    Forwarded,
}

/// Keeps the endpoint and its listener thread alive for the resident process.
pub(crate) struct InstanceGuard {
    _inner: Box<dyn Send>,
}

pub(crate) fn acquire_or_forward(intent: ClientIntent) -> io::Result<LaunchOutcome> {
    platform::acquire(intent)
}

fn write_frame(writer: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    let payload = serde_json::to_vec(value).map_err(invalid_data)?;
    if payload.len() > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "control frame is too large",
        ));
    }
    writer.write_all(&(payload.len() as u32).to_be_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()
}

fn read_frame<T: DeserializeOwned>(reader: &mut impl Read) -> io::Result<T> {
    let mut length = [0u8; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "control frame is too large",
        ));
    }
    let mut payload = vec![0; length];
    reader.read_exact(&mut payload)?;
    serde_json::from_slice(&payload).map_err(invalid_data)
}

fn invalid_data(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

#[cfg(any(unix, windows))]
fn owner_lock_path(endpoint: &std::path::Path) -> std::path::PathBuf {
    let mut path = endpoint.as_os_str().to_os_string();
    path.push(".lock");
    path.into()
}

#[cfg(any(unix, windows))]
fn try_owner_lock(endpoint: &std::path::Path) -> io::Result<Option<std::fs::File>> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(owner_lock_path(endpoint))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(error)) => Err(error),
    }
}

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as platform;

#[cfg(any(windows, test))]
mod windows_fallback;

#[cfg(windows)]
use windows_fallback as platform;

#[cfg(not(any(unix, windows)))]
mod platform {
    use super::*;

    pub(super) struct Guard;

    pub(super) fn acquire(_intent: ClientIntent) -> io::Result<LaunchOutcome> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "single-instance guard is unsupported on this platform",
        ))
    }
}
