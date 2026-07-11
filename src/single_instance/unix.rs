//! Unix-domain-socket single-instance transport.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use super::*;
use vbuff_types::ServerResponse;

const IO_TIMEOUT: Duration = Duration::from_secs(2);

pub(super) struct Guard {
    path: PathBuf,
    _owner_lock: fs::File,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        let _ = UnixStream::connect(&self.path);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = fs::remove_file(&self.path);
    }
}

pub(super) fn acquire(intent: ClientIntent) -> io::Result<LaunchOutcome> {
    acquire_at(endpoint_path()?, intent)
}

fn acquire_at(path: PathBuf, intent: ClientIntent) -> io::Result<LaunchOutcome> {
    let Some(owner_lock) = try_owner_lock(&path)? else {
        forward_with_retry(&path, intent)?;
        return Ok(LaunchOutcome::Forwarded);
    };

    match bind(&path) {
        Ok(listener) => Ok(primary(listener, path, owner_lock)),
        Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
            // This supports a resident process from before the owner lock
            // existed. New processes cannot race here because only one of
            // them can hold the recovery lock.
            if forward(&path, intent).is_ok() {
                return Ok(LaunchOutcome::Forwarded);
            }

            // A crashed process leaves the filesystem socket behind.
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            bind(&path).map(|listener| primary(listener, path, owner_lock))
        }
        Err(error) => Err(error),
    }
}

fn endpoint_path() -> io::Result<PathBuf> {
    #[cfg(target_os = "linux")]
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .or_else(dirs::config_dir)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no runtime directory"))?;
    #[cfg(target_os = "macos")]
    let base = std::env::temp_dir();
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let base = dirs::config_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no runtime directory"))?;

    let dir = base.join("vbuff");
    fs::create_dir_all(&dir)?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    Ok(dir.join("control.sock"))
}

fn bind(path: &Path) -> io::Result<UnixListener> {
    let listener = UnixListener::bind(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

fn primary(listener: UnixListener, path: PathBuf, owner_lock: fs::File) -> LaunchOutcome {
    let (sender, intents) = mpsc::channel();
    let shutdown = Arc::new(AtomicBool::new(false));
    let thread_shutdown = Arc::clone(&shutdown);
    let thread = std::thread::spawn(move || server_loop(listener, sender, thread_shutdown));

    LaunchOutcome::Primary {
        guard: InstanceGuard {
            _inner: Box::new(Guard {
                path,
                _owner_lock: owner_lock,
                shutdown,
                thread: Some(thread),
            }),
        },
        intents,
    }
}

fn server_loop(listener: UnixListener, sender: Sender<ClientIntent>, shutdown: Arc<AtomicBool>) {
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
        if let Err(error) = handle_stream(&mut stream, &sender) {
            tracing::warn!("single-instance request ignored: {error}");
        }
    }
}

fn handle_stream(stream: &mut UnixStream, sender: &Sender<ClientIntent>) -> io::Result<()> {
    let intent: ClientIntent = read_frame(stream)?;
    let response = match intent {
        ClientIntent::Ping => ServerResponse::Pong,
        ClientIntent::ShowPopup => match sender.send(intent) {
            Ok(()) => ServerResponse::Ack,
            Err(_) => ServerResponse::Rejected {
                message: "resident event loop unavailable".into(),
            },
        },
    };
    write_frame(stream, &response)
}

fn forward(path: &Path, intent: ClientIntent) -> io::Result<()> {
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    write_frame(&mut stream, &intent)?;
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
    use std::sync::{Barrier, Condvar, Mutex};
    use std::time::Duration;

    use super::*;

    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

    fn test_path() -> PathBuf {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("vbuff-instance-{}-{id}.sock", std::process::id()))
    }

    fn cleanup(path: &Path) {
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(owner_lock_path(path));
    }

    #[test]
    fn second_instance_forwards_show_to_primary() {
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
    fn stale_socket_is_removed_and_rebound_once() {
        let path = test_path();
        let stale = UnixListener::bind(&path).unwrap();
        drop(stale);
        assert!(path.exists());

        let outcome = acquire_at(path.clone(), ClientIntent::ShowPopup).unwrap();
        let LaunchOutcome::Primary { guard, .. } = outcome else {
            panic!("stale endpoint should be recovered");
        };
        drop(guard);
        assert!(!path.exists());
        cleanup(&path);
    }

    #[test]
    fn concurrent_stale_recovery_keeps_exactly_one_primary() {
        let path = test_path();
        let stale = UnixListener::bind(&path).unwrap();
        drop(stale);

        let start = Arc::new(Barrier::new(3));
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let (roles_tx, roles_rx) = mpsc::channel();
        let mut threads = Vec::new();

        for _ in 0..2 {
            let path = path.clone();
            let start = Arc::clone(&start);
            let release = Arc::clone(&release);
            let roles_tx = roles_tx.clone();
            threads.push(std::thread::spawn(move || {
                start.wait();
                match acquire_at(path, ClientIntent::ShowPopup) {
                    Ok(LaunchOutcome::Primary { guard, intents }) => {
                        roles_tx.send("primary".to_owned()).unwrap();
                        let (lock, wake) = &*release;
                        let mut released = lock.lock().unwrap();
                        while !*released {
                            released = wake.wait(released).unwrap();
                        }
                        drop(intents);
                        drop(guard);
                    }
                    Ok(LaunchOutcome::Forwarded) => {
                        roles_tx.send("forwarded".to_owned()).unwrap();
                    }
                    Err(error) => {
                        roles_tx.send(format!("error: {error}")).unwrap();
                    }
                }
            }));
        }
        drop(roles_tx);
        start.wait();

        let first = roles_rx.recv_timeout(Duration::from_secs(3));
        let second = roles_rx.recv_timeout(Duration::from_secs(3));
        let (lock, wake) = &*release;
        *lock.lock().unwrap() = true;
        wake.notify_all();
        for thread in threads {
            thread.join().unwrap();
        }

        let mut roles = vec![first.unwrap(), second.unwrap()];
        roles.sort_unstable();
        assert_eq!(roles, ["forwarded", "primary"]);
        cleanup(&path);
    }

    #[test]
    fn ping_proves_liveness_without_reaching_the_app_loop() {
        let path = test_path();
        let primary = acquire_at(path.clone(), ClientIntent::ShowPopup).unwrap();
        let LaunchOutcome::Primary { guard, intents } = primary else {
            panic!("first instance must become primary");
        };

        forward(&path, ClientIntent::Ping).unwrap();
        assert!(intents.try_recv().is_err());
        drop(guard);
        cleanup(&path);
    }
}
