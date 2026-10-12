//! Local return hints. Same-cell steps follow traversable tiles.
//! Anything farther away is described, not walked.

use std::collections::VecDeque;

use super::generate::{GeneratedRegion, Terrain, Tile};
use super::identity::{RegionIdentity, WildernessPos, GATE_LOCAL_X, GATE_LOCAL_Y, REGION_SIZE};
use super::records::GeographyState;
use super::travel::{effective_tile, Cardinal};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalPath {
    Here,
    Open(Vec<Cardinal>),
    Blocked,
}

const EDGE: i32 = REGION_SIZE;
type StepGrid = [[Option<(u8, u8, Cardinal)>; EDGE as usize]; EDGE as usize];

/// Shortest walk inside one birth cell. Water and rock are never steps.
/// The search does not leave the cell.
pub fn local_path(
    generated: &GeneratedRegion,
    geo: &GeographyState,
    identity: RegionIdentity,
    from: (u8, u8),
    to: (u8, u8),
) -> LocalPath {
    if from == to {
        return LocalPath::Here;
    }
    let mut seen = [[false; EDGE as usize]; EDGE as usize];
    let mut prev: StepGrid = [[None; EDGE as usize]; EDGE as usize];
    let mut queue = VecDeque::new();
    seen[from.1 as usize][from.0 as usize] = true;
    queue.push_back(from);
    while let Some((x, y)) = queue.pop_front() {
        for dir in [Cardinal::North, Cardinal::East, Cardinal::South, Cardinal::West] {
            let (dx, dy) = dir.delta();
            let nx = i32::from(x) + dx;
            let ny = i32::from(y) + dy;
            if nx < 0 || ny < 0 || nx >= EDGE || ny >= EDGE {
                continue;
            }
            let next = (u8::try_from(nx).unwrap_or(0), u8::try_from(ny).unwrap_or(0));
            if seen[next.1 as usize][next.0 as usize] {
                continue;
            }
            if !tile_open(generated, geo, identity, next) {
                continue;
            }
            seen[next.1 as usize][next.0 as usize] = true;
            prev[next.1 as usize][next.0 as usize] = Some((x, y, dir));
            if next == to {
                return rebuild(&prev, from, to);
            }
            queue.push_back(next);
        }
    }
    LocalPath::Blocked
}

fn tile_open(
    generated: &GeneratedRegion,
    geo: &GeographyState,
    identity: RegionIdentity,
    at: (u8, u8),
) -> bool {
    let pos = WildernessPos { identity, local_x: at.0, local_y: at.1 };
    let baseline =
        generated.tile(at.0, at.1).unwrap_or(Tile { terrain: Terrain::Rock, landmark: None });
    effective_tile(pos, baseline, geo).traversable()
}

fn rebuild(prev: &StepGrid, from: (u8, u8), to: (u8, u8)) -> LocalPath {
    let mut steps = Vec::new();
    let mut cursor = to;
    while cursor != from {
        let Some((px, py, dir)) = prev[cursor.1 as usize][cursor.0 as usize] else {
            return LocalPath::Blocked;
        };
        steps.push(dir);
        cursor = (px, py);
        if steps.len() > (EDGE as usize) * (EDGE as usize) {
            return LocalPath::Blocked;
        }
    }
    steps.reverse();
    if steps.is_empty() {
        LocalPath::Here
    } else {
        LocalPath::Open(steps)
    }
}

/// Two birth cells in the same generation. This names the offset and refuses a single bearing.
pub fn cross_region_hint(from: WildernessPos, to: WildernessPos) -> String {
    if from.identity.world != to.identity.world
        || from.identity.epoch_id != to.identity.epoch_id
        || from.identity.generation_version != to.identity.generation_version
    {
        return "The destination is not in this world generation. The record was kept and not moved. This is not a route.".into();
    }
    let dx = to.identity.origin_x - from.identity.origin_x;
    let dy = to.identity.origin_y - from.identity.origin_y;
    let shift = shift_phrase(dx, dy);
    let relation = if dx.abs() + dy.abs() == 1 {
        format!("That birth cell shares a border ({shift}).")
    } else if shift.is_empty() {
        "That tile is in this birth cell.".to_string()
    } else {
        format!("That birth cell sits {shift} of yours.")
    };
    format!(
        "Destination birth ({}, {}), tile {},{}. You are in birth ({}, {}). {relation} This is not a route.",
        to.identity.origin_x,
        to.identity.origin_y,
        to.local_x,
        to.local_y,
        from.identity.origin_x,
        from.identity.origin_y
    )
}

/// A wilderness destination seen from a city room, when it is not the gate's birth cell.
pub fn distant_cell_hint(to: WildernessPos) -> String {
    format!(
        "Destination birth ({}, {}), tile {},{}. The city gate is birth (0, 0), tile {},{}. This is not a route.",
        to.identity.origin_x,
        to.identity.origin_y,
        to.local_x,
        to.local_y,
        GATE_LOCAL_X,
        GATE_LOCAL_Y
    )
}

