//! Movement and presentation over generated tiles and persistent overlays.
//!
//! The gate tile is an authored seam back to the Undernet. It does not alter the
//! baseline the generator returns.

use super::generate::{GeneratedRegion, LandmarkKind, Terrain, Tile};
use super::identity::{shift, IdentityError, WildernessPos, REGION_SIZE};
use super::records::{
    GeographyState, Structure, CABIN_KIND, MARKER_KIND, SIGN_KIND, TRAIL_KIND, WORKSHOP_KIND,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cardinal {
    North,
    South,
    East,
    West,
}

impl Cardinal {
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "n" | "north" => Some(Self::North),
            "s" | "south" => Some(Self::South),
            "e" | "east" => Some(Self::East),
            "w" | "west" => Some(Self::West),
            _ => None,
        }
    }

    pub const fn keyword(self) -> &'static str {
        match self {
            Self::North => "north",
            Self::South => "south",
            Self::East => "east",
            Self::West => "west",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::North => "North",
            Self::South => "South",
            Self::East => "East",
            Self::West => "West",
        }
    }

    pub const fn delta(self) -> (i32, i32) {
        match self {
            Self::North => (0, 1),
            Self::South => (0, -1),
            Self::East => (1, 0),
            Self::West => (-1, 0),
        }
    }
}

/// What a player actually walks on: baseline, plus mods, plus the city seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectiveTile {
    pub terrain: Terrain,
    pub landmark: Option<LandmarkKind>,
    pub gate: bool,
    pub modified: bool,
}

impl EffectiveTile {
    /// The gate stays walkable so a terrain edit cannot seal the city.
    pub const fn traversable(self) -> bool {
        self.gate || self.terrain.traversable()
    }

    pub const fn block_reason(self) -> Option<&'static str> {
        if self.traversable() {
            return None;
        }
        Some(match self.terrain {
            Terrain::Water => "Water blocks the way",
            Terrain::Rock => "Rock blocks the way",
            Terrain::Grass | Terrain::Forest | Terrain::Scrub => "You cannot go that way",
        })
    }
}

pub fn effective_tile(pos: WildernessPos, baseline: Tile, geo: &GeographyState) -> EffectiveTile {
    let mut tile = EffectiveTile {
        terrain: baseline.terrain,
        landmark: baseline.landmark,
        gate: pos.is_gate(),
        modified: false,
    };
    if let Some(region) = geo.region(pos.identity.coord_key()) {
        if let Some(overlay) = geo.mod_at(region.id, pos.local_x, pos.local_y) {
            tile.terrain = overlay.terrain;
            tile.landmark = None;
            tile.modified = true;
        }
    }
    tile
}

pub fn step_origin(pos: WildernessPos, dir: Cardinal) -> Result<(i32, i32, u8, u8), IdentityError> {
    let (dx, dy) = dir.delta();
    shift(pos, dx, dy)
}

pub struct MarkerSight {
    pub kind: String,
    pub name: String,
    pub owner_name: String,
    /// Optional "Part of settlement X" line when the structure is linked and visible.
    pub settlement_note: Option<String>,
}

pub struct TileScene {
    pub title: String,
    pub description: String,
    pub map: String,
    pub objects: Vec<(String, String)>,
    pub status: Vec<String>,
}

pub fn describe_tile(
    pos: WildernessPos,
    generated: &GeneratedRegion,
    geo: &GeographyState,
    markers_here: &[MarkerSight],
    markers_in_region: &[(u8, u8, char)],
) -> TileScene {
    let baseline = generated
        .tile(pos.local_x, pos.local_y)
        .unwrap_or(Tile { terrain: Terrain::Rock, landmark: None });
    let tile = effective_tile(pos, baseline, geo);
    let title =
        if pos.is_gate() { "The Outskirts".to_string() } else { tile.terrain.label().to_string() };
    let mut lines = vec![terrain_prose(tile)];
    if let Some(kind) = tile.landmark {
        lines.push(kind.prose().to_string());
    }
    for sight in markers_here {
        let mut line = match sight.kind.as_str() {
            CABIN_KIND => {
                format!(
                    "A cabin named \"{}\" stands here. {} built it.",
                    sight.name, sight.owner_name
                )
            }
            WORKSHOP_KIND => {
                format!(
                    "A workshop named \"{}\" stands here. {} keeps it.",
                    sight.name, sight.owner_name
                )
            }
            SIGN_KIND => {
                format!("A sign reads \"{}\". {} left it.", sight.name, sight.owner_name)
            }
            TRAIL_KIND => {
                format!("A trail post marks \"{}\". {} set it.", sight.name, sight.owner_name)
            }
            _ => format!("A marker reads \"{}\". {} left it.", sight.name, sight.owner_name),
        };
        if let Some(note) = &sight.settlement_note {
            line.push(' ');
            line.push_str(note);
        }
        lines.push(line);
    }
    let (placement_x, placement_y) = geo
        .region(pos.identity.coord_key())
        .map(|region| (region.placement_x, region.placement_y))
        .unwrap_or((pos.identity.origin_x, pos.identity.origin_y));
    let status = vec![
        format!("epoch {}", pos.identity.epoch_id),
        format!("birth ({}, {})", pos.identity.origin_x, pos.identity.origin_y),
        format!("atlas ({placement_x}, {placement_y})"),
        format!("generator v{}", pos.identity.generation_version),
        format!("tile {},{}", pos.local_x, pos.local_y),
    ];
    let mut objects = vec![(tile.terrain.keyword().to_string(), tile.terrain.label().to_string())];
    if let Some(kind) = tile.landmark {
        objects.push((kind.keyword().to_string(), kind.name().to_string()));
    }
    for sight in markers_here {
        let label = match sight.kind.as_str() {
            CABIN_KIND => format!("cabin \"{}\"", sight.name),
            WORKSHOP_KIND => format!("workshop \"{}\"", sight.name),
            SIGN_KIND => format!("sign \"{}\"", sight.name),
            TRAIL_KIND => format!("trail \"{}\"", sight.name),
            _ => format!("marker \"{}\"", sight.name),
        };
        objects.push((sight.name.to_ascii_lowercase(), label));
    }
    TileScene {
        title,
        description: lines.join("\n"),
        map: render_map(pos, generated, geo, markers_in_region, placement_x, placement_y),
        objects,
        status,
    }
}

