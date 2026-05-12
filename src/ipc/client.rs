//! One-shot IPC client for querying daemon state.
//!
//! Used by `sotto complete` (and future subcommands) to skip expensive local
//! diff work when the daemon already knows the answer.

use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use super::protocol::{
    ClientOp, ClientRequest, IPC_PROTOCOL_VERSION, RepoStateSnapshot, ResponseBody,
    decode_server_response, encode_client_request, read_frame, write_frame,
};

/// Connect to the daemon socket, send `GetState`, return the snapshot.
///
/// Returns `None` on any failure — socket missing, daemon not running,
/// timeout, protocol mismatch. Callers should always have a fallback.
pub fn query_state(socket_path: &Path, repo_id: &str) -> Option<RepoStateSnapshot> {
    let mut stream = UnixStream::connect(socket_path).ok()?;
    stream.set_read_timeout(Some(QUERY_TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(QUERY_TIMEOUT)).ok()?;

    let req = ClientRequest {
        v: IPC_PROTOCOL_VERSION,
        repo_id: repo_id.into(),
        request_id: 0,
        op: ClientOp::GetState,
    };

    let payload = encode_client_request(&req).ok()?;
    write_frame(&mut stream, &payload).ok()?;

    let frame = read_frame(&mut stream).ok()?;
    let resp = decode_server_response(&frame).ok()?;

    match resp.body {
        ResponseBody::State { state } => Some(state),
        _ => None,
    }
}

const QUERY_TIMEOUT: Duration = Duration::from_millis(500);
