//! Urban density presentation layered on the version-1 generator.
//!
//! Districts, block patterns, environmental traces, and the near-Hub clinic
//! story are pure functions of absolute coordinates. They do not read or write
//! the baseline tile grid, so the origin fingerprint and place keys stay put.

use super::generate::Terrain;
use super::identity::{gate_identity, WildernessPos, GATE_LOCAL_X, GATE_LOCAL_Y};

/// Absolute size of a shared block composition (tiles).
const BLOCK: i64 = 4;
/// Absolute size of a district cell (tiles). Spans region borders.
const DISTRICT: i64 = 32;

/// Gate absolute coordinates (region 0,0 local 8,8).
const GATE_ABS_X: i64 = GATE_LOCAL_X as i64;
const GATE_ABS_Y: i64 = GATE_LOCAL_Y as i64;

/// Chebyshev bands from the Hub gate.
///
/// Distance changes atmosphere and texture frequency, not the underlying
/// district/block vocabulary. Origin 62 (~992 tiles east) is deep Squalor and
/// still receives full urban composition — never an empty remote fallback.
const NEAR_HUB_MAX: i64 = 20;
const FRINGE_MAX: i64 = 48;

/// Birth cell used by the remote-world acceptance scenario (~1000 tiles east).
/// Absolute x ≈ `62 * REGION_SIZE` = 992.
pub const REMOTE_ACCEPTANCE_ORIGIN_X: i32 = 62;
pub const REMOTE_ACCEPTANCE_ORIGIN_Y: i32 = 0;

/// Authored clinic story — three walkable tiles in the origin region, clear of
/// the gate, survey stone (7,8), and Ash Plaza metro (2,8).
pub const CLINIC_DIRECTORY_LOCAL_X: u8 = 6;
pub const CLINIC_DIRECTORY_LOCAL_Y: u8 = 10;
pub const CLINIC_ENTRANCE_LOCAL_X: u8 = 5;
pub const CLINIC_ENTRANCE_LOCAL_Y: u8 = 12;
pub const CLINIC_CORRIDOR_LOCAL_X: u8 = 6;
pub const CLINIC_CORRIDOR_LOCAL_Y: u8 = 12;

pub const CLINIC_DIRECTORY_ID: &str = "clinic-directory";
pub const CLINIC_NOTICE_ID: &str = "clinic-notice";
pub const CLINIC_CORRIDOR_ID: &str = "clinic-corridor";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DistrictKind {
    Residential,
    Commercial,
    Civic,
}

impl DistrictKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Residential => "residential towers",
            Self::Commercial => "commercial strip",
            Self::Civic => "civic quarter",
        }
    }

    pub const fn short(self) -> &'static str {
        match self {
            Self::Residential => "residential",
            Self::Commercial => "commercial",
            Self::Civic => "civic",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockPattern {
    Courtyard,
    ShopRow,
    Plaza,
    ServiceYard,
}

impl BlockPattern {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Courtyard => "courtyard block",
            Self::ShopRow => "shop row",
            Self::Plaza => "plaza",
            Self::ServiceYard => "service yard",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DensityBand {
    NearHub,
    Fringe,
    Deep,
}

impl DensityBand {
    pub const fn label(self) -> &'static str {
        match self {
            Self::NearHub => "near the Hub",
            Self::Fringe => "outer fringe",
            Self::Deep => "deep Squalor",
        }
    }
}

/// Mundane environmental texture a player can inspect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceKind {
    Directory,
    Notice,
    Mailbox,
    Vending,
    Timetable,
    Barrier,
    Advert,
}

