//! Stable world, region, and tile identity.
//!
//! A region is not "whatever is currently drawn at these coordinates." Birth
//! coordinates and the generation version stay on the region forever. A separate
//! placement column can later change where a region is labelled on an atlas
//! without rewriting that identity or the place keys players already occupy.

use std::fmt;

/// Worlds known to this server. New worlds are an explicit enum change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorldId {
    Undernet,
}

impl WorldId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Undernet => "undernet",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "undernet" => Some(Self::Undernet),
            _ => None,
        }
    }

    /// Stable mixer tag. Not a display coordinate.
    pub const fn tag(self) -> u64 {
        match self {
            Self::Undernet => 1,
        }
    }
}

impl fmt::Display for WorldId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Tiles along one edge of a region. Version 1 fixes this at 16.
pub const REGION_SIZE: i32 = 16;

/// The hand-authored room that opens onto the procedural outskirts.
pub const ROOF_LOCATION: &str = "roof-array";

pub const OUTSKIRTS_KEYWORD: &str = "outskirts";
pub const CITY_KEYWORD: &str = "city";
pub const OUTSKIRTS_FROM: &str = ROOF_LOCATION;

/// Local tile where the Undernet fence breaks. Forced walkable by the seam overlay.
pub const GATE_LOCAL_X: u8 = 8;
pub const GATE_LOCAL_Y: u8 = 8;

/// Birth coordinates plus the generator that owns them.
///
/// `generation_version` is part of the identity so a later algorithm cannot
/// silently reuse a version number that already produced persistent regions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RegionIdentity {
    pub world: WorldId,
    pub epoch_id: u32,
    pub origin_x: i32,
    pub origin_y: i32,
    pub generation_version: u16,
}

impl RegionIdentity {
    pub fn coord_key(self) -> RegionCoordKey {
        RegionCoordKey {
            world: self.world,
            epoch_id: self.epoch_id,
            origin_x: self.origin_x,
            origin_y: self.origin_y,
        }
    }
}

/// Lookup key for "which region occupies this birth cell in this epoch."
///
/// One epoch has at most one region per birth coordinate. The generation
/// version lives on the region record, not in this key, so a second algorithm
/// cannot occupy the same cell by accident.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RegionCoordKey {
    pub world: WorldId,
    pub epoch_id: u32,
    pub origin_x: i32,
    pub origin_y: i32,
}

/// A tile inside a region. Local coordinates are `0..REGION_SIZE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WildernessPos {
    pub identity: RegionIdentity,
    pub local_x: u8,
    pub local_y: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdentityError {
    LocalOutOfRange,
    RegionOutOfRange,
    BadPlaceKey,
}

impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LocalOutOfRange => write!(f, "tile is outside the region"),
            Self::RegionOutOfRange => write!(f, "region coordinate is out of range"),
            Self::BadPlaceKey => write!(f, "place key is not valid"),
        }
    }
}

pub fn gate_identity() -> RegionIdentity {
    RegionIdentity {
        world: WorldId::Undernet,
        epoch_id: 1,
        origin_x: 0,
        origin_y: 0,
        generation_version: 1,
    }
}

pub fn gate_position() -> WildernessPos {
    WildernessPos { identity: gate_identity(), local_x: GATE_LOCAL_X, local_y: GATE_LOCAL_Y }
}

impl WildernessPos {
    pub fn is_gate(self) -> bool {
        self.identity == gate_identity()
            && self.local_x == GATE_LOCAL_X
            && self.local_y == GATE_LOCAL_Y
    }

    /// Absolute tile in the epoch's generation grid.
    ///
    /// North is `+Y` and east is `+X`. Region `(0, 0)` covers absolute tiles
    /// `0..16` on both axes. Region `(-1, 0)` covers absolute `x` of `-16..0`.
    pub fn absolute(self) -> (i64, i64) {
        let size = i64::from(REGION_SIZE);
        let x = i64::from(self.identity.origin_x) * size + i64::from(self.local_x);
        let y = i64::from(self.identity.origin_y) * size + i64::from(self.local_y);
        (x, y)
    }

    /// Canonical place key stored in `operators.location_id` and connections.
    ///
    /// `w:<world>:<epoch>:<origin_x>:<origin_y>:<version>:<local_x>:<local_y>`
    ///
    /// Static Undernet ids never contain `:`, so they cannot collide with this form.
    pub fn key(self) -> String {
        format!(
            "w:{}:{}:{}:{}:{}:{}:{}",
            self.identity.world,
            self.identity.epoch_id,
            self.identity.origin_x,
            self.identity.origin_y,
            self.identity.generation_version,
            self.local_x,
            self.local_y
        )
    }

