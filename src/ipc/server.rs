//! Unix-socket listener and event broadcasting for the daemon.
//!
//! Binds [`Paths::socket`], accepts client connections on a background thread,
//! and lets the watcher broadcast [`ServerEvent`] frames to every connected client.
//!
//! Each client gets a dedicated writer thread that owns the stream. The watcher
//! never touches streams directly — it sends pre-serialized frames through
//! per-client channels, so `broadcast` is non-blocking by construction.

use std::io::Write;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock, mpsc};
use std::time::Duration;
use std::{io, thread};

use super::protocol::{
    ClientOp, EventKind, IPC_PROTOCOL_VERSION, RepoPhase, RepoStateSnapshot, ResponseBody,
    ServerEvent, ServerResponse, decode_client_request, encode_server_event,
    encode_server_response, read_frame, write_frame,
};

impl EventBus {
    /// Bind the Unix socket and spawn the accept thread.
    ///
    /// Handles both edge cases from the issue:
    /// - stale socket from a crash: unlinked only when connect fails with
    ///   `ConnectionRefused` (nothing listening)
    /// - another live daemon: connect succeeds, returns `AddrInUse`
    pub fn bind(
        socket_path: &Path,
        shutdown: Arc<AtomicBool>,
        repo_id: String,
    ) -> io::Result<Self> {
        prepare_socket_for_bind(socket_path)?;

        let listener = UnixListener::bind(socket_path)?;
        restrict_socket_permissions(socket_path)?;
        listener.set_nonblocking(true)?;

        let (tx, rx) = mpsc::channel();

        let current_state = Arc::new(RwLock::new(RepoStateSnapshot {
            phase: RepoPhase::Watching,
            diff_hash: None,
            error: None,
        }));

        let accept_stop = Arc::new(AtomicBool::new(false));
        let accept_state = Arc::clone(&current_state);
        let accept_stop_thread = Arc::clone(&accept_stop);
        let shutdown_thread = Arc::clone(&shutdown);
        let accept_repo_id = repo_id.clone();

        let accept_thread = thread::Builder::new()
            .name("sotto-ipc-accept".into())
            .spawn(move || {
                accept_loop(
                    listener,
                    tx,
                    shutdown_thread,
                    accept_stop_thread,
                    accept_state,
                    accept_repo_id,
                );
            })?;

        Ok(Self {
            clients: Vec::new(),
            incoming: rx,
            socket_path: socket_path.to_path_buf(),
            repo_id,
            current_state,
            accept_stop,
            accept_thread: Some(accept_thread),
        })
    }

    /// Push a phase-change event to every connected client.
    ///
    /// Sends the serialized frame through each client's bounded queue (`try_send`).
    /// Full or disconnected queues drop that client so slow readers cannot grow
    /// memory without bound.
    pub fn broadcast(
        &mut self,
        phase: RepoPhase,
        diff_hash: Option<String>,
        error: Option<String>,
    ) {
        while let Ok(stream) = self.incoming.try_recv() {
            if let Some(handle) = spawn_client_writer(stream) {
                self.clients.push(handle);
            } else {
                eprintln!("sotto: ipc: failed to spawn client writer thread");
            }
        }

        let snapshot = RepoStateSnapshot {
            phase: phase.clone(),
            diff_hash: diff_hash.clone(),
            error: error.clone(),
        };
        if let Ok(mut state) = self.current_state.write() {
            *state = snapshot;
        }

        if self.clients.is_empty() {
            return;
        }

        let Some(frame) = self.build_frame(phase, diff_hash, error) else {
            return;
        };

        self.clients
            .retain(|client| match client.tx.try_send(frame.clone()) {
                Ok(()) => true,
                Err(mpsc::TrySendError::Full(_)) | Err(mpsc::TrySendError::Disconnected(_)) => {
                    false
                }
            });
    }

    fn build_frame(
        &self,
        phase: RepoPhase,
        diff_hash: Option<String>,
        error: Option<String>,
    ) -> Option<Vec<u8>> {
        let event = ServerEvent {
            v: IPC_PROTOCOL_VERSION,
            repo_id: self.repo_id.clone(),
            event: EventKind::RepoState {
                state: RepoStateSnapshot {
                    phase,
                    diff_hash,
                    error,
                },
            },
        };

        let payload = encode_server_event(&event).ok()?;
        let mut frame = Vec::new();
        write_frame(&mut frame, &payload).ok()?;
        Some(frame)
    }
}