fn terrain_prose(tile: EffectiveTile) -> String {
    if tile.gate {
        let mut text =
            "Packed earth and a break in the fencing. The roof array is still a dark shape \
             behind you. The land ahead is not on any Undernet map."
                .to_string();
        if tile.modified {
            text.push_str(
                "\nSomeone has worked the ground, but the track back to the city remains.",
            );
        }
        return text;
    }
    let base = match tile.terrain {
        Terrain::Grass => "Open grass, bent by weather that still smells like the city.",
        Terrain::Forest => "Trees stand close enough to slow you. The wood would take a saw.",
        Terrain::Scrub => "Low scrub and scattered wire-grass. Something in it could be salvaged.",
        Terrain::Water => "Dark water. There is no footing.",
        Terrain::Rock => "Broken rock. There is no footing.",
    };
    if tile.modified {
        format!("{base}\nThe ground here has been changed.")
    } else {
        base.to_string()
    }
}

fn render_map(
    pos: WildernessPos,
    generated: &GeneratedRegion,
    geo: &GeographyState,
    markers: &[(u8, u8, char)],
    placement_x: i32,
    placement_y: i32,
) -> String {
    let mut rows = Vec::with_capacity(EDGE_LINES);
    let height = usize::try_from(REGION_SIZE).unwrap_or(16);
    for local_y in (0..height).rev() {
        let mut row = String::with_capacity(height);
        for local_x in 0..height {
            let lx = u8::try_from(local_x).unwrap_or(0);
            let ly = u8::try_from(local_y).unwrap_or(0);
            let symbol = if lx == pos.local_x && ly == pos.local_y {
                '@'
            } else if let Some((_, _, mark)) = markers.iter().find(|(x, y, _)| *x == lx && *y == ly)
            {
                *mark
            } else {
                let here = WildernessPos { identity: pos.identity, local_x: lx, local_y: ly };
                let baseline = generated
                    .tile(lx, ly)
                    .unwrap_or(Tile { terrain: Terrain::Rock, landmark: None });
                let tile = effective_tile(here, baseline, geo);
                if tile.landmark.is_some() {
                    '*'
                } else if tile.gate {
                    '+'
                } else {
                    tile.terrain.symbol()
                }
            };
            row.push(symbol);
        }
        rows.push(row);
    }
    format!(
        "\
epoch {}  birth ({}, {})  atlas ({placement_x}, {placement_y})  generator v{}
{}
@ you  + city gate  * landmark  M marker  C cabin  S sign  T trail
. grass  t trees  , scrub  ~ water  ^ rock",
        pos.identity.epoch_id,
        pos.identity.origin_x,
        pos.identity.origin_y,
        pos.identity.generation_version,
        rows.join("\n"),
    )
}

const EDGE_LINES: usize = 20;

pub fn validate_marker_name(raw: &str) -> Result<String, &'static str> {
    if raw.len() < 2 || raw.len() > 16 {
        return Err("A marker name is 2–16 letters, digits, or hyphens.");
    }
    let Some(first) = raw.chars().next() else {
        return Err("A marker name must start with a letter.");
    };
    if !first.is_ascii_alphabetic() {
        return Err("A marker name must start with a letter.");
    }
    if !raw.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-') {
        return Err("A marker name is 2–16 letters, digits, or hyphens.");
    }
    if is_reserved_word(&raw.to_ascii_lowercase()) {
        return Err("That name is reserved for the land itself.");
    }
    Ok(raw.to_string())
}

