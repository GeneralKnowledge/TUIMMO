//! Procedural geography for the Undernet's Squalor (abandoned urban sectors).
//!
//! Generated terrain is a pure function of epoch, birth coordinates, and a
//! generation version. Player changes live in [`GeographyState`] and are stored
//! apart from that baseline. Nothing in this module knows about SSH or the TUI.

mod atmosphere;
mod density;
mod field;
mod generate;
mod identity;
mod records;
mod route;
mod travel;
mod ugc;

#[cfg(test)]
mod explore;
#[cfg(test)]
mod remote;

pub use atmosphere::{atmosphere_at, exit_clarity_note, fogged_map_symbol, Atmosphere};
pub use density::{
    density_band, district_at, story_aliases, story_finding, story_sight, trace_aliases,
    BlockPattern, DensityBand, DistrictKind, StorySite, TraceKind, UrbanContext,
    CLINIC_CORRIDOR_ID, CLINIC_CORRIDOR_LOCAL_X, CLINIC_CORRIDOR_LOCAL_Y, CLINIC_DIRECTORY_ID,
    CLINIC_DIRECTORY_LOCAL_X, CLINIC_DIRECTORY_LOCAL_Y, CLINIC_ENTRANCE_LOCAL_X,
    CLINIC_ENTRANCE_LOCAL_Y, CLINIC_NOTICE_ID, REMOTE_ACCEPTANCE_ORIGIN_X,
    REMOTE_ACCEPTANCE_ORIGIN_Y,
};
pub use field::{
    closure_inspect_text, corroboration_inspect_line, corroboration_record_line, delisted_floor,
    delisted_panel_waiting, delisted_terminal_waiting, describe_place, format_entry,
    format_overview, format_recent, is_survey_tile, landmark_sight, panel_after_log, roof_scratch,
    stacks_after_log, survey_aliases, survey_closure, survey_finding, survey_look_line,
    survey_position, survey_sight, torn_page_text, AuthoredFinding, JournalLine, Sight,
    DELISTED_FLOOR_ID, SURVEY_CLOSURE_ID, SURVEY_LOCAL_X, SURVEY_LOCAL_Y, SURVEY_MARK_FLAG,
    SURVEY_MARK_ID, SURVEY_SCRATCH_ID,
};
pub use generate::{
    derive_region_seed, epoch_spec, fingerprint, format_seed, frontier_epoch, frontier_identity,
    generate_region, parse_seed, sample_baseline, thread_placement_hash, EpochSpec, GenerateError,
    GeneratedRegion, LandmarkKind, RegionCache, Terrain, Tile, EPOCH1_WORLD_SEED, EPOCH_1,
    GENERATION_V1,
};
pub use identity::{
    gate_identity, gate_position, origins_for_absolute, shift, IdentityError, Place,
    RegionCoordKey, RegionIdentity, WildernessPos, WorldId, CITY_KEYWORD, GATE_LOCAL_X,
    GATE_LOCAL_Y, OUTSKIRTS_FROM, OUTSKIRTS_KEYWORD, REGION_SIZE, ROOF_LOCATION,
};
pub use records::{
    Discovery, GeographyState, MaterializedRegion, Structure, TerrainMod, WorldConnection,
    ACCESS_FRIENDS, ACCESS_PRIVATE, ACCESS_PUBLIC, CABIN_KIND, ENTERED_KIND, FIELD_KIND,
    CHALK_KIND, MARKER_KIND, SHARE_COMMUNAL, SHARE_PRIVATE, SHARE_SHARED, SIGN_KIND, TRAIL_KIND,
    WORKSHOP_KIND,
};
pub use ugc::{
    age_suffix, chalk_ttl_ms, count_operator_structures_in_region, filter_traces_for_tile,
    permanent_traces_allowed, reject_permanent_trace_reason, structure_is_active,
    validate_chalk_text, band_label as ugc_band_label, chebyshev_from_gate,
    CHALK_FRINGE_TTL_MS, CHALK_NEAR_HUB_TTL_MS, FRINGE_MAX_CHEB, MAX_SIGNS_PER_OPERATOR_REGION,
    MAX_TRAILS_PER_OPERATOR_REGION, NEAR_HUB_MAX_CHEB,
};
pub(crate) use route::{cross_region_hint, distant_cell_hint, local_path, LocalPath};
pub use travel::{
    describe_tile, effective_tile, is_reserved_word, map_symbol_for_structures, marker_sights,
    step_origin, validate_marker_name, Cardinal, EffectiveTile, MarkerSight, TileLookOptions,
    TileScene,
};
