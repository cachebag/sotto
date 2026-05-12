//! Unix socket IPC types, framing, client, and server.
//!
//! One daemon per repo today; a future supervisor may multiplex. The socket
//! lives under `$XDG_DATA_HOME/sotto/` with `0600` permissions (local-user
//! threat model only). Stale sockets are detected and unlinked on bind;
//! a live daemon causes `AddrInUse`. See `docs/architecture.md` for details.

pub mod protocol;

#[cfg(unix)]
pub mod client;

#[cfg(unix)]
pub mod server;