impl TraceKind {
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Directory => "directory",
            Self::Notice => "notice",
            Self::Mailbox => "mailbox",
            Self::Vending => "vending",
            Self::Timetable => "timetable",
            Self::Barrier => "barrier",
            Self::Advert => "advert",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Directory => "building directory",
            Self::Notice => "municipal notice",
            Self::Mailbox => "abandoned mailbox",
            Self::Vending => "broken vending machine",
            Self::Timetable => "faded timetable",
            Self::Barrier => "temporary barrier",
            Self::Advert => "obsolete advertisement",
        }
    }

    pub const fn look_line(self, band: DensityBand) -> &'static str {
        match (self, band) {
            (Self::Directory, DensityBand::NearHub) => {
                "A building directory hangs by the entrance. Most names are still legible. `inspect directory`."
            }
            (Self::Directory, _) => {
                "A building directory has weathered almost blank. `inspect directory`."
            }
            (Self::Notice, DensityBand::NearHub) => {
                "A municipal notice board still holds a few sheets. `inspect notice`."
            }
            (Self::Notice, _) => {
                "A municipal notice board leans empty against a wall. `inspect notice`."
            }
            (Self::Mailbox, _) => {
                "An abandoned mailbox stands open. No letters remain. `inspect mailbox`."
            }
            (Self::Vending, DensityBand::Deep) => {
                "A vending machine is rusted shut. The glass is opaque with dust. `inspect vending`."
            }
            (Self::Vending, _) => {
                "A broken vending machine hums once, then dies. `inspect vending`."
            }
            (Self::Timetable, DensityBand::NearHub) => {
                "A transit timetable is posted under cracked plastic. `inspect timetable`."
            }
            (Self::Timetable, _) => {
                "A faded timetable lists routes that no longer run. `inspect timetable`."
            }
            (Self::Barrier, _) => {
                "A temporary barrier still blocks half the walkway. `inspect barrier`."
            }
            (Self::Advert, DensityBand::Deep) => {
                "An obsolete advertisement peels from a pillar. `inspect advert`."
            }
            (Self::Advert, _) => {
                "An obsolete advertisement promises a sale that never ended. `inspect advert`."
            }
        }
    }

    pub fn inspect_text(self, band: DensityBand, district: DistrictKind) -> String {
        match self {
            Self::Directory => match band {
                DensityBand::NearHub => format!(
                    "Building directory. Floors and unit numbers for the {} are still painted in. \
Several names have been scratched out; the rest wait for residents who did not return.",
                    district.name()
                ),
                DensityBand::Fringe => {
                    "Building directory. Half the plates are missing. What remains lists shops \
beside apartments, as if the block never chose one use."
                        .into()
                }
                DensityBand::Deep => {
                    "Building directory. The lettering has gone to pale ghosts. You can still \
feel the raised numbers under a fingertip."
                        .into()
                }
            },
            Self::Notice => match band {
                DensityBand::NearHub => {
                    "Municipal notice. Water rationing schedule, last revised before the Hub \
closed this sector. The dates still look recent enough to matter."
                        .into()
                }
                _ => "Municipal notice. Evacuation directions point two ways at once. Neither \
arrow has been corrected."
                    .into(),
            },
            Self::Mailbox => {
                "Abandoned mailbox. The hatch sticks open. Dust inside, and a delivery slip \
addressed only to a block number."
                    .into()
            }
            Self::Vending => {
                "Broken vending machine. The coin slot is welded with rust. A faded strip still \
prices water cheaper than bread."
                    .into()
            }
            Self::Timetable => match district {
                DistrictKind::Civic => {
                    "Faded timetable. Ash Plaza and Ward West are listed as if the line still \
ran on the surface. The clock face is blank."
                        .into()
                }
                _ => "Faded timetable. Local shuttle loops and a night freight window. None of \
the departure times have hands to match."
                    .into(),
            },
            Self::Barrier => {
                "Temporary barrier. City-issue tape and a steel frame left across the walk. \
The \"works in progress\" card has bleached white."
                    .into()
            }
            Self::Advert => {
                "Obsolete advertisement. A smiling family under a tower that still stands \
three blocks over. The offer expired in a year you cannot place."
                    .into()
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorySite {
    Directory,
    Clinic,
    Corridor,
}

impl StorySite {
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Directory => "directory",
            Self::Clinic => "clinic",
            Self::Corridor => "corridor",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Directory => "Ward Clinic directory",
            Self::Clinic => "closed clinic",
            Self::Corridor => "service corridor",
        }
    }

    pub const fn look_line(self) -> &'static str {
        match self {
            Self::Directory => {
                "A lobby directory lists Ward Clinic on level two — stamped CLOSED, but a handwritten \
strip still points toward the entrance for records. `inspect directory`."
            }
            Self::Clinic => {
                "Shuttered clinic doors face the lane. A notice is taped over the handle; the tape \
looks newer than the closure order. `inspect clinic`."
            }
            Self::Corridor => {
                "A service corridor mouth sits half-bricked. Dust on the near side is scuffed, as if \
the passage stayed in use after the seal began. `inspect corridor`."
            }
        }
    }

    pub fn inspect_text(self) -> String {
        match self {
            Self::Directory => {
                "Ward Clinic directory. Level 2 — CLOSED in heavy ink. A later handwritten strip \
points northwest: \"records held at the clinic entrance until the corridor job finishes.\" \
The two messages do not agree on whether the building is still in use."
                    .into()
            }
            Self::Clinic => {
                "Closed clinic. The taped notice repeats the closure number from the lobby directory \
and warns that the service corridor east of this door stays unsafe until the seal is complete. \
The tape looks newer than the notice beneath it — as if someone re-posted the warning after \
the corridor kept seeing traffic."
                    .into()
            }
            Self::Corridor => {
                "Service corridor. Bricks stacked for a seal that was never finished. A work order \
clipped to the scaffold names Ward Clinic and stops mid-sentence: \"…resume access once records \
are—\" Scuff marks in the dust lead through the gap. Who kept using the passage, and why the \
seal stalled, is not written on the scaffold."
                    .into()
            }
        }
    }

    pub const fn content_key(self) -> &'static str {
        match self {
            Self::Directory => CLINIC_DIRECTORY_ID,
            Self::Clinic => CLINIC_NOTICE_ID,
            Self::Corridor => CLINIC_CORRIDOR_ID,
        }
    }

    pub const fn finding_title(self) -> &'static str {
        match self {
            Self::Directory => "Ward Clinic closed",
            Self::Clinic => "Clinic corridor notice",
            Self::Corridor => "Unfinished clinic seal",
        }
    }

    pub const fn finding_note(self) -> &'static str {
        match self {
            Self::Directory => {
                "A lobby directory near the Hub lists Ward Clinic as closed while a handwritten \
note still sends clerks to the entrance for records."
            }
            Self::Clinic => {
                "The shuttered Ward Clinic re-posted a corridor warning on fresh tape, as if the \
east passage had stayed in use after the official closure."
            }
            Self::Corridor => {
                "East of the closed clinic, a half-bricked service corridor shows scuffed dust and \
a truncated work order — evidence the seal was abandoned, not completed."
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UrbanContext {
    pub district: DistrictKind,
    pub block: BlockPattern,
    pub band: DensityBand,
    pub story: Option<StorySite>,
    pub trace: Option<TraceKind>,
}

impl UrbanContext {
    pub fn at(pos: WildernessPos) -> Self {
        let (abs_x, abs_y) = pos.absolute();
        let band = density_band(abs_x, abs_y);
        let district = district_at(abs_x, abs_y);
        let block = block_at(abs_x, abs_y, district);
        let story = story_at(pos);
        let trace = if story.is_some() { None } else { trace_at(abs_x, abs_y, band, district) };
        Self { district, block, band, story, trace }
    }

    pub fn title_suffix(self) -> String {
        format!("{}, {}", self.district.short(), self.block.name())
    }

    pub fn ordinary_prose(self, terrain: Terrain) -> &'static str {
        match (self.district, self.block, terrain, self.band) {
            (_, _, Terrain::Water, _) => "A flooded cut. There is no footing.",
            (_, _, Terrain::Rock, _) => "A sealed or collapsed structure. There is no footing.",
            (DistrictKind::Residential, BlockPattern::Courtyard, Terrain::Forest, _) => {
                "Apartment towers face a shared courtyard. Walkways sag between the landings."
            }
            (DistrictKind::Residential, BlockPattern::Courtyard, _, DensityBand::NearHub) => {
                "Residential courtyard. Entrances still carry unit numbers; the fountain is dry."
            }
            (DistrictKind::Residential, BlockPattern::Courtyard, _, _) => {
                "Residential courtyard. The towers lean inward over cracked paving."
            }
            (DistrictKind::Residential, BlockPattern::ServiceYard, _, _) => {
                "Service yard behind the towers. Pipe runs and loading bays, long unused."
            }
            (DistrictKind::Residential, _, Terrain::Forest, DensityBand::Deep) => {
                "Endless residential stacks. One lane looks like the last, and the next."
            }
            (DistrictKind::Residential, _, _, _) => {
                "Residential frontage. Stairwells and mail slots wait without names."
            }
            (DistrictKind::Commercial, BlockPattern::ShopRow, _, DensityBand::NearHub) => {
                "Shop row on a pedestrian street. Shutters down, signs still selling yesterday."
            }
            (DistrictKind::Commercial, BlockPattern::ShopRow, _, _) => {
                "Shop row. Glass gone from the fronts; the awnings hang in strips."
            }
            (DistrictKind::Commercial, BlockPattern::Plaza, _, _) => {
                "Commercial plaza. Empty stalls and a dry fountain mark the crossing."
            }
            (DistrictKind::Commercial, _, Terrain::Scrub, _) => {
                "Stripped shopfronts and cable. Someone claimed the easy metal years ago."
            }
            (DistrictKind::Commercial, _, _, _) => {
                "Commercial lane. Delivery doors and empty display windows."
            }
            (DistrictKind::Civic, BlockPattern::Plaza, _, DensityBand::NearHub) => {
                "Civic plaza. The paving still suggests a crowd that will not arrive."
            }
            (DistrictKind::Civic, BlockPattern::Plaza, _, _) => {
                "Open plaza between civic shells. Wind crosses it without slowing."
            }
            (DistrictKind::Civic, BlockPattern::ServiceYard, _, _) => {
                "Utility apron behind a civic hall. Maintenance hatches and a dead generator pad."
            }
            (DistrictKind::Civic, _, _, DensityBand::NearHub) => {
                "Civic frontage. Signage is clearer here; routes still almost make sense."
            }
            (DistrictKind::Civic, _, _, _) => {
                "Civic block. Blank offices and a public stair that climbs to nowhere useful."
            }
        }
    }

    /// Extra look line for texture or story. Ordinary tiles return None.
    pub fn detail_look_line(self) -> Option<&'static str> {
        if let Some(story) = self.story {
            return Some(story.look_line());
        }
        self.trace.map(|trace| trace.look_line(self.band))
    }

    pub fn map_header_district(self) -> String {
        format!("{} / {}", self.district.short(), self.block.name())
    }

    /// Compact familiarity line for journals, homes, and waypoints.
    /// Pure presentation — does not require the region to be materialized.
    pub fn familiarity(self) -> String {
        format!("{}, {}, {}", self.district.name(), self.block.name(), self.band.label())
    }
}

