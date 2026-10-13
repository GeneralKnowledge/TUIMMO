//! Personal homes in the Squalor.
//!
//! A home is a designation on an existing wilderness place key. It is not a
//! cabin, a settlement claim, a waypoint, or a journal entry. Construction is
//! not required. Abandoned homes keep a public world record; private player
//! data never moves into the home row.
//!
//! Active homes gain habitation through elapsed claim time and meaningful
//! returns. Distinctive details are derived from the owner's existing journal
//! notes, discovery flags, and entered regions — never invented.

use terminus_core::text::clean_line;
use terminus_core::PlayerId;
use uuid::Uuid;

/// Elapsed real time after abandonment before the "forgotten" stage.
pub const RECENTLY_ABANDONED_MS: u64 = 7 * 24 * 60 * 60 * 1000;
/// Elapsed real time after abandonment before the home presentation fades.
pub const FORGOTTEN_MS: u64 = 30 * 24 * 60 * 60 * 1000;

/// Claim age before a home can feel like settling in (time alone).
pub const SETTLING_TIME_MS: u64 = 24 * 60 * 60 * 1000;
/// Claim age before a home can feel established (time alone).
pub const ESTABLISHED_TIME_MS: u64 = 7 * 24 * 60 * 60 * 1000;
/// Minimum absence before an arrival counts as a meaningful return.
pub const MIN_AWAY_MS: u64 = 10 * 60 * 1000;
/// Returns needed (with or without time) to reach settling in.
pub const SETTLING_RETURNS: u32 = 1;
/// Returns needed (with or without time) to reach established.
pub const ESTABLISHED_RETURNS: u32 = 3;

pub const NAME_MIN: usize = 2;
pub const NAME_MAX: usize = 24;
pub const DESCRIPTION_MAX: usize = 160;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HomeStage {
    Active,
    RecentlyAbandoned,
    Forgotten,
    Absorbed,
}

impl HomeStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::RecentlyAbandoned => "recently abandoned",
            Self::Forgotten => "forgotten",
            Self::Absorbed => "absorbed",
        }
    }
}

/// How strongly a home had been lived in before abandonment (derived from durable rows).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum LivedInTrace {
    Faint,
    Moderate,
    Deep,
}

/// Descriptive habitation for an active home. Not a character level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Habitation {
    Unsettled,
    SettlingIn,
    Established,
    Distinctive,
}

impl Habitation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unsettled => "unsettled",
            Self::SettlingIn => "settling in",
            Self::Established => "established",
            Self::Distinctive => "distinctive",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Unsettled => "Unsettled",
            Self::SettlingIn => "Settling in",
            Self::Established => "Established",
            Self::Distinctive => "Distinctive",
        }
    }
}

/// Owner exploration that can flavor an established home. Derived at read time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExplorationTraits {
    pub journal_notes: bool,
    pub survey_interest: bool,
    pub districts: bool,
    pub subway: bool,
}

impl ExplorationTraits {
    pub fn any(self) -> bool {
        self.journal_notes || self.survey_interest || self.districts || self.subway
    }

