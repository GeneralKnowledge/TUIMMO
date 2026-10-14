//! Jittered-grid anchors so walkable tiles stay within a bounded gap of inspectables.

use crate::geography::{
    gate_position, generate_region, thread_placement_hash, GeneratedRegion, RegionIdentity,
    WildernessPos, REGION_SIZE,
};
use crate::subway;

use super::coverage_params::CoverageParams;

/// Grid cell size for coverage anchors (tiles). With [`INSPECTABLE_MAX_GAP`], every
/// walkable tile in a 16×16 region sits within that gap of some anchor.
pub const COVERAGE_GRID_STEP: u8 = 4;

/// Maximum Chebyshev distance (tiles) from a walkable tile to the nearest coverage anchor.
pub const INSPECTABLE_MAX_GAP: u8 = 4;

const _: () = assert!(INSPECTABLE_MAX_GAP >= COVERAGE_GRID_STEP);

fn max_gap_guarantee_holds_for_region(
    walkable: &[WildernessPos],
    features: &[WildernessPos],
    max_gap: i32,
) -> bool {
    walkable.iter().all(|pos| {
        features.iter().any(|f| {
            let dx = (i32::from(pos.local_x) - i32::from(f.local_x)).abs();
            let dy = (i32::from(pos.local_y) - i32::from(f.local_y)).abs();
            dx.max(dy) <= max_gap
        })
    })
}

fn cell_count(params: CoverageParams) -> i32 {
    REGION_SIZE.div_euclid(i32::from(params.grid_step))
}

fn anchor_hash(world_seed: u64, identity: RegionIdentity, cell_x: i32, cell_y: i32) -> u64 {
    let clue = format!("cover:{cell_x}:{cell_y}");
    thread_placement_hash(world_seed, "coverage-grid", &clue, identity.origin_x, identity.origin_y)
}

/// Deterministic anchor inside a grid cell (local coordinates).
pub fn jittered_anchor_local(
    world_seed: u64,
    identity: RegionIdentity,
    cell_x: i32,
    cell_y: i32,
    params: CoverageParams,
) -> (u8, u8) {
    let h = anchor_hash(world_seed, identity, cell_x, cell_y);
    let step = i32::from(params.grid_step);
    let base_x = cell_x * step;
    let base_y = cell_y * step;
    let jx = (h & 0xFF) as i32 % step;
    let jy = ((h >> 8) & 0xFF) as i32 % step;
    let lx = (base_x + jx).clamp(0, REGION_SIZE - 1);
    let ly = (base_y + jy).clamp(0, REGION_SIZE - 1);
    (u8::try_from(lx).unwrap_or(0), u8::try_from(ly).unwrap_or(0))
}

fn walkable_anchor(
    region: &GeneratedRegion,
    identity: RegionIdentity,
    world_seed: u64,
    cell_x: i32,
    cell_y: i32,
    params: CoverageParams,
) -> Option<WildernessPos> {
    let (lx, ly) = jittered_anchor_local(world_seed, identity, cell_x, cell_y, params);
    let step = params.grid_step;
    for attempt in 0..i32::from(step * step) {
        let ax = (i32::from(lx) + attempt % i32::from(step)) as u8;
        let ay = (i32::from(ly) + (attempt / i32::from(step)) % i32::from(step)) as u8;
        let pos = WildernessPos { identity, local_x: ax, local_y: ay };
        if pos == gate_position() || subway::entrance_at(pos).is_some() {
            continue;
        }
        let tile = region.tile(ax, ay)?;
        if tile.terrain.traversable() {
            return Some(pos);
        }
    }
    let pos = WildernessPos { identity, local_x: lx, local_y: ly };
    if region.tile(lx, ly).is_some_and(|t| t.terrain.traversable()) {
        return Some(pos);
    }
    None
}

fn within_gap(pos: WildernessPos, anchors: &[WildernessPos], gap: i32) -> bool {
    anchors.iter().any(|anchor| {
        let dx = (i32::from(pos.local_x) - i32::from(anchor.local_x)).abs();
        let dy = (i32::from(pos.local_y) - i32::from(anchor.local_y)).abs();
        dx.max(dy) <= gap
    })
}