fn mix_coords(abs_x: i64, abs_y: i64, salt: u64) -> u64 {
    let x = abs_x as u64;
    let y = abs_y as u64;
    x.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(y.wrapping_mul(0xC2B2_AE3D_27D4_EB4F))
        .wrapping_add(salt)
        .wrapping_mul(0xBF58_476D_1CE4_E5B9)
}

pub fn density_band(abs_x: i64, abs_y: i64) -> DensityBand {
    let dx = (abs_x - GATE_ABS_X).abs();
    let dy = (abs_y - GATE_ABS_Y).abs();
    let chebyshev = dx.max(dy);
    if chebyshev <= NEAR_HUB_MAX {
        DensityBand::NearHub
    } else if chebyshev <= FRINGE_MAX {
        DensityBand::Fringe
    } else {
        DensityBand::Deep
    }
}

pub fn district_at(abs_x: i64, abs_y: i64) -> DistrictKind {
    let dx = abs_x.div_euclid(DISTRICT);
    let dy = abs_y.div_euclid(DISTRICT);
    match mix_coords(dx, dy, 0xD157_0001) % 3 {
        0 => DistrictKind::Residential,
        1 => DistrictKind::Commercial,
        _ => DistrictKind::Civic,
    }
}

pub fn block_at(abs_x: i64, abs_y: i64, district: DistrictKind) -> BlockPattern {
    let bx = abs_x.div_euclid(BLOCK);
    let by = abs_y.div_euclid(BLOCK);
    let roll = mix_coords(bx, by, 0xB10C_0002) % 4;
    // Bias patterns toward district character without forbidding the others.
    match (district, roll) {
        (DistrictKind::Residential, 0 | 1) => BlockPattern::Courtyard,
        (DistrictKind::Residential, 2) => BlockPattern::ServiceYard,
        (DistrictKind::Residential, _) => BlockPattern::ShopRow,
        (DistrictKind::Commercial, 0 | 1) => BlockPattern::ShopRow,
        (DistrictKind::Commercial, 2) => BlockPattern::Plaza,
        (DistrictKind::Commercial, _) => BlockPattern::ServiceYard,
        (DistrictKind::Civic, 0 | 1) => BlockPattern::Plaza,
        (DistrictKind::Civic, 2) => BlockPattern::ServiceYard,
        (DistrictKind::Civic, _) => BlockPattern::Courtyard,
    }
}

