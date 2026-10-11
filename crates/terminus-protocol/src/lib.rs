//! Versioned, length-prefixed JSON frames.
//!
//! Trust boundary: every byte on this socket is untrusted until it passes the
//! size check, version check, and serde validation. The simulation validates
//! actions again. This crate does not grant authority by itself.
//!
//! Wire layout:
//!
//! ```text
//! | u32 big-endian length | UTF-8 JSON object |
//! ```
//!
//! The JSON object is a [`ClientFrame`] or [`ServerFrame`]. Pushes from the
//! server use `id = 0`. Responses copy the request id.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used))]

mod client;
mod codec;
mod message;

pub use client::ProtocolClient;
pub use codec::{decode_client, decode_server, encode_frame, FrameDecoder};
pub use message::{
    ClientFrame, ClientMessage, RevealedOnce, ServerFrame, ServerMessage, MAX_FRAME_BYTES,
    PROTOCOL_VERSION,
};

use thiserror::Error;

/// Protocol failure that is safe to show to a client.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("frame exceeds {MAX_FRAME_BYTES} bytes")]
    FrameTooLarge,
    #[error("decoder buffer exceeded")]
    BufferFull,
    #[error("incomplete frame")]
    Incomplete,
    #[error("malformed frame")]
    Malformed,
    #[error("unsupported protocol version {0}")]
    BadVersion(u16),
    #[error("connection closed")]
    Closed,
    #[error("timed out waiting for a response")]
    Timeout,
    #[error("{0}")]
    Rejected(String),
}
