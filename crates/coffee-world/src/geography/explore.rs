//! Deterministic walks over generated regions. Tests only.
//!
//! These measurements decide whether landmark spacing, water, or presentation is
//! what a short stroll from the city gate actually runs into. They do not change
//! the generator.

use std::collections::VecDeque;

use super::generate::{generate_region, LandmarkKind, Terrain, GENERATION_V1};
use super::identity::{gate_identity, gate_position, RegionIdentity, WildernessPos, REGION_SIZE};
use super::records::GeographyState;
use super::travel::{describe_tile, effective_tile, step_origin, Cardinal};

const EDGE: usize = 16;

struct Probe {
    reachable: usize,
    landmarks: usize,
    unreachable_landmarks: usize,
    first_landmark: Option<u32>,
    /// Consecutive BFS distances between landmarks, sorted by distance from the start.
    gaps: Vec<u32>,
    blocked_edges: u32,
    open_edges: u32,
    nearest: Option<(u8, u8, LandmarkKind, u32)>,
}

fn probe(identity: RegionIdentity, start_x: u8, start_y: u8) -> Probe {
    let generated = generate_region(identity).expect("version 1 region");
    let geo = GeographyState::default();
    let mut seen = [[false; EDGE]; EDGE];
    let mut dist = [[u32::MAX; EDGE]; EDGE];
    let mut queue = VecDeque::new();
    let start = WildernessPos { identity, local_x: start_x, local_y: start_y };
    if walkable(start, &generated, &geo) {
        seen[usize::from(start_y)][usize::from(start_x)] = true;
        dist[usize::from(start_y)][usize::from(start_x)] = 0;
        queue.push_back((start_x, start_y));
    }
    let mut blocked_edges = 0u32;
    let mut open_edges = 0u32;
    while let Some((x, y)) = queue.pop_front() {
        let pos = WildernessPos { identity, local_x: x, local_y: y };
        let here = dist[usize::from(y)][usize::from(x)];
        for dir in [Cardinal::North, Cardinal::South, Cardinal::East, Cardinal::West] {
            let Ok((origin_x, origin_y, local_x, local_y)) = step_origin(pos, dir) else {
                blocked_edges += 1;
                continue;
            };
            if origin_x != identity.origin_x || origin_y != identity.origin_y {
                // Another birth cell. Count it only as a leaving edge, not as this chart.
                let next = WildernessPos {
                    identity: RegionIdentity { origin_x, origin_y, ..identity },
                    local_x,
                    local_y,
                };
                let next_region = generate_region(next.identity).expect("neighbour region");
                if walkable(next, &next_region, &geo) {
                    open_edges += 1;
                } else {
                    blocked_edges += 1;
                }
                continue;
            }
            let next = WildernessPos { identity, local_x, local_y };
            if !walkable(next, &generated, &geo) {
                blocked_edges += 1;
                continue;
            }
            open_edges += 1;
            let slot = &mut seen[usize::from(local_y)][usize::from(local_x)];
            if *slot {
                continue;
            }
            *slot = true;
            dist[usize::from(local_y)][usize::from(local_x)] = here.saturating_add(1);
            queue.push_back((local_x, local_y));
        }
    }

    let mut reached = Vec::new();
    let mut unreachable_landmarks = 0usize;
    for local_y in 0..EDGE {
        for local_x in 0..EDGE {
            let Some(tile) = generated.tile(local_x as u8, local_y as u8) else {
                continue;
            };
            let Some(kind) = tile.landmark else {
                continue;
            };
            if seen[local_y][local_x] {
                reached.push((dist[local_y][local_x], local_x as u8, local_y as u8, kind));
            } else {
                unreachable_landmarks += 1;
            }
        }
    }
    reached.sort_by_key(|item| (item.0, item.1, item.2));
    let gaps = reached.windows(2).map(|pair| pair[1].0.saturating_sub(pair[0].0)).collect();
    let nearest = reached.first().map(|item| (item.1, item.2, item.3, item.0));
    let reachable = seen.iter().flatten().filter(|cell| **cell).count();
    Probe {
        reachable,
        landmarks: reached.len(),
        unreachable_landmarks,
        first_landmark: nearest.map(|item| item.3),
        gaps,
        blocked_edges,
        open_edges,
        nearest,
    }
}

fn walkable(
    pos: WildernessPos,
    generated: &super::generate::GeneratedRegion,
    geo: &GeographyState,
) -> bool {
    let baseline = generated
        .tile(pos.local_x, pos.local_y)
        .unwrap_or(super::generate::Tile { terrain: Terrain::Rock, landmark: None });
    effective_tile(pos, baseline, geo).traversable()
}

fn median(values: &mut [u32]) -> u32 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    values[values.len() / 2]
}

struct RegionSummary {
    origin: (i32, i32),
    starts: usize,
    median_reachable: u32,
    median_landmarks: u32,
    median_first: u32,
    starts_with_no_landmark: usize,
    unreachable_from_first_walkable: usize,
}

