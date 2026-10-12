//! Version 1 procedural terrain.
//!
//! The same world seed, epoch, birth coordinates, and generation version always
//! rebuild the same region. Call order, server restarts, and which other regions
//! were visited do not matter. The mixer is not `std` hashing and is not the
//! process-wide random number generator.

use std::collections::{HashMap, VecDeque};

use thiserror::Error;

use super::identity::{origins_for_absolute, RegionIdentity, WorldId, REGION_SIZE};

/// Epoch 1 master seed. Changing it would be a new epoch, not an edit to version 1.
pub const EPOCH1_WORLD_SEED: u64 = 0x00C0_FFEE_5EED_0001;

/// First published generator. Do not change what this number means.
pub const GENERATION_V1: u16 = 1;

/// Coherent elevation is sampled on a lattice of this many tiles.
const LATTICE: i64 = 4;

/// Elevation below this, out of 65536, is water. About 22%.
const V1_WATER_MAX: u16 = 14_418;
/// Elevation at or above this is rock. About 86%.
const V1_ROCK_MIN: u16 = 56_361;
/// Moisture below this, on walkable ground, is scrub. About 28%.
const V1_SCRUB_MAX: u16 = 18_350;
/// Moisture at or above this, on walkable ground, is forest. About 68%.
const V1_FOREST_MIN: u16 = 44_564;
/// Feature hash at or above this plants a landmark on walkable ground. About 97%.
const V1_LANDMARK_MIN: u16 = 63_570;

const EDGE: usize = 16;

/// A coherent generation regime. Old regions keep the epoch they were born in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EpochSpec {
    pub id: u32,
    pub name: &'static str,
    pub world_seed: u64,
    pub generation_version: u16,
    pub notes: &'static str,
}

pub static EPOCH_1: EpochSpec = EpochSpec {
    id: 1,
    name: "First Squalor",
    world_seed: EPOCH1_WORLD_SEED,
    generation_version: GENERATION_V1,
    notes: "Abandoned urban sectors on an absolute-coordinate elevation lattice. \
            Borders match inside version 1. A later epoch does not rewrite these regions.",
};

pub fn epoch_spec(id: u32) -> Option<&'static EpochSpec> {
    match id {
        1 => Some(&EPOCH_1),
        _ => None,
    }
}

pub fn frontier_epoch() -> &'static EpochSpec {
    &EPOCH_1
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GenerateError {
    #[error("unknown generation version {0}")]
    UnknownVersion(u16),
    #[error("unknown world epoch {0}")]
    UnknownEpoch(u32),
}

/// Walkable ground and the two blockers. Landmarks sit on top of a terrain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terrain {
    Grass,
    Forest,
    Scrub,
    Water,
    Rock,
}

impl Terrain {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Grass => "grass",
            Self::Forest => "forest",
            Self::Scrub => "scrub",
            Self::Water => "water",
            Self::Rock => "rock",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "grass" => Some(Self::Grass),
            "forest" => Some(Self::Forest),
            "scrub" => Some(Self::Scrub),
            "water" => Some(Self::Water),
            "rock" => Some(Self::Rock),
            _ => None,
        }
    }

    pub const fn traversable(self) -> bool {
        matches!(self, Self::Grass | Self::Forest | Self::Scrub)
    }

    pub const fn symbol(self) -> char {
        match self {
            Self::Grass => '.',
            Self::Forest => 't',
            Self::Scrub => ',',
            Self::Water => '~',
            Self::Rock => '^',
        }
    }

    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Grass => "ground",
            Self::Forest => "blocks",
            Self::Scrub => "rubble",
            Self::Water => "flood",
            Self::Rock => "sealed",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Grass => "open street",
            Self::Forest => "tower blocks",
            Self::Scrub => "rubble yard",
            Self::Water => "flooded cut",
            Self::Rock => "sealed structure",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LandmarkKind {
    Spring,
    StandingStone,
    FallenMast,
    Milepost,
}

