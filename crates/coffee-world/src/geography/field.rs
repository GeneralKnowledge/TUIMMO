//! Authored field observations layered on the generated wilderness and the Undernet.
//!
//! Terrain does not read this module. A discovery record does not change a tile.
//! Notable landmarks are chosen by a fixed salt of absolute coordinates, so the
//! same place always yields the same text.

use super::generate::LandmarkKind;
use super::identity::{gate_identity, WildernessPos, GATE_LOCAL_X, GATE_LOCAL_Y};
use crate::undernet;

pub const SURVEY_SCRATCH_ID: &str = "survey-scratch";
pub const SURVEY_MARK_ID: &str = "survey-mark";
pub const SURVEY_CLOSURE_ID: &str = "survey-closure";

/// Set when an operator has examined the outskirts mark. Not a quest acceptance.
pub const SURVEY_MARK_FLAG: &str = "survey-mark";

/// Authored site: one step west of the city gate. Version 1 leaves this tile
/// walkable and without a generated landmark. The stone is an overlay, not a
/// change to `generate_region`.
pub const SURVEY_LOCAL_X: u8 = 7;
pub const SURVEY_LOCAL_Y: u8 = 8;

const JOURNAL_HINT: &str = "This could go in your field journal. `journal record`.";
const OVERVIEW_LIMIT: usize = 8;
const RECENT_LIMIT: usize = 5;
const RECENT_NOTE_CHARS: usize = 68;

pub fn survey_position() -> WildernessPos {
    WildernessPos { identity: gate_identity(), local_x: SURVEY_LOCAL_X, local_y: SURVEY_LOCAL_Y }
}

pub fn is_survey_tile(pos: WildernessPos) -> bool {
    pos == survey_position()
}

pub fn survey_look_line() -> &'static str {
    "A flat stone lies in the grass, cut twice with the same short mark. `inspect mark`."
}

/// Something an operator can keep. The note is the finding, without command hints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthoredFinding {
    pub content_key: String,
    pub title: String,
    pub note: String,
}

/// What `inspect` says, and the finding it offers when one should be kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sight {
    pub text: String,
    pub finding: Option<AuthoredFinding>,
}

pub fn landmark_is_notable(abs_x: i64, abs_y: i64) -> bool {
    landmark_salt(abs_x, abs_y).is_multiple_of(4)
}

fn landmark_salt(abs_x: i64, abs_y: i64) -> u64 {
    let x = abs_x as u64;
    let y = abs_y as u64;
    x.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(y).wrapping_mul(0xBF58_476D_1CE4_E5B9)
}

pub fn landmark_content_key(pos: WildernessPos, kind: LandmarkKind) -> String {
    format!(
        "lm:{}:{}:{}:{}:{}:{}",
        pos.identity.world,
        pos.identity.epoch_id,
        kind.keyword(),
        pos.identity.origin_x,
        pos.identity.origin_y,
        format_local(pos.local_x, pos.local_y)
    )
}

fn format_local(local_x: u8, local_y: u8) -> String {
    format!("{local_x}:{local_y}")
}

pub fn landmark_sight(pos: WildernessPos, kind: LandmarkKind) -> Sight {
    let (abs_x, abs_y) = pos.absolute();
    let notable = landmark_is_notable(abs_x, abs_y);
    let (title, body) = landmark_copy(kind, notable);
    if notable {
        let finding = AuthoredFinding {
            content_key: landmark_content_key(pos, kind),
            title: title.to_string(),
            note: body.to_string(),
        };
        Sight { text: format!("{body}\n{JOURNAL_HINT}"), finding: Some(finding) }
    } else {
        Sight { text: body.to_string(), finding: None }
    }
}

