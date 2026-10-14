//! Deterministic clue placement from world seed and region identity.

use super::{ClueCatalog, ClueDef, ClueSite, SiteRule, WildSiteFilter};
use crate::geography::gate_position;
use crate::geography::{
    density_band, district_at, frontier_epoch, generate_region, thread_placement_hash, DensityBand,
    DistrictKind, GenerateError, RegionIdentity, Terrain, WildernessPos, WorldId, REGION_SIZE,
};
use crate::subway::{self, ash_entrance_pos, ward_entrance_pos};

const REGION_SCAN: i32 = 12;

pub use crate::geography::thread_placement_hash as placement_hash;

fn band_matches(filter: DensityBand, band: DensityBand) -> bool {
    filter == band
}

fn district_matches(filter: Option<DistrictKind>, district: DistrictKind) -> bool {
    filter.is_none_or(|want| want == district)
}

fn tile_matches_filter(abs_x: i64, abs_y: i64, filter: &WildSiteFilter) -> bool {
    let band = density_band(abs_x, abs_y);
    let district = district_at(abs_x, abs_y);
    band_matches(filter.band, band) && district_matches(filter.district, district)
}

fn tile_rank(
    world_seed: u64,
    thread_id: &str,
    clue_id: &str,
    origin_x: i32,
    origin_y: i32,
    local_x: u8,
    local_y: u8,
) -> u64 {
    let region_hash = thread_placement_hash(world_seed, thread_id, clue_id, origin_x, origin_y);
    region_hash ^ (u64::from(local_x) << 24) ^ (u64::from(local_y) << 32)
}

fn resolve_wild_site(
    world_seed: u64,
    thread_id: &str,
    clue: &ClueDef,
    filter: &WildSiteFilter,
) -> Result<Option<ClueSite>, GenerateError> {
    let mut best: Option<(WildernessPos, u64)> = None;
    for origin_y in -REGION_SCAN..=REGION_SCAN {
        for origin_x in -REGION_SCAN..=REGION_SCAN {
            let identity = RegionIdentity {
                world: WorldId::Undernet,
                epoch_id: frontier_epoch().id,
                origin_x,
                origin_y,
                generation_version: frontier_epoch().generation_version,
            };
            let region = generate_region(identity)?;
            for local_y in 0..REGION_SIZE {
                for local_x in 0..REGION_SIZE {
                    let lx = u8::try_from(local_x).unwrap_or(0);
                    let ly = u8::try_from(local_y).unwrap_or(0);
                    let abs_x = i64::from(origin_x) * i64::from(REGION_SIZE) + i64::from(lx);
                    let abs_y = i64::from(origin_y) * i64::from(REGION_SIZE) + i64::from(ly);
                    if !tile_matches_filter(abs_x, abs_y, filter) {
                        continue;
                    }
                    let pos = WildernessPos { identity, local_x: lx, local_y: ly };
                    if pos == gate_position() || subway::entrance_at(pos).is_some() {
                        continue;
                    }
                    let Some(tile) = region.tile(lx, ly) else {
                        continue;
                    };
                    if matches!(tile.terrain, Terrain::Water | Terrain::Rock) {
                        continue;
                    }
                    let rank =
                        tile_rank(world_seed, thread_id, &clue.id, origin_x, origin_y, lx, ly);
                    if best.is_none_or(|(_, best_rank)| rank > best_rank) {
                        best = Some((pos, rank));
                    }
                }
            }
        }
    }
    Ok(best.map(|(pos, _)| ClueSite::Wild(pos)))
}

fn static_site(location_id: &str) -> ClueSite {
    ClueSite::Static(location_id.to_string())
}

fn metro_site(name: &str) -> Option<ClueSite> {
    let pos = match name {
        "ash" => ash_entrance_pos(),
        "ward" => ward_entrance_pos(),
        _ => return None,
    };
    Some(ClueSite::Wild(pos))
}

/// Resolve every clue site once per catalog load (pure function of epoch seed).
pub fn resolve_all_sites(
    catalog: &ClueCatalog,
    world_seed: u64,
) -> Result<Vec<(String, ClueSite)>, GenerateError> {
    let mut out = Vec::new();
    for thread in &catalog.threads {
        for clue in &thread.clues {
            let site = match &clue.site {
                SiteRule::Wild(filter) => resolve_wild_site(world_seed, &thread.id, clue, filter)?
                    .ok_or(GenerateError::UnknownVersion(0))?,
                SiteRule::Static { location } => static_site(location),
                SiteRule::MetroEntrance { name } => {
                    metro_site(name).ok_or(GenerateError::UnknownEpoch(0))?
                }
            };
            out.push((clue.id.clone(), site));
        }
    }
    Ok(out)
}

/// Build the placement map used at runtime. Panics in debug if resolution fails for shipped data.
pub fn build_placement_map(
    catalog: &ClueCatalog,
    world_seed: u64,
) -> std::collections::HashMap<String, ClueSite> {
    match resolve_all_sites(catalog, world_seed) {
        Ok(rows) => rows.into_iter().collect(),
        Err(err) => {
            panic!("thread placement failed for shipped catalog: {err}");
        }
    }
}

pub fn clue_at_wild_pos(
    map: &std::collections::HashMap<String, ClueSite>,
    pos: WildernessPos,
) -> Option<String> {
    map.iter()
        .find(|(_, site)| matches!(site, ClueSite::Wild(p) if *p == pos))
        .map(|(id, _)| id.clone())
}

pub fn clue_at_static(
    map: &std::collections::HashMap<String, ClueSite>,
    location: &str,
) -> Option<String> {
    map.iter()
        .find(|(_, site)| matches!(site, ClueSite::Static(loc) if loc == location))
        .map(|(id, _)| id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geography::gate_position;
    use crate::threads::load::load_threads_from_str;

    const FIXTURE: &str = include_str!("../../data/threads.toml");

    #[test]
    fn wild_site_rules_match_at_least_one_region() {
        let catalog = load_threads_from_str(FIXTURE).unwrap();
        let seed = frontier_epoch().world_seed;
        for thread in &catalog.threads {
            for clue in &thread.clues {
                if let SiteRule::Wild(filter) = &clue.site {
                    let site = resolve_wild_site(seed, &thread.id, clue, filter).unwrap();
                    assert!(site.is_some(), "no site for clue {}", clue.id);
                }
            }
        }
    }

    #[test]
    fn same_seed_same_placement_different_seeds_differ() {
        let catalog = load_threads_from_str(FIXTURE).unwrap();
        let a = build_placement_map(&catalog, 0x00C0_FFEE_5EED_0001);
        let b = build_placement_map(&catalog, 0x00C0_FFEE_5EED_0001);
        assert_eq!(a, b);
        let c = build_placement_map(&catalog, 0x00C0_FFEE_5EED_0002);
        assert_ne!(a, c);
    }

    #[test]
    fn clues_never_sit_on_the_hub_gate_tile() {
        let catalog = load_threads_from_str(FIXTURE).unwrap();
        let map = build_placement_map(&catalog, frontier_epoch().world_seed);
        for site in map.values() {
            if let ClueSite::Wild(pos) = site {
                assert_ne!(*pos, gate_position());
            }
        }
    }
}
