//! World events and who should hear them.

use serde::{Deserialize, Serialize};

use crate::id::PlayerId;
use crate::view::{Channel, FeedLine};

/// Coarse audience stored with a log line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudienceKind {
    Everyone,
    Location,
    Player,
}

/// Who should observe an event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Audience {
    Everyone,
    Location(String),
    Player(PlayerId),
}

impl Audience {
    pub fn kind(&self) -> AudienceKind {
        match self {
            Self::Everyone => AudienceKind::Everyone,
            Self::Location(_) => AudienceKind::Location,
            Self::Player(_) => AudienceKind::Player,
        }
    }

    pub fn key(&self) -> String {
        match self {
            Self::Everyone => String::new(),
            Self::Location(id) => id.clone(),
            Self::Player(id) => id.to_string(),
        }
    }
}

/// A line recorded in the world log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEntry {
    pub at_ms: u64,
    pub channel: Channel,
    pub audience: Audience,
    pub speaker: Option<String>,
    pub text: String,
}

impl LogEntry {
    pub fn to_feed(&self) -> FeedLine {
        FeedLine {
            at_ms: self.at_ms,
            channel: self.channel,
            speaker: self.speaker.clone(),
            text: self.text.clone(),
        }
    }

    pub fn visible_to(&self, location_id: &str, player_id: PlayerId) -> bool {
        match &self.audience {
            Audience::Everyone => true,
            Audience::Location(id) => id == location_id,
            Audience::Player(id) => *id == player_id,
        }
    }
}

/// An event produced by a state transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameEvent {
    pub entry: LogEntry,
}
