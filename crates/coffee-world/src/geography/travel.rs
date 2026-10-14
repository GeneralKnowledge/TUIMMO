//! Movement and presentation over generated tiles and persistent overlays.
//!
//! The gate tile is an authored seam back to the Undernet. It does not alter the
//! baseline the generator returns.

use super::atmosphere::{exit_clarity_note, fogged_map_symbol, Atmosphere};
use super::density::UrbanContext;
use super::generate::{GeneratedRegion, LandmarkKind, Terrain, Tile};
use super::identity::{shift, IdentityError, WildernessPos, REGION_SIZE};
use super::records::{
    GeographyState, Structure, CABIN_KIND, CHALK_KIND, MARKER_KIND, SIGN_KIND, TRAIL_KIND,
    WORKSHOP_KIND,
};
use super::ugc::age_suffix;
use crate::subway;

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

/// What a player actually walks on: baseline, plus mods, plus authored seams.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectiveTile {
    pub terrain: Terrain,
    pub landmark: Option<LandmarkKind>,
    pub gate: bool,
    /// Authored subway stair on the surface. Stays walkable like the city gate.
    pub metro: bool,
    pub modified: bool,
}

impl EffectiveTile {
    /// The gate and metro stairs stay walkable so a terrain edit cannot seal them.
    pub const fn traversable(self) -> bool {
        self.gate || self.metro || self.terrain.traversable()
    }

    pub const fn block_reason(self) -> Option<&'static str> {
        if self.traversable() {
            return None;
        }
        Some(match self.terrain {
            Terrain::Water => "Floodwater blocks the way",
            Terrain::Rock => "A sealed structure blocks the way",
            Terrain::Grass | Terrain::Forest | Terrain::Scrub => "You cannot go that way",
        })
    }
}