fn story_at(pos: WildernessPos) -> Option<StorySite> {
    if pos.identity != gate_identity() {
        return None;
    }
    match (pos.local_x, pos.local_y) {
        (CLINIC_DIRECTORY_LOCAL_X, CLINIC_DIRECTORY_LOCAL_Y) => Some(StorySite::Directory),
        (CLINIC_ENTRANCE_LOCAL_X, CLINIC_ENTRANCE_LOCAL_Y) => Some(StorySite::Clinic),
        (CLINIC_CORRIDOR_LOCAL_X, CLINIC_CORRIDOR_LOCAL_Y) => Some(StorySite::Corridor),
        _ => None,
    }
}

fn trace_at(
    abs_x: i64,
    abs_y: i64,
    band: DensityBand,
    district: DistrictKind,
) -> Option<TraceKind> {
    // Skip gate absolute tile — authored seam owns the prose.
    if abs_x == GATE_ABS_X && abs_y == GATE_ABS_Y {
        return None;
    }
    let roll = mix_coords(abs_x, abs_y, 0x7ACE_0003) % 100;
    // Texture clusters: denser near Hub, quieter in the deep (longer ordinary stretches).
    let threshold = match band {
        DensityBand::NearHub => 28,
        DensityBand::Fringe => 22,
        DensityBand::Deep => 14,
    };
    if roll >= threshold {
        return None;
    }
    let kind_roll = mix_coords(abs_x, abs_y, 0x7ACE_0004) % 7;
    let kind = match kind_roll {
        0 => TraceKind::Directory,
        1 => TraceKind::Notice,
        2 => TraceKind::Mailbox,
        3 => TraceKind::Vending,
        4 => TraceKind::Timetable,
        5 => TraceKind::Barrier,
        _ => TraceKind::Advert,
    };
    // Civic districts prefer notices and timetables when the roll allows a swap.
    if district == DistrictKind::Civic && matches!(kind, TraceKind::Vending | TraceKind::Advert) {
        if kind_roll.is_multiple_of(2) {
            return Some(TraceKind::Notice);
        }
        return Some(TraceKind::Timetable);
    }
    Some(kind)
}

