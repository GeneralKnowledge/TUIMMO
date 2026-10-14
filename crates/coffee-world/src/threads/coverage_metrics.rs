//! Offline measurement of inspectable coverage and density by hub distance.

use serde::{Deserialize, Serialize};

use crate::geography::{
    frontier_epoch, gate_position, generate_region, origins_for_absolute, RegionIdentity,
    WildernessPos, WorldId, GATE_LOCAL_X, GATE_LOCAL_Y, REGION_SIZE,
};

use super::clue_class::{is_coverage_flavor, is_thread_clue};
use super::coverage_params::CoverageParams;
use super::template_load::load_default_templates;
use super::{procedural::generate_for_region_with_params, ClueSite};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HubDistanceBucket {
    Tiles0To100,
    Tiles100To500,
    Tiles500To1000,
    Tiles1000Plus,
}

impl HubDistanceBucket {
    pub fn from_chebyshev_from_gate(dist: i64) -> Self {
        if dist <= 100 {
            Self::Tiles0To100
        } else if dist <= 500 {
            Self::Tiles100To500
        } else if dist <= 1000 {
            Self::Tiles500To1000
        } else {
            Self::Tiles1000Plus
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Tiles0To100 => "0-100",
            Self::Tiles100To500 => "100-500",
            Self::Tiles500To1000 => "500-1000",
            Self::Tiles1000Plus => "1000+",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GapBucketStats {
    pub bucket: HubDistanceBucket,
    pub sample_tiles: u32,
    pub p50: f64,
    pub p95: f64,
    pub max: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DensityBucketStats {
    pub bucket: HubDistanceBucket,
    pub features_per_1000_walkable: f64,
    pub walkable_tiles: u32,
    pub feature_sites: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThreadCoverageReport {
    pub coverage_params: CoverageParams,
    pub world_seed: u64,
    pub gap_by_bucket: Vec<GapBucketStats>,
    pub density_by_bucket: Vec<DensityBucketStats>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlacementBucketStats {
    pub bucket: HubDistanceBucket,
    pub walkable_tiles: u32,
    /// All inspectable sites (coverage flavor + thread clues).
    pub inspectable_features: u32,
    /// Authored + procedural thread clues (excludes coverage flavor).
    pub thread_clue_sites: u32,
    pub thread_clues_per_1000_walkable: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlacementInventoryReport {
    pub coverage_params: CoverageParams,
    pub world_seed: u64,
    pub by_bucket: Vec<PlacementBucketStats>,
}

pub fn chebyshev_from_gate(abs_x: i64, abs_y: i64) -> i64 {
    let gx = i64::from(GATE_LOCAL_X);
    let gy = i64::from(GATE_LOCAL_Y);
    (abs_x - gx).abs().max((abs_y - gy).abs())
}

/// Deterministic sample regions at approximate hub distances for measurement.
pub fn sample_regions_for_measurement() -> Vec<RegionIdentity> {
    let epoch = frontier_epoch();
    let base = RegionIdentity {
        world: WorldId::Undernet,
        epoch_id: epoch.id,
        origin_x: 0,
        origin_y: 0,
        generation_version: epoch.generation_version,
    };
    let targets: [(i64, i64); 12] = [
        (8, 8),
        (50, 8),
        (100, 8),
        (8, 100),
        (250, 8),
        (250, 250),
        (500, 8),
        (750, 8),
        (750, 750),
        (1500, 8),
        (1500, 1500),
        (1000, 500),
    ];
    let mut out = Vec::new();
    out.push(base);
    for (ax, ay) in targets {
        let Ok((origin_x, origin_y, _, _)) = origins_for_absolute(ax, ay) else {
            continue;
        };
        out.push(RegionIdentity { origin_x, origin_y, ..base });
    }
    out.sort_by_key(|id| (id.origin_x, id.origin_y));
    out.dedup_by_key(|id| (id.origin_x, id.origin_y));
    out
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    assert!(!sorted.is_empty());
    let rank = (p * sorted.len() as f64).ceil() as usize;
    let idx = rank.saturating_sub(1).min(sorted.len() - 1);
    sorted[idx]
}

fn nearest_feature_gap(tile: WildernessPos, features: &[WildernessPos]) -> i64 {
    features
        .iter()
        .map(|f| {
            let dx = (i32::from(tile.local_x) - i32::from(f.local_x)).abs();
            let dy = (i32::from(tile.local_y) - i32::from(f.local_y)).abs();
            i64::from(dx.max(dy))
        })
        .min()
        .unwrap_or(i64::MAX)
}

/// All inspectable procedural sites in one region (coverage + template clues).
pub fn procedural_feature_sites(
    world_seed: u64,
    identity: RegionIdentity,
    params: CoverageParams,
) -> Result<Vec<WildernessPos>, crate::geography::GenerateError> {
    let templates = load_default_templates();
    let region = generate_for_region_with_params(world_seed, identity, &templates, params)?;
    Ok(region
        .sites
        .values()
        .filter_map(|site| match site {
            super::ClueSite::Wild(p) => Some(*p),
            super::ClueSite::Static(_) => None,
        })
        .collect())
}

pub fn measure_placement_inventory(
    world_seed: u64,
    params: CoverageParams,
) -> PlacementInventoryReport {
    let templates = load_default_templates();
    let mut inspectable: std::collections::HashMap<HubDistanceBucket, u32> =
        std::collections::HashMap::new();
    let mut thread_sites: std::collections::HashMap<HubDistanceBucket, u32> =
        std::collections::HashMap::new();
    let mut walkable: std::collections::HashMap<HubDistanceBucket, u32> =
        std::collections::HashMap::new();

    for identity in sample_regions_for_measurement() {
        let Ok(region) = generate_region(identity) else {
            continue;
        };
        for ly in 0..REGION_SIZE {
            for lx in 0..REGION_SIZE {
                let lx = u8::try_from(lx).expect("REGION_SIZE fits in u8");
                let ly = u8::try_from(ly).expect("REGION_SIZE fits in u8");
                let Some(tile) = region.tile(lx, ly) else {
                    continue;
                };
                if !tile.terrain.traversable() {
                    continue;
                }
                let pos = WildernessPos { identity, local_x: lx, local_y: ly };
                if pos == gate_position() {
                    continue;
                }
                let (ax, ay) = pos.absolute();
                let bucket =
                    HubDistanceBucket::from_chebyshev_from_gate(chebyshev_from_gate(ax, ay));
                *walkable.entry(bucket).or_default() += 1;
            }
        }
        let Ok(proc) = generate_for_region_with_params(world_seed, identity, &templates, params)
        else {
            continue;
        };
        for (clue_id, site) in &proc.sites {
            let ClueSite::Wild(pos) = site else {
                continue;
            };
            let Some(clue) = proc.clues.get(clue_id) else {
                continue;
            };
            let (ax, ay) = pos.absolute();
            let bucket = HubDistanceBucket::from_chebyshev_from_gate(chebyshev_from_gate(ax, ay));
            *inspectable.entry(bucket).or_default() += 1;
            if is_thread_clue(clue) {
                *thread_sites.entry(bucket).or_default() += 1;
            } else if is_coverage_flavor(clue) {
                let _ = clue;
            }
        }
    }

    let by_bucket = [
        HubDistanceBucket::Tiles0To100,
        HubDistanceBucket::Tiles100To500,
        HubDistanceBucket::Tiles500To1000,
        HubDistanceBucket::Tiles1000Plus,
    ]
    .into_iter()
    .map(|bucket| {
        let walk = *walkable.get(&bucket).unwrap_or(&0);
        let features = *inspectable.get(&bucket).unwrap_or(&0);
        let thread = *thread_sites.get(&bucket).unwrap_or(&0);
        let thread_clues_per_1000_walkable =
            if walk == 0 { 0.0 } else { f64::from(thread) / f64::from(walk) * 1000.0 };
        PlacementBucketStats {
            bucket,
            walkable_tiles: walk,
            inspectable_features: features,
            thread_clue_sites: thread,
            thread_clues_per_1000_walkable,
        }
    })
    .collect();

    PlacementInventoryReport { coverage_params: params, world_seed, by_bucket }
}

pub fn measure_coverage_report(world_seed: u64, params: CoverageParams) -> ThreadCoverageReport {
    let templates = load_default_templates();
    let mut gaps: std::collections::HashMap<HubDistanceBucket, Vec<f64>> =
        std::collections::HashMap::new();
    let mut density_num: std::collections::HashMap<HubDistanceBucket, u32> =
        std::collections::HashMap::new();
    let mut density_den: std::collections::HashMap<HubDistanceBucket, u32> =
        std::collections::HashMap::new();

    for identity in sample_regions_for_measurement() {
        let Ok(region) = generate_region(identity) else {
            continue;
        };
        let Ok(features) = procedural_feature_sites(world_seed, identity, params) else {
            continue;
        };
        let feature_locals: Vec<WildernessPos> = features;
        for ly in 0..REGION_SIZE {
            for lx in 0..REGION_SIZE {
                let lx = u8::try_from(lx).expect("REGION_SIZE fits in u8");
                let ly = u8::try_from(ly).expect("REGION_SIZE fits in u8");
                let Some(tile) = region.tile(lx, ly) else {
                    continue;
                };
                if !tile.terrain.traversable() {
                    continue;
                }
                let pos = WildernessPos { identity, local_x: lx, local_y: ly };
                if pos == gate_position() {
                    continue;
                }
                let (ax, ay) = pos.absolute();
                let bucket =
                    HubDistanceBucket::from_chebyshev_from_gate(chebyshev_from_gate(ax, ay));
                let gap = nearest_feature_gap(pos, &feature_locals);
                if gap != i64::MAX {
                    gaps.entry(bucket).or_default().push(gap as f64);
                }
                *density_den.entry(bucket).or_default() += 1;
            }
        }
        for feat in &feature_locals {
            let (ax, ay) = feat.absolute();
            let bucket = HubDistanceBucket::from_chebyshev_from_gate(chebyshev_from_gate(ax, ay));
            *density_num.entry(bucket).or_default() += 1;
        }
    }

    let gap_by_bucket = [
        HubDistanceBucket::Tiles0To100,
        HubDistanceBucket::Tiles100To500,
        HubDistanceBucket::Tiles500To1000,
        HubDistanceBucket::Tiles1000Plus,
    ]
    .into_iter()
    .map(|bucket| {
        let mut samples = gaps.get(&bucket).cloned().unwrap_or_default();
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        if samples.is_empty() {
            GapBucketStats { bucket, sample_tiles: 0, p50: 0.0, p95: 0.0, max: 0.0 }
        } else {
            GapBucketStats {
                bucket,
                sample_tiles: u32::try_from(samples.len()).unwrap_or(u32::MAX),
                p50: percentile(&samples, 0.5),
                p95: percentile(&samples, 0.95),
                max: samples.last().copied().unwrap_or(0.0),
            }
        }
    })
    .collect();

    let density_by_bucket = [
        HubDistanceBucket::Tiles0To100,
        HubDistanceBucket::Tiles100To500,
        HubDistanceBucket::Tiles500To1000,
        HubDistanceBucket::Tiles1000Plus,
    ]
    .into_iter()
    .map(|bucket| {
        let walkable = *density_den.get(&bucket).unwrap_or(&0);
        let features = *density_num.get(&bucket).unwrap_or(&0);
        let features_per_1000_walkable =
            if walkable == 0 { 0.0 } else { f64::from(features) / f64::from(walkable) * 1000.0 };
        DensityBucketStats {
            bucket,
            features_per_1000_walkable,
            walkable_tiles: walkable,
            feature_sites: features,
        }
    })
    .collect();

    let _ = templates;
    ThreadCoverageReport { coverage_params: params, world_seed, gap_by_bucket, density_by_bucket }
}

/// Returns false if any sampled walkable tile exceeds `params.max_gap` from nearest feature.
pub fn max_gap_guarantee_holds(
    world_seed: u64,
    params: CoverageParams,
    hub_targets: &[i64],
) -> bool {
    let epoch = frontier_epoch();
    let base = RegionIdentity {
        world: WorldId::Undernet,
        epoch_id: epoch.id,
        origin_x: 0,
        origin_y: 0,
        generation_version: epoch.generation_version,
    };
    for &target in hub_targets {
        let ax = i64::from(GATE_LOCAL_X) + target;
        let ay = i64::from(GATE_LOCAL_Y);
        let Ok((origin_x, origin_y, _, _)) = origins_for_absolute(ax, ay) else {
            return false;
        };
        let identity = RegionIdentity { origin_x, origin_y, ..base };
        let Ok(region) = generate_region(identity) else {
            return false;
        };
        let Ok(features) = procedural_feature_sites(world_seed, identity, params) else {
            return false;
        };
        let gap_limit = i64::from(params.max_gap);
        for ly in 0..REGION_SIZE {
            for lx in 0..REGION_SIZE {
                let lx = u8::try_from(lx).expect("REGION_SIZE fits in u8");
                let ly = u8::try_from(ly).expect("REGION_SIZE fits in u8");
                let Some(tile) = region.tile(lx, ly) else {
                    continue;
                };
                if !tile.terrain.traversable() {
                    continue;
                }
                let pos = WildernessPos { identity, local_x: lx, local_y: ly };
                if pos == gate_position() || crate::subway::entrance_at(pos).is_some() {
                    continue;
                }
                if nearest_feature_gap(pos, &features) > gap_limit {
                    return false;
                }
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geography::frontier_epoch;

    #[test]
    fn default_params_gap_report_is_non_empty() {
        let seed = frontier_epoch().world_seed;
        let report = measure_coverage_report(seed, CoverageParams::default());
        assert!(report.gap_by_bucket.iter().any(|b| b.sample_tiles > 0));
    }

    #[test]
    fn max_gap_guarantee_at_hub_distances_for_all_experiment_settings() {
        let seed = frontier_epoch().world_seed;
        let settings = [(4_u8, 4_u8), (8, 8), (12, 12), (6, 6)];
        let hub_targets = [0_i64, 500, 1500];
        for (grid, gap) in settings {
            let params = CoverageParams::new(grid, gap).validate().expect("valid");
            assert!(
                max_gap_guarantee_holds(seed, params, &hub_targets),
                "gap guarantee failed for ({grid},{gap})"
            );
        }
    }
}
