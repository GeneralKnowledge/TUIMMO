//! Persistent character records. Locations themselves live in the world definition.

use std::collections::BTreeSet;

use terminus_core::PlayerId;
use uuid::Uuid;

use crate::settlement::{InviteStatus, MemberRole, SettlementVisibility};
use crate::social::FriendshipStatus;

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
    pub bio: String,
    pub status: String,
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
    Profile {
        operator_id: PlayerId,
        bio: String,
        status: String,
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
        access: String,
        interior_place: Option<String>,
        settlement_id: Option<Uuid>,
        share: String,
        created_at: u64,
    },
    UpdateStructure {
        id: Uuid,
        name: String,
        access: String,
        interior_place: Option<String>,
        share: String,
    },
    LinkStructureSettlement {
        id: Uuid,
        settlement_id: Option<Uuid>,
    },
    DeleteStructure {
        id: Uuid,
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
        region_id: Option<Uuid>,
        local_x: u8,
        local_y: u8,
        kind: String,
        note: String,
        actor_id: Option<PlayerId>,
        created_at: u64,
        content_key: Option<String>,
        title: Option<String>,
        place_key: Option<String>,
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
    DeleteConnection {
        id: Uuid,
    },
    FriendshipUpsert {
        requester_id: PlayerId,
        recipient_id: PlayerId,
        status: FriendshipStatus,
        created_at: u64,
        updated_at: u64,
    },
    FriendshipDelete {
        requester_id: PlayerId,
        recipient_id: PlayerId,
    },
    IgnoreUpsert {
        operator_id: PlayerId,
        ignored_id: PlayerId,
        created_at: u64,
    },
    IgnoreDelete {
        operator_id: PlayerId,
        ignored_id: PlayerId,
    },
    StructureGrantUpsert {
        structure_id: Uuid,
        grantee_id: PlayerId,
        created_at: u64,
    },
    StructureGrantDelete {
        structure_id: Uuid,
        grantee_id: PlayerId,
    },
    NoticeUpsert {
        id: Uuid,
        place_id: String,
        author_id: PlayerId,
        body: String,
        created_at: u64,
        expires_at: u64,
    },
    NoticeDelete {
        id: Uuid,
    },
    SettlementUpsert {
        id: Uuid,
        name: String,
        name_key: String,
        description: String,
        visibility: SettlementVisibility,
        anchor_place: String,
        owner_id: PlayerId,
        created_at: u64,
    },
    SettlementDelete {
        id: Uuid,
    },
    SettlementMemberUpsert {
        settlement_id: Uuid,
        operator_id: PlayerId,
        role: MemberRole,
        joined_at: u64,
    },
    SettlementMemberDelete {
        settlement_id: Uuid,
        operator_id: PlayerId,
    },
    SettlementInviteUpsert {
        id: Uuid,
        settlement_id: Uuid,
        inviter_id: PlayerId,
        invitee_id: PlayerId,
        status: InviteStatus,
        created_at: u64,
        expires_at: u64,
    },
}

pub const STARTER_LOCATION: &str = "coffee-shop";
pub const LOG_CAP: usize = 200;
pub const FEED_CAP: usize = 40;
