//! Terminal rendering for Terminus.
//!
//! The widgets draw a [`terminus_core::GameView`]. They do not contain Coffee.shop
//! rules. A cyberpunk palette is supplied by the game as a [`Theme`].

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used))]

mod app;
mod decode;
mod editor;
mod local;
mod onboard;
mod remote;
mod theme;

pub use app::{App, Effect, Screen};
pub use decode::InputDecoder;
pub use local::{run_local, spawn_crossterm_input};
pub use remote::{run_remote, Input, StartMode};
pub use theme::Theme;

use terminus_core::GameError;
use terminus_protocol::ProtocolError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TuiError {
    #[error("terminal error: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Game(#[from] GameError),
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
}
