//! Abandoned municipal subway: stations, surface seams, and the eight-times hop.
//!
//! Stations are static places. Entrances are authored wilderness seams that do
//! not alter the generator baseline. One station-to-station hop covers eight
//! orthogonal surface tiles between the linked entrances.

use crate::geography::{gate_identity, RegionIdentity, WildernessPos, WorldId};
use crate::undernet::{ExitDef, LocationDef, ObjectDef};

/// Orthogonal surface tiles covered by one ordinary subway hop in this slice.
pub const SUBWAY_SURFACE_SCALE: i32 = 8;

/// Hub-side entrance: west of the city gate on the open corridor.
pub const ASH_ENTRANCE_LOCAL_X: u8 = 2;
pub const ASH_ENTRANCE_LOCAL_Y: u8 = 8;

/// Far entrance: eight tiles west of the Ash Plaza entrance (crosses into region -1).
pub const WARD_ENTRANCE_ORIGIN_X: i32 = -1;
pub const WARD_ENTRANCE_ORIGIN_Y: i32 = 0;
pub const WARD_ENTRANCE_LOCAL_X: u8 = 10;
pub const WARD_ENTRANCE_LOCAL_Y: u8 = 8;

pub const ASH_STATION: &str = "metro-ash-plaza";
pub const WARD_STATION: &str = "metro-ward-west";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MetroEntrance {
    pub station_id: &'static str,
    pub surface: WildernessPos,
    /// Keyword used on the surface tile to enter the station.
    pub down_keyword: &'static str,
    /// Keyword used in the station to return to this surface tile.
    pub up_keyword: &'static str,
    pub surface_label: &'static str,
    pub station_label: &'static str,
}

pub fn ash_entrance_pos() -> WildernessPos {
    WildernessPos {
        identity: gate_identity(),
        local_x: ASH_ENTRANCE_LOCAL_X,
        local_y: ASH_ENTRANCE_LOCAL_Y,
    }
}

pub fn ward_entrance_pos() -> WildernessPos {
    WildernessPos {
        identity: RegionIdentity {
            world: WorldId::Undernet,
            epoch_id: 1,
            origin_x: WARD_ENTRANCE_ORIGIN_X,
            origin_y: WARD_ENTRANCE_ORIGIN_Y,
            generation_version: 1,
        },
        local_x: WARD_ENTRANCE_LOCAL_X,
        local_y: WARD_ENTRANCE_LOCAL_Y,
    }
}

pub fn entrances() -> &'static [MetroEntrance] {
    &ENTRANCES
}

pub fn entrance_at(pos: WildernessPos) -> Option<&'static MetroEntrance> {
    ENTRANCES.iter().find(|entry| entry.surface == pos)
}

pub fn entrance_for_station(station_id: &str) -> Option<&'static MetroEntrance> {
    ENTRANCES.iter().find(|entry| entry.station_id == station_id)
}

/// Surface look prose for a metro seam tile. Authored like the gate seam — not a random trace.
pub fn entrance_landing_prose(entrance: &MetroEntrance) -> &'static str {
    if entrance.station_id == ASH_STATION {
        "Municipal landing under the Hub fringe. Adhesive ghosts on the stair housing outline \
where a Line 8 route map was peeled away whole — the stencil still reads ASH PLAZA — LINE 8 WEST. \
`go metro` descends to the platform."
    } else {
        "Municipal landing eight surface blocks west. The surface sign and the pillar name below \
no longer agree. Dust on the stair is scuffed, as if the line stayed in use after the towers \
emptied. `go metro` descends to Ward West Station."
    }
}

pub fn entrance_landing_prose_at(pos: WildernessPos) -> Option<&'static str> {
    entrance_at(pos).map(entrance_landing_prose)
}

pub fn is_station(id: &str) -> bool {
    location(id).is_some()
}

pub fn location(id: &str) -> Option<&'static LocationDef> {
    LOCATIONS.iter().find(|loc| loc.id == id)
}

pub fn locations() -> &'static [LocationDef] {
    &LOCATIONS
}

/// Station-to-station exits only. Surface seams are resolved in WorldState.
pub fn station_exits() -> &'static [ExitDef] {
    &STATION_EXITS
}

/// Absolute orthogonal distance between the two vertical-slice entrances.
pub fn entrance_surface_distance() -> i32 {
    let (ax, ay) = ash_entrance_pos().absolute();
    let (bx, by) = ward_entrance_pos().absolute();
    let dx = (ax - bx).abs();
    let dy = (ay - by).abs();
    i32::try_from(dx + dy).unwrap_or(i32::MAX)
}