/// All coverage anchors for one region (walkable tiles only).
pub fn region_coverage_anchors_with_params(
    world_seed: u64,
    identity: RegionIdentity,
    region: &GeneratedRegion,
    params: CoverageParams,
) -> Vec<WildernessPos> {
    let mut out = Vec::new();
    let cells = cell_count(params);
    for cy in 0..cells {
        for cx in 0..cells {
            if let Some(pos) = walkable_anchor(region, identity, world_seed, cx, cy, params) {
                out.push(pos);
            }
        }
    }
    let gap = i32::from(params.max_gap);
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
            if pos == gate_position() || subway::entrance_at(pos).is_some() {
                continue;
            }
            if within_gap(pos, &out, gap) {
                continue;
            }
            out.push(pos);
        }
    }
    let walkable = walkable_tiles_in_region(region, identity);
    while !max_gap_guarantee_holds_for_region(&walkable, &out, gap) {
        let mut worst: Option<(WildernessPos, i32)> = None;
        for pos in &walkable {
            let nearest = out
                .iter()
                .map(|f| {
                    let dx = (i32::from(pos.local_x) - i32::from(f.local_x)).abs();
                    let dy = (i32::from(pos.local_y) - i32::from(f.local_y)).abs();
                    dx.max(dy)
                })
                .min()
                .unwrap_or(i32::MAX);
            if nearest > gap && worst.map(|(_, d)| nearest > d).unwrap_or(true) {
                worst = Some((*pos, nearest));
            }
        }
        let Some((pos, _)) = worst else {
            break;
        };
        out.push(pos);
    }
    out
}

fn walkable_tiles_in_region(
    region: &GeneratedRegion,
    identity: RegionIdentity,
) -> Vec<WildernessPos> {
    let mut tiles = Vec::new();
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
            if pos == gate_position() || subway::entrance_at(pos).is_some() {
                continue;
            }
            tiles.push(pos);
        }
    }
    tiles
}

pub fn region_coverage_anchors(
    world_seed: u64,
    identity: RegionIdentity,
    region: &GeneratedRegion,
) -> Vec<WildernessPos> {
    region_coverage_anchors_with_params(world_seed, identity, region, CoverageParams::default())
}

/// Load or generate anchors for a region.
pub fn coverage_anchors_for_region_with_params(
    world_seed: u64,
    identity: RegionIdentity,
    params: CoverageParams,
) -> Result<Vec<WildernessPos>, crate::geography::GenerateError> {
    let region = generate_region(identity)?;
    Ok(region_coverage_anchors_with_params(world_seed, identity, &region, params))
}

/// Load or generate anchors for a region (default coverage params).
pub fn coverage_anchors_for_region(
    world_seed: u64,
    identity: RegionIdentity,
) -> Result<Vec<WildernessPos>, crate::geography::GenerateError> {
    let region = generate_region(identity)?;
    Ok(region_coverage_anchors(world_seed, identity, &region))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geography::frontier_epoch;

    #[test]
    fn max_gap_holds_for_walkable_tiles_in_origin_region() {
        let params = CoverageParams::default();
        let epoch = frontier_epoch();
        let identity = RegionIdentity {
            world: crate::geography::WorldId::Undernet,
            epoch_id: epoch.id,
            origin_x: 0,
            origin_y: 0,
            generation_version: epoch.generation_version,
        };
        let region = generate_region(identity).unwrap();
        let anchors =
            region_coverage_anchors_with_params(epoch.world_seed, identity, &region, params);
        assert!(!anchors.is_empty());
        let gap = i32::from(params.max_gap);
        for ly in 0..REGION_SIZE {
            for lx in 0..REGION_SIZE {
                let lx = u8::try_from(lx).unwrap();
                let ly = u8::try_from(ly).unwrap();
                let tile = region.tile(lx, ly).unwrap();
                if !tile.terrain.traversable() {
                    continue;
                }
                let pos = WildernessPos { identity, local_x: lx, local_y: ly };
                if pos == gate_position() {
                    continue;
                }
                let near = anchors.iter().any(|anchor| {
                    let dx = (i32::from(pos.local_x) - i32::from(anchor.local_x)).abs();
                    let dy = (i32::from(pos.local_y) - i32::from(anchor.local_y)).abs();
                    dx.max(dy) <= gap
                });
                assert!(near, "tile {lx},{ly} farther than {gap} from any anchor");
            }
        }
    }
}
