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

    pub(super) fn query_history(
        _request: HistoryRequest,
    ) -> io::Result<vbuff_types::ServerResponse> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "history endpoint unsupported",
        ))
    }

    pub(super) fn acquire(_intent: ClientIntent) -> io::Result<LaunchOutcome> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "single-instance guard is unsupported on this platform",
        ))
    }
}

/// Queries are served by the resident store owner, never by a second database writer.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum HistoryRequest {
    Ask { query: String, limit: usize },
    Doctor,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
enum ControlRequest {
    Intent(ClientIntent),
    History(HistoryRequest),
}

type HistoryHandler = dyn Fn(HistoryRequest) -> anyhow::Result<String> + Send + Sync;
static HISTORY_HANDLER: std::sync::OnceLock<std::sync::Arc<HistoryHandler>> =
    std::sync::OnceLock::new();

pub(crate) fn install_history_handler(
    handler: impl Fn(HistoryRequest) -> anyhow::Result<String> + Send + Sync + 'static,
) {
    let _ = HISTORY_HANDLER.set(std::sync::Arc::new(handler));
}

fn history_response(request: HistoryRequest) -> vbuff_types::ServerResponse {
    if let HistoryRequest::Ask { query, limit } = &request
        && (query.trim().is_empty() || query.len() > 4096 || !(1..=512).contains(limit))
    {
        return vbuff_types::ServerResponse::Rejected {
            message: "invalid history query".into(),
        };
    }
    let Some(handler) = HISTORY_HANDLER.get() else {
        return vbuff_types::ServerResponse::Rejected {
            message: "history is still opening".into(),
        };
    };
    match handler(request) {
        Ok(json) if json.len() <= MAX_FRAME_BYTES / 2 => {
            vbuff_types::ServerResponse::HistoryResult { json }
        }
        Ok(_) => vbuff_types::ServerResponse::Rejected {
            message: "result is too large; reduce --limit".into(),
        },
        Err(_) => vbuff_types::ServerResponse::Rejected {
            message: "history query failed".into(),
        },
    }
}

pub(crate) fn request_history(request: HistoryRequest) -> anyhow::Result<Option<String>> {
    match platform::query_history(request) {
        Ok(vbuff_types::ServerResponse::HistoryResult { json }) => Ok(Some(json)),
        Ok(vbuff_types::ServerResponse::Rejected { message }) => anyhow::bail!("{message}"),
        Ok(_) => anyhow::bail!("unexpected history response"),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
            ) =>
        {
            Ok(None)
        }
        Err(error) => Err(error.into()),
    }
}