const ENTRANCES: [MetroEntrance; 2] = [
    MetroEntrance {
        station_id: ASH_STATION,
        surface: WildernessPos {
            identity: RegionIdentity {
                world: WorldId::Undernet,
                epoch_id: 1,
                origin_x: 0,
                origin_y: 0,
                generation_version: 1,
            },
            local_x: ASH_ENTRANCE_LOCAL_X,
            local_y: ASH_ENTRANCE_LOCAL_Y,
        },
        down_keyword: "metro",
        up_keyword: "surface",
        surface_label: "Ash Plaza entrance",
        station_label: "Ash Plaza Station",
    },
    MetroEntrance {
        station_id: WARD_STATION,
        surface: WildernessPos {
            identity: RegionIdentity {
                world: WorldId::Undernet,
                epoch_id: 1,
                origin_x: WARD_ENTRANCE_ORIGIN_X,
                origin_y: WARD_ENTRANCE_ORIGIN_Y,
                generation_version: 1,
            },
            local_x: WARD_ENTRANCE_LOCAL_X,
            local_y: WARD_ENTRANCE_LOCAL_Y,
        },
        down_keyword: "metro",
        up_keyword: "surface",
        surface_label: "Ward West entrance",
        station_label: "Ward West Station",
    },
];

const LOCATIONS: [LocationDef; 2] = [
    LocationDef {
        id: ASH_STATION,
        name: "Ash Plaza Station",
        description: "A municipal subway platform under the Hub fringe. Tile is cracked; \
the service diagram has been carefully peeled from the wall rather than torn. \
Dust lies thick, but the platform edge is still painted. A stair leads up to the Squalor. \
One tunnel mouths west toward Ward West.",
        objects: &[
            ObjectDef {
                keyword: "diagram",
                aliases: &["map", "chart", "wall"],
                name: "service diagram",
                description: "Adhesive ghosts outline where the route map sat. \
Whoever removed it took the sheet whole — the same peeling visible on the surface landing above. \
The remaining stencil reads ASH PLAZA — LINE 8 WEST.",
                grant: None,
            },
            ObjectDef {
                keyword: "platform",
                aliases: &["edge", "track"],
                name: "platform edge",
                description: "Yellow paint still marks the drop. No train waits. \
The tunnel air is colder than the surface and smells of old oil.",
                grant: None,
            },
        ],
    },
    LocationDef {
        id: WARD_STATION,
        name: "Ward West Station",
        description: "A quieter platform eight surface blocks from Ash Plaza. \
The district name on the pillar no longer matches any Hub map you have seen. \
Stairs climb to a sealed stairwell in the Squalor. The tunnel runs east back toward Ash Plaza.",
        objects: &[
            ObjectDef {
                keyword: "pillar",
                aliases: &["sign", "name"],
                name: "district pillar",
                description:
                    "Cast lettering: WARD WEST. Under it, a newer stencil reads SECTOR 14-W, \
struck through once — the same renaming argument posted on civic timetables still listing \
Ash Plaza and Ward West as paired stops. Neither name appears on the Coffee Shop district board.",
                grant: None,
            },
            ObjectDef {
                keyword: "platform",
                aliases: &["edge", "track"],
                name: "platform edge",
                description: "The edge paint is newer than the rest of the station. \
Someone maintained this after the surrounding towers emptied — or wanted it to look that way. \
Who kept Line 8 nominally alive, and why the map was taken whole, is not written on the platform.",
                grant: None,
            },
        ],
    },
];

const STATION_EXITS: [ExitDef; 2] = [
    ExitDef {
        from: ASH_STATION,
        keyword: "west",
        to: WARD_STATION,
        label: "Line 8 west — Ward West",
        requires_flag: None,
    },
    ExitDef {
        from: WARD_STATION,
        keyword: "east",
        to: ASH_STATION,
        label: "Line 8 east — Ash Plaza",
        requires_flag: None,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_slice_entrances_are_eight_surface_tiles_apart() {
        assert_eq!(entrance_surface_distance(), SUBWAY_SURFACE_SCALE);
        let (ax, ay) = ash_entrance_pos().absolute();
        let (bx, by) = ward_entrance_pos().absolute();
        assert_eq!((ax, ay), (2, 8));
        assert_eq!((bx, by), (-6, 8));
        assert_ne!(ash_entrance_pos().identity.origin_x, ward_entrance_pos().identity.origin_x);
    }

    #[test]
    fn station_ids_are_stable_and_not_wilderness_keys() {
        assert!(ASH_STATION.contains('-') && !ASH_STATION.contains(':'));
        assert!(location(ASH_STATION).is_some());
        assert!(location(WARD_STATION).is_some());
        assert!(entrance_for_station(ASH_STATION).unwrap().surface == ash_entrance_pos());
    }

    #[test]
    fn ash_entrance_is_reachable_west_from_gate() {
        use crate::geography::{
            effective_tile, gate_position, generate_region, step_origin, Cardinal, GeographyState,
        };
        let geo = GeographyState::default();
        let mut pos = gate_position();
        for _ in 0..16 {
            if pos == ash_entrance_pos() {
                return;
            }
            let (ox, oy, lx, ly) = step_origin(pos, Cardinal::West).unwrap();
            let identity = RegionIdentity { origin_x: ox, origin_y: oy, ..pos.identity };
            let next = WildernessPos { identity, local_x: lx, local_y: ly };
            let baseline = generate_region(identity).unwrap().tile(lx, ly).unwrap();
            let tile = effective_tile(next, baseline, &geo);
            assert!(
                tile.traversable(),
                "blocked at {:?} terrain={:?} metro={}",
                next.absolute(),
                tile.terrain,
                tile.metro
            );
            pos = next;
        }
        panic!("never reached ash entrance from gate; last={:?}", pos.absolute());
    }
}