    pub fn bit_mask(self) -> u8 {
        let mut bits = 0u8;
        if self.journal_notes {
            bits |= 1;
        }
        if self.survey_interest {
            bits |= 2;
        }
        if self.districts {
            bits |= 4;
        }
        if self.subway {
            bits |= 8;
        }
        bits
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonalHome {
    pub id: Uuid,
    pub operator_id: PlayerId,
    pub place_key: String,
    pub claimed_at: u64,
    pub abandoned_at: Option<u64>,
    /// Public title. Empty means the default untitled presentation.
    pub public_name: String,
    pub public_description: String,
    pub meaningful_returns: u32,
    pub last_departed_at: Option<u64>,
    pub last_returned_at: Option<u64>,
}

impl PersonalHome {
    pub fn is_active(&self) -> bool {
        self.abandoned_at.is_none()
    }

    pub fn display_name(&self) -> Option<&str> {
        if self.public_name.is_empty() {
            None
        } else {
            Some(self.public_name.as_str())
        }
    }

    pub fn stage_at(&self, now_ms: u64) -> HomeStage {
        match self.abandoned_at {
            None => HomeStage::Active,
            Some(abandoned_at) => {
                let elapsed = now_ms.saturating_sub(abandoned_at);
                if elapsed < RECENTLY_ABANDONED_MS {
                    HomeStage::RecentlyAbandoned
                } else if elapsed < FORGOTTEN_MS {
                    HomeStage::Forgotten
                } else {
                    HomeStage::Absorbed
                }
            }
        }
    }

    /// Habitation from claim age and meaningful returns only (no exploration).
    pub fn base_habitation(&self, now_ms: u64) -> Habitation {
        if !self.is_active() {
            return Habitation::Unsettled;
        }
        let age = now_ms.saturating_sub(self.claimed_at);
        let by_time = if age >= ESTABLISHED_TIME_MS {
            Habitation::Established
        } else if age >= SETTLING_TIME_MS {
            Habitation::SettlingIn
        } else {
            Habitation::Unsettled
        };
        let by_returns = if self.meaningful_returns >= ESTABLISHED_RETURNS {
            Habitation::Established
        } else if self.meaningful_returns >= SETTLING_RETURNS {
            Habitation::SettlingIn
        } else {
            Habitation::Unsettled
        };
        by_time.max(by_returns)
    }

    pub fn habitation(&self, now_ms: u64, traits: ExplorationTraits) -> Habitation {
        let base = self.base_habitation(now_ms);
        if base >= Habitation::Established && traits.any() {
            Habitation::Distinctive
        } else {
            base
        }
    }

    /// Frozen history at abandonment: tenure and counted returns only (no private data).
    fn lived_in_trace_at_abandon(&self) -> LivedInTrace {
        let Some(abandoned_at) = self.abandoned_at else {
            return LivedInTrace::Faint;
        };
        let tenure = abandoned_at.saturating_sub(self.claimed_at);
        if self.meaningful_returns >= ESTABLISHED_RETURNS || tenure >= ESTABLISHED_TIME_MS {
            LivedInTrace::Deep
        } else if self.meaningful_returns >= SETTLING_RETURNS || tenure >= SETTLING_TIME_MS {
            LivedInTrace::Moderate
        } else {
            LivedInTrace::Faint
        }
    }

    /// Public look line. Exploration details stay generic; private keys stay out.
    pub fn look_line(
        &self,
        now_ms: u64,
        viewer_is_owner: bool,
        traits: ExplorationTraits,
    ) -> Option<String> {
        match self.stage_at(now_ms) {
            HomeStage::Active => {
                let named = match self.display_name() {
                    Some(name) => format!(" \"{name}\""),
                    None => String::new(),
                };
                let detail = active_detail(self, now_ms, traits);
                if viewer_is_owner {
                    Some(format!("You have made a home here{named}. {detail}"))
                } else {
                    Some(format!("Someone has made a home here{named}. {detail}"))
                }
            }
            HomeStage::RecentlyAbandoned => {
                let named = match self.display_name() {
                    Some(name) => format!(" once called \"{name}\""),
                    None => String::new(),
                };
                Some(recently_abandoned_look_line(named, self.lived_in_trace_at_abandon()))
            }
            HomeStage::Forgotten => {
                let named = match self.display_name() {
                    Some(name) => format!(" once called \"{name}\""),
                    None => String::new(),
                };
                Some(forgotten_look_line(named, self.lived_in_trace_at_abandon()))
            }
            HomeStage::Absorbed => None,
        }
    }

    /// Owner-facing status lines (no raw timestamps or ids).
    pub fn owner_status_lines(&self, now_ms: u64, traits: ExplorationTraits) -> Vec<String> {
        let habitation = self.habitation(now_ms, traits);
        let mut lines = Vec::new();
        match self.display_name() {
            Some(name) => lines.push(format!("Name: {name}")),
            None => lines.push("Name: (untitled — `home name <name>`)".into()),
        }
        lines.push(format!("Condition: {}", habitation.label()));
        lines.push(habitation_blurb(habitation).into());
        lines.push(self.settlement_progress_line(now_ms));
        if habitation == Habitation::Distinctive {
            lines.extend(owner_exploration_lines(traits));
        } else if traits.any() && habitation < Habitation::Established {
            lines.push(
                "Your notes and travels will mark this place once it feels established.".into(),
            );
        }
        lines.push("Names are public on the street. Journals and waypoints stay private.".into());
        lines
    }

    /// Owner-only line when a meaningful return was just recorded (movement arrival).
    pub fn meaningful_return_message(&self, now_ms: u64, traits: ExplorationTraits) -> String {
        let named = self.display_name().map(|name| format!(" \"{name}\"")).unwrap_or_default();
        match self.habitation(now_ms, traits) {
            Habitation::Distinctive => format!(
                "You return to your{named} home. It feels lived in — marked by what you have actually found."
            ),
            Habitation::Established => {
                "You return home after time away. The room feels kept; dust stays outside.".into()
            }
            Habitation::SettlingIn => {
                "You return home after a real stretch away. The place is beginning to feel like yours.".into()
            }
            Habitation::Unsettled => {
                "You return after time away. The claim is still new, but coming back matters.".into()
            }
        }
    }

    /// Owner-only progress toward settling (no raw timestamps).
    pub fn settlement_progress_line(&self, now_ms: u64) -> String {
        if !self.is_active() {
            return String::new();
        }
        let returns = self.meaningful_returns;
        let returns_part = if returns == 0 {
            "Counted returns from expeditions: none yet (leave for at least ten minutes, then return)."
                .to_string()
        } else if returns < ESTABLISHED_RETURNS {
            format!(
                "Counted returns from expeditions: {returns} (three returns deepen the settled feeling)."
            )
        } else {
            format!("Counted returns from expeditions: {returns}.")
        };
        let age = now_ms.saturating_sub(self.claimed_at);
        let time_part = if age >= ESTABLISHED_TIME_MS {
            " You have kept the claim for over a week."
        } else if age >= SETTLING_TIME_MS {
            " You have kept the claim for over a day."
        } else {
            " The claim is still new."
        };
        format!("{returns_part}{time_part}")
    }
}

fn active_detail(home: &PersonalHome, now_ms: u64, traits: ExplorationTraits) -> String {
    let habitation = home.habitation(now_ms, traits);
    match habitation {
        Habitation::Unsettled => {
            "The space is still sparse and exposed — claimed, but not yet settled.".into()
        }
        Habitation::SettlingIn => {
            "A corner has been cleared. The place feels less exposed than the street around it."
                .into()
        }
        Habitation::Established => {
            "It feels lived in. Dust is kept back from a small resting place, and the Squalor waits outside."
                .into()
        }
        Habitation::Distinctive => distinctive_public_detail(home.id, traits),
    }
}

fn distinctive_public_detail(home_id: Uuid, traits: ExplorationTraits) -> String {
    // Deterministic pick among traits that apply. Never names private discoveries.
    let variants: &[&str] = match traits.bit_mask() {
        0 => &["It feels lived in, with a few personal marks against the decay."],
        bits if bits & 1 != 0 && bits & 2 != 0 => &[
            "A desk holds folded notes and a rough chart. Someone has been comparing unmarked places.",
            "Papers and a worn map sit within reach — a kept corner for someone who records what they find.",
        ],
        bits if bits & 1 != 0 => &[
            "A few personal notes rest within reach, as if the owner keeps a private record nearby.",
            "An informal stack of notes sits aside from the dust — nothing shared, but clearly used.",
        ],
        bits if bits & 2 != 0 => &[
            "A rough sketch of bearings leans against the wall — interest in unmarked ground.",
            "Scratched bearings and a short list of landmarks sit where a traveler would leave them.",
        ],
        bits if bits & 8 != 0 => &[
            "A transit strip map is pinned beside the door — Line 8 marks still readable.",
            "Mud from station stairs has been brushed aside near the entrance more than once.",
        ],
        bits if bits & 4 != 0 => &[
            "A rough chart of nearby districts is kept against the wall.",
            "Marks on a salvaged map show more than one birth cell of the Squalor.",
        ],
        _ => &["Personal marks set this kept room apart from the empty towers around it."],
    };
    let idx =
        (home_id.as_u128() as usize).wrapping_add(traits.bit_mask() as usize) % variants.len();
    variants[idx].to_string()
}

fn recently_abandoned_look_line(named: String, trace: LivedInTrace) -> String {
    match trace {
        LivedInTrace::Faint => format!(
            "This place was left behind recently{named}. A corner had been cleared, but it never quite settled into a kept room."
        ),
        LivedInTrace::Moderate => format!(
            "This place was inhabited until recently{named}. Scuffed dust and a kept corner still show through the Squalor."
        ),
        LivedInTrace::Deep => format!(
            "This place was inhabited until recently{named}. It had been kept — dust held back, a path worn in from returns."
        ),
    }
}

fn forgotten_look_line(named: String, trace: LivedInTrace) -> String {
    match trace {
        LivedInTrace::Deep => format!(
            "Dust covers what was once a well-kept home{named}. Even the deeper marks are dissolving into the decay around them."
        ),
        LivedInTrace::Moderate => format!(
            "Dust covers what was once a home{named}. The kept corner is fading into the same grey decay as the towers around it."
        ),
        LivedInTrace::Faint => format!(
            "Dust covers what was once a home{named}. Whatever traces remained have become nearly indistinguishable from the decay surrounding it."
        ),
    }
}

fn habitation_blurb(habitation: Habitation) -> &'static str {
    match habitation {
        Habitation::Unsettled => {
            "Recently claimed. Time and returns from elsewhere will settle it."
        }
        Habitation::SettlingIn => {
            "Familiarity shows. Keep returning after real expeditions and it will deepen."
        }
        Habitation::Established => {
            "Recognisably lived in. Your discoveries can give it a more personal character."
        }
        Habitation::Distinctive => {
            "Lived in, and marked by what you have actually found and recorded."
        }
    }
}

fn owner_exploration_lines(traits: ExplorationTraits) -> Vec<String> {
    let mut lines = vec!["What marks it (only you see this list):".into()];
    if traits.journal_notes {
        lines.push("- Notes from your field journal.".into());
    }
    if traits.survey_interest {
        lines.push("- Interest in unmarked surveys and struck rooms.".into());
    }
    if traits.districts {
        lines.push("- Charts of districts you have opened.".into());
    }
    if traits.subway {
        lines.push("- Familiarity with the municipal subway.".into());
    }
    lines
}

/// Validate a public home name. Display keeps the typed spelling after cleaning.
pub fn validate_home_name(raw: &str) -> Result<String, &'static str> {
    let cleaned = clean_line(raw, NAME_MAX + 1);
    if cleaned.is_empty() {
        return Err("A home name needs some text.");
    }
    let count = cleaned.chars().count();
    if count < NAME_MIN {
        return Err("Home names need at least 2 characters.");
    }
    if count > NAME_MAX {
        return Err("Home names can be at most 24 characters.");
    }
    if cleaned.contains('\n') || cleaned.contains('\r') {
        return Err("Home names must stay on one line.");
    }
    // Refuse names that try to look like system feed lines.
    let lower = cleaned.to_ascii_lowercase();
    if lower.starts_with("you ")
        || lower.starts_with("someone ")
        || lower.starts_with("system:")
        || lower.starts_with('[')
    {
        return Err("That name would look like a system message.");
    }
    Ok(cleaned)
}

