//! Persistent character records. Locations themselves live in the world definition.

use std::collections::BTreeSet;

use terminus_core::PlayerId;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub keyword: String,
    pub name: String,
    pub detail: String,
}

impl Item {
    pub fn chip() -> Self {
        Self {
            keyword: "chip".into(),
            name: "loyalty chip".into(),
            detail: "A scuffed Coffee Shop chip. It opens nothing important.".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Operator {
    pub id: PlayerId,
    pub account_id: Uuid,
    pub username: String,
    pub name: String,
    pub location_id: String,
    pub flags: BTreeSet<String>,
    pub items: Vec<Item>,
}

/// Writes that must commit after a successful in-memory transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PersistOp {
    Move {
        operator_id: PlayerId,
        location_id: String,
        last_seen: u64,
    },
    Seen {
        operator_id: PlayerId,
        last_seen: u64,
    },
    Message {
        at_ms: u64,
        channel: String,
        audience: String,
        audience_key: String,
        speaker: Option<String>,
        body: String,
    },
    Flag {
        operator_id: PlayerId,
        flag: String,
    },
    Item {
        operator_id: PlayerId,
        keyword: String,
        name: String,
        detail: String,
    },
    /// First visit records identity only. The tile grid stays derived.
    MaterializeRegion {
        id: Uuid,
        world_id: String,
        epoch_id: u32,
        origin_x: i32,
        origin_y: i32,
        generation_version: u16,
        region_seed_hex: String,
        placement_x: i32,
        placement_y: i32,
        discovered_at: u64,
    },
    /// Move the atlas label. Birth coordinates and the place key stay put.
    RelocateRegion {
        id: Uuid,
        placement_x: i32,
        placement_y: i32,
    },
    Structure {
        id: Uuid,
        region_id: Uuid,
        local_x: u8,
        local_y: u8,
        kind: String,
        name: String,
        owner_id: Option<PlayerId>,
        created_at: u64,
    },
    TerrainMod {
        region_id: Uuid,
        local_x: u8,
        local_y: u8,
        terrain: String,
        reason: String,
        actor_id: Option<PlayerId>,
        created_at: u64,
    },
    Discovery {
        id: Uuid,
        region_id: Uuid,
        local_x: u8,
        local_y: u8,
        kind: String,
        note: String,
        actor_id: Option<PlayerId>,
        created_at: u64,
    },
    Connection {
        id: Uuid,
        from_place: String,
        keyword: String,
        to_place: String,
        label: String,
        kind: String,
        created_by: Option<PlayerId>,
        created_at: u64,
    },
}

pub const STARTER_LOCATION: &str = "coffee-shop";
pub const LOG_CAP: usize = 200;
pub const FEED_CAP: usize = 40;
