//! Execute one command line through authoritative game rules.

use coffee_world::{Transition, WorldState};
use terminus_core::{GameError, SessionId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionResult {
    Success { command: String },
    Rejected { command: String, reason: String },
    Failed { command: String, error: String },
}

impl ActionResult {
    pub fn command(&self) -> &str {
        match self {
            Self::Success { command }
            | Self::Rejected { command, .. }
            | Self::Failed { command, .. } => command,
        }
    }

    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success { .. })
    }
}

pub fn execute_line(
    world: &mut WorldState,
    session_id: SessionId,
    line: &str,
) -> (ActionResult, Option<Transition>) {
    let command = line.trim().to_string();
    if command.is_empty() {
        return (ActionResult::Success { command: String::new() }, None);
    }
    match world.apply(session_id, &command) {
        Ok(transition) => (ActionResult::Success { command }, Some(transition)),
        Err(GameError::Rejected(reason)) => (ActionResult::Rejected { command, reason }, None),
        Err(other) => (ActionResult::Failed { command, error: other.to_string() }, None),
    }
}
