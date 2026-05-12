//! Unix-socket listener and event broadcasting for the daemon.
//!
//! Binds [`Paths::socket`], accepts client connections on a background thread,
//! and lets the watcher broadcast [`ServerEvent`] frames to every connected client.
//!
//! Each client gets a dedicated writer thread that owns the stream. The watcher
//! never touches streams directly — it sends pre-serialized frames through
//! per-client channels, so `broadcast` is non-blocking by construction.

use std::io::{Read, Write};
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
    encode_server_response, write_frame,
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

fn write_err_response(
    stream: &mut UnixStream,
    daemon_repo_id: &str,
    request_id: u64,
    code: &str,
    message: &str,
) {
    let resp = ServerResponse {
        v: IPC_PROTOCOL_VERSION,
        repo_id: daemon_repo_id.into(),
        request_id,
        body: ResponseBody::Err {
            code: code.into(),
            message: message.into(),
        },
    };
    let _ = encode_server_response(&resp)
        .ok()
        .and_then(|payload| write_frame(stream, &payload).ok());
}

fn reset_subscriber_stream(stream: &mut UnixStream) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(None);
    let _ = stream.set_write_timeout(None);
}

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
            Ok((mut stream, _)) => {
                if !try_handle_request(&mut stream, &state, &repo_id) && tx.send(stream).is_err() {
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

/// If the client sends a framed request immediately, handle it and return `true`.
/// If no data is pending yet, reset the stream for the subscriber writer and return `false`.
fn try_handle_request(
    stream: &mut UnixStream,
    state: &Arc<RwLock<RepoStateSnapshot>>,
    daemon_repo_id: &str,
) -> bool {
    if stream.set_nonblocking(true).is_err() {
        return false;
    }

    let mut len_buf = [0u8; 4];
    let mut filled = 0usize;

    loop {
        match stream.read(&mut len_buf[filled..]) {
            Ok(0) => {
                reset_subscriber_stream(stream);
                return false;
            }
            Ok(n) => {
                filled += n;
                if filled == 4 {
                    break;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                if filled == 0 {
                    reset_subscriber_stream(stream);
                    return false;
                }
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_read_timeout(Some(REQUEST_BODY_READ_TIMEOUT));
                let ok = stream.read_exact(&mut len_buf[filled..]).is_ok();
                let _ = stream.set_read_timeout(None);
                if !ok {
                    return true;
                }
                break;
            }
            Err(_) => {
                reset_subscriber_stream(stream);
                return false;
            }
        }
    }

    let len = u32::from_le_bytes(len_buf);
    if len > super::protocol::MAX_FRAME_BYTES {
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_write_timeout(Some(REQUEST_WRITE_TIMEOUT));
        write_err_response(
            stream,
            daemon_repo_id,
            0,
            "e_frame_too_large",
            "request frame exceeds max size",
        );
        let _ = stream.set_write_timeout(None);
        return true;
    }

    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(REQUEST_BODY_READ_TIMEOUT));
    let mut body = vec![0u8; len as usize];
    if stream.read_exact(&mut body).is_err() {
        let _ = stream.set_read_timeout(None);
        return true;
    }
    let _ = stream.set_read_timeout(None);

    let req = match decode_client_request(&body) {
        Ok(r) => r,
        Err(_) => {
            let _ = stream.set_write_timeout(Some(REQUEST_WRITE_TIMEOUT));
            write_err_response(
                stream,
                daemon_repo_id,
                0,
                "e_bad_request",
                "could not decode client request",
            );
            let _ = stream.set_write_timeout(None);
            return true;
        }
    };

    if req.repo_id != daemon_repo_id {
        let _ = stream.set_write_timeout(Some(REQUEST_WRITE_TIMEOUT));
        write_err_response(
            stream,
            daemon_repo_id,
            req.request_id,
            "e_repo_mismatch",
            "client repo_id does not match this daemon",
        );
        let _ = stream.set_write_timeout(None);
        return true;
    }

    let resp_body = match req.op {
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
        repo_id: daemon_repo_id.into(),
        request_id: req.request_id,
        body: resp_body,
    };

    let _ = stream.set_write_timeout(Some(REQUEST_WRITE_TIMEOUT));
    let _ = encode_server_response(&resp)
        .ok()
        .and_then(|payload| write_frame(stream, &payload).ok());
    let _ = stream.set_write_timeout(None);

    true
}

const REQUEST_BODY_READ_TIMEOUT: Duration = Duration::from_millis(200);
const REQUEST_WRITE_TIMEOUT: Duration = Duration::from_millis(200);

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
    use std::time::{Duration, Instant};

    use crate::ipc::client::query_state;
    use crate::ipc::protocol::{
        EventKind, IPC_PROTOCOL_VERSION, RepoPhase, decode_server_event, read_frame,
    };

    fn poll_get_state(sock: &Path, repo_id: &str) -> RepoStateSnapshot {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(s) = query_state(sock, repo_id) {
                return s;
            }
            assert!(
                Instant::now() < deadline,
                "query_state timed out for {}",
                sock.display()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn connect_retry(sock: &Path) -> UnixStream {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Ok(s) = UnixStream::connect(sock) {
                return s;
            }
            assert!(
                Instant::now() < deadline,
                "connect timed out for {}",
                sock.display()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

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
        let start = Instant::now();
        let ev = loop {
            bus.broadcast(RepoPhase::Ready, Some("abc123".into()), None);
            assert!(
                start.elapsed() < Duration::from_secs(2),
                "broadcast test timed out"
            );
            if reader.is_finished() {
                break reader.join().unwrap();
            }
            thread::sleep(Duration::from_millis(10));
        };
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

        let state = poll_get_state(&sock, "repo_xyz");
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

        let state = poll_get_state(&sock, "repo");
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

        let state = poll_get_state(&sock, "repo");
        assert_eq!(state.phase, RepoPhase::Ready);
        assert_eq!(state.diff_hash.as_deref(), Some("final_hash"));
    }

    #[test]
    fn query_state_returns_none_on_repo_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mismatch.sock");
        let shutdown = Arc::new(AtomicBool::new(false));
        let mut bus = EventBus::bind(&sock, Arc::clone(&shutdown), "daemon_repo".into()).unwrap();

        bus.broadcast(RepoPhase::Ready, Some("h".into()), None);

        let _ = poll_get_state(&sock, "daemon_repo");
        assert!(query_state(&sock, "wrong_client_repo").is_none());
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

        let mut stream = connect_retry(&sock);
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

        assert_eq!(resp.repo_id, "repo");
        assert_eq!(resp.request_id, 42);
        let ResponseBody::HelloAck { server_version } = resp.body else {
            panic!("expected HelloAck, got {:?}", resp.body);
        };
        assert_eq!(server_version, IPC_PROTOCOL_VERSION);
    }
}