fn shift_phrase(dx: i32, dy: i32) -> String {
    [axis_phrase(dx, "east", "west"), axis_phrase(dy, "north", "south")]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" and ")
}

fn axis_phrase(delta: i32, positive: &str, negative: &str) -> Option<String> {
    match delta {
        0 => None,
        1 => Some(format!("one cell {positive}")),
        -1 => Some(format!("one cell {negative}")),
        n if n > 1 => Some(format!("{n} cells {positive}")),
        n => Some(format!("{} cells {negative}", n.abs())),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        cross_region_hint, distant_cell_hint, local_path, tile_open, Cardinal, GeographyState,
        LocalPath, WildernessPos,
    };
    use crate::geography::{
        gate_identity, generate_region, MaterializedRegion, Terrain, TerrainMod, GATE_LOCAL_X,
        GATE_LOCAL_Y,
    };
    use uuid::Uuid;

    fn landings(start: (u8, u8), steps: &[Cardinal]) -> Vec<(u8, u8)> {
        let mut at = start;
        let mut out = Vec::new();
        for step in steps {
            let (dx, dy) = step.delta();
            at = (
                u8::try_from(i32::from(at.0) + dx).unwrap_or(0),
                u8::try_from(i32::from(at.1) + dy).unwrap_or(0),
            );
            out.push(at);
        }
        out
    }

    #[test]
    fn the_gate_steps_west_onto_grass_and_not_into_water() {
        let generated = generate_region(gate_identity()).unwrap();
        let geo = GeographyState::default();
        let identity = gate_identity();
        let west = local_path(
            &generated,
            &geo,
            identity,
            (GATE_LOCAL_X, GATE_LOCAL_Y),
            (GATE_LOCAL_X - 1, GATE_LOCAL_Y),
        );
        assert_eq!(west, LocalPath::Open(vec![Cardinal::West]));
        let north = local_path(
            &generated,
            &geo,
            identity,
            (GATE_LOCAL_X, GATE_LOCAL_Y),
            (GATE_LOCAL_X, GATE_LOCAL_Y + 1),
        );
        assert_eq!(north, LocalPath::Blocked);
        assert_eq!(
            local_path(
                &generated,
                &geo,
                identity,
                (GATE_LOCAL_X, GATE_LOCAL_Y),
                (GATE_LOCAL_X, GATE_LOCAL_Y)
            ),
            LocalPath::Here
        );
    }

    #[test]
    fn a_water_edit_is_not_a_step() {
        let generated = generate_region(gate_identity()).unwrap();
        let identity = gate_identity();
        let region_id = Uuid::new_v4();
        let mut geo = GeographyState::default();
        geo.regions.insert(
            identity.coord_key(),
            MaterializedRegion {
                id: region_id,
                identity,
                region_seed: 0,
                placement_x: 0,
                placement_y: 0,
                discovered_at: 0,
            },
        );
        geo.terrain_mods.push(TerrainMod {
            region_id,
            local_x: GATE_LOCAL_X - 1,
            local_y: GATE_LOCAL_Y,
            terrain: Terrain::Water,
            reason: "test".into(),
            actor_id: None,
            created_at: 0,
        });
        let direct = local_path(
            &generated,
            &geo,
            identity,
            (GATE_LOCAL_X, GATE_LOCAL_Y),
            (GATE_LOCAL_X - 1, GATE_LOCAL_Y),
        );
        assert_eq!(direct, LocalPath::Blocked);
        let around = local_path(
            &generated,
            &geo,
            identity,
            (GATE_LOCAL_X, GATE_LOCAL_Y),
            (GATE_LOCAL_X - 2, GATE_LOCAL_Y),
        );
        match around {
            LocalPath::Blocked => {}
            LocalPath::Here => panic!("the far tile is not the gate"),
            LocalPath::Open(steps) => {
                let walked = landings((GATE_LOCAL_X, GATE_LOCAL_Y), &steps);
                assert!(!walked.contains(&(GATE_LOCAL_X - 1, GATE_LOCAL_Y)));
                for tile in walked {
                    assert!(tile_open(&generated, &geo, identity, tile));
                }
            }
        }
    }

    #[test]
    fn another_birth_cell_is_not_a_single_direction() {
        let here = WildernessPos {
            identity: gate_identity(),
            local_x: GATE_LOCAL_X,
            local_y: GATE_LOCAL_Y,
        };
        let mut there = here;
        there.identity.origin_x = 1;
        there.local_x = 3;
        there.local_y = 4;
        let hint = cross_region_hint(here, there);
        assert!(hint.contains("one cell east"), "{hint}");
        assert!(hint.contains("This is not a route."), "{hint}");
        assert!(!hint.contains("Head"), "{hint}");
        let far = distant_cell_hint(there);
        assert!(far.contains("birth (1, 0)"), "{far}");
        assert!(far.contains("tile 8,8"), "{far}");
        assert!(far.contains("This is not a route."), "{far}");
    }
}
