//! Unix socket IPC types, framing, and server.

pub mod protocol;

#[cfg(unix)]
pub mod server;