impl Drop for EventBus {
    fn drop(&mut self) {
        self.accept_stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.accept_thread.take() {
            let _ = h.join();
        }
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

fn restrict_socket_permissions(socket_path: &Path) -> io::Result<()> {
    let mut perms = std::fs::metadata(socket_path)?.permissions();
    perms.set_mode(SOCKET_MODE);
    std::fs::set_permissions(socket_path, perms)
}

const SOCKET_MODE: u32 = 0o600;

/// If `socket_path` exists: must be a socket; connect determines stale vs live.
fn prepare_socket_for_bind(socket_path: &Path) -> io::Result<()> {
    let meta = match socket_path.metadata() {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
        Ok(m) => m,
    };

    if !meta.file_type().is_socket() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} exists and is not a Unix socket", socket_path.display()),
        ));
    }

    match UnixStream::connect(socket_path) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            format!(
                "another sotto daemon is already listening on {}",
                socket_path.display()
            ),
        )),
        Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
            std::fs::remove_file(socket_path)?;
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Spawn a writer thread that owns the stream and drains frames from the channel.
/// Returns a handle the bus holds onto for sending.
fn spawn_client_writer(mut stream: UnixStream) -> Option<ClientHandle> {
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(CLIENT_FRAME_QUEUE_CAP);

    match thread::Builder::new()
        .name("sotto-ipc-client".into())
        .spawn(move || {
            for frame in rx {
                if stream.write_all(&frame).is_err() {
                    break;
                }
            }
        }) {
        Ok(_) => Some(ClientHandle { tx }),
        Err(_) => None,
    }
}

const CLIENT_FRAME_QUEUE_CAP: usize = 64;

fn accept_loop(
    listener: UnixListener,
    tx: mpsc::Sender<UnixStream>,
    shutdown: Arc<AtomicBool>,
    accept_stop: Arc<AtomicBool>,
    state: Arc<RwLock<RepoStateSnapshot>>,
    repo_id: String,
) {
    while !shutdown.load(Ordering::Relaxed) && !accept_stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                if !try_handle_request(&stream, &state, &repo_id) && tx.send(stream).is_err() {
                    break;
                }
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => {
                eprintln!("sotto: ipc accept error: {e}");
                break;
            }
        }
    }
}

/// Peek at a newly accepted connection. If the client sent a request (GetState,
/// Hello), respond immediately and return `true` (stream consumed). If there's
/// no data within the peek window, return `false` so the stream is passed to
/// the broadcast subscriber list.
fn try_handle_request(
    stream: &UnixStream,
    state: &Arc<RwLock<RepoStateSnapshot>>,
    repo_id: &str,
) -> bool {
    let _ = stream.set_read_timeout(Some(REQUEST_PEEK_TIMEOUT));
    let _ = stream.set_write_timeout(Some(REQUEST_PEEK_TIMEOUT));

    let mut reader = stream;
    let frame = match read_frame(&mut reader) {
        Ok(f) => f,
        Err(_) => return false,
    };

    let req = match decode_client_request(&frame) {
        Ok(r) => r,
        Err(_) => return false,
    };

    let body = match req.op {
        ClientOp::Hello => ResponseBody::HelloAck {
            server_version: IPC_PROTOCOL_VERSION,
        },
        ClientOp::GetState => {
            let snapshot = state
                .read()
                .map(|s| s.clone())
                .unwrap_or(RepoStateSnapshot {
                    phase: RepoPhase::Error,
                    diff_hash: None,
                    error: Some("state lock poisoned".into()),
                });
            ResponseBody::State { state: snapshot }
        }
    };

    let resp = ServerResponse {
        v: IPC_PROTOCOL_VERSION,
        repo_id: repo_id.into(),
        request_id: req.request_id,
        body,
    };

    let mut writer = stream;
    let _ = encode_server_response(&resp)
        .ok()
        .and_then(|payload| write_frame(&mut writer, &payload).ok());

    true
}

const REQUEST_PEEK_TIMEOUT: Duration = Duration::from_millis(50);

/// Handle held by the watcher to push events to connected IPC clients.
pub struct EventBus {
    clients: Vec<ClientHandle>,
    incoming: mpsc::Receiver<UnixStream>,
    socket_path: PathBuf,
    repo_id: String,
    current_state: Arc<RwLock<RepoStateSnapshot>>,
    accept_stop: Arc<AtomicBool>,
    accept_thread: Option<thread::JoinHandle<()>>,
}

/// One per connected client. Dropping it closes the channel, which causes
/// the writer thread to exit and close the stream.
struct ClientHandle {
    tx: mpsc::SyncSender<Vec<u8>>,
}

#[cfg(all(unix, test))]
mod tests {
    use super::*;
    use std::time::Duration;

    use crate::ipc::client::query_state;
    use crate::ipc::protocol::{
        EventKind, IPC_PROTOCOL_VERSION, RepoPhase, decode_server_event, read_frame,
    };

    #[test]
    fn prepare_rejects_non_socket_path() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("not-a-socket");
        std::fs::write(&p, b"x").unwrap();
        assert_eq!(
            prepare_socket_for_bind(&p).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn prepare_removes_stale_socket() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("stale.sock");
        let listener = UnixListener::bind(&p).unwrap();
        drop(listener);
        prepare_socket_for_bind(&p).unwrap();
        assert!(UnixListener::bind(&p).is_ok());
    }

