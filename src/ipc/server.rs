//! Unix-socket listener and event broadcasting for the daemon.
//!
//! Binds [`Paths::socket`], accepts client connections on a background thread,
//! and lets the watcher broadcast [`ServerEvent`] frames to every connected client.
//!
//! Each client gets a dedicated writer thread that owns the stream. The watcher
//! never touches streams directly — it sends pre-serialized frames through
//! per-client channels, so `broadcast` is non-blocking by construction.

use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;
use std::{io, thread};

use super::protocol::{
    EventKind, IPC_PROTOCOL_VERSION, RepoPhase, RepoStateSnapshot, ServerEvent,
    encode_server_event, write_frame,
};

impl EventBus {
    /// Bind the Unix socket and spawn the accept thread.
    ///
    /// Handles both edge cases from the issue:
    /// - stale socket from a crash: unlinked before bind
    /// - another live daemon: returns `AddrInUse` immediately
    pub fn bind(
        socket_path: &Path,
        shutdown: Arc<AtomicBool>,
        repo_id: String,
    ) -> io::Result<Self> {
        if socket_path.exists() {
            if UnixStream::connect(socket_path).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    format!(
                        "another sotto daemon is already listening on {}",
                        socket_path.display()
                    ),
                ));
            }
            std::fs::remove_file(socket_path)?;
        }

        let listener = UnixListener::bind(socket_path)?;
        listener.set_nonblocking(true)?;

        let (tx, rx) = mpsc::channel();

        thread::Builder::new()
            .name("sotto-ipc-accept".into())
            .spawn(move || accept_loop(listener, tx, shutdown))?;

        Ok(Self {
            clients: Vec::new(),
            incoming: rx,
            socket_path: socket_path.to_path_buf(),
            repo_id,
        })
    }

    /// Push a phase-change event to every connected client.
    ///
    /// Sends the serialized frame through each client's channel. Channels whose
    /// receiver has hung up (writer thread exited = dead client) fail instantly
    /// on `send` and get pruned. The watcher never blocks on I/O here.
    pub fn broadcast(
        &mut self,
        phase: RepoPhase,
        diff_hash: Option<String>,
        error: Option<String>,
    ) {
        while let Ok(stream) = self.incoming.try_recv() {
            self.clients.push(spawn_client_writer(stream));
        }

        if self.clients.is_empty() {
            return;
        }

        let Some(frame) = self.build_frame(phase, diff_hash, error) else {
            return;
        };

        self.clients
            .retain(|client| client.tx.send(frame.clone()).is_ok());
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
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

/// Spawn a writer thread that owns the stream and drains frames from the channel.
/// Returns a handle the bus holds onto for sending.
fn spawn_client_writer(mut stream: UnixStream) -> ClientHandle {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();

    thread::Builder::new()
        .name("sotto-ipc-client".into())
        .spawn(move || {
            for frame in rx {
                if stream.write_all(&frame).is_err() {
                    break;
                }
            }
        })
        .expect("failed to spawn client writer thread");

    ClientHandle { tx }
}

fn accept_loop(listener: UnixListener, tx: mpsc::Sender<UnixStream>, shutdown: Arc<AtomicBool>) {
    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                if tx.send(stream).is_err() {
                    break;
                }
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(200));
            }
            Err(_) => break,
        }
    }
}

/// Handle held by the watcher to push events to connected IPC clients.
pub struct EventBus {
    clients: Vec<ClientHandle>,
    incoming: mpsc::Receiver<UnixStream>,
    socket_path: PathBuf,
    repo_id: String,
}

/// One per connected client. Dropping it closes the channel, which causes
/// the writer thread to exit and close the stream.
struct ClientHandle {
    tx: mpsc::Sender<Vec<u8>>,
}
