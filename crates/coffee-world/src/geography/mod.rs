//! Procedural geography for the Undernet's outskirts.
//!
//! Generated terrain is a pure function of epoch, birth coordinates, and a
//! generation version. Player changes live in [`GeographyState`] and are stored
//! apart from that baseline. Nothing in this module knows about SSH or the TUI.

mod generate;
mod identity;
mod records;
mod travel;

pub use generate::{
    derive_region_seed, epoch_spec, fingerprint, format_seed, frontier_epoch, frontier_identity,
    generate_region, parse_seed, sample_baseline, EpochSpec, GenerateError, GeneratedRegion,
    LandmarkKind, RegionCache, Terrain, Tile, EPOCH1_WORLD_SEED, EPOCH_1, GENERATION_V1,
};
pub use identity::{
    gate_identity, gate_position, origins_for_absolute, shift, IdentityError, Place,
    RegionCoordKey, RegionIdentity, WildernessPos, WorldId, CITY_KEYWORD, GATE_LOCAL_X,
    GATE_LOCAL_Y, OUTSKIRTS_FROM, OUTSKIRTS_KEYWORD, REGION_SIZE, ROOF_LOCATION,
};
pub use records::{
    Discovery, GeographyState, MaterializedRegion, Structure, TerrainMod, WorldConnection,
    ACCESS_FRIENDS, ACCESS_PRIVATE, ACCESS_PUBLIC, CABIN_KIND, MARKER_KIND, SHARE_COMMUNAL,
    SHARE_PRIVATE, SHARE_SHARED, SIGN_KIND, TRAIL_KIND, WORKSHOP_KIND,
};
pub use travel::{
    describe_tile, effective_tile, is_reserved_word, map_symbol_for_structures, marker_sights,
    step_origin, validate_marker_name, Cardinal, EffectiveTile, MarkerSight, TileScene,
};
