//! Private letters between operators. A row belongs to the recipient.
//!
//! Whisper stays a live line. Mail is the copy that waits while someone is offline.

use std::collections::HashMap;

use terminus_core::text::{clean_line, MAX_SPEECH_CHARS};
use terminus_core::PlayerId;

/// Same cap as a spoken line. Longer letters are refused, not trimmed.
pub const BODY_MAX: usize = MAX_SPEECH_CHARS;
/// Letters kept for one recipient. A full mailbox refuses the new letter.
pub const INBOX_CAP: usize = 32;
/// Minimum gap between sends from one operator. Not persisted across restart.
pub const MAIL_SEND_INTERVAL_MS: u64 = 2_000;
pub const PREVIEW_CHARS: usize = 48;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailMessage {
    pub id: u64,
    pub sender_id: PlayerId,
    pub recipient_id: PlayerId,
    pub body: String,
    pub created_at: u64,
    pub read_at: Option<u64>,
}

/// In-memory mailbox. `next_id` survives deletes so an old id is never reissued.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mailbox {
    pub messages: Vec<MailMessage>,
    pub next_id: u64,
    /// Last successful send, per sender. Rolled back with the mailbox if the commit fails.
    pub last_send_at: HashMap<PlayerId, u64>,
}

impl Default for Mailbox {
    fn default() -> Self {
        Self { messages: Vec::new(), next_id: 1, last_send_at: HashMap::new() }
    }
}

impl Mailbox {
    pub fn from_stored(messages: Vec<MailMessage>, next_id: u64) -> Self {
        let max_id = messages.iter().map(|message| message.id).max().unwrap_or(0);
        let next_id = next_id.max(max_id.saturating_add(1)).max(1);
        Self { messages, next_id, last_send_at: HashMap::new() }
    }

    pub fn count_for(&self, recipient: PlayerId) -> usize {
        self.messages.iter().filter(|message| message.recipient_id == recipient).count()
    }

    pub fn unread_for(&self, recipient: PlayerId) -> usize {
        self.messages
            .iter()
            .filter(|message| message.recipient_id == recipient && message.read_at.is_none())
            .count()
    }

    /// Newest first. Equal timestamps break ties by the higher id.
    pub fn inbox(&self, recipient: PlayerId) -> Vec<&MailMessage> {
        let mut rows: Vec<&MailMessage> =
            self.messages.iter().filter(|message| message.recipient_id == recipient).collect();
        rows.sort_by(|left, right| {
            right.created_at.cmp(&left.created_at).then(right.id.cmp(&left.id))
        });
        rows
    }

    pub fn owned(&self, recipient: PlayerId, id: u64) -> Option<&MailMessage> {
        self.messages.iter().find(|message| message.id == id && message.recipient_id == recipient)
    }
}

pub fn send_is_too_soon(last_at: Option<u64>, now_ms: u64) -> bool {
    match last_at {
        Some(last) => now_ms.saturating_sub(last) < MAIL_SEND_INTERVAL_MS,
        None => false,
    }
}

/// `42` and `#42` are the same id. Zero and other text are refused.
pub fn parse_mail_id(raw: &str) -> Result<u64, &'static str> {
    let raw = raw.trim();
    let digits = raw.strip_prefix('#').unwrap_or(raw);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) || digits.len() > 18 {
        return Err("Message ids are numbers, such as 42 or #42.");
    }
    let id: u64 = digits.parse().map_err(|_| "Message ids are numbers, such as 42 or #42.")?;
    if id == 0 {
        return Err("No message #0 in your mailbox.");
    }
    Ok(id)
}

