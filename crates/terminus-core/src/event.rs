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
    Region,
    Player,
}

/// Who should observe an event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Audience {
    Everyone,
    Location(String),
    /// Physical region key (`r:…` or `u:…`). Eligibility is always server-derived.
    Region(String),
    Player(PlayerId),
}

impl Audience {
    pub fn kind(&self) -> AudienceKind {
        match self {
            Self::Everyone => AudienceKind::Everyone,
            Self::Location(_) => AudienceKind::Location,
            Self::Region(_) => AudienceKind::Region,
            Self::Player(_) => AudienceKind::Player,
        }
    }

    pub fn key(&self) -> String {
        match self {
            Self::Everyone => String::new(),
            Self::Location(id) => id.clone(),
            Self::Region(key) => key.clone(),
            Self::Player(id) => id.to_string(),
        }
    }
}

/// Viewer position used when filtering the log for a feed or live push.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewerContext<'a> {
    pub player_id: PlayerId,
    pub location_id: &'a str,
    pub region_key: &'a str,
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

    pub fn visible_to(&self, viewer: &ViewerContext<'_>) -> bool {
        match &self.audience {
            Audience::Everyone => true,
            Audience::Location(id) => id == viewer.location_id,
            Audience::Region(key) => key == viewer.region_key,
            Audience::Player(id) => *id == viewer.player_id,
        }
    }
}

/// An event produced by a state transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameEvent {
    pub entry: LogEntry,
}
