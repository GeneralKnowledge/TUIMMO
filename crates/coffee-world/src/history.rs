//! Discoverable world history and theory subject resolution.
//!
//! Uses persisted `entered` rows and existing theory/journal keys — no invented activity.

use crate::geography::{is_survey_tile, landmark_sight};
use crate::geography::{
    story_finding, UrbanContext, WildernessPos, DELISTED_FLOOR_ID, SURVEY_MARK_ID,
    SURVEY_SCRATCH_ID,
};
use crate::state::WorldState;
use crate::undernet::READ_TERMINAL_FLAG;
use terminus_core::PlayerId;

pub const CHART_HINT_PREFIX: &str = "chart-hint:";

/// Operator flag: one private charting note per birth cell.
pub fn chart_hint_flag(identity: &crate::geography::RegionIdentity) -> String {
    format!(
        "{CHART_HINT_PREFIX}{}:{}:{}:{}",
        identity.world.as_str(),
        identity.epoch_id,
        identity.origin_x,
        identity.origin_y
    )
}

/// Stable subject tokens for shared theories (content keys and place keys).
pub fn validate_theory_subject(raw: &str) -> Result<String, &'static str> {
    let subject = raw.trim();
    if subject.len() < 2 {
        return Err("A theory subject needs at least 2 characters.");
    }
    if subject.len() > 80 {
        return Err("A theory subject can be at most 80 characters.");
    }
    if subject.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
        return Err("Theory subjects stay on one token (use a content key or place key).");
    }
    if !subject.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ':' | '-' | '.' | '_'))
    {
        return Err("Theory subjects use letters, digits, and : - . _ only.");
    }
    Ok(subject.to_string())
}

/// First word is a subject token when it looks like a journal content key or place key.
pub fn split_subject_from_body(tail: &str) -> (Option<String>, &str) {
    let tail = tail.trim();
    let Some((first, rest)) = tail.split_once(char::is_whitespace) else {
        return (None, tail);
    };
    if !looks_like_subject_token(first) {
        return (None, tail);
    }
    if validate_theory_subject(first).is_err() {
        return (None, tail);
    }
    let body = rest.trim();
    if body.is_empty() {
        return (None, tail);
    }
    (Some(first.to_string()), body)
}

fn looks_like_subject_token(word: &str) -> bool {
    if word.is_empty() || !word.bytes().all(|b| b.is_ascii()) {
        return false;
    }
    if word.starts_with("lm:") || word.starts_with("w:") {
        return true;
    }
    word.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-:._".contains(&b))
        && (word.contains('-') || word.contains(':'))
}

impl WorldState {
    /// Best subject for theories at the operator's current place, if any.
    pub(crate) fn theory_subject_at_location(&self, player: PlayerId) -> Option<String> {
        let location = self.operators.get(&player)?.location_id.clone();
        if let Some(pos) = WildernessPos::parse(&location) {
            let urban = UrbanContext::at(pos);
            if is_survey_tile(pos) {
                return Some(SURVEY_MARK_ID.to_string());
            }
            if let Some(site) = urban.story {
                return Some(story_finding(site).content_key);
            }
            if let Ok(tile) = self.effective_at(pos) {
                if let Some(kind) = tile.landmark {
                    if let Some(finding) = landmark_sight(pos, kind).finding {
                        return Some(finding.content_key);
                    }
                }
            }
            return Some(location);
        }
        if matches!(location.as_str(), "service-corridor" | "abandoned-exchange" | "black-archive")
            && self.operators.get(&player).is_some_and(|op| op.flags.contains(READ_TERMINAL_FLAG))
        {
            return Some(DELISTED_FLOOR_ID.to_string());
        }
        if location == "roof-array" {
            return Some(SURVEY_SCRATCH_ID.to_string());
        }
        if location == "old-relay"
            && self
                .operators
                .get(&player)
                .is_some_and(|op| op.flags.contains(crate::geography::SURVEY_MARK_FLAG))
        {
            return Some(crate::geography::SURVEY_CLOSURE_ID.to_string());
        }
        None
    }

    pub(crate) fn public_name_for(&self, player: PlayerId) -> String {
        self.operators
            .get(&player)
            .map(|op| op.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "unknown".into())
    }

    /// Line for inspect when another operator first materialized this birth cell.
    pub(crate) fn charting_inspect_line(
        &self,
        player: PlayerId,
        pos: WildernessPos,
    ) -> Option<String> {
        let region = self.geography.region(pos.identity.coord_key())?;
        let entered = self.geography.entered_for_region(region.id)?;
        let charter = entered.actor_id?;
        if charter == player {
            return None;
        }
        let name = self.public_name_for(charter);
        Some(format!("This district was first charted by {name}."))
    }

    pub(crate) fn theories_summary_line(&self, subject: &str) -> Option<String> {
        let count = self.theories.count_topic(subject);
        if count == 0 {
            return None;
        }
        Some(format!(
            "{count} public {} on `{subject}`. `theory list {subject}`.",
            if count == 1 { "entry" } else { "entries" }
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_splitting_keeps_legacy_sentences() {
        let (subject, body) = split_subject_from_body("The panel still shows B-4.");
        assert!(subject.is_none());
        assert_eq!(body, "The panel still shows B-4.");
        let (subject, body) = split_subject_from_body("survey-mark The stone matches the relay.");
        assert_eq!(subject.as_deref(), Some("survey-mark"));
        assert_eq!(body, "The stone matches the relay.");
    }

    #[test]
    fn validate_rejects_whitespace_subjects() {
        assert!(validate_theory_subject("delisted-floor").is_ok());
        assert!(validate_theory_subject("bad subject").is_err());
    }
}