fn landmark_copy(kind: LandmarkKind, notable: bool) -> (&'static str, &'static str) {
    match (kind, notable) {
        (LandmarkKind::Spring, false) => (
            "Spring",
            "Spring. Clear water beads from a crack. The ground around it is darker and cold.",
        ),
        (LandmarkKind::Spring, true) => (
            "Wire in the spring",
            "Spring. Clear water beads from a crack, and a length of insulated wire is caught in it, still cold.",
        ),
        (LandmarkKind::StandingStone, false) => (
            "Standing stone",
            "Standing stone. It is waist-high, with lichen on the lee side. No inscription you can read.",
        ),
        (LandmarkKind::StandingStone, true) => (
            "Repeated cut",
            "Standing stone. The same short mark is cut twice, one above the other, fresh enough to catch a nail.",
        ),
        (LandmarkKind::FallenMast, false) => (
            "Fallen mast",
            "Fallen mast. The aluminum is bent and half buried. The elements point at the dirt. There is no power.",
        ),
        (LandmarkKind::FallenMast, true) => (
            "Clean connector",
            "Fallen mast. A coax connector at the base is cleaner than the rest. Someone seated a cable here and pulled it free.",
        ),
        (LandmarkKind::Milepost, false) => (
            "Milepost",
            "Milepost. The post is city-issue. The mileage plate is blank from weather. No destination remains.",
        ),
        (LandmarkKind::Milepost, true) => (
            "Scratched mile",
            "Milepost. One destination is scratched out. Under it, a newer hand has cut a dash and stopped.",
        ),
    }
}

pub fn survey_aliases(target: &str) -> bool {
    matches!(target, "mark" | "stone" | "cairn" | "repeated")
}

pub fn survey_finding() -> AuthoredFinding {
    AuthoredFinding {
        content_key: SURVEY_MARK_ID.to_string(),
        title: "Repeated mark".into(),
        note: "A flat stone one step west of the city gate is cut twice with the same mark. \
               A scrap of relay insulation is pinched under it."
            .into(),
    }
}

pub fn survey_sight() -> Sight {
    let finding = survey_finding();
    Sight {
        text: format!(
            "Repeated mark. The same short cut is made twice, one above the other. \
             A grey fiber scrap is pinched under the stone. It is relay insulation, \
             and it does not say which dish it left.\n{JOURNAL_HINT}"
        ),
        finding: Some(finding),
    }
}

pub fn roof_scratch() -> AuthoredFinding {
    AuthoredFinding {
        content_key: SURVEY_SCRATCH_ID.to_string(),
        title: "Gate scratch".into(),
        note: "Under the roof mast's tape, a scratch reads: one step west of the gate. \
               The mark repeats."
            .into(),
    }
}

pub fn survey_closure() -> AuthoredFinding {
    AuthoredFinding {
        content_key: SURVEY_CLOSURE_ID.to_string(),
        title: "Unfiled survey".into(),
        note: "The scuffed dish cable matches the fiber under the outskirts stone. \
               The relay crew turned the dish toward the gate, closed the survey with \
               the repeated mark, and never filed it. The archive was already sealed."
            .into(),
    }
}

pub fn closure_inspect_text() -> String {
    format!("{}\n{JOURNAL_HINT}", survey_closure().note)
}

/// One field note for the delisted floor. The panel, the torn page, and the archive
/// shelf can all record this same key. None of them open the Black Archive.
pub const DELISTED_FLOOR_ID: &str = "delisted-floor";

pub fn delisted_floor() -> AuthoredFinding {
    AuthoredFinding {
        content_key: DELISTED_FLOOR_ID.to_string(),
        title: "Delisted floor".into(),
        note: "Service line B-4 was struck from the public list. \
               The lock was left with the relay."
            .into(),
    }
}

pub fn delisted_panel_waiting() -> &'static str {
    "The lowest line is crossed out. The code is still there, \
     and you have nothing yet that explains the strike."
}

pub fn delisted_terminal_waiting() -> &'static str {
    "The terminal still has a page mounted. You have not read it."
}

/// Personal rereading of the shared panel, once this operator holds the torn page.
pub fn panel_after_log() -> String {
    format!(
        "B-4 matches the torn page. Struck from the public list; the code was left.\n{JOURNAL_HINT}"
    )
}

/// The readable page, then how to keep it. The carried item keeps the page alone.
pub fn torn_page_text() -> String {
    format!("{}\nYou tear the page free. `journal record`.", undernet::TORN_LOG_DETAIL)
}

/// Corroboration on the archive shelf. The gap stays empty.
pub fn stacks_after_log() -> &'static str {
    "The empty shelf is tagged B-4, the code from the torn log."
}

