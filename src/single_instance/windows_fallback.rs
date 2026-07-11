//! Authenticated loopback fallback used until the Windows named pipe lands.

use std::fs::{self, File, OpenOptions};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use vbuff_types::ServerResponse;

use super::*;

const IO_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Serialize, Deserialize)]
struct Endpoint {
    port: u16,
    token: String,
}

#[derive(Serialize, Deserialize)]
struct AuthenticatedIntent {
    token: String,
    intent: ClientIntent,
}

pub(super) struct Guard {
    path: PathBuf,
    address: SocketAddr,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    endpoint: Option<File>,
    _owner_lock: File,
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        let _ = TcpStream::connect(self.address);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        self.endpoint.take();
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(windows)]
pub(super) fn acquire(intent: ClientIntent) -> io::Result<LaunchOutcome> {
    acquire_at(endpoint_path()?, intent)
}

fn acquire_at(path: PathBuf, intent: ClientIntent) -> io::Result<LaunchOutcome> {
    let Some(owner_lock) = try_owner_lock(&path)? else {
        forward_with_retry(&path, intent)?;
        return Ok(LaunchOutcome::Forwarded);
    };

    if path.exists() {
        // Compatibility with a resident process created before owner-lock
        // coordination was added.
        if forward(&path, intent).is_ok() {
            return Ok(LaunchOutcome::Forwarded);
        }
        fs::remove_file(&path)?;
    }
    create_primary(&path, owner_lock)
}

#[cfg(windows)]
fn endpoint_path() -> io::Result<PathBuf> {
    let dir = dirs::data_local_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no local data directory"))?
        .join("vbuff");
    fs::create_dir_all(&dir)?;
    Ok(dir.join("control.json"))
}

fn create_primary(path: &Path, owner_lock: File) -> io::Result<LaunchOutcome> {
    let mut endpoint_file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    let listener = match TcpListener::bind(("127.0.0.1", 0)) {
        Ok(listener) => listener,
        Err(error) => {
            drop(endpoint_file);
            let _ = fs::remove_file(path);
            return Err(error);
        }
    };
    let address = listener.local_addr()?;
    let token = format!(
        "{:x}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        std::process::id()
    );
    let endpoint = Endpoint {
        port: address.port(),
        token: token.clone(),
    };
    serde_json::to_writer(&mut endpoint_file, &endpoint).map_err(invalid_data)?;
    endpoint_file.flush()?;
    endpoint_file.sync_all()?;

    let (sender, intents) = mpsc::channel();
    let shutdown = Arc::new(AtomicBool::new(false));
    let thread_shutdown = Arc::clone(&shutdown);
    let thread = std::thread::spawn(move || server_loop(listener, sender, thread_shutdown, token));

    Ok(LaunchOutcome::Primary {
        guard: InstanceGuard {
            _inner: Box::new(Guard {
                path: path.to_owned(),
                address,
                shutdown,
                thread: Some(thread),
                endpoint: Some(endpoint_file),
                _owner_lock: owner_lock,
            }),
        },
        intents,
    })
}

fn server_loop(
    listener: TcpListener,
    sender: Sender<ClientIntent>,
    shutdown: Arc<AtomicBool>,
    token: String,
) {
    while !shutdown.load(Ordering::Acquire) {
        let (mut stream, _) = match listener.accept() {
            Ok(connection) => connection,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                tracing::warn!("single-instance listener stopped: {error}");
                break;
            }
        };
        if shutdown.load(Ordering::Acquire) {
            break;
        }
        let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
        let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
        if let Err(error) = handle_stream(&mut stream, &sender, &token) {
            tracing::warn!("single-instance request ignored: {error}");
        }
    }
}

fn handle_stream(
    stream: &mut TcpStream,
    sender: &Sender<ClientIntent>,
    token: &str,
) -> io::Result<()> {
    let request: AuthenticatedIntent = read_frame(stream)?;
    let response = if request.token != token {
        ServerResponse::Rejected {
            message: "invalid control token".into(),
        }
    } else {
        match request.intent {
            ClientIntent::Ping => ServerResponse::Pong,
            ClientIntent::ShowPopup => match sender.send(request.intent) {
                Ok(()) => ServerResponse::Ack,
                Err(_) => ServerResponse::Rejected {
                    message: "resident event loop unavailable".into(),
                },
            },
        }
    };
    write_frame(stream, &response)
}

fn forward(path: &Path, intent: ClientIntent) -> io::Result<()> {
    let file = File::open(path)?;
    let endpoint: Endpoint = serde_json::from_reader(file).map_err(invalid_data)?;
    let mut stream = TcpStream::connect(("127.0.0.1", endpoint.port))?;
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    write_frame(
        &mut stream,
        &AuthenticatedIntent {
            token: endpoint.token,
            intent,
        },
    )?;
    match read_frame::<ServerResponse>(&mut stream)? {
        ServerResponse::Ack if intent == ClientIntent::ShowPopup => Ok(()),
        ServerResponse::Pong if intent == ClientIntent::Ping => Ok(()),
        ServerResponse::Rejected { message } => {
            Err(io::Error::new(io::ErrorKind::ConnectionRefused, message))
        }
        response => Err(invalid_data(format!(
            "unexpected control response: {response:?}"
        ))),
    }
}

fn forward_with_retry(path: &Path, intent: ClientIntent) -> io::Result<()> {
    let mut last_error = None;
    for attempt in 0..10 {
        match forward(path, intent) {
            Ok(()) => return Ok(()),
            Err(error) => last_error = Some(error),
        }
        if attempt < 9 {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    Err(last_error.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::ConnectionRefused,
            "resident endpoint unavailable",
        )
    }))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

    fn test_path() -> PathBuf {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "vbuff-windows-instance-{}-{id}.json",
            std::process::id()
        ))
    }

    fn cleanup(path: &Path) {
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(owner_lock_path(path));
    }

    #[test]
    fn loopback_fallback_forwards_show_to_primary() {
        let path = test_path();
        let primary = acquire_at(path.clone(), ClientIntent::ShowPopup).unwrap();
        let LaunchOutcome::Primary { guard, intents } = primary else {
            panic!("first instance must become primary");
        };

        assert!(matches!(
            acquire_at(path.clone(), ClientIntent::ShowPopup).unwrap(),
            LaunchOutcome::Forwarded
        ));
        assert_eq!(
            intents.recv_timeout(Duration::from_secs(1)).unwrap(),
            ClientIntent::ShowPopup
        );

        drop(guard);
        assert!(!path.exists());
        cleanup(&path);
    }

    #[test]
    fn loopback_fallback_replaces_stale_metadata() {
        let path = test_path();
        fs::write(&path, b"stale endpoint").unwrap();

        let outcome = acquire_at(path.clone(), ClientIntent::ShowPopup).unwrap();
        let LaunchOutcome::Primary { guard, .. } = outcome else {
            panic!("stale endpoint should be recovered");
        };
        drop(guard);
        assert!(!path.exists());
        cleanup(&path);
    }
}