fn summarize(origin_x: i32, origin_y: i32, start_x: u8, start_y: u8) -> RegionSummary {
    let identity = RegionIdentity { origin_x, origin_y, ..gate_identity() };
    let from_gate_like = probe(identity, start_x, start_y);
    let generated = generate_region(identity).expect("region");
    let geo = GeographyState::default();
    let mut reachable_counts = Vec::new();
    let mut landmark_counts = Vec::new();
    let mut firsts = Vec::new();
    let mut starts_with_no_landmark = 0usize;
    let mut starts = 0usize;
    for local_y in 0..u8::try_from(REGION_SIZE).unwrap_or(16) {
        for local_x in 0..u8::try_from(REGION_SIZE).unwrap_or(16) {
            let pos = WildernessPos { identity, local_x, local_y };
            if !walkable(pos, &generated, &geo) {
                continue;
            }
            starts += 1;
            let sample = probe(identity, local_x, local_y);
            reachable_counts.push(u32::try_from(sample.reachable).unwrap_or(0));
            landmark_counts.push(u32::try_from(sample.landmarks).unwrap_or(0));
            match sample.first_landmark {
                Some(steps) => firsts.push(steps),
                None => starts_with_no_landmark += 1,
            }
        }
    }
    RegionSummary {
        origin: (origin_x, origin_y),
        unreachable_from_first_walkable: from_gate_like.unreachable_landmarks,
        starts,
        median_reachable: median(&mut reachable_counts),
        median_landmarks: median(&mut landmark_counts),
        median_first: median(&mut firsts),
        starts_with_no_landmark,
    }
}

fn kind_name(kind: LandmarkKind) -> &'static str {
    match kind {
        LandmarkKind::Spring => "spring",
        LandmarkKind::StandingStone => "stone",
        LandmarkKind::FallenMast => "mast",
        LandmarkKind::Milepost => "milepost",
    }
}

#[test]
fn wilderness_exploration_is_measured_without_retuning_the_generator() {
    assert_eq!(gate_identity().generation_version, GENERATION_V1);
    let gate = gate_position();
    let origin = probe(gate.identity, gate.local_x, gate.local_y);
    let generated = generate_region(gate.identity).expect("origin");
    let scene =
        describe_tile(gate, &generated, &GeographyState::default(), &[], &[], &Default::default());
    assert!(
        scene.map.contains('*'),
        "reachable or not, landmarks in this region are marked on the chart"
    );
    for local_y in 0..EDGE {
        for local_x in 0..EDGE {
            if let Some(kind) =
                generated.tile(local_x as u8, local_y as u8).and_then(|tile| tile.landmark)
            {
                assert!(!kind.prose().is_empty());
            }
        }
    }

    let samples = [
        summarize(0, 0, GATE_X, GATE_Y),
        summarize(1, 0, 0, 8),
        summarize(0, 1, 8, 0),
        summarize(-1, 0, 15, 8),
        summarize(2, -1, 8, 8),
    ];
    let mut lines = vec![format!(
        "gate reachable={} landmarks={} unreachable={} first={:?} blocked_edges={} open_edges={} nearest={}",
        origin.reachable,
        origin.landmarks,
        origin.unreachable_landmarks,
        origin.first_landmark,
        origin.blocked_edges,
        origin.open_edges,
        origin
            .nearest
            .map(|item| format!("{}:{},{}@{}", kind_name(item.2), item.0, item.1, item.3))
            .unwrap_or_else(|| "none".into()),
    )];
    lines.push(format!("gate gaps={:?}", origin.gaps));
    for sample in &samples {
        lines.push(format!(
            "region {},{} starts={} median_reachable={} median_landmarks={} median_first={} no_landmark_starts={} chart_unreachable={}",
            sample.origin.0,
            sample.origin.1,
            sample.starts,
            sample.median_reachable,
            sample.median_landmarks,
            sample.median_first,
            sample.starts_with_no_landmark,
            sample.unreachable_from_first_walkable,
        ));
    }
    let snapshot = lines.join("\n");
    assert_eq!(snapshot, MEASURED, "{snapshot}");
}

const GATE_X: u8 = 8;
const GATE_Y: u8 = 8;

/// Locked measurement for version 1 plus the packed-earth gate seam.
///
/// From the gate, 221 of 256 tiles are one walkable component, all 6 landmarks
/// in the origin region are reachable, and the nearest is 8 steps away. Blocked
/// edges are 43 against 841 open ones. Neighbouring regions show the same
/// pattern: landmarks are uncommon and sometimes a single chart holds only one,
/// but none of the sampled charts hide a landmark behind impassable ground.
/// Median steps to a first landmark, across walkable starts, stay in the range
/// 4–10. That is the published ~3% landmark rate, not a broken layout. The
/// generator probabilities stay as they are.
const MEASURED: &str = "\
gate reachable=221 landmarks=6 unreachable=0 first=Some(8) blocked_edges=43 open_edges=841 nearest=stone:5,13@8
gate gaps=[0, 2, 2, 1, 3]
region 0,0 starts=221 median_reachable=221 median_landmarks=6 median_first=4 no_landmark_starts=0 chart_unreachable=0
region 1,0 starts=216 median_reachable=216 median_landmarks=1 median_first=10 no_landmark_starts=0 chart_unreachable=0
region 0,1 starts=249 median_reachable=249 median_landmarks=8 median_first=5 no_landmark_starts=0 chart_unreachable=0
region -1,0 starts=252 median_reachable=252 median_landmarks=8 median_first=4 no_landmark_starts=0 chart_unreachable=0
region 2,-1 starts=241 median_reachable=241 median_landmarks=2 median_first=8 no_landmark_starts=0 chart_unreachable=0";