/// Keep punctuation and spaces. Refuse empty and over-long bodies instead of trimming them to fit.
pub fn clean_body(raw: &str) -> Result<String, &'static str> {
    let cleaned = clean_line(raw, BODY_MAX + 1);
    if cleaned.is_empty() {
        return Err("Write a message.");
    }
    if cleaned.chars().count() > BODY_MAX {
        return Err("Mail can be at most 400 characters.");
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

/// Real clocks are UTC. The deterministic test clock is a millisecond count.
pub fn format_received(ms: u64) -> String {
    if ms < 1_000_000_000_000 {
        return format!("{ms} ms");
    }
    let secs = ms / 1000;
    let days = i64::try_from(secs / 86_400).unwrap_or(i64::MAX);
    let tod = secs % 86_400;
    let hour = tod / 3_600;
    let minute = (tod % 3_600) / 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

fn civil_from_days(days_since_unix: i64) -> (i64, u32, u32) {
    let z = days_since_unix + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = u64::try_from(z - era * 146_097).unwrap_or(0);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = i64::try_from(yoe).unwrap_or(0) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    (year, u32::try_from(month).unwrap_or(0), u32::try_from(day).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_accept_a_hash_and_reject_text() {
        assert_eq!(parse_mail_id("42").unwrap(), 42);
        assert_eq!(parse_mail_id("#42").unwrap(), 42);
        assert_eq!(parse_mail_id("  #7 ").unwrap(), 7);
        assert!(parse_mail_id("0").is_err());
        assert!(parse_mail_id("#0").is_err());
        assert!(parse_mail_id("mail").is_err());
        assert!(parse_mail_id("#").is_err());
        assert!(parse_mail_id("-3").is_err());
    }

    #[test]
    fn bodies_keep_punctuation_and_refuse_overflow() {
        let body = clean_body("  I found the old relay. The dish still faces the gate. ").unwrap();
        assert_eq!(body, "I found the old relay. The dish still faces the gate.");
        assert!(clean_body("   ").is_err());
        assert!(clean_body(&"a".repeat(BODY_MAX)).is_ok());
        assert!(clean_body(&"a".repeat(BODY_MAX + 1)).is_err());
        assert_eq!(clean_body("café").unwrap(), "café");
    }

    #[test]
    fn inbox_is_newest_first_and_ids_break_timestamp_ties() {
        let ada = PlayerId::new();
        let bob = PlayerId::new();
        let cy = PlayerId::new();
        let mailbox = Mailbox {
            next_id: 4,
            last_send_at: HashMap::new(),
            messages: vec![
                sample(1, ada, bob, 10),
                sample(2, ada, bob, 30),
                sample(3, cy, bob, 30),
                sample(9, ada, cy, 30),
            ],
        };
        let ids: Vec<u64> = mailbox.inbox(bob).iter().map(|message| message.id).collect();
        assert_eq!(ids, vec![3, 2, 1]);
        assert_eq!(mailbox.count_for(bob), 3);
        assert_eq!(mailbox.unread_for(bob), 3);
        assert!(mailbox.owned(cy, 1).is_none());
        assert!(mailbox.owned(bob, 9).is_none());
    }

    #[test]
    fn stored_sequence_stays_ahead_of_deleted_ids() {
        let mailbox = Mailbox::from_stored(vec![sample(2, PlayerId::new(), PlayerId::new(), 1)], 5);
        assert_eq!(mailbox.next_id, 5);
        let behind = Mailbox::from_stored(vec![sample(8, PlayerId::new(), PlayerId::new(), 1)], 3);
        assert_eq!(behind.next_id, 9);
    }

    #[test]
    fn send_gap_and_clock_format() {
        assert!(!send_is_too_soon(None, 10));
        assert!(send_is_too_soon(Some(10), 10 + MAIL_SEND_INTERVAL_MS - 1));
        assert!(!send_is_too_soon(Some(10), 10 + MAIL_SEND_INTERVAL_MS));
        assert_eq!(format_received(5), "5 ms");
        assert_eq!(format_received(1_705_325_040_000), "2024-01-15 13:24 UTC");
    }

    fn sample(id: u64, sender: PlayerId, recipient: PlayerId, created_at: u64) -> MailMessage {
        MailMessage {
            id,
            sender_id: sender,
            recipient_id: recipient,
            body: format!("body-{id}"),
            created_at,
            read_at: None,
        }
    }
}
