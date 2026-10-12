//! Personal place marks. A waypoint names where an operator stood.
//! It is not a journal entry, a shared marker, or a travel command.

use terminus_core::text::clean_line;
use terminus_core::PlayerId;
use uuid::Uuid;

pub const NAME_MIN: usize = 2;
pub const NAME_MAX: usize = 24;
pub const NOTE_MAX: usize = 160;
pub const LIST_LIMIT: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Waypoint {
    pub id: Uuid,
    pub operator_id: PlayerId,
    pub name_key: String,
    pub display_name: String,
    pub place_key: String,
    pub note: String,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaypointName {
    pub key: String,
    pub display: String,
}

/// Letters, digits, and hyphens. The stored key is lowercase; the display keeps the typed case.
pub fn parse_name(raw: &str) -> Result<WaypointName, &'static str> {
    let display = raw.trim();
    let count = display.chars().count();
    if count < NAME_MIN {
        return Err("Waypoint names need at least 2 characters.");
    }
    if count > NAME_MAX {
        return Err("Waypoint names can be at most 24 characters.");
    }
    let mut chars = display.chars();
    let Some(first) = chars.next() else {
        return Err("Waypoint names need at least 2 characters.");
    };
    if !first.is_ascii_alphabetic() {
        return Err("Waypoint names start with a letter.");
    }
    if display.chars().any(|ch| !ch.is_ascii_alphanumeric() && ch != '-') {
        return Err(
            "Waypoint names use letters, digits, and hyphens. Spaces and other punctuation are not part of the name.",
        );
    }
    Ok(WaypointName { key: display.to_ascii_lowercase(), display: display.to_string() })
}

/// Keep punctuation and spaces. Reject empty and over-long notes instead of truncating them.
pub fn clean_note(raw: &str) -> Result<String, &'static str> {
    let cleaned = clean_line(raw, NOTE_MAX + 1);
    if cleaned.is_empty() {
        return Err("A waypoint note needs some text.");
    }
    if cleaned.chars().count() > NOTE_MAX {
        return Err("Waypoint notes can be at most 160 characters.");
    }
    Ok(cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_keep_display_case_and_reject_punctuation() {
        let name = parse_name("Old-Relay").unwrap();
        assert_eq!(name.key, "old-relay");
        assert_eq!(name.display, "Old-Relay");
        assert!(parse_name("a").is_err());
        assert!(parse_name("").is_err());
        assert!(parse_name("1stone").is_err());
        assert!(parse_name("old_relay").is_err());
        assert!(parse_name("old relay").is_err());
        assert!(parse_name(&"a".repeat(NAME_MAX + 1)).is_err());
        assert!(parse_name(&"a".repeat(NAME_MAX)).is_ok());
    }

    #[test]
    fn notes_keep_punctuation_and_refuse_blanks() {
        assert_eq!(
            clean_note("  Dish points toward the city gate. ").unwrap(),
            "Dish points toward the city gate."
        );
        assert!(clean_note("   ").is_err());
        assert!(clean_note(&"n".repeat(NOTE_MAX + 1)).is_err());
        assert_eq!(clean_note(&"n".repeat(NOTE_MAX)).unwrap().chars().count(), NOTE_MAX);
    }
}
