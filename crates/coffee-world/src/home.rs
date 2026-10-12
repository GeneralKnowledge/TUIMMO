//! Personal homes in the Squalor.
//!
//! A home is a designation on an existing wilderness place key. It is not a
//! cabin, a settlement claim, a waypoint, or a journal entry. Construction is
//! not required. Abandoned homes keep a public world record; private player
//! data never moves into the home row.

use terminus_core::PlayerId;
use uuid::Uuid;

/// Elapsed real time after abandonment before the "forgotten" stage.
pub const RECENTLY_ABANDONED_MS: u64 = 7 * 24 * 60 * 60 * 1000;
/// Elapsed real time after abandonment before the home presentation fades.
pub const FORGOTTEN_MS: u64 = 30 * 24 * 60 * 60 * 1000;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonalHome {
    pub id: Uuid,
    pub operator_id: PlayerId,
    pub place_key: String,
    pub claimed_at: u64,
    pub abandoned_at: Option<u64>,
    pub public_name: String,
    pub public_description: String,
}

impl PersonalHome {
    pub fn is_active(&self) -> bool {
        self.abandoned_at.is_none()
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

    /// Public look line for this home at `now_ms`. Absorbed homes return None.
    pub fn look_line(&self, now_ms: u64, viewer_is_owner: bool) -> Option<String> {
        match self.stage_at(now_ms) {
            HomeStage::Active => {
                let named = if self.public_name.is_empty() {
                    String::new()
                } else {
                    format!(" \"{}\"", self.public_name)
                };
                let detail = if self.public_description.is_empty() {
                    "Traces of habitation sit against the street's decay — a kept corner in the Squalor."
                        .to_string()
                } else {
                    self.public_description.clone()
                };
                if viewer_is_owner {
                    Some(format!("You have made a home here{named}. {detail}"))
                } else {
                    Some(format!("Someone has made a home here{named}. {detail}"))
                }
            }
            HomeStage::RecentlyAbandoned => Some(
                "This place was inhabited until recently. Signs of a kept room remain among the Squalor's dust."
                    .into(),
            ),
            HomeStage::Forgotten => Some(
                "Dust covers what was once a home. Whatever traces of its former occupant remained have become nearly indistinguishable from the decay surrounding it."
                    .into(),
            ),
            HomeStage::Absorbed => None,
        }
    }
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
        }
    }

    #[test]
    fn stages_follow_configured_thresholds() {
        let home = sample(Some(10_000));
        assert_eq!(home.stage_at(10_000), HomeStage::RecentlyAbandoned);
        assert_eq!(home.stage_at(10_000 + RECENTLY_ABANDONED_MS - 1), HomeStage::RecentlyAbandoned);
        assert_eq!(home.stage_at(10_000 + RECENTLY_ABANDONED_MS), HomeStage::Forgotten);
        assert_eq!(home.stage_at(10_000 + FORGOTTEN_MS - 1), HomeStage::Forgotten);
        assert_eq!(home.stage_at(10_000 + FORGOTTEN_MS), HomeStage::Absorbed);
        assert_eq!(sample(None).stage_at(u64::MAX), HomeStage::Active);
    }

    #[test]
    fn absorbed_homes_have_no_look_line() {
        let home = sample(Some(0));
        assert!(home.look_line(FORGOTTEN_MS, false).is_none());
        assert!(sample(None).look_line(0, true).unwrap().contains("You have made a home"));
        assert!(sample(Some(0)).look_line(1, false).unwrap().contains("inhabited until recently"));
    }

    #[test]
    fn one_active_helper_and_presentation_prefer_active() {
        let owner = PlayerId::new();
        let mut state = HomeState::default();
        let place = "w:undernet:1:0:0:1:7:8";
        state.insert(PersonalHome {
            id: Uuid::new_v4(),
            operator_id: owner,
            place_key: place.into(),
            claimed_at: 1,
            abandoned_at: Some(2),
            public_name: String::new(),
            public_description: String::new(),
        });
        assert!(state.active_for(owner).is_none());
        assert!(state.presentation_at(place, 3).is_some());
        let active_id = Uuid::new_v4();
        state.insert(PersonalHome {
            id: active_id,
            operator_id: owner,
            place_key: place.into(),
            claimed_at: 5,
            abandoned_at: None,
            public_name: String::new(),
            public_description: String::new(),
        });
        assert_eq!(state.active_for(owner).unwrap().id, active_id);
        assert_eq!(state.presentation_at(place, 6).unwrap().id, active_id);
    }
}
