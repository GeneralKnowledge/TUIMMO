//! Persistent geography. These rows are the history the generator cannot rebuild.

use std::collections::HashMap;

use terminus_core::PlayerId;
use uuid::Uuid;

use super::generate::Terrain;
use super::identity::{RegionCoordKey, RegionIdentity};

pub const MARKER_KIND: &str = "marker";
pub const CABIN_KIND: &str = "cabin";
pub const WORKSHOP_KIND: &str = "workshop";
pub const SIGN_KIND: &str = "sign";
pub const TRAIL_KIND: &str = "trail";

/// First visit to a birth cell. Shared region history, not a personal note.
pub const ENTERED_KIND: &str = "entered";
/// A player's field journal entry. Private to `actor_id`.
pub const FIELD_KIND: &str = "field";

pub const ACCESS_PUBLIC: &str = "public";
pub const ACCESS_PRIVATE: &str = "private";
pub const ACCESS_FRIENDS: &str = "friends";

/// Settlement-member sharing for enterable structures. Orthogonal to visitor `access`.
pub const SHARE_PRIVATE: &str = "private";
pub const SHARE_SHARED: &str = "shared";
pub const SHARE_COMMUNAL: &str = "communal";

/// A region that has been entered or changed. The tile grid is not stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializedRegion {
    pub id: Uuid,
    pub identity: RegionIdentity,
    /// Audit copy of [`super::generate::derive_region_seed`]. Terrain does not read this.
    pub region_seed: u64,
    /// Atlas label. Starts equal to the birth coordinates and can move later.
    pub placement_x: i32,
    pub placement_y: i32,
    pub discovered_at: u64,
}

/// A player-authored object on one tile. Free-form text stays in `name`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Structure {
    pub id: Uuid,
    pub region_id: Uuid,
    pub local_x: u8,
    pub local_y: u8,
    pub kind: String,
    pub name: String,
    pub owner_id: Option<PlayerId>,
    /// `public`, `private`, or `friends`. Meaningful for cabins/workshops.
    pub access: String,
    /// Interior place key for enterable structures (`i:<structure-id>`).
    pub interior_place: Option<String>,
    /// Optional link to a player settlement. Cleared on disband; never deletes the structure.
    pub settlement_id: Option<Uuid>,
    /// `private`, `shared`, or `communal`. Settlement membership never implies access when private.
    pub share: String,
    pub created_at: u64,
}

impl Structure {
    pub fn interior_key(id: Uuid) -> String {
        format!("i:{id}")
    }

    pub fn parse_interior_key(raw: &str) -> Option<Uuid> {
        let rest = raw.strip_prefix("i:")?;
        Uuid::parse_str(rest).ok()
    }

    pub fn is_enterable(&self) -> bool {
        matches!(self.kind.as_str(), CABIN_KIND | WORKSHOP_KIND) && self.interior_place.is_some()
    }

    pub fn is_workshop(&self) -> bool {
        self.kind == WORKSHOP_KIND || (self.kind == CABIN_KIND && self.share == SHARE_COMMUNAL)
    }
}

/// An override of the generated baseline at one tile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerrainMod {
    pub region_id: Uuid,
    pub local_x: u8,
    pub local_y: u8,
    pub terrain: Terrain,
    pub reason: String,
    pub actor_id: Option<PlayerId>,
    pub created_at: u64,
}

/// A recorded visit or a personal field note. This is history, not the terrain itself.
///
/// Visit rows (`entered`) keep a region id. Field notes may name a static Undernet
/// room instead, in which case `region_id` is absent and `place_key` is the room id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Discovery {
    pub id: Uuid,
    pub region_id: Option<Uuid>,
    pub local_x: u8,
    pub local_y: u8,
    pub kind: String,
    pub note: String,
    pub actor_id: Option<PlayerId>,
    pub created_at: u64,
    /// Stable authored identity. The same feature shares this key across players.
    pub content_key: Option<String>,
    pub title: Option<String>,
    /// Wilderness place key or static location id. How the player returns.
    pub place_key: Option<String>,
}

/// An explicit link that adjacency does not imply. Player links survive reseed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldConnection {
    pub id: Uuid,
    pub from_place: String,
    pub keyword: String,
    pub to_place: String,
    pub label: String,
    pub kind: String,
    pub created_by: Option<PlayerId>,
    pub created_at: u64,
}

/// In-memory copy of the persistent geography tables.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GeographyState {
    pub regions: HashMap<RegionCoordKey, MaterializedRegion>,
    pub structures: Vec<Structure>,
    pub terrain_mods: Vec<TerrainMod>,
    pub discoveries: Vec<Discovery>,
    pub connections: Vec<WorldConnection>,
}

impl GeographyState {
    pub fn region(&self, key: RegionCoordKey) -> Option<&MaterializedRegion> {
        self.regions.get(&key)
    }

    pub fn region_by_id(&self, id: Uuid) -> Option<&MaterializedRegion> {
        self.regions.values().find(|region| region.id == id)
    }

    pub fn mod_at(&self, region_id: Uuid, local_x: u8, local_y: u8) -> Option<&TerrainMod> {
        self.terrain_mods.iter().find(|item| {
            item.region_id == region_id && item.local_x == local_x && item.local_y == local_y
        })
    }

    pub fn structures_at(&self, region_id: Uuid, local_x: u8, local_y: u8) -> Vec<&Structure> {
        self.structures
            .iter()
            .filter(|item| {
                item.region_id == region_id && item.local_x == local_x && item.local_y == local_y
            })
            .collect()
    }

    pub fn structure_of_kind_at(
        &self,
        region_id: Uuid,
        local_x: u8,
        local_y: u8,
        kind: &str,
    ) -> Option<&Structure> {
        self.structures.iter().find(|item| {
            item.region_id == region_id
                && item.local_x == local_x
                && item.local_y == local_y
                && item.kind == kind
        })
    }

    pub fn structure_by_id(&self, id: Uuid) -> Option<&Structure> {
        self.structures.iter().find(|item| item.id == id)
    }

    pub fn structure_by_interior(&self, place: &str) -> Option<&Structure> {
        self.structures.iter().find(|item| item.interior_place.as_deref() == Some(place))
    }

    pub fn connection(&self, from_place: &str, keyword: &str) -> Option<&WorldConnection> {
        self.connections
            .iter()
            .find(|link| link.from_place == from_place && link.keyword == keyword)
    }

    pub fn connections_touching(&self, place: &str) -> Vec<&WorldConnection> {
        self.connections
            .iter()
            .filter(|link| link.from_place == place || link.to_place == place)
            .collect()
    }

    /// Personal field notes, newest first. Other players' rows are omitted.
    pub fn field_notes_for(&self, actor: PlayerId) -> Vec<&Discovery> {
        let mut notes: Vec<&Discovery> = self
            .discoveries
            .iter()
            .filter(|item| {
                item.kind == FIELD_KIND
                    && item.actor_id == Some(actor)
                    && item.content_key.is_some()
            })
            .collect();
        notes.sort_by(|left, right| {
            right.created_at.cmp(&left.created_at).then_with(|| left.id.cmp(&right.id))
        });
        notes
    }

    pub fn field_note(&self, actor: PlayerId, content_key: &str) -> Option<&Discovery> {
        self.discoveries.iter().find(|item| {
            item.kind == FIELD_KIND
                && item.actor_id == Some(actor)
                && item.content_key.as_deref() == Some(content_key)
        })
    }
}
