//! JSON message types and length-prefixed framing for the Unix socket IPC.
//!
//! # Wire format
//!
//! Each frame is **`u32` little-endian length** (byte count of the payload) followed by **UTF-8 JSON**
//! for a single value (`ClientRequest`, `ServerResponse`, or `ServerEvent`).
#![cfg_attr(not(test), allow(dead_code))]

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};

pub fn write_frame<W: Write>(writer: &mut W, json_payload: &[u8]) -> Result<(), ProtocolError> {
    let len: u32 = json_payload
        .len()
        .try_into()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "frame payload exceeds u32"))?;
    check_payload_len(len)?;
    writer.write_all(&len.to_le_bytes())?;
    writer.write_all(json_payload)?;
    Ok(())
}

pub fn read_frame<R: Read>(reader: &mut R) -> Result<Vec<u8>, ProtocolError> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf);
    check_payload_len(len)?;
    let mut buf = vec![0u8; len as usize];
    reader.read_exact(&mut buf)?;
    Ok(buf)
}

pub fn expect_version(v: u32) -> Result<(), ProtocolError> {
    if v != IPC_PROTOCOL_VERSION {
        return Err(ProtocolError::VersionMismatch {
            got: v,
            expected: IPC_PROTOCOL_VERSION,
        });
    }
    Ok(())
}

pub fn encode_client_request(req: &ClientRequest) -> Result<Vec<u8>, ProtocolError> {
    Ok(serde_json::to_vec(req)?)
}

pub fn decode_client_request(bytes: &[u8]) -> Result<ClientRequest, ProtocolError> {
    let r: ClientRequest = serde_json::from_slice(bytes)?;
    expect_version(r.v)?;
    Ok(r)
}

pub fn encode_server_response(res: &ServerResponse) -> Result<Vec<u8>, ProtocolError> {
    Ok(serde_json::to_vec(res)?)
}

pub fn decode_server_response(bytes: &[u8]) -> Result<ServerResponse, ProtocolError> {
    let r: ServerResponse = serde_json::from_slice(bytes)?;
    expect_version(r.v)?;
    Ok(r)
}

pub fn encode_server_event(ev: &ServerEvent) -> Result<Vec<u8>, ProtocolError> {
    Ok(serde_json::to_vec(ev)?)
}

pub fn decode_server_event(bytes: &[u8]) -> Result<ServerEvent, ProtocolError> {
    let e: ServerEvent = serde_json::from_slice(bytes)?;
    expect_version(e.v)?;
    Ok(e)
}

fn check_payload_len(len: u32) -> Result<(), ProtocolError> {
    if len > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge {
            size: len,
            max: MAX_FRAME_BYTES,
        });
    }
    Ok(())
}

#[derive(Debug)]
pub enum ProtocolError {
    Io(io::Error),
    SerdeJson(serde_json::Error),
    VersionMismatch { got: u32, expected: u32 },
    FrameTooLarge { size: u32, max: u32 },
}

impl From<io::Error> for ProtocolError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for ProtocolError {
    fn from(value: serde_json::Error) -> Self {
        Self::SerdeJson(value)
    }
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtocolError::Io(e) => write!(f, "{e}"),
            ProtocolError::SerdeJson(e) => write!(f, "{e}"),
            ProtocolError::VersionMismatch { got, expected } => write!(
                f,
                "ipc protocol version mismatch (got {got}, expected {expected})"
            ),
            ProtocolError::FrameTooLarge { size, max } => {
                write!(f, "ipc frame too large ({size} bytes, max {max})")
            }
        }
    }
}

impl std::error::Error for ProtocolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ProtocolError::Io(e) => Some(e),
            ProtocolError::SerdeJson(e) => Some(e),
            _ => None,
        }
    }
}

/// Current IPC protocol version. Bump when breaking JSON shapes or semantics.
pub const IPC_PROTOCOL_VERSION: u32 = 1;

/// Largest allowed frame payload in bytes (length prefix must be ≤ this).
pub const MAX_FRAME_BYTES: u32 = 4 * 1024 * 1024;