    pub fn parse(raw: &str) -> Option<Self> {
        let mut parts = raw.split(':');
        let tag = parts.next()?;
        if tag != "w" {
            return None;
        }
        let world = WorldId::parse(parts.next()?)?;
        let epoch_id: u32 = parts.next()?.parse().ok()?;
        let origin_x: i32 = parts.next()?.parse().ok()?;
        let origin_y: i32 = parts.next()?.parse().ok()?;
        let generation_version: u16 = parts.next()?.parse().ok()?;
        let local_x: u8 = parts.next()?.parse().ok()?;
        let local_y: u8 = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        if u32::from(local_x) >= u32::try_from(REGION_SIZE).unwrap_or(16)
            || u32::from(local_y) >= u32::try_from(REGION_SIZE).unwrap_or(16)
        {
            return None;
        }
        Some(Self {
            identity: RegionIdentity { world, epoch_id, origin_x, origin_y, generation_version },
            local_x,
            local_y,
        })
    }
}

/// Where a place sits in the simulation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Place {
    Static(String),
    Wild(WildernessPos),
}

impl Place {
    pub fn parse(raw: &str) -> Self {
        if let Some(pos) = WildernessPos::parse(raw) {
            Self::Wild(pos)
        } else {
            Self::Static(raw.to_string())
        }
    }

    pub fn key(&self) -> String {
        match self {
            Self::Static(id) => id.clone(),
            Self::Wild(pos) => pos.key(),
        }
    }
}

/// Split an absolute tile into birth coordinates and a local tile.
pub fn origins_for_absolute(abs_x: i64, abs_y: i64) -> Result<(i32, i32, u8, u8), IdentityError> {
    let size = i64::from(REGION_SIZE);
    let origin_x =
        i32::try_from(abs_x.div_euclid(size)).map_err(|_| IdentityError::RegionOutOfRange)?;
    let origin_y =
        i32::try_from(abs_y.div_euclid(size)).map_err(|_| IdentityError::RegionOutOfRange)?;
    let local_x =
        u8::try_from(abs_x.rem_euclid(size)).map_err(|_| IdentityError::LocalOutOfRange)?;
    let local_y =
        u8::try_from(abs_y.rem_euclid(size)).map_err(|_| IdentityError::LocalOutOfRange)?;
    Ok((origin_x, origin_y, local_x, local_y))
}

/// Move by whole tiles on the generation grid. `dx` east, `dy` north.
pub fn shift(pos: WildernessPos, dx: i32, dy: i32) -> Result<(i32, i32, u8, u8), IdentityError> {
    let (x, y) = pos.absolute();
    let nx = x.checked_add(i64::from(dx)).ok_or(IdentityError::RegionOutOfRange)?;
    let ny = y.checked_add(i64::from(dy)).ok_or(IdentityError::RegionOutOfRange)?;
    origins_for_absolute(nx, ny)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn place_keys_round_trip_including_negative_regions() {
        let pos = WildernessPos {
            identity: RegionIdentity {
                world: WorldId::Undernet,
                epoch_id: 1,
                origin_x: -2,
                origin_y: 4,
                generation_version: 1,
            },
            local_x: 15,
            local_y: 0,
        };
        let key = pos.key();
        assert_eq!(key, "w:undernet:1:-2:4:1:15:0");
        assert_eq!(WildernessPos::parse(&key), Some(pos));
        assert!(WildernessPos::parse("coffee-shop").is_none());
        assert!(WildernessPos::parse("w:undernet:1:0:0:1:16:0").is_none());
        assert!(WildernessPos::parse("w:other:1:0:0:1:0:0").is_none());
    }

    #[test]
    fn adjacent_regions_share_an_absolute_edge() {
        let east_edge = WildernessPos { identity: gate_identity(), local_x: 15, local_y: 3 };
        let (ox, oy, lx, ly) = shift(east_edge, 1, 0).unwrap();
        assert_eq!((ox, oy, lx, ly), (1, 0, 0, 3));
        assert_eq!(east_edge.absolute().0 + 1, 16);

        let west_edge = WildernessPos { identity: gate_identity(), local_x: 0, local_y: 0 };
        let (ox, oy, lx, ly) = shift(west_edge, -1, 0).unwrap();
        assert_eq!((ox, oy, lx, ly), (-1, 0, 15, 0));
        let moved = WildernessPos {
            identity: RegionIdentity { origin_x: ox, origin_y: oy, ..gate_identity() },
            local_x: lx,
            local_y: ly,
        };
        assert_eq!(moved.absolute(), (-1, 0));
    }

    #[test]
    fn region_past_i32_is_rejected() {
        let edge = WildernessPos {
            identity: RegionIdentity { origin_x: i32::MAX, origin_y: 0, ..gate_identity() },
            local_x: 15,
            local_y: 0,
        };
        assert_eq!(shift(edge, 1, 0), Err(IdentityError::RegionOutOfRange));
    }

    #[test]
    fn version_is_part_of_identity() {
        let v1 = gate_identity();
        let v2 = RegionIdentity { generation_version: 2, ..v1 };
        assert_ne!(v1, v2);
        assert_eq!(v1.coord_key(), v2.coord_key());
    }

    #[test]
    fn static_ids_do_not_parse_as_wilderness() {
        assert!(matches!(Place::parse("coffee-shop"), Place::Static(_)));
        assert!(matches!(Place::parse(&gate_position().key()), Place::Wild(_)));
    }
}
