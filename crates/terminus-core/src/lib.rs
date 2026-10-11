//! Terminus core: game-agnostic commands, sessions, views, and simulation interface.
//!
//! This crate does not know about any particular setting. A game implements [`Game`]
//! and renders [`GameView`] with `terminus-tui` or any other client.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod command;
pub mod error;
pub mod event;
pub mod id;
pub mod input;
pub mod session;
pub mod text;
pub mod view;

pub use command::{ArgMode, CommandHint, CommandRegistry, CommandSpec, ParsedCommand};
pub use error::{GameError, ParseError};
pub use event::{Audience, AudienceKind, GameEvent, LogEntry, ViewerContext};
pub use id::{PlayerId, SessionId};
pub use input::{InputEvent, Key};
pub use session::{Permissions, SessionIdentity};
pub use view::{
    Channel, ConnectionState, ExitView, FeedLine, GameView, ItemView, ObjectView, PresenceView,
};

use command::ParsedCommand as Parsed;

/// Result of applying one command to a session.
#[derive(Clone, Debug, Default)]
pub struct CommandOutcome {
    /// Events produced for the world log. The simulation has already recorded them.
    pub events: Vec<GameEvent>,
    /// When true, the client should close after applying the response.
    pub quit: bool,
}

/// Outcome of attaching a player to the simulation.
#[derive(Clone, Debug)]
pub struct ConnectOutcome {
    pub events: Vec<GameEvent>,
    /// Previous session replaced by this connection, if the player was already online.
    pub replaced: Option<SessionId>,
}

/// Authoritative game adapter.
///
/// Implementations must be deterministic given their inputs and must validate every
/// action. Transport, terminal drawing, and account passwords live outside this trait.
pub trait Game: Send {
    /// Display name of the world, shown in the client header.
    fn world_name(&self) -> &str;

    /// Command definitions used for parsing, help, and completion.
    fn registry(&self) -> &CommandRegistry;

    /// Attach an authenticated identity. Emits join or reconnect events.
    fn connect(&mut self, identity: SessionIdentity) -> Result<ConnectOutcome, GameError>;

    /// Detach a session. Missing sessions are not an error.
    fn disconnect(&mut self, session_id: SessionId) -> Result<Vec<GameEvent>, GameError>;

    /// Parse and apply one command line.
    fn command(&mut self, session_id: SessionId, line: &str) -> Result<CommandOutcome, GameError>;

    /// Build the presentation snapshot for a connected session.
    fn view(&self, session_id: SessionId) -> Result<GameView, GameError>;

    /// Stable location key for the session's character, when connected.
    fn location_key(&self, session_id: SessionId) -> Option<String>;
}

/// Helper for games that want the standard parse-then-dispatch flow.
pub fn parse_command(registry: &CommandRegistry, line: &str) -> Result<Parsed, ParseError> {
    registry.parse(line)
}