/// Rough daemon-side phases
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepoPhase {
    Idle,
    Watching,
    Debouncing,
    Generating,
    Ready,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoStateSnapshot {
    pub phase: RepoPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum EventKind {
    RepoState { state: RepoStateSnapshot },
}

/// Unsolicited server -> client message (subscriptions / push).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerEvent {
    pub v: u32,
    pub repo_id: String,
    pub event: EventKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ResponseBody {
    HelloAck {
        /// Protocol version the daemon speaks (should match [`IPC_PROTOCOL_VERSION`] for v1).
        server_version: u32,
    },
    State {
        state: RepoStateSnapshot,
    },
    Err {
        code: String,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerResponse {
    pub v: u32,
    pub repo_id: String,
    pub request_id: u64,
    pub body: ResponseBody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ClientOp {
    /// Version and capability check; server replies with [`ResponseBody::HelloAck`].
    Hello,
    /// Snapshot of live repo state (phase, hashes, errors).
    GetState,
}

/// Every top-level body includes `v` + `repo_id` per epic #10.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientRequest {
    pub v: u32,
    pub repo_id: String,
    pub request_id: u64,
    pub op: ClientOp,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn frame_roundtrip() {
        let req = ClientRequest {
            v: IPC_PROTOCOL_VERSION,
            repo_id: "deadbeef".to_string(),
            request_id: 1,
            op: ClientOp::Hello,
        };
        let bytes = encode_client_request(&req).unwrap();

        let mut buf = Vec::new();
        write_frame(&mut buf, &bytes).unwrap();

        let mut cur = Cursor::new(buf);
        let frame = read_frame(&mut cur).unwrap();
        let got = decode_client_request(&frame).unwrap();
        assert_eq!(got, req);
    }

    #[test]
    fn reject_oversized_length() {
        let mut buf = vec![0xffu8, 0xff, 0xff, 0xff];
        buf.extend_from_slice(&[0u8; 16]);
        let mut cur = Cursor::new(buf);
        let err = read_frame(&mut cur).unwrap_err();
        assert!(matches!(err, ProtocolError::FrameTooLarge { .. }));
    }

    #[test]
    fn version_mismatch_decode() {
        let req = ClientRequest {
            v: 999,
            repo_id: "x".into(),
            request_id: 0,
            op: ClientOp::GetState,
        };
        let bytes = serde_json::to_vec(&req).unwrap();
        let err = decode_client_request(&bytes).unwrap_err();
        assert!(matches!(err, ProtocolError::VersionMismatch { .. }));
    }

    #[test]
    fn server_response_json_roundtrip() {
        let res = ServerResponse {
            v: IPC_PROTOCOL_VERSION,
            repo_id: "abc".into(),
            request_id: 7,
            body: ResponseBody::State {
                state: RepoStateSnapshot {
                    phase: RepoPhase::Generating,
                    diff_hash: Some("cafefea".into()),
                    error: None,
                },
            },
        };
        let bytes = encode_server_response(&res).unwrap();
        assert_eq!(decode_server_response(&bytes).unwrap(), res);
    }

    #[test]
    fn server_event_json_roundtrip() {
        let ev = ServerEvent {
            v: IPC_PROTOCOL_VERSION,
            repo_id: "abc".into(),
            event: EventKind::RepoState {
                state: RepoStateSnapshot {
                    phase: RepoPhase::Ready,
                    diff_hash: None,
                    error: None,
                },
            },
        };
        let bytes = encode_server_event(&ev).unwrap();
        assert_eq!(decode_server_event(&bytes).unwrap(), ev);
    }

    #[test]
    fn server_response_frame_roundtrip() {
        let res = ServerResponse {
            v: IPC_PROTOCOL_VERSION,
            repo_id: "x".into(),
            request_id: 0,
            body: ResponseBody::HelloAck {
                server_version: IPC_PROTOCOL_VERSION,
            },
        };
        let payload = encode_server_response(&res).unwrap();
        let mut buf = Vec::new();
        write_frame(&mut buf, &payload).unwrap();
        let mut cur = Cursor::new(buf);
        let frame = read_frame(&mut cur).unwrap();
        assert_eq!(decode_server_response(&frame).unwrap(), res);
    }
}
