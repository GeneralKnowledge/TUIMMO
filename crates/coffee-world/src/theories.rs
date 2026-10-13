//! Shared public theories — player interpretations tied to a subject token.
//!
//! A row is a player statement. It is not a journal entry, a noticeboard post,
//! or a verified fact. The `topic` column holds a stable subject (usually the
//! same string as a field-journal `content_key`, or a place key). This module
//! does not score, resolve, or mark entries as correct.

use std::collections::HashMap;

use terminus_core::text::clean_line;
use terminus_core::PlayerId;

/// Default subject when posting without an explicit token (backward compatible).
pub const TOPIC: &str = "delisted-floor";
/// Characters in one post. Longer text is refused, not trimmed.
pub const BODY_MAX: usize = 120;
/// Entries kept for one topic. A full topic refuses the new post.
pub const TOPIC_CAP: usize = 32;
/// Minimum gap between posts from one operator. Not persisted across restart.
pub const POST_INTERVAL_MS: u64 = 2_000;
pub const PREVIEW_CHARS: usize = 42;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TheoryKind {
    Observation,
    Theory,
}

impl TheoryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Observation => "observation",
            Self::Theory => "theory",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "observation" => Some(Self::Observation),
            "theory" => Some(Self::Theory),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TheoryEntry {
    pub id: u64,
    pub topic: String,
    pub author_id: PlayerId,
    pub kind: TheoryKind,
    pub body: String,
    pub created_at: u64,
}

/// In-memory theory record. `next_id` survives deletes so an old id is never reissued.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TheoryBoard {
    pub entries: Vec<TheoryEntry>,
    pub next_id: u64,
    /// Last successful post, per author. Rolled back with the board if the commit fails.
    pub last_post_at: HashMap<PlayerId, u64>,
}

impl Default for TheoryBoard {
    fn default() -> Self {
        Self { entries: Vec::new(), next_id: 1, last_post_at: HashMap::new() }
    }
}

impl TheoryBoard {
    pub fn from_stored(entries: Vec<TheoryEntry>, next_id: u64) -> Self {
        let max_id = entries.iter().map(|entry| entry.id).max().unwrap_or(0);
        let next_id = next_id.max(max_id.saturating_add(1)).max(1);
        Self { entries, next_id, last_post_at: HashMap::new() }
    }

    pub fn count_topic(&self, topic: &str) -> usize {
        self.entries.iter().filter(|entry| entry.topic == topic).count()
    }

    /// Newest first. Equal timestamps break ties by the higher id.
    pub fn for_topic(&self, topic: &str) -> Vec<&TheoryEntry> {
        let mut rows: Vec<&TheoryEntry> =
            self.entries.iter().filter(|entry| entry.topic == topic).collect();
        rows.sort_by(|left, right| {
            right.created_at.cmp(&left.created_at).then(right.id.cmp(&left.id))
        });
        rows
    }

    pub fn get(&self, topic: &str, id: u64) -> Option<&TheoryEntry> {
        self.entries.iter().find(|entry| entry.topic == topic && entry.id == id)
    }

    pub fn entry_by_id(&self, id: u64) -> Option<&TheoryEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// Distinct subjects with entry counts, sorted by subject name.
    pub fn topic_summaries(&self) -> Vec<(String, usize)> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for entry in &self.entries {
            *counts.entry(entry.topic.clone()).or_default() += 1;
        }
        let mut rows: Vec<(String, usize)> = counts.into_iter().collect();
        rows.sort_by(|left, right| left.0.cmp(&right.0));
        rows
    }

    pub fn posted_too_soon(&self, author: PlayerId, now_ms: u64) -> bool {
        match self.last_post_at.get(&author).copied() {
            Some(last) => now_ms.saturating_sub(last) < POST_INTERVAL_MS,
            None => false,
        }
    }
}

/// `3` and `#3` are the same id. Zero and other text are refused.
pub fn parse_entry_id(raw: &str) -> Result<u64, &'static str> {
    let raw = raw.trim();
    let digits = raw.strip_prefix('#').unwrap_or(raw);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) || digits.len() > 18 {
        return Err("Entry ids are numbers, such as 3 or #3.");
    }
    let id: u64 = digits.parse().map_err(|_| "Entry ids are numbers, such as 3 or #3.")?;
    if id == 0 {
        return Err("No entry #0.");
    }
    Ok(id)
}

/// Keep punctuation and spaces. Refuse empty and over-long bodies instead of trimming them to fit.
pub fn clean_body(raw: &str) -> Result<String, &'static str> {
    let cleaned = clean_line(raw, BODY_MAX + 1);
    if cleaned.is_empty() {
        return Err("Write an observation or a theory.");
    }
    if cleaned.chars().count() > BODY_MAX {
        return Err("An entry is at most 120 characters.");
    }
    Ok(cleaned)
}

pub fn format_id(id: u64) -> String {
    format!("#{id}")
}

pub fn preview(body: &str) -> String {
    let mut out = String::new();
    for ch in body.chars() {
        if out.chars().count() >= PREVIEW_CHARS {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: u64, author: PlayerId, at: u64, kind: TheoryKind) -> TheoryEntry {
        TheoryEntry {
            id,
            topic: TOPIC.into(),
            author_id: author,
            kind,
            body: format!("note {id}"),
            created_at: at,
        }
    }

    #[test]
    fn ids_accept_a_hash_and_reject_text() {
        assert_eq!(parse_entry_id("3").unwrap(), 3);
        assert_eq!(parse_entry_id("#3").unwrap(), 3);
        assert_eq!(parse_entry_id("  #7 ").unwrap(), 7);
        assert!(parse_entry_id("0").is_err());
        assert!(parse_entry_id("theory").is_err());
        assert!(parse_entry_id("-2").is_err());
    }

    #[test]
    fn bodies_keep_punctuation_and_refuse_overflow() {
        let body = clean_body("  The panel still shows B-4. ").unwrap();
        assert_eq!(body, "The panel still shows B-4.");
        assert!(clean_body("   ").is_err());
        assert!(clean_body(&"a".repeat(BODY_MAX)).is_ok());
        assert!(clean_body(&"a".repeat(BODY_MAX + 1)).is_err());
        assert!(!clean_body("line\u{1b}[31m").unwrap().contains('\u{1b}'));
    }

    #[test]
    fn listing_is_newest_first_and_deletes_do_not_rewind_the_sequence() {
        let ada = PlayerId::new();
        let board = TheoryBoard {
            next_id: 4,
            last_post_at: HashMap::new(),
            entries: vec![
                sample(1, ada, 10, TheoryKind::Observation),
                sample(2, ada, 30, TheoryKind::Theory),
                sample(3, PlayerId::new(), 30, TheoryKind::Observation),
            ],
        };
        let ids: Vec<u64> = board.for_topic(TOPIC).iter().map(|entry| entry.id).collect();
        assert_eq!(ids, vec![3, 2, 1]);
        assert_eq!(board.count_topic(TOPIC), 3);
        let stored = TheoryBoard::from_stored(vec![sample(2, ada, 1, TheoryKind::Theory)], 5);
        assert_eq!(stored.next_id, 5);
        assert!(stored.get(TOPIC, 2).is_some());
        assert!(stored.get(TOPIC, 1).is_none());
    }

    #[test]
    fn the_topic_matches_the_field_note_key() {
        assert_eq!(TOPIC, crate::geography::DELISTED_FLOOR_ID);
    }
}