/// In-memory home table. Mirrors the SQLite rows, including abandoned history.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HomeState {
    pub homes: Vec<PersonalHome>,
}

impl HomeState {
    pub fn active_for(&self, operator: PlayerId) -> Option<&PersonalHome> {
        self.homes.iter().find(|home| home.operator_id == operator && home.is_active())
    }

    pub fn active_for_mut(&mut self, operator: PlayerId) -> Option<&mut PersonalHome> {
        self.homes.iter_mut().find(|home| home.operator_id == operator && home.is_active())
    }

    pub fn active_at(&self, place_key: &str) -> Option<&PersonalHome> {
        self.homes.iter().find(|home| home.place_key == place_key && home.is_active())
    }

    /// Prefer an active home; otherwise the most recently abandoned non-absorbed record.
    pub fn presentation_at(&self, place_key: &str, now_ms: u64) -> Option<&PersonalHome> {
        if let Some(active) = self.active_at(place_key) {
            return Some(active);
        }
        self.homes
            .iter()
            .filter(|home| home.place_key == place_key && !home.is_active())
            .filter(|home| home.stage_at(now_ms) != HomeStage::Absorbed)
            .max_by_key(|home| home.abandoned_at.unwrap_or(0))
    }

    pub fn insert(&mut self, home: PersonalHome) {
        self.homes.push(home);
    }

