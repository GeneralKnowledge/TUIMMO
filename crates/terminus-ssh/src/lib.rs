//! SSH entry point for Terminus.
//!
//! The gateway authenticates a player, then runs the game TUI on the session
//! channel. It does not start an operating-system shell, does not honour
//! `exec` requests, and does not log passwords.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used))]

mod gateway;
mod keys;
mod limits;

pub use gateway::{serve, AuthReject, Authenticator, SessionGrant, SshConfig};
pub use keys::load_or_create_host_key;
pub use limits::{GateState, Limits};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SshError {
    #[error("ssh transport error: {0}")]
    Russh(#[from] russh::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Message(String),
}