/// Inspect text for a story site, with the journal hint used elsewhere in the field.
pub fn story_sight(site: StorySite) -> super::field::Sight {
    let finding = super::field::AuthoredFinding {
        content_key: site.content_key().to_string(),
        title: site.finding_title().to_string(),
        note: site.finding_note().to_string(),
    };
    super::field::Sight {
        text: format!(
            "{}\nThis could go in your field journal. `journal record`.",
            site.inspect_text()
        ),
        finding: Some(finding),
    }
}

pub fn story_finding(site: StorySite) -> super::field::AuthoredFinding {
    super::field::AuthoredFinding {
        content_key: site.content_key().to_string(),
        title: site.finding_title().to_string(),
        note: site.finding_note().to_string(),
    }
}

pub fn story_aliases(site: StorySite, target: &str) -> bool {
    let t = target.to_ascii_lowercase();
    match site {
        StorySite::Directory => {
            matches!(t.as_str(), "directory" | "lobby" | "plaque" | "ward")
                || t.contains("clinic") && t.contains("directory")
        }
        StorySite::Clinic => {
            matches!(t.as_str(), "clinic" | "notice" | "door" | "shutter" | "doors")
        }
        StorySite::Corridor => {
            matches!(t.as_str(), "corridor" | "service" | "scaffold" | "tools" | "brick")
        }
    }
}