pub fn effective_tile(pos: WildernessPos, baseline: Tile, geo: &GeographyState) -> EffectiveTile {
    let mut tile = EffectiveTile {
        terrain: baseline.terrain,
        landmark: baseline.landmark,
        gate: pos.is_gate(),
        metro: subway::entrance_at(pos).is_some(),
        modified: false,
    };
    if let Some(region) = geo.region(pos.identity.coord_key()) {
        if let Some(overlay) = geo.mod_at(region.id, pos.local_x, pos.local_y) {
            tile.terrain = overlay.terrain;
            tile.landmark = None;
            tile.modified = true;
        }
    }
    // The fence break is packed earth. The generated baseline is left alone, so the
    // origin fingerprint still reports whatever tile the generator produced here.
    // A stored terrain modification is shown as recorded; the seam stays walkable.
    if tile.gate && !tile.modified {
        tile.terrain = Terrain::Grass;
        tile.landmark = None;
    }
    // Metro stairs are concrete landings. Baseline fingerprint is unchanged.
    if tile.metro && !tile.gate && !tile.modified {
        tile.terrain = Terrain::Grass;
        tile.landmark = None;
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

#[derive(Clone, Debug)]
pub struct TileLookOptions {
    pub now_ms: u64,
    pub atmosphere: Atmosphere,
    pub hidden_trace_count: u32,
}

impl Default for TileLookOptions {
    fn default() -> Self {
        Self { now_ms: 0, atmosphere: Atmosphere::clear(), hidden_trace_count: 0 }
    }
}

pub fn describe_tile(
    pos: WildernessPos,
    generated: &GeneratedRegion,
    geo: &GeographyState,
    markers_here: &[MarkerSight],
    markers_in_region: &[(u8, u8, char)],
    opts: &TileLookOptions,
) -> TileScene {
    let baseline = generated
        .tile(pos.local_x, pos.local_y)
        .unwrap_or(Tile { terrain: Terrain::Rock, landmark: None });
    let tile = effective_tile(pos, baseline, geo);
    let urban = UrbanContext::at(pos);
    let title = if pos.is_gate() {
        "The Squalor".to_string()
    } else if let Some(entrance) = subway::entrance_at(pos) {
        entrance.surface_label.to_string()
    } else {
        format!("{} — {}", tile.terrain.label(), urban.title_suffix())
    };
    let mut lines = vec![terrain_prose(pos, tile, generated, geo, urban, opts.atmosphere)];
    if tile.metro {
        if let Some(entrance) = subway::entrance_at(pos) {
            lines.push(format!(
                "A municipal stair sinks into shadow. Dust on the railings is undisturbed. \
`go {}` enters {}. `inspect stair` reads the entrance.",
                entrance.down_keyword, entrance.station_label
            ));
        }
    }
    if let Some(kind) = tile.landmark {
        lines.push(kind.prose().to_string());
    }
    if let Some(detail) = urban.detail_look_line() {
        // Story and texture stay off the gate/metro seams; those own the look.
        if !tile.gate && !tile.metro {
            lines.push(detail.to_string());
        }
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
            CHALK_KIND => {
                format!("Chalk reads \"{}\". {} left it.", sight.name, sight.owner_name)
            }
            _ => format!("A marker reads \"{}\". {} left it.", sight.name, sight.owner_name),
        };
        if let Some(note) = &sight.settlement_note {
            line.push(' ');
            line.push_str(note);
        }
        lines.push(line);
    }
    if opts.hidden_trace_count > 0 {
        lines.push("Other marks have faded from view.".to_string());
    }
    if let Some(note) = exit_clarity_note(opts.atmosphere) {
        lines.push(note.to_string());
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
        urban.map_header_district(),
        urban.band.label().to_string(),
    ];
    let mut objects = vec![(tile.terrain.keyword().to_string(), tile.terrain.label().to_string())];
    if tile.metro {
        objects.push(("stair".into(), "subway stair".into()));
        objects.push(("metro".into(), "subway entrance".into()));
        objects.push(("entrance".into(), "subway entrance".into()));
    }
    if let Some(kind) = tile.landmark {
        objects.push((kind.keyword().to_string(), kind.name().to_string()));
    }
    if !tile.gate && !tile.metro {
        if let Some(story) = urban.story {
            objects.push((story.keyword().to_string(), story.label().to_string()));
        } else if let Some(trace) = urban.trace {
            objects.push((trace.keyword().to_string(), trace.label().to_string()));
        }
    }
    if markers_here.iter().any(|sight| sight.kind == SIGN_KIND) {
        objects.push(("sign".to_string(), "roadside sign".to_string()));
    }
    if markers_here.iter().any(|sight| sight.kind == TRAIL_KIND) {
        objects.push(("trail".to_string(), "trail post".to_string()));
    }
    if markers_here.iter().any(|sight| sight.kind == CHALK_KIND) {
        objects.push(("chalk".to_string(), "chalk mark".to_string()));
    }
    for sight in markers_here {
        let label = match sight.kind.as_str() {
            CABIN_KIND => format!("cabin \"{}\"", sight.name),
            WORKSHOP_KIND => format!("workshop \"{}\"", sight.name),
            SIGN_KIND => format!("sign \"{}\"", sight.name),
            TRAIL_KIND => format!("trail \"{}\"", sight.name),
            CHALK_KIND => format!("chalk \"{}\"", sight.name),
            _ => format!("marker \"{}\"", sight.name),
        };
        let keyword = match sight.kind.as_str() {
            CHALK_KIND => "chalk".to_string(),
            SIGN_KIND => "sign".to_string(),
            TRAIL_KIND => "trail".to_string(),
            _ => sight.name.to_ascii_lowercase(),
        };
        objects.push((keyword, label));
    }
    TileScene {
        title,
        description: lines.join("\n"),
        map: render_map(
            pos,
            generated,
            geo,
            markers_in_region,
            placement_x,
            placement_y,
            &urban,
            opts.atmosphere,
        ),
        objects,
        status,
    }
}

fn terrain_prose(
    pos: WildernessPos,
    tile: EffectiveTile,
    generated: &GeneratedRegion,
    geo: &GeographyState,
    urban: UrbanContext,
    atmosphere: Atmosphere,
) -> String {
    if tile.gate {
        let mut text = format!(
            "Broken paving where the Hub fence fails. The Roof Array is a dark shape behind you. \
Beyond it, abandoned towers stretch without an end you can trust. \
{} `city` returns to the roof.",
            gate_opening(pos, generated, geo, atmosphere)
        );
        if tile.modified {
            text.push_str(
                "\nSomeone has worked the ground, but the track back to the Hub remains.",
            );
        }
        return text;
    }
    let (abs_x, abs_y) = pos.absolute();
    let base = if tile.metro {
        subway::entrance_at(pos)
            .map(subway::entrance_landing_prose)
            .unwrap_or("Municipal landing. The stair is clearer than the streets it opens onto.")
    } else {
        urban.ordinary_prose(tile.terrain, abs_x, abs_y)
    };
    if tile.modified {
        format!("{base}\nThe ground here has been changed.")
    } else {
        base.to_string()
    }
}

#[allow(clippy::too_many_arguments)]
fn render_map(
    pos: WildernessPos,
    generated: &GeneratedRegion,
    geo: &GeographyState,
    markers: &[(u8, u8, char)],
    placement_x: i32,
    placement_y: i32,
    urban: &UrbanContext,
    atmosphere: Atmosphere,
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
                let base = if tile.landmark.is_some() {
                    '*'
                } else if tile.gate {
                    '+'
                } else if tile.metro {
                    'm'
                } else {
                    tile.terrain.symbol()
                };
                fogged_map_symbol(base, pos, here, atmosphere)
            };
            row.push(symbol);
        }
        rows.push(row);
    }
    format!(
        "\
epoch {}  birth ({}, {})  atlas ({placement_x}, {placement_y})  generator v{}  surface  {} / {}
{}
@ you  + Hub gate  m metro  * landmark  M marker  C cabin  S sign  T trail
. street  t towers  , rubble  ~ flood  ^ sealed",
        pos.identity.epoch_id,
        pos.identity.origin_x,
        pos.identity.origin_y,
        pos.identity.generation_version,
        urban.map_header_district(),
        urban.band.label(),
        rows.join("\n"),
    )
}

