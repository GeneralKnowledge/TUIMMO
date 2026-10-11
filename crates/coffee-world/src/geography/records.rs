//! Persistent geography. These rows are the history the generator cannot rebuild.

use std::collections::HashMap;

use terminus_core::PlayerId;
use uuid::Uuid;

use super::generate::Terrain;
use super::identity::{RegionCoordKey, RegionIdentity};

pub const MARKER_KIND: &str = "marker";

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
    pub created_at: u64,
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

/// A recorded visit or noted landmark. This is history, not the terrain itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Discovery {
    pub id: Uuid,
    pub region_id: Uuid,
    pub local_x: u8,
    pub local_y: u8,
    pub kind: String,
    pub note: String,
    pub actor_id: Option<PlayerId>,
    pub created_at: u64,
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

    pub fn connection(&self, from_place: &str, keyword: &str) -> Option<&WorldConnection> {
        self.connections
            .iter()
            .find(|link| link.from_place == from_place && link.keyword == keyword)
    }
}
