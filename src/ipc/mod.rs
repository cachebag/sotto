//! Unix socket IPC types, framing, client, and server.

pub mod protocol;

#[cfg(unix)]
pub mod client;

#[cfg(unix)]
pub mod server;