const EDGE_LINES: usize = 20;

/// What a step off the gate actually does, in the same terms movement uses.
fn gate_opening(
    pos: WildernessPos,
    generated: &GeneratedRegion,
    geo: &GeographyState,
    atmosphere: Atmosphere,
) -> String {
    let mut open = Vec::new();
    let mut water = Vec::new();
    let mut rock = Vec::new();
    let mut closed = Vec::new();
    for dir in [Cardinal::West, Cardinal::North, Cardinal::East, Cardinal::South] {
        let word = dir.keyword();
        let Ok((origin_x, origin_y, local_x, local_y)) = step_origin(pos, dir) else {
            closed.push(word);
            continue;
        };
        if origin_x != pos.identity.origin_x || origin_y != pos.identity.origin_y {
            closed.push(word);
            continue;
        }
        let here = WildernessPos { identity: pos.identity, local_x, local_y };
        let baseline = generated
            .tile(local_x, local_y)
            .unwrap_or(Tile { terrain: Terrain::Rock, landmark: None });
        let neighbor = effective_tile(here, baseline, geo);
        if neighbor.traversable() {
            open.push(word);
        } else {
            match neighbor.terrain {
                Terrain::Water => water.push(word),
                Terrain::Rock => rock.push(word),
                Terrain::Grass | Terrain::Forest | Terrain::Scrub => closed.push(word),
            }
        }
    }
    let mut parts = Vec::new();
    if open.is_empty() {
        parts.push("No cardinal step leaves the gate.".to_string());
    } else {
        parts.push(format!("Open: {}.", join_words(&open)));
    }
    if !water.is_empty() {
        parts.push(format!("Floodwater blocks the way {}.", join_words(&water)));
    }
    if !rock.is_empty() {
        parts.push(format!("Sealed structures block the way {}.", join_words(&rock)));
    }
    if !closed.is_empty() {
        parts.push(format!("You cannot go {}.", join_words(&closed)));
    }
    if atmosphere.exit_clarity < 0.75 && !open.is_empty() {
        parts.push("The opening west is the one most operators trust first.".to_string());
    }
    parts.join(" ")
}