    pub fn abandon(&mut self, id: Uuid, abandoned_at: u64) -> bool {
        if let Some(home) = self.homes.iter_mut().find(|home| home.id == id && home.is_active()) {
            home.abandoned_at = Some(abandoned_at);
            home.last_departed_at = None;
            true
        } else {
            false
        }
    }

    pub fn set_name(&mut self, id: Uuid, operator: PlayerId, name: String) -> bool {
        if let Some(home) = self
            .homes
            .iter_mut()
            .find(|home| home.id == id && home.operator_id == operator && home.is_active())
        {
            home.public_name = name;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(abandoned_at: Option<u64>) -> PersonalHome {
        PersonalHome {
            id: Uuid::new_v4(),
            operator_id: PlayerId::new(),
            place_key: "w:undernet:1:0:0:1:7:8".into(),
            claimed_at: 1_000,
            abandoned_at,
            public_name: String::new(),
            public_description: String::new(),
            meaningful_returns: 0,
            last_departed_at: None,
            last_returned_at: None,
        }
    }

    #[test]
    fn abandonment_stages_follow_configured_thresholds() {
        let home = sample(Some(10_000));
        assert_eq!(home.stage_at(10_000), HomeStage::RecentlyAbandoned);
        assert_eq!(home.stage_at(10_000 + RECENTLY_ABANDONED_MS - 1), HomeStage::RecentlyAbandoned);
        assert_eq!(home.stage_at(10_000 + RECENTLY_ABANDONED_MS), HomeStage::Forgotten);
        assert_eq!(home.stage_at(10_000 + FORGOTTEN_MS - 1), HomeStage::Forgotten);
        assert_eq!(home.stage_at(10_000 + FORGOTTEN_MS), HomeStage::Absorbed);
        assert_eq!(sample(None).stage_at(u64::MAX), HomeStage::Active);
    }

    #[test]
    fn habitation_advances_by_time_and_returns() {
        let mut home = sample(None);
        home.claimed_at = 0;
        assert_eq!(home.base_habitation(0), Habitation::Unsettled);
        assert_eq!(home.base_habitation(SETTLING_TIME_MS), Habitation::SettlingIn);
        assert_eq!(home.base_habitation(ESTABLISHED_TIME_MS), Habitation::Established);
        home.claimed_at = ESTABLISHED_TIME_MS;
        home.meaningful_returns = SETTLING_RETURNS;
        assert_eq!(home.base_habitation(ESTABLISHED_TIME_MS), Habitation::SettlingIn);
        home.meaningful_returns = ESTABLISHED_RETURNS;
        assert_eq!(home.base_habitation(ESTABLISHED_TIME_MS), Habitation::Established);
    }

    #[test]
    fn distinctive_requires_established_and_traits() {
        let mut home = sample(None);
        home.claimed_at = 0;
        let traits = ExplorationTraits { journal_notes: true, ..ExplorationTraits::default() };
        assert_eq!(home.habitation(0, traits), Habitation::Unsettled);
        assert_eq!(
            home.habitation(ESTABLISHED_TIME_MS, ExplorationTraits::default()),
            Habitation::Established
        );
        assert_eq!(home.habitation(ESTABLISHED_TIME_MS, traits), Habitation::Distinctive);
    }

    #[test]
    fn absorbed_homes_have_no_look_line() {
        let home = sample(Some(0));
        assert!(home.look_line(FORGOTTEN_MS, false, ExplorationTraits::default()).is_none());
        assert!(home
            .look_line(1, false, ExplorationTraits::default())
            .unwrap()
            .contains("left behind recently"));
    }

    #[test]
    fn abandoned_look_reflects_lived_in_trace_without_private_data() {
        let mut faint = sample(Some(1_000));
        faint.claimed_at = 0;
        faint.meaningful_returns = 0;
        let faint_line = faint.look_line(1_500, false, ExplorationTraits::default()).unwrap();
        assert!(faint_line.contains("never quite settled"));
        assert!(!faint_line.contains("journal"));

        let mut deep = sample(Some(10_000));
        deep.claimed_at = 0;
        deep.meaningful_returns = ESTABLISHED_RETURNS;
        let recent = deep.look_line(10_001, false, ExplorationTraits::default()).unwrap();
        assert!(recent.contains("path worn"));
        let forgotten = deep
            .look_line(10_000 + FORGOTTEN_MS - 1, false, ExplorationTraits::default())
            .unwrap();
        assert!(forgotten.contains("well-kept home"));
        assert!(!forgotten.contains("survey-mark"));
    }

    #[test]
    fn names_reject_controls_and_system_impersonation() {
        assert_eq!(validate_home_name("  Dust Shelf ").unwrap(), "Dust Shelf");
        assert!(validate_home_name("").is_err());
        assert!(validate_home_name("x").is_err());
        assert!(validate_home_name(&"a".repeat(NAME_MAX + 1)).is_err());
        assert!(validate_home_name("You step into danger").is_err());
        // Control characters are stripped before length checks.
        assert_eq!(validate_home_name("a\nb").unwrap(), "ab");
        assert!(validate_home_name("\u{0007}").is_err());
    }

    #[test]
    fn distinctive_public_prose_stays_generic() {
        let home = sample(None);
        let traits = ExplorationTraits {
            journal_notes: true,
            survey_interest: true,
            ..ExplorationTraits::default()
        };
        let line = distinctive_public_detail(home.id, traits);
        assert!(!line.contains("delisted"));
        assert!(!line.contains("survey-mark"));
        assert!(!line.contains("journal"));
    }
}