pub fn is_reserved_word(word: &str) -> bool {
    matches!(
        word,
        "n" | "s"
            | "e"
            | "w"
            | "north"
            | "south"
            | "east"
            | "west"
            | "city"
            | "outskirts"
            | "ground"
            | "trees"
            | "water"
            | "rock"
            | "scrub"
            | "spring"
            | "stone"
            | "mast"
            | "milepost"
            | "enter"
            | "leave"
            | "cabin"
            | "workshop"
            | "sign"
            | "trail"
            | "home"
            | "door"
            | "mark"
            | "journal"
    )
}

pub fn marker_sights<'a>(
    structures: impl IntoIterator<Item = &'a Structure>,
    owner_name: impl Fn(Option<terminus_core::PlayerId>) -> String,
    settlement_note: impl Fn(&Structure) -> Option<String>,
) -> Vec<MarkerSight> {
    structures
        .into_iter()
        .filter(|item| {
            matches!(
                item.kind.as_str(),
                MARKER_KIND | CABIN_KIND | WORKSHOP_KIND | SIGN_KIND | TRAIL_KIND
            )
        })
        .map(|item| MarkerSight {
            kind: item.kind.clone(),
            name: item.name.clone(),
            owner_name: owner_name(item.owner_id),
            settlement_note: settlement_note(item),
        })
        .collect()
}

pub fn map_symbol_for_structures(structures: &[&Structure]) -> Option<char> {
    if structures.iter().any(|item| item.kind == CABIN_KIND) {
        Some('C')
    } else if structures.iter().any(|item| item.kind == WORKSHOP_KIND) {
        Some('W')
    } else if structures.iter().any(|item| item.kind == SIGN_KIND) {
        Some('S')
    } else if structures.iter().any(|item| item.kind == TRAIL_KIND) {
        Some('T')
    } else if structures.iter().any(|item| item.kind == MARKER_KIND) {
        Some('M')
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geography::generate::{generate_region, Terrain};
    use crate::geography::identity::{gate_identity, gate_position, WildernessPos};
    use crate::geography::records::{GeographyState, MaterializedRegion, TerrainMod};
    use uuid::Uuid;

    #[test]
    fn gate_overlay_does_not_rewrite_the_baseline() {
        let generated = generate_region(gate_identity()).unwrap();
        let baseline = generated.tile(8, 8).unwrap();
        let geo = GeographyState::default();
        let effective = effective_tile(gate_position(), baseline, &geo);
        assert!(effective.gate);
        assert!(effective.traversable());
        assert_eq!(effective.terrain, baseline.terrain);
    }

    #[test]
    fn terrain_mod_overrides_baseline_but_not_the_gate_exit() {
        let generated = generate_region(gate_identity()).unwrap();
        let baseline = generated.tile(8, 8).unwrap();
        let region_id = Uuid::new_v4();
        let mut geo = GeographyState::default();
        geo.regions.insert(
            gate_identity().coord_key(),
            MaterializedRegion {
                id: region_id,
                identity: gate_identity(),
                region_seed: 1,
                placement_x: 0,
                placement_y: 0,
                discovered_at: 1,
            },
        );
        geo.terrain_mods.push(TerrainMod {
            region_id,
            local_x: 8,
            local_y: 8,
            terrain: Terrain::Water,
            reason: "test".into(),
            actor_id: None,
            created_at: 1,
        });
        let effective = effective_tile(gate_position(), baseline, &geo);
        assert_eq!(effective.terrain, Terrain::Water);
        assert!(effective.modified);
        assert!(effective.traversable());
        assert!(effective.landmark.is_none());
    }

    #[test]
    fn north_increases_local_y_inside_the_region() {
        let (ox, oy, lx, ly) = step_origin(gate_position(), Cardinal::North).unwrap();
        assert_eq!((ox, oy, lx, ly), (0, 0, 8, 9));
    }

    #[test]
    fn east_from_the_edge_enters_the_next_region() {
        let edge = WildernessPos { identity: gate_identity(), local_x: 15, local_y: 4 };
        let (ox, oy, lx, ly) = step_origin(edge, Cardinal::East).unwrap();
        assert_eq!((ox, oy, lx, ly), (1, 0, 0, 4));
    }

    #[test]
    fn marker_names_reject_reserved_words() {
        assert!(validate_marker_name("camp").is_ok());
        assert!(validate_marker_name("a").is_err());
        assert!(validate_marker_name("north").is_err());
        assert!(validate_marker_name("1camp").is_err());
    }

    #[test]
    fn map_uses_atlas_placement_without_changing_birth() {
        let generated = generate_region(gate_identity()).unwrap();
        let region_id = Uuid::new_v4();
        let mut geo = GeographyState::default();
        geo.regions.insert(
            gate_identity().coord_key(),
            MaterializedRegion {
                id: region_id,
                identity: gate_identity(),
                region_seed: 0,
                placement_x: 40,
                placement_y: -3,
                discovered_at: 1,
            },
        );
        let scene = describe_tile(gate_position(), &generated, &geo, &[], &[]);
        assert!(scene.map.contains("birth (0, 0)"));
        assert!(scene.map.contains("atlas (40, -3)"));
        assert!(scene.map.contains('@'));
        assert_eq!(scene.title, "The Outskirts");
    }
}
