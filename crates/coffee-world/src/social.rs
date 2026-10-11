//! Persistent social graph: friendships, ignores, profiles, and noticeboard posts.
//!
//! These rows are history. They are not rebuilt by world generation or Undernet reseeding.

use std::collections::{HashMap, HashSet};

use terminus_core::PlayerId;
use uuid::Uuid;

pub const MAX_BIO_CHARS: usize = 120;
pub const MAX_STATUS_CHARS: usize = 40;
pub const MAX_SIGN_CHARS: usize = 80;
pub const MAX_NOTICE_CHARS: usize = 160;
pub const NOTICE_RETENTION_MS: u64 = 7 * 24 * 60 * 60 * 1000;
pub const NOTICE_PLACE_CAP: usize = 40;
pub const NOTICE_RATE_MS: u64 = 30_000;
pub const BUILD_RATE_MS: u64 = 1_500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FriendshipStatus {
    Pending,
    Accepted,
}

impl FriendshipStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "pending" => Some(Self::Pending),
            "accepted" => Some(Self::Accepted),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Friendship {
    pub requester_id: PlayerId,
    pub recipient_id: PlayerId,
    pub status: FriendshipStatus,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IgnoreEdge {
    pub operator_id: PlayerId,
    pub ignored_id: PlayerId,
    pub created_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructureGrant {
    pub structure_id: Uuid,
    pub grantee_id: PlayerId,
    pub created_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoticePost {
    pub id: Uuid,
    pub place_id: String,
    pub author_id: PlayerId,
    pub body: String,
    pub created_at: u64,
    pub expires_at: u64,
}

/// In-memory social tables. Snapshotted with geography so failed commits roll back cleanly.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SocialState {
    pub friendships: Vec<Friendship>,
    pub ignores: Vec<IgnoreEdge>,
    pub grants: Vec<StructureGrant>,
    pub notices: Vec<NoticePost>,
    /// Soft rate limits keyed by operator. Not persisted.
    pub last_notice_at: HashMap<PlayerId, u64>,
    pub last_build_at: HashMap<PlayerId, u64>,
}

impl SocialState {
    pub fn ignores(&self, operator: PlayerId, other: PlayerId) -> bool {
        self.ignores.iter().any(|edge| edge.operator_id == operator && edge.ignored_id == other)
    }

    pub fn blocked_either_way(&self, a: PlayerId, b: PlayerId) -> bool {
        self.ignores(a, b) || self.ignores(b, a)
    }

    pub fn friendship_between(&self, a: PlayerId, b: PlayerId) -> Option<&Friendship> {
        self.friendships.iter().find(|row| {
            (row.requester_id == a && row.recipient_id == b)
                || (row.requester_id == b && row.recipient_id == a)
        })
    }

    pub fn are_friends(&self, a: PlayerId, b: PlayerId) -> bool {
        self.friendship_between(a, b).is_some_and(|row| row.status == FriendshipStatus::Accepted)
    }

    pub fn pending_from(&self, requester: PlayerId, recipient: PlayerId) -> Option<&Friendship> {
        self.friendships.iter().find(|row| {
            row.requester_id == requester
                && row.recipient_id == recipient
                && row.status == FriendshipStatus::Pending
        })
    }

    pub fn friends_of(&self, player: PlayerId) -> Vec<PlayerId> {
        self.friendships
            .iter()
            .filter(|row| row.status == FriendshipStatus::Accepted)
            .filter_map(|row| {
                if row.requester_id == player {
                    Some(row.recipient_id)
                } else if row.recipient_id == player {
                    Some(row.requester_id)
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn pending_incoming(&self, player: PlayerId) -> Vec<&Friendship> {
        self.friendships
            .iter()
            .filter(|row| row.recipient_id == player && row.status == FriendshipStatus::Pending)
            .collect()
    }

    pub fn pending_outgoing(&self, player: PlayerId) -> Vec<&Friendship> {
        self.friendships
            .iter()
            .filter(|row| row.requester_id == player && row.status == FriendshipStatus::Pending)
            .collect()
    }

    pub fn ignored_ids(&self, operator: PlayerId) -> HashSet<PlayerId> {
        self.ignores
            .iter()
            .filter(|edge| edge.operator_id == operator)
            .map(|edge| edge.ignored_id)
            .collect()
    }

    pub fn has_grant(&self, structure_id: Uuid, grantee: PlayerId) -> bool {
        self.grants
            .iter()
            .any(|grant| grant.structure_id == structure_id && grant.grantee_id == grantee)
    }

    pub fn active_notices(&self, place_id: &str, now_ms: u64) -> Vec<&NoticePost> {
        let mut posts: Vec<&NoticePost> = self
            .notices
            .iter()
            .filter(|post| post.place_id == place_id && post.expires_at > now_ms)
            .collect();
        posts.sort_by_key(|post| std::cmp::Reverse(post.created_at));
        posts
    }

    pub fn prune_notices(&mut self, now_ms: u64) -> Vec<Uuid> {
        let before = self.notices.len();
        let mut removed = Vec::new();
        self.notices.retain(|post| {
            if post.expires_at <= now_ms {
                removed.push(post.id);
                false
            } else {
                true
            }
        });
        let _ = before;
        removed
    }
}

pub fn validate_bio(raw: &str) -> Result<String, &'static str> {
    let cleaned = terminus_core::text::clean_line(raw, MAX_BIO_CHARS);
    if cleaned.chars().count() > MAX_BIO_CHARS {
        return Err("A biography is at most 120 characters.");
    }
    Ok(cleaned)
}

pub fn validate_status(raw: &str) -> Result<String, &'static str> {
    let cleaned = terminus_core::text::clean_line(raw, MAX_STATUS_CHARS);
    if cleaned.chars().count() > MAX_STATUS_CHARS {
        return Err("A status is at most 40 characters.");
    }
    Ok(cleaned)
}

pub fn validate_sign_text(raw: &str) -> Result<String, &'static str> {
    let cleaned = terminus_core::text::clean_line(raw, MAX_SIGN_CHARS);
    if cleaned.is_empty() {
        return Err("A sign needs some text.");
    }
    if cleaned.chars().count() > MAX_SIGN_CHARS {
        return Err("A sign is at most 80 characters.");
    }
    Ok(cleaned)
}

pub fn validate_notice_text(raw: &str) -> Result<String, &'static str> {
    let cleaned = terminus_core::text::clean_line(raw, MAX_NOTICE_CHARS);
    if cleaned.is_empty() {
        return Err("Write something on the noticeboard.");
    }
    if cleaned.chars().count() > MAX_NOTICE_CHARS {
        return Err("A notice is at most 160 characters.");
    }
    Ok(cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friendship_lookup_is_symmetric_for_accepted() {
        let mut social = SocialState::default();
        let a = PlayerId::new();
        let b = PlayerId::new();
        social.friendships.push(Friendship {
            requester_id: a,
            recipient_id: b,
            status: FriendshipStatus::Accepted,
            created_at: 1,
            updated_at: 1,
        });
        assert!(social.are_friends(a, b));
        assert!(social.are_friends(b, a));
        assert!(!social.are_friends(a, PlayerId::new()));
    }

    #[test]
    fn ignore_is_directional() {
        let mut social = SocialState::default();
        let a = PlayerId::new();
        let b = PlayerId::new();
        social.ignores.push(IgnoreEdge { operator_id: a, ignored_id: b, created_at: 1 });
        assert!(social.ignores(a, b));
        assert!(!social.ignores(b, a));
        assert!(social.blocked_either_way(a, b));
    }
}