pub fn trace_aliases(kind: TraceKind, target: &str) -> bool {
    let t = target.to_ascii_lowercase();
    t == kind.keyword()
        || t == kind.label()
        || match kind {
            TraceKind::Directory => matches!(t.as_str(), "plaque" | "lobby"),
            TraceKind::Notice => matches!(t.as_str(), "board" | "sheet"),
            TraceKind::Mailbox => matches!(t.as_str(), "mail" | "post"),
            TraceKind::Vending => matches!(t.as_str(), "machine" | "vendor"),
            TraceKind::Timetable => matches!(t.as_str(), "schedule" | "transit"),
            TraceKind::Barrier => matches!(t.as_str(), "tape" | "works"),
            TraceKind::Advert => matches!(t.as_str(), "ad" | "poster" | "advertisement"),
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geography::generate::{fingerprint, generate_region, GENERATION_V1};
    use crate::geography::identity::{gate_position, RegionIdentity, WildernessPos};

    fn pos(origin_x: i32, origin_y: i32, local_x: u8, local_y: u8) -> WildernessPos {
        WildernessPos {
            identity: RegionIdentity {
                world: crate::geography::identity::WorldId::Undernet,
                epoch_id: 1,
                origin_x,
                origin_y,
                generation_version: GENERATION_V1,
            },
            local_x,
            local_y,
        }
    }

    #[test]
    fn density_does_not_change_the_origin_fingerprint() {
        let region = generate_region(gate_identity()).unwrap();
        assert_eq!(fingerprint(&region), 0x8812_F1DB_241C_39A1);
    }

    #[test]
    fn identical_inputs_yield_identical_urban_context() {
        let a = UrbanContext::at(pos(1, 0, 3, 9));
        let b = UrbanContext::at(pos(1, 0, 3, 9));
        assert_eq!(a, b);
    }

    #[test]
    fn adjacent_tiles_in_a_block_share_pattern() {
        // Absolute (0,0)-(3,3) are one 4×4 block.
        let a = UrbanContext::at(pos(0, 0, 0, 0));
        let b = UrbanContext::at(pos(0, 0, 3, 3));
        assert_eq!(a.block, b.block);
        assert_eq!(a.district, b.district);
    }

    #[test]
    fn block_pattern_is_stable_across_region_border() {
        // Absolute x=15 is region (0,0) local 15; x=16 is region (1,0) local 0.
        // Both sit in block floor(x/4)=3 when y aligns — pick y=4..7 → block y=1.
        let west = pos(0, 0, 15, 5);
        let east = pos(1, 0, 0, 5);
        assert_eq!(west.absolute(), (15, 5));
        assert_eq!(east.absolute(), (16, 5));
        // Different 4×4 blocks (3 vs 4), but same 32-tile district cell (0).
        assert_eq!(district_at(15, 5), district_at(16, 5));
        // Within one block, shared pattern:
        let left = UrbanContext::at(pos(0, 0, 12, 5));
        let right = UrbanContext::at(pos(0, 0, 15, 5));
        assert_eq!(left.block, right.block);
    }

    #[test]
    fn near_hub_band_covers_the_gate() {
        let (x, y) = gate_position().absolute();
        assert_eq!(density_band(x, y), DensityBand::NearHub);
        assert_eq!(density_band(x + 60, y), DensityBand::Deep);
    }

    #[test]
    fn clinic_story_sites_are_distinct_and_authored() {
        let directory =
            UrbanContext::at(pos(0, 0, CLINIC_DIRECTORY_LOCAL_X, CLINIC_DIRECTORY_LOCAL_Y));
        let clinic = UrbanContext::at(pos(0, 0, CLINIC_ENTRANCE_LOCAL_X, CLINIC_ENTRANCE_LOCAL_Y));
        let corridor =
            UrbanContext::at(pos(0, 0, CLINIC_CORRIDOR_LOCAL_X, CLINIC_CORRIDOR_LOCAL_Y));
        assert_eq!(directory.story, Some(StorySite::Directory));
        assert_eq!(clinic.story, Some(StorySite::Clinic));
        assert_eq!(corridor.story, Some(StorySite::Corridor));
        assert!(directory.trace.is_none());
        assert!(clinic.detail_look_line().unwrap().contains("inspect clinic"));
        assert!(corridor.inspect_text_for_tests().contains("work order"));
    }

    impl UrbanContext {
        fn inspect_text_for_tests(self) -> String {
            if let Some(story) = self.story {
                return story.inspect_text();
            }
            if let Some(trace) = self.trace {
                return trace.inspect_text(self.band, self.district);
            }
            String::new()
        }
    }

    #[test]
    fn ordinary_tiles_are_the_majority_in_a_sample() {
        let mut ordinary = 0u32;
        let mut textured = 0u32;
        let mut story = 0u32;
        for ox in 0..3 {
            for oy in 0..3 {
                for lx in 0..16u8 {
                    for ly in 0..16u8 {
                        let ctx = UrbanContext::at(pos(ox, oy, lx, ly));
                        if ctx.story.is_some() {
                            story += 1;
                        } else if ctx.trace.is_some() {
                            textured += 1;
                        } else {
                            ordinary += 1;
                        }
                    }
                }
            }
        }
        let total = ordinary + textured + story;
        assert_eq!(total, 9 * 256);
        // Story is exactly three authored tiles in origin; elsewhere zero.
        assert_eq!(story, 3);
        let ordinary_pct = f64::from(ordinary) / f64::from(total);
        let texture_pct = f64::from(textured) / f64::from(total);
        assert!(ordinary_pct > 0.60, "ordinary {ordinary_pct:.2} textured {texture_pct:.2}");
        assert!(
            texture_pct > 0.10 && texture_pct < 0.35,
            "ordinary {ordinary_pct:.2} textured {texture_pct:.2}"
        );
    }

    #[test]
    fn ward_clinic_inspect_texts_support_the_closure_paradox() {
        let directory = story_sight(StorySite::Directory).text;
        let clinic = story_sight(StorySite::Clinic).text;
        let corridor = story_sight(StorySite::Corridor).text;
        assert!(directory.contains("CLOSED") && directory.contains("corridor job"), "{directory}");
        assert!(clinic.contains("corridor") && clinic.contains("tape"), "{clinic}");
        let corridor_lower = corridor.to_ascii_lowercase();
        assert!(corridor_lower.contains("scuff") && corridor_lower.contains("seal"), "{corridor}");
        assert!(
            corridor_lower.contains("not written"),
            "corridor should leave uncertainty explicit: {corridor}"
        );
    }

    #[test]
    fn clinic_story_tiles_are_walkable_on_effective_baseline() {
        use crate::geography::generate::generate_region;
        use crate::geography::travel::effective_tile;
        use crate::geography::GeographyState;
        let generated = generate_region(gate_identity()).unwrap();
        let geo = GeographyState::default();
        for (lx, ly) in [
            (CLINIC_DIRECTORY_LOCAL_X, CLINIC_DIRECTORY_LOCAL_Y),
            (CLINIC_ENTRANCE_LOCAL_X, CLINIC_ENTRANCE_LOCAL_Y),
            (CLINIC_CORRIDOR_LOCAL_X, CLINIC_CORRIDOR_LOCAL_Y),
        ] {
            let p = pos(0, 0, lx, ly);
            let baseline = generated.tile(lx, ly).unwrap();
            let tile = effective_tile(p, baseline, &geo);
            assert!(
                tile.traversable(),
                "clinic story tile {lx},{ly} should be walkable, got {:?}",
                tile.terrain
            );
        }
    }

    #[test]
    fn playtest_route_sample_reports_density_feel() {
        use crate::geography::generate::generate_region;
        use crate::geography::travel::{describe_tile, effective_tile, step_origin, Cardinal};
        use crate::geography::GeographyState;
        // Zig-zag walk from the gate so the sample crosses block and district cells.
        let identity = gate_identity();
        let mut origin_x = 0i32;
        let mut origin_y = 0i32;
        let mut lx = 8u8;
        let mut ly = 8u8;
        let geo = GeographyState::default();
        let mut ordinary = 0u32;
        let mut textured = 0u32;
        let mut story = 0u32;
        let mut landmarks = 0u32;
        let mut districts = std::collections::BTreeSet::new();
        let mut blocks = std::collections::BTreeSet::new();
        let dirs = [
            Cardinal::West,
            Cardinal::West,
            Cardinal::North,
            Cardinal::West,
            Cardinal::South,
            Cardinal::West,
            Cardinal::North,
            Cardinal::North,
        ];
        for step in 0..40 {
            let pos = pos(origin_x, origin_y, lx, ly);
            let generated = generate_region(pos.identity).unwrap();
            let baseline = generated.tile(lx, ly).unwrap();
            let tile = effective_tile(pos, baseline, &geo);
            if !tile.traversable() {
                break;
            }
            let ctx = UrbanContext::at(pos);
            districts.insert(ctx.district.short());
            blocks.insert(ctx.block.name());
            if ctx.story.is_some() {
                story += 1;
            } else if ctx.trace.is_some() {
                textured += 1;
            } else {
                ordinary += 1;
            }
            if tile.landmark.is_some() {
                landmarks += 1;
            }
            let _scene = describe_tile(pos, &generated, &geo, &[], &[]);
            let prefer = dirs[step % dirs.len()];
            let mut order =
                vec![prefer, Cardinal::West, Cardinal::North, Cardinal::East, Cardinal::South];
            order.dedup();
            let mut stepped = false;
            for dir in order {
                let Ok((ox, oy, nx, ny)) = step_origin(pos, dir) else {
                    continue;
                };
                let next_id = RegionIdentity { origin_x: ox, origin_y: oy, ..identity };
                let next_gen = generate_region(next_id).unwrap();
                let next_pos = WildernessPos { identity: next_id, local_x: nx, local_y: ny };
                let next_base = next_gen.tile(nx, ny).unwrap();
                if effective_tile(next_pos, next_base, &geo).traversable() {
                    origin_x = ox;
                    origin_y = oy;
                    lx = nx;
                    ly = ny;
                    stepped = true;
                    break;
                }
            }
            if !stepped {
                break;
            }
        }
        let total = ordinary + textured + story;
        assert!(total >= 20, "expected a continuous walk, got {total}");
        assert!(ordinary * 100 / total >= 50, "ordinary={ordinary} textured={textured}");
        assert!(textured >= 1, "expected some environmental texture");
        assert!(!districts.is_empty());
        assert!(!blocks.is_empty());
        assert_eq!(fingerprint(&generate_region(gate_identity()).unwrap()), 0x8812_F1DB_241C_39A1);
        let _ = (story, landmarks, districts, blocks);
    }
}