/// Readable label and a way back with existing movement commands.
pub fn describe_place(place_key: &str) -> (String, String) {
    if let Some(pos) = WildernessPos::parse(place_key) {
        let label = format!(
            "outskirts, birth ({}, {}), tile {},{}",
            pos.identity.origin_x, pos.identity.origin_y, pos.local_x, pos.local_y
        );
        let hint = if pos.identity == gate_identity() {
            gate_return(pos.local_x, pos.local_y)
        } else {
            format!(
                "Birth region ({}, {}), tile {},{}. The city gate is birth (0, 0), tile {},{}.",
                pos.identity.origin_x,
                pos.identity.origin_y,
                pos.local_x,
                pos.local_y,
                GATE_LOCAL_X,
                GATE_LOCAL_Y
            )
        };
        return (label, hint);
    }
    if let Some(loc) = undernet::location(place_key) {
        let hint = match place_key {
            "roof-array" => "From the Coffee Shop, go up.",
            "old-relay" => "From the Cooling Tunnels or the Abandoned Exchange, go relay.",
            "abandoned-exchange" => "From the Service Corridor, go down.",
            "service-corridor" => "From the Exchange, go service.",
            "coffee-shop" => "New operators appear here.",
            "black-archive" => "From the Old Relay, go archive, once the door answers.",
            _ => "map shows the Undernet exits.",
        };
        return (loc.name.to_string(), hint.to_string());
    }
    (format!("unknown place {place_key}"), "That place key is not on the current map.".to_string())
}

fn gate_return(local_x: u8, local_y: u8) -> String {
    let dx = i32::from(local_x) - i32::from(GATE_LOCAL_X);
    let dy = i32::from(local_y) - i32::from(GATE_LOCAL_Y);
    if dx == 0 && dy == 0 {
        return "You are on the city gate. city returns to the Roof Array.".into();
    }
    let mut steps = Vec::new();
    push_steps(&mut steps, if dx > 0 { "east" } else { "west" }, dx.unsigned_abs());
    push_steps(&mut steps, if dy > 0 { "north" } else { "south" }, dy.unsigned_abs());
    format!("From the city gate: {}. city returns to the Roof Array.", steps.join(", "))
}