impl LandmarkKind {
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Spring => "spring",
            Self::StandingStone => "stone",
            Self::FallenMast => "mast",
            Self::Milepost => "milepost",
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Spring => "leaking hydrant",
            Self::StandingStone => "civic monument",
            Self::FallenMast => "fallen mast",
            Self::Milepost => "district post",
        }
    }

    pub const fn prose(self) -> &'static str {
        match self {
            Self::Spring => "A hydrant leaks into a cracked curb. The water is still cold.",
            Self::StandingStone => "A civic monument, plaque gone, still marks the corner.",
            Self::FallenMast => "A fallen antenna mast lies across a broken walkway.",
            Self::Milepost => "An old district post. The sector number has worn off.",
        }
    }

    fn from_hash(hash: u64) -> Self {
        match (hash >> 17) % 4 {
            0 => Self::Spring,
            1 => Self::StandingStone,
            2 => Self::FallenMast,
            _ => Self::Milepost,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tile {
    pub terrain: Terrain,
    pub landmark: Option<LandmarkKind>,
}

/// Baseline region. This value is not stored; it is rebuilt from [`RegionIdentity`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GeneratedRegion {
    pub identity: RegionIdentity,
    pub tiles: [[Tile; EDGE]; EDGE],
}

impl GeneratedRegion {
    pub fn tile(self, local_x: u8, local_y: u8) -> Option<Tile> {
        let x = usize::from(local_x);
        let y = usize::from(local_y);
        if x >= EDGE || y >= EDGE {
            return None;
        }
        Some(self.tiles[y][x])
    }
}

/// TUIMMO mix v1, the SplitMix64 finalizer used as a pure function.
/// Do not replace this with `std` hashing.
fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn pack_i32(value: i32) -> u64 {
    u64::from(value.cast_unsigned())
}

fn pack_i64(value: i64) -> u64 {
    value.cast_unsigned()
}

fn mix_channel(world_seed: u64, version: u16, x: i64, y: i64, channel: u64) -> u64 {
    let mut h = world_seed ^ 0xA5A5_A5A5_A5A5_A5A5;
    h = mix(h ^ u64::from(version).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    h = mix(h ^ pack_i64(x).wrapping_mul(0xBF58_476D_1CE4_E5B9));
    h = mix(h ^ pack_i64(y).wrapping_mul(0x94D0_49BB_1331_11EB));
    mix(h ^ channel.wrapping_mul(0xD1B5_4A32_D192_ED03))
}

fn unit(hash: u64) -> u16 {
    u16::try_from(hash >> 48).unwrap_or(0)
}

fn smooth_elevation(world_seed: u64, abs_x: i64, abs_y: i64) -> u16 {
    let gx = abs_x.div_euclid(LATTICE);
    let gy = abs_y.div_euclid(LATTICE);
    let fx = u32::try_from(abs_x.rem_euclid(LATTICE)).unwrap_or(0);
    let fy = u32::try_from(abs_y.rem_euclid(LATTICE)).unwrap_or(0);
    let span = u32::try_from(LATTICE).unwrap_or(4);
    let at = |x: i64, y: i64| u32::from(unit(mix_channel(world_seed, GENERATION_V1, x, y, 1)));
    let v00 = at(gx, gy);
    let v10 = at(gx + 1, gy);
    let v01 = at(gx, gy + 1);
    let v11 = at(gx + 1, gy + 1);
    let top = v00 * (span - fx) + v10 * fx;
    let bot = v01 * (span - fx) + v11 * fx;
    let mixed = top * (span - fy) + bot * fy;
    let denom = span * span;
    u16::try_from(mixed / denom).unwrap_or(u16::MAX)
}

/// Version 1 tile at an absolute coordinate. Independent of region boundaries.
pub fn sample_baseline(world_seed: u64, abs_x: i64, abs_y: i64) -> Tile {
    let elev = smooth_elevation(world_seed, abs_x, abs_y);
    let moist = unit(mix_channel(world_seed, GENERATION_V1, abs_x, abs_y, 2));
    let feature = unit(mix_channel(world_seed, GENERATION_V1, abs_x, abs_y, 3));
    let terrain = if elev < V1_WATER_MAX {
        Terrain::Water
    } else if elev >= V1_ROCK_MIN {
        Terrain::Rock
    } else if moist < V1_SCRUB_MAX {
        Terrain::Scrub
    } else if moist >= V1_FOREST_MIN {
        Terrain::Forest
    } else {
        Terrain::Grass
    };
    let landmark = if terrain.traversable() && feature >= V1_LANDMARK_MIN {
        let kind_hash = mix_channel(world_seed, GENERATION_V1, abs_x, abs_y, 4);
        Some(LandmarkKind::from_hash(kind_hash))
    } else {
        None
    };
    Tile { terrain, landmark }
}

pub fn generate_region(identity: RegionIdentity) -> Result<GeneratedRegion, GenerateError> {
    let epoch =
        epoch_spec(identity.epoch_id).ok_or(GenerateError::UnknownEpoch(identity.epoch_id))?;
    if identity.generation_version != GENERATION_V1 {
        return Err(GenerateError::UnknownVersion(identity.generation_version));
    }
    let mut tiles = [[Tile { terrain: Terrain::Grass, landmark: None }; EDGE]; EDGE];
    for (local_y, row) in tiles.iter_mut().enumerate() {
        for (local_x, tile) in row.iter_mut().enumerate() {
            let lx = u8::try_from(local_x).unwrap_or(0);
            let ly = u8::try_from(local_y).unwrap_or(0);
            debug_assert!(absolute_matches_region(identity, lx, ly));
            let size = i64::from(REGION_SIZE);
            let abs_x = i64::from(identity.origin_x) * size + i64::from(lx);
            let abs_y = i64::from(identity.origin_y) * size + i64::from(ly);
            *tile = sample_baseline(epoch.world_seed, abs_x, abs_y);
        }
    }
    Ok(GeneratedRegion { identity, tiles })
}

/// Audit seed stored with a materialized region. Version 1 terrain does not read it back.
pub fn derive_region_seed(identity: RegionIdentity) -> Result<u64, GenerateError> {
    let epoch =
        epoch_spec(identity.epoch_id).ok_or(GenerateError::UnknownEpoch(identity.epoch_id))?;
    let mut h = epoch.world_seed;
    h = mix(h ^ identity.world.tag().wrapping_mul(0x1000_0000_01B3));
    h = mix(h ^ u64::from(identity.epoch_id).wrapping_mul(0x9E37_79B9));
    h = mix(h ^ (pack_i32(identity.origin_x) << 32) ^ pack_i32(identity.origin_y));
    h = mix(h ^ u64::from(identity.generation_version));
    Ok(h)
}

pub fn format_seed(seed: u64) -> String {
    format!("{seed:016x}")
}

pub fn parse_seed(hex: &str) -> Option<u64> {
    u64::from_str_radix(hex, 16).ok()
}

/// Stable fingerprint of a generated region, for regression locks.
pub fn fingerprint(region: &GeneratedRegion) -> u64 {
    let mut h = 0u64;
    for local_y in 0..EDGE {
        for local_x in 0..EDGE {
            let tile = region.tiles[local_y][local_x];
            let code = match tile.terrain {
                Terrain::Grass => 1u64,
                Terrain::Forest => 2,
                Terrain::Scrub => 3,
                Terrain::Water => 4,
                Terrain::Rock => 5,
            };
            let mark = match tile.landmark {
                None => 0,
                Some(LandmarkKind::Spring) => 1,
                Some(LandmarkKind::StandingStone) => 2,
                Some(LandmarkKind::FallenMast) => 3,
                Some(LandmarkKind::Milepost) => 4,
            };
            h = mix(h ^ code ^ (mark << 8) ^ ((local_x as u64) << 16) ^ ((local_y as u64) << 24));
        }
    }
    h
}

/// Bounded FIFO cache of generated baselines. Eviction cannot change results.
pub struct RegionCache {
    cap: usize,
    order: VecDeque<RegionIdentity>,
    map: HashMap<RegionIdentity, GeneratedRegion>,
}

impl RegionCache {
    pub fn new(cap: usize) -> Self {
        Self { cap, order: VecDeque::new(), map: HashMap::new() }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn get_or_generate(
        &mut self,
        identity: RegionIdentity,
    ) -> Result<GeneratedRegion, GenerateError> {
        if let Some(found) = self.map.get(&identity) {
            return Ok(*found);
        }
        let generated = generate_region(identity)?;
        if self.cap == 0 {
            return Ok(generated);
        }
        while self.map.len() >= self.cap {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            } else {
                break;
            }
        }
        self.order.push_back(identity);
        self.map.insert(identity, generated);
        Ok(generated)
    }
}

/// Confirm a birth coordinate still belongs to the region that claims it.
pub fn absolute_matches_region(identity: RegionIdentity, local_x: u8, local_y: u8) -> bool {
    let size = i64::from(REGION_SIZE);
    let abs_x = i64::from(identity.origin_x) * size + i64::from(local_x);
    let abs_y = i64::from(identity.origin_y) * size + i64::from(local_y);
    origins_for_absolute(abs_x, abs_y)
        .map(|(ox, oy, lx, ly)| {
            ox == identity.origin_x && oy == identity.origin_y && lx == local_x && ly == local_y
        })
        .unwrap_or(false)
}

pub fn frontier_identity(world: WorldId, origin_x: i32, origin_y: i32) -> RegionIdentity {
    let epoch = frontier_epoch();
    RegionIdentity {
        world,
        epoch_id: epoch.id,
        origin_x,
        origin_y,
        generation_version: epoch.generation_version,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geography::identity::gate_identity;

    fn at(x: i32, y: i32) -> RegionIdentity {
        RegionIdentity { origin_x: x, origin_y: y, ..gate_identity() }
    }

    #[test]
    fn identical_inputs_match_and_order_does_not_matter() {
        let first = generate_region(at(0, 0)).unwrap();
        let other = generate_region(at(3, -2)).unwrap();
        let again = generate_region(at(0, 0)).unwrap();
        assert_eq!(first, again);
        assert_ne!(first, other);
        assert_ne!(fingerprint(&first), fingerprint(&other));
        assert_ne!(fingerprint(&first), fingerprint(&generate_region(at(0, 1)).unwrap()));

        let mut forward = RegionCache::new(8);
        let mut backward = RegionCache::new(8);
        let a = forward.get_or_generate(at(0, 0)).unwrap();
        let _ = forward.get_or_generate(at(1, 4)).unwrap();
        let _ = backward.get_or_generate(at(1, 4)).unwrap();
        let b = backward.get_or_generate(at(0, 0)).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn golden_fingerprint_of_the_origin_region() {
        let region = generate_region(at(0, 0)).unwrap();
        assert_eq!(fingerprint(&region), 0x8812_F1DB_241C_39A1);
    }

    #[test]
    fn edges_match_the_absolute_field_across_regions() {
        let left = generate_region(at(0, 0)).unwrap();
        let right = generate_region(at(1, 0)).unwrap();
        let below = generate_region(at(0, -1)).unwrap();
        for local in 0..16u8 {
            let east = left.tile(15, local).unwrap();
            let west = right.tile(0, local).unwrap();
            assert_eq!(east, sample_baseline(EPOCH1_WORLD_SEED, 15, i64::from(local)));
            assert_eq!(west, sample_baseline(EPOCH1_WORLD_SEED, 16, i64::from(local)));
            let north_edge = left.tile(local, 0).unwrap();
            let south_neighbor = below.tile(local, 15).unwrap();
            assert_eq!(north_edge, sample_baseline(EPOCH1_WORLD_SEED, i64::from(local), 0));
            assert_eq!(south_neighbor, sample_baseline(EPOCH1_WORLD_SEED, i64::from(local), -1));
        }
    }

    #[test]
    fn negative_region_continues_the_same_field() {
        let region = generate_region(at(-1, 0)).unwrap();
        let tile = region.tile(15, 0).unwrap();
        assert_eq!(tile, sample_baseline(EPOCH1_WORLD_SEED, -1, 0));
        assert!(absolute_matches_region(region.identity, 15, 0));
    }

    #[test]
    fn unknown_version_and_epoch_are_rejected() {
        let mut identity = at(0, 0);
        identity.generation_version = 2;
        assert_eq!(generate_region(identity), Err(GenerateError::UnknownVersion(2)));
        identity.generation_version = GENERATION_V1;
        identity.epoch_id = 9;
        assert_eq!(generate_region(identity), Err(GenerateError::UnknownEpoch(9)));
        assert!(generate_region(at(0, 0)).is_ok());
    }

    #[test]
    fn region_seed_is_stable_and_not_a_terrain_input() {
        let identity = at(4, -7);
        let seed = derive_region_seed(identity).unwrap();
        assert_eq!(seed, derive_region_seed(identity).unwrap());
        assert_ne!(seed, derive_region_seed(at(4, -6)).unwrap());
        let region = generate_region(identity).unwrap();
        let again = generate_region(identity).unwrap();
        assert_eq!(region, again);
    }

    #[test]
    fn cache_eviction_regenerates_the_same_region() {
        let mut cache = RegionCache::new(1);
        let first = cache.get_or_generate(at(0, 0)).unwrap();
        let _ = cache.get_or_generate(at(2, 2)).unwrap();
        assert_eq!(cache.len(), 1);
        let restored = cache.get_or_generate(at(0, 0)).unwrap();
        assert_eq!(first, restored);
    }

    #[test]
    fn frontier_is_the_first_epoch() {
        assert_eq!(frontier_epoch().id, 1);
        assert_eq!(frontier_epoch().generation_version, GENERATION_V1);
        assert_eq!(frontier_identity(WorldId::Undernet, 0, 0).origin_x, 0);
    }

    #[test]
    fn origin_region_has_varied_terrain() {
        let region = generate_region(at(0, 0)).unwrap();
        let mut saw_water = false;
        let mut saw_walkable = false;
        let mut saw_blocked = false;
        for row in region.tiles {
            for tile in row {
                if tile.terrain == Terrain::Water {
                    saw_water = true;
                }
                if tile.terrain.traversable() {
                    saw_walkable = true;
                } else {
                    saw_blocked = true;
                }
            }
        }
        assert!(saw_water && saw_walkable && saw_blocked);
    }
}