fn join_words(words: &[&str]) -> String {
    match words {
        [] => String::new(),
        [one] => (*one).to_string(),
        [first, second] => format!("{first} and {second}"),
        _ => {
            let last = words[words.len() - 1];
            let rest = &words[..words.len() - 1];
            format!("{}, and {last}", rest.join(", "))
        }
    }
}

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
            | "squalor"
            | "ground"
            | "trees"
            | "blocks"
            | "water"
            | "flood"
            | "rock"
            | "sealed"
            | "scrub"
            | "rubble"
            | "spring"
            | "stone"
            | "mast"
            | "milepost"
            | "metro"
            | "surface"
            | "stair"
            | "entrance"
            | "enter"
            | "leave"
            | "cabin"
            | "workshop"
            | "sign"
            | "trail"
            | "chalk"
            | "listen"
            | "traces"
            | "survey"
            | "home"
            | "door"
            | "mark"
            | "journal"
            | "directory"
            | "notice"
            | "mailbox"
            | "vending"
            | "timetable"
            | "barrier"
            | "advert"
            | "clinic"
            | "corridor"
    )
}

pub fn marker_sights<'a>(
    structures: impl IntoIterator<Item = &'a Structure>,
    now_ms: u64,
    owner_name: impl Fn(Option<terminus_core::PlayerId>) -> String,
    settlement_note: impl Fn(&Structure) -> Option<String>,
) -> Vec<MarkerSight> {
    structures
        .into_iter()
        .filter(|item| {
            matches!(
                item.kind.as_str(),
                MARKER_KIND | CABIN_KIND | WORKSHOP_KIND | SIGN_KIND | TRAIL_KIND | CHALK_KIND
            )
        })
        .map(|item| {
            let base = owner_name(item.owner_id);
            let owner_name = if matches!(item.kind.as_str(), SIGN_KIND | TRAIL_KIND | CHALK_KIND) {
                format!("{base} ({})", age_suffix(now_ms, item.created_at))
            } else {
                base
            };
            MarkerSight {
                kind: item.kind.clone(),
                name: item.name.clone(),
                owner_name,
                settlement_note: settlement_note(item),
            }
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
    } else if structures.iter().any(|item| item.kind == CHALK_KIND) {
        Some('k')
    } else if structures.iter().any(|item| item.kind == MARKER_KIND) {
        Some('M')
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geography::generate::{fingerprint, generate_region, Terrain};
    use crate::geography::identity::{gate_identity, gate_position, RegionIdentity, WildernessPos};
    use crate::geography::records::{GeographyState, MaterializedRegion, TerrainMod};
    use uuid::Uuid;

    #[test]
    fn gate_seam_is_packed_earth_and_leaves_the_baseline_alone() {
        let generated = generate_region(gate_identity()).unwrap();
        assert_eq!(fingerprint(&generated), 0x8812_F1DB_241C_39A1);
        let baseline = generated.tile(8, 8).unwrap();
        assert_eq!(baseline.terrain, Terrain::Water);
        let geo = GeographyState::default();
        let effective = effective_tile(gate_position(), baseline, &geo);
        assert!(effective.gate);
        assert!(effective.traversable());
        assert_eq!(effective.terrain, Terrain::Grass);
        assert!(effective.landmark.is_none());

        let elsewhere = RegionIdentity { origin_x: 3, origin_y: -2, ..gate_identity() };
        let other = generate_region(elsewhere).unwrap();
        let other_base = other.tile(8, 8).unwrap();
        let other_pos = WildernessPos { identity: elsewhere, local_x: 8, local_y: 8 };
        let other_effective = effective_tile(other_pos, other_base, &geo);
        assert!(!other_effective.gate);
        assert_eq!(other_effective.terrain, other_base.terrain);
    }

    #[test]
    fn gate_look_matches_the_only_open_step() {
        let generated = generate_region(gate_identity()).unwrap();
        let scene = describe_tile(
            gate_position(),
            &generated,
            &GeographyState::default(),
            &[],
            &[],
            &Default::default(),
        );
        assert!(scene.description.contains("Open: west"), "{}", scene.description);
        assert!(
            scene.description.contains("Floodwater blocks the way north, east, and south"),
            "{}",
            scene.description
        );
        assert!(scene.description.contains("returns to the roof"), "{}", scene.description);
        assert!(!scene.description.contains("land ahead"), "{}", scene.description);
        assert!(scene.objects.iter().any(|(keyword, _)| keyword == "ground"));
        assert!(scene.objects.iter().all(|(keyword, _)| keyword != "water"));
        let west = map_symbol(&scene.map, 7, 8);
        let north = map_symbol(&scene.map, 8, 9);
        let east = map_symbol(&scene.map, 9, 8);
        let south = map_symbol(&scene.map, 8, 7);
        assert_ne!(west, '~', "west of the gate is the opening");
        assert_ne!(west, '^');
        assert_eq!(north, '~');
        assert_eq!(east, '~');
        assert_eq!(south, '~');
        assert_eq!(map_symbol(&scene.map, 8, 8), '@');
    }

    fn map_symbol(map: &str, local_x: u8, local_y: u8) -> char {
        let grid: Vec<&str> = map.lines().skip(1).take(16).collect();
        let row = grid[15 - usize::from(local_y)];
        row.chars().nth(usize::from(local_x)).unwrap_or('?')
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
        let scene = describe_tile(gate_position(), &generated, &geo, &[], &[], &Default::default());
        assert!(scene.map.contains("birth (0, 0)"));
        assert!(scene.map.contains("atlas (40, -3)"));
        assert!(scene.map.contains('@'));
        assert_eq!(scene.title, "The Squalor");
    }

    #[test]
    fn look_titles_carry_district_and_block_without_touching_fingerprint() {
        let generated = generate_region(gate_identity()).unwrap();
        assert_eq!(fingerprint(&generated), 0x8812_F1DB_241C_39A1);
        let street = WildernessPos { identity: gate_identity(), local_x: 4, local_y: 4 };
        let scene = describe_tile(
            street,
            &generated,
            &GeographyState::default(),
            &[],
            &[],
            &Default::default(),
        );
        assert!(scene.title.contains('—'), "{}", scene.title);
        assert!(
            scene.title.contains("residential")
                || scene.title.contains("commercial")
                || scene.title.contains("civic"),
            "{}",
            scene.title
        );
        assert!(
            scene.map.contains("near the Hub")
                || scene.map.contains("fringe")
                || scene.map.contains("deep")
        );
        assert!(scene.description.lines().count() >= 1);
    }

    #[test]
    fn clinic_story_appears_on_look_and_as_an_object() {
        use crate::geography::density::{CLINIC_DIRECTORY_LOCAL_X, CLINIC_DIRECTORY_LOCAL_Y};
        let generated = generate_region(gate_identity()).unwrap();
        let pos = WildernessPos {
            identity: gate_identity(),
            local_x: CLINIC_DIRECTORY_LOCAL_X,
            local_y: CLINIC_DIRECTORY_LOCAL_Y,
        };
        let scene = describe_tile(
            pos,
            &generated,
            &GeographyState::default(),
            &[],
            &[],
            &Default::default(),
        );
        assert!(scene.description.contains("Ward Clinic"), "{}", scene.description);
        assert!(scene.objects.iter().any(|(keyword, _)| keyword == "directory"));
    }
}