    #[test]
    fn second_bind_while_first_listens_is_addr_in_use() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("live.sock");
        let shutdown = Arc::new(AtomicBool::new(false));
        let _first = EventBus::bind(&p, Arc::clone(&shutdown), "a".into()).unwrap();
        let err = match EventBus::bind(&p, Arc::new(AtomicBool::new(false)), "b".into()) {
            Err(e) => e,
            Ok(_) => panic!("expected second bind to fail"),
        };
        assert_eq!(err.kind(), io::ErrorKind::AddrInUse);
    }

    #[test]
    fn broadcast_one_decodable_frame() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("bus.sock");
        let shutdown = Arc::new(AtomicBool::new(false));
        let mut bus = EventBus::bind(&sock, Arc::clone(&shutdown), "repo_hash".into()).unwrap();

        let (connected_send, connected_recv) = mpsc::channel::<()>();
        let path = sock.clone();
        let reader = thread::spawn(move || {
            let mut s = UnixStream::connect(path).unwrap();
            let _ = connected_send.send(());
            let buf = read_frame(&mut s).unwrap();
            decode_server_event(&buf).unwrap()
        });

        connected_recv.recv().unwrap();
        // Non-blocking accept may complete connect() before our `tx.send`; give the accept
        // thread time to enqueue the `UnixStream` before we broadcast.
        thread::sleep(Duration::from_millis(250));

        bus.broadcast(RepoPhase::Ready, Some("abc123".into()), None);

        let ev = reader.join().unwrap();
        assert_eq!(ev.v, IPC_PROTOCOL_VERSION);
        assert_eq!(ev.repo_id, "repo_hash");
        let EventKind::RepoState { state } = &ev.event;
        assert_eq!(state.phase, RepoPhase::Ready);
        assert_eq!(state.diff_hash.as_deref(), Some("abc123"));
    }

    #[test]
    fn query_state_returns_current_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("query.sock");
        let shutdown = Arc::new(AtomicBool::new(false));
        let mut bus = EventBus::bind(&sock, Arc::clone(&shutdown), "repo_xyz".into()).unwrap();

        bus.broadcast(RepoPhase::Ready, Some("hash999".into()), None);

        // Give the accept loop time to be ready
        thread::sleep(Duration::from_millis(50));

        let state = query_state(&sock, "repo_xyz").expect("query_state should succeed");
        assert_eq!(state.phase, RepoPhase::Ready);
        assert_eq!(state.diff_hash.as_deref(), Some("hash999"));
    }

    #[test]
    fn query_state_returns_none_when_no_socket() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("missing.sock");
        assert!(query_state(&sock, "x").is_none());
    }

    #[test]
    fn query_state_returns_non_ready_phases() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("generating.sock");
        let shutdown = Arc::new(AtomicBool::new(false));
        let mut bus = EventBus::bind(&sock, Arc::clone(&shutdown), "repo".into()).unwrap();

        bus.broadcast(RepoPhase::Generating, None, None);
        thread::sleep(Duration::from_millis(50));

        let state = query_state(&sock, "repo").expect("should return Generating state");
        assert_eq!(state.phase, RepoPhase::Generating);
        assert!(state.diff_hash.is_none());
    }

    #[test]
    fn query_state_reflects_latest_broadcast() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("transition.sock");
        let shutdown = Arc::new(AtomicBool::new(false));
        let mut bus = EventBus::bind(&sock, Arc::clone(&shutdown), "repo".into()).unwrap();

        bus.broadcast(RepoPhase::Generating, None, None);
        bus.broadcast(RepoPhase::Ready, Some("final_hash".into()), None);
        thread::sleep(Duration::from_millis(50));

        let state = query_state(&sock, "repo").expect("should return latest state");
        assert_eq!(state.phase, RepoPhase::Ready);
        assert_eq!(state.diff_hash.as_deref(), Some("final_hash"));
    }

    #[test]
    fn hello_request_returns_server_version() {
        use crate::ipc::protocol::{
            ClientOp, ClientRequest, ResponseBody, decode_server_response, encode_client_request,
        };

        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("hello.sock");
        let shutdown = Arc::new(AtomicBool::new(false));
        let _bus = EventBus::bind(&sock, Arc::clone(&shutdown), "repo".into()).unwrap();

        thread::sleep(Duration::from_millis(50));

        let mut stream = UnixStream::connect(&sock).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(500)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_millis(500)))
            .unwrap();

        let req = ClientRequest {
            v: IPC_PROTOCOL_VERSION,
            repo_id: "repo".into(),
            request_id: 42,
            op: ClientOp::Hello,
        };
        let payload = encode_client_request(&req).unwrap();
        write_frame(&mut stream, &payload).unwrap();

        let frame = read_frame(&mut stream).unwrap();
        let resp = decode_server_response(&frame).unwrap();

        assert_eq!(resp.request_id, 42);
        let ResponseBody::HelloAck { server_version } = resp.body else {
            panic!("expected HelloAck, got {:?}", resp.body);
        };
        assert_eq!(server_version, IPC_PROTOCOL_VERSION);
    }
}
