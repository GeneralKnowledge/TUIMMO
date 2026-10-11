//! Presentation model. Games map their own types into this snapshot.

use serde::{Deserialize, Serialize};

use crate::command::CommandHint;

/// Whether the client currently holds a live session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Connected,
    Offline,
}

/// Where a feed line came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    System,
    Local,
    Global,
    /// Private speech between two operators. Never shown in room or global feeds.
    Private,
}

/// One line in the world feed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedLine {
    pub at_ms: u64,
    pub channel: Channel,
    pub speaker: Option<String>,
    pub text: String,
}

/// An exit the player is allowed to see.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExitView {
    pub keyword: String,
    pub label: String,
    pub destination: String,
}

/// Another operator visible in the current location.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresenceView {
    pub name: String,
}

/// Something in the room that can be inspected.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectView {
    pub keyword: String,
    pub name: String,
}

/// A carried item. This is a label, not an economy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemView {
    pub keyword: String,
    pub name: String,
    pub detail: String,
}

/// Everything the terminal needs to draw one player.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameView {
    pub world_name: String,
    pub connection: ConnectionState,
    pub operator_name: String,
    pub location_name: String,
    pub description: String,
    pub exits: Vec<ExitView>,
    pub present: Vec<PresenceView>,
    pub objects: Vec<ObjectView>,
    pub status: Vec<String>,
    pub inventory: Vec<ItemView>,
    pub feed: Vec<FeedLine>,
    /// Optional panel text that replaces the location description (help, map).
    pub overlay: Option<String>,
    pub commands: Vec<CommandHint>,
}

impl GameView {
    pub fn empty(world_name: impl Into<String>) -> Self {
        Self {
            world_name: world_name.into(),
            connection: ConnectionState::Offline,
            operator_name: String::new(),
            location_name: String::new(),
            description: String::new(),
            exits: Vec::new(),
            present: Vec::new(),
            objects: Vec::new(),
            status: Vec::new(),
            inventory: Vec::new(),
            feed: Vec::new(),
            overlay: None,
            commands: Vec::new(),
        }
    }
}