fn push_steps(steps: &mut Vec<String>, word: &str, count: u32) {
    for _ in 0..count {
        steps.push(word.to_string());
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalLine {
    pub content_key: String,
    pub title: String,
    pub note: String,
    pub place_key: String,
    pub recorded_at: u64,
}

pub fn format_overview(entries: &[JournalLine]) -> String {
    if entries.is_empty() {
        return "Your field journal is empty.\n\
                Only you can read it.\n\
                Inspect something unusual, then `journal record`.\n\
                `look` returns to the place."
            .into();
    }
    let mut lines = vec![format!("Field journal ({})", entries.len())];
    let shown = entries.iter().take(OVERVIEW_LIMIT);
    for entry in shown {
        lines.push(format!("{}  {}", entry.content_key, entry.title));
    }
    if entries.len() > OVERVIEW_LIMIT {
        lines.push(format!(
            "Showing {OVERVIEW_LIMIT} of {}. `journal recent` lists the latest.",
            entries.len()
        ));
    }
    lines.push("`journal read <id>`".into());
    lines.push("`look` returns to the place.".into());
    lines.join("\n")
}

pub fn format_recent(entries: &[JournalLine]) -> String {
    if entries.is_empty() {
        return format_overview(entries);
    }
    let mut lines = vec!["Recent field notes".to_string()];
    for entry in entries.iter().take(RECENT_LIMIT) {
        let (label, _) = describe_place(&entry.place_key);
        lines.push(format!("{}  {}", entry.content_key, entry.title));
        lines.push(format!("  {label}"));
        lines.push(format!("  {}", clip_chars(&entry.note, RECENT_NOTE_CHARS)));
    }
    lines.push("`journal read <id>`".into());
    lines.push("`look` returns to the place.".into());
    lines.join("\n")
}

pub fn format_entry(entry: &JournalLine) -> String {
    let (label, hint) = describe_place(&entry.place_key);
    format!(
        "{key} — {title}\n{label}\n{note}\nReturn: {hint}\nPlace: {place}\nRecorded: {when}\n`look` returns to the place.",
        key = entry.content_key,
        title = entry.title,
        note = entry.note,
        place = entry.place_key,
        when = format_recorded(entry.recorded_at),
    )
}

fn clip_chars(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if out.chars().count() >= max_chars {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

fn format_recorded(at: u64) -> String {
    if at < 1_000_000_000_000 {
        return format!("sequence {at}");
    }
    let secs = at / 1000;
    let days = i64::try_from(secs / 86_400).unwrap_or(i64::MAX);
    let tod = secs % 86_400;
    let hour = tod / 3600;
    let minute = (tod % 3600) / 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

/// Howard Hinnant's civil_from_days, for UTC dates without a time crate.
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    (year as i32, month as u32, day as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geography::generate::{fingerprint, generate_region, LandmarkKind};
    use crate::geography::identity::gate_identity;

    #[test]
    fn landmark_copy_is_stable_and_distinct() {
        let pos = WildernessPos { identity: gate_identity(), local_x: 5, local_y: 13 };
        let first = landmark_sight(pos, LandmarkKind::StandingStone);
        let again = landmark_sight(pos, LandmarkKind::StandingStone);
        assert_eq!(first, again);
        assert!(first.text.starts_with("Standing stone."));
        let spring = landmark_sight(pos, LandmarkKind::Spring);
        assert!(spring.text.starts_with("Spring."));
        assert_ne!(first.text, spring.text);
        let mile = landmark_sight(pos, LandmarkKind::Milepost);
        let mast = landmark_sight(pos, LandmarkKind::FallenMast);
        assert!(mile.text.starts_with("Milepost."));
        assert!(mast.text.starts_with("Fallen mast."));
        assert_ne!(first.text, mile.text);
        assert_ne!(mile.text, mast.text);
    }

    #[test]
    fn notable_flag_depends_only_on_coordinates() {
        let (x, y) = survey_position().absolute();
        assert_eq!(landmark_is_notable(x, y), landmark_is_notable(x, y));
        let region = generate_region(gate_identity()).unwrap();
        assert_eq!(fingerprint(&region), 0x8812_F1DB_241C_39A1);
        let tile = region.tile(SURVEY_LOCAL_X, SURVEY_LOCAL_Y).unwrap();
        assert!(tile.terrain.traversable());
        assert!(tile.landmark.is_none());
        let mut saw_notable = false;
        let mut saw_ordinary = false;
        for local_y in 0..16u8 {
            for local_x in 0..16u8 {
                let tile = region.tile(local_x, local_y).unwrap();
                let Some(kind) = tile.landmark else { continue };
                let pos = WildernessPos { identity: gate_identity(), local_x, local_y };
                let sight = landmark_sight(pos, kind);
                let key = landmark_content_key(pos, kind);
                assert_eq!(landmark_content_key(pos, kind), key);
                if sight.finding.is_some() {
                    saw_notable = true;
                    assert!(sight.text.contains("journal record"));
                } else {
                    saw_ordinary = true;
                    assert!(!sight.text.contains("journal record"));
                }
            }
        }
        assert!(saw_notable && saw_ordinary);
    }

    #[test]
    fn survey_texts_do_not_contain_the_conclusion() {
        let scratch = roof_scratch();
        let mark = survey_sight();
        assert!(!scratch.note.contains("never filed"));
        assert!(!mark.text.contains("never filed"));
        assert!(mark.text.contains("does not say which dish"));
        assert!(survey_closure().note.contains("never filed"));
        assert!(survey_look_line().contains("inspect mark"));
        assert!(!survey_closure().note.contains("B-4"));
        assert!(!delisted_floor().note.contains("never filed"));
        assert!(!delisted_floor().note.contains("cipher"));
        assert!(delisted_floor().note.contains("B-4"));
        assert!(delisted_floor().note.contains("public list"));
        assert!(panel_after_log().contains("public list"));
        assert!(torn_page_text().contains("service panel"));
        assert!(!stacks_after_log().contains("journal record"));
        assert!(!delisted_panel_waiting().contains("public list"));
    }

    #[test]
    fn journal_formats_stay_narrow_and_private_to_the_list() {
        let entry = JournalLine {
            content_key: SURVEY_MARK_ID.into(),
            title: "Repeated mark".into(),
            note: survey_finding().note,
            place_key: survey_position().key(),
            recorded_at: 4,
        };
        let overview = format_overview(std::slice::from_ref(&entry));
        assert!(overview.contains(SURVEY_MARK_ID));
        assert!(!overview.contains("relay insulation"));
        let recent = format_recent(std::slice::from_ref(&entry));
        assert!(recent.contains("outskirts"));
        let full = format_entry(&entry);
        assert!(full.contains("From the city gate: west."));
        assert!(full.contains("sequence 4"));
        assert!(full.contains(&survey_position().key()));
        assert!(format_overview(&[]).contains("empty"));
    }

    #[test]
    fn unix_millis_format_as_utc() {
        assert_eq!(format_recorded(0), "sequence 0");
        assert_eq!(format_recorded(1_700_000_000_000), "2023-11-14 22:13 UTC");
    }
}
