//! Settlements: named membership groups anchored to an outskirts tile.
//!
//! Regional chat is physical proximity only. Settlement boards reuse
//! `noticeboard_posts` with `place_id = settle:<uuid>`.

use std::collections::HashMap;

use terminus_core::PlayerId;
use uuid::Uuid;

use crate::geography::{is_reserved_word, RegionIdentity, WildernessPos, WorldId};

pub const MAX_SETTLEMENT_NAME: usize = 24;
pub const MIN_SETTLEMENT_NAME: usize = 2;
pub const MAX_SETTLEMENT_DESC: usize = 160;
pub const INVITE_EXPIRY_MS: u64 = 7 * 24 * 60 * 60 * 1000;

pub const VIS_PUBLIC: &str = "public";
pub const VIS_PRIVATE: &str = "private";
pub const ROLE_OWNER: &str = "owner";
pub const ROLE_MEMBER: &str = "member";
pub const INVITE_PENDING: &str = "pending";
pub const INVITE_ACCEPTED: &str = "accepted";
pub const INVITE_DECLINED: &str = "declined";
pub const INVITE_EXPIRED: &str = "expired";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettlementVisibility {
    Public,
    Private,
}

impl SettlementVisibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => VIS_PUBLIC,
            Self::Private => VIS_PRIVATE,
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            VIS_PUBLIC => Some(Self::Public),
            VIS_PRIVATE => Some(Self::Private),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemberRole {
    Owner,
    Member,
}

impl MemberRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Owner => ROLE_OWNER,
            Self::Member => ROLE_MEMBER,
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            ROLE_OWNER => Some(Self::Owner),
            ROLE_MEMBER => Some(Self::Member),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InviteStatus {
    Pending,
    Accepted,
    Declined,
    Expired,
}

impl InviteStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => INVITE_PENDING,
            Self::Accepted => INVITE_ACCEPTED,
            Self::Declined => INVITE_DECLINED,
            Self::Expired => INVITE_EXPIRED,
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            INVITE_PENDING => Some(Self::Pending),
            INVITE_ACCEPTED => Some(Self::Accepted),
            INVITE_DECLINED => Some(Self::Declined),
            INVITE_EXPIRED => Some(Self::Expired),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settlement {
    pub id: Uuid,
    pub name: String,
    pub name_key: String,
    pub description: String,
    pub visibility: SettlementVisibility,
    pub anchor_place: String,
    pub owner_id: PlayerId,
    pub created_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementMember {
    pub settlement_id: Uuid,
    pub operator_id: PlayerId,
    pub role: MemberRole,
    pub joined_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementInvite {
    pub id: Uuid,
    pub settlement_id: Uuid,
    pub inviter_id: PlayerId,
    pub invitee_id: PlayerId,
    pub status: InviteStatus,
    pub created_at: u64,
    pub expires_at: u64,
}

/// In-memory settlement tables. Snapshotted with social/geography for commit rollback.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SettlementState {
    pub settlements: Vec<Settlement>,
    pub members: Vec<SettlementMember>,
    pub invites: Vec<SettlementInvite>,
}

impl SettlementState {
    pub fn by_id(&self, id: Uuid) -> Option<&Settlement> {
        self.settlements.iter().find(|row| row.id == id)
    }

    pub fn by_name_key(&self, key: &str) -> Option<&Settlement> {
        self.settlements.iter().find(|row| row.name_key == key)
    }

    pub fn membership_of(&self, operator: PlayerId) -> Option<&SettlementMember> {
        self.members.iter().find(|row| row.operator_id == operator)
    }

    pub fn settlement_of(&self, operator: PlayerId) -> Option<&Settlement> {
        let member = self.membership_of(operator)?;
        self.by_id(member.settlement_id)
    }

    pub fn is_member(&self, settlement_id: Uuid, operator: PlayerId) -> bool {
        self.members
            .iter()
            .any(|row| row.settlement_id == settlement_id && row.operator_id == operator)
    }

    pub fn is_owner(&self, settlement_id: Uuid, operator: PlayerId) -> bool {
        self.members.iter().any(|row| {
            row.settlement_id == settlement_id
                && row.operator_id == operator
                && row.role == MemberRole::Owner
        })
    }

    pub fn members_of(&self, settlement_id: Uuid) -> Vec<&SettlementMember> {
        self.members.iter().filter(|row| row.settlement_id == settlement_id).collect()
    }

    pub fn member_count(&self, settlement_id: Uuid) -> usize {
        self.members.iter().filter(|row| row.settlement_id == settlement_id).count()
    }

    pub fn pending_invite(
        &self,
        settlement_id: Uuid,
        invitee: PlayerId,
        now_ms: u64,
    ) -> Option<&SettlementInvite> {
        self.invites.iter().find(|row| {
            row.settlement_id == settlement_id
                && row.invitee_id == invitee
                && row.status == InviteStatus::Pending
                && row.expires_at > now_ms
        })
    }

    /// Actionable pending invites for `invitee` (not yet expired by `now_ms`).
    pub fn pending_for(&self, invitee: PlayerId, now_ms: u64) -> Vec<&SettlementInvite> {
        self.invites
            .iter()
            .filter(|row| {
                row.invitee_id == invitee
                    && row.status == InviteStatus::Pending
                    && row.expires_at > now_ms
            })
            .collect()
    }

    /// Pending rows that have passed `expires_at` but are not yet marked expired.
    pub fn stale_pending(&self, now_ms: u64) -> Vec<&SettlementInvite> {
        self.invites
            .iter()
            .filter(|row| row.status == InviteStatus::Pending && row.expires_at <= now_ms)
            .collect()
    }

    /// Mark expired pending invites. Returns ids that changed.
    /// Call only on Ok command paths so hub commit/rollback stays consistent.
    pub fn prune_invites(&mut self, now_ms: u64) -> Vec<Uuid> {
        let mut expired = Vec::new();
        for invite in &mut self.invites {
            if invite.status == InviteStatus::Pending && invite.expires_at <= now_ms {
                invite.status = InviteStatus::Expired;
                expired.push(invite.id);
            }
        }
        expired
    }
}

pub fn settlement_place_id(id: Uuid) -> String {
    format!("settle:{id}")
}

pub fn workshop_place_id(id: Uuid) -> String {
    format!("workshop:{id}")
}

pub fn parse_workshop_place_id(raw: &str) -> Option<Uuid> {
    let rest = raw.strip_prefix("workshop:")?;
    Uuid::parse_str(rest).ok()
}

pub fn parse_settlement_place_id(raw: &str) -> Option<Uuid> {
    let rest = raw.strip_prefix("settle:")?;
    Uuid::parse_str(rest).ok()
}

/// Physical region chat key for a wilderness birth cell.
pub fn region_key_for_identity(identity: RegionIdentity) -> String {
    format!(
        "r:{}:{}:{}:{}",
        identity.world, identity.epoch_id, identity.origin_x, identity.origin_y
    )
}

/// Static Undernet room region key (one room = one region; borders are room exits).
pub fn region_key_for_static(location_id: &str) -> String {
    format!("u:{location_id}")
}

pub fn region_key_for_wild_place(place: &str) -> Option<String> {
    WildernessPos::parse(place).map(|pos| region_key_for_identity(pos.identity))
}

pub fn validate_settlement_name(raw: &str) -> Result<(String, String), &'static str> {
    let trimmed = raw.trim();
    let chars: Vec<char> = trimmed.chars().collect();
    if !(MIN_SETTLEMENT_NAME..=MAX_SETTLEMENT_NAME).contains(&chars.len()) {
        return Err("A settlement name is 2–24 letters, digits, or hyphens.");
    }
    let Some(first) = chars.first() else {
        return Err("A settlement name must start with a letter.");
    };
    if !first.is_ascii_alphabetic() {
        return Err("A settlement name must start with a letter.");
    }
    if !chars.iter().all(|ch| ch.is_ascii_alphanumeric() || *ch == '-') {
        return Err("A settlement name is 2–24 letters, digits, or hyphens.");
    }
    let name_key = trimmed.to_ascii_lowercase();
    if is_reserved_word(&name_key)
        || matches!(
            name_key.as_str(),
            "settle"
                | "settlement"
                | "settlements"
                | "public"
                | "private"
                | "board"
                | "region"
                | "coffee"
                | "coffeeshop"
                | "undernet"
        )
    {
        return Err("That name is reserved.");
    }
    Ok((trimmed.to_string(), name_key))
}

pub fn validate_settlement_description(raw: &str) -> Result<String, &'static str> {
    let cleaned = terminus_core::text::clean_line(raw, MAX_SETTLEMENT_DESC);
    if cleaned.chars().count() > MAX_SETTLEMENT_DESC {
        return Err("A settlement description is at most 160 characters.");
    }
    Ok(cleaned)
}

/// Anchor title for inspect overlays — birth region coords, not raw tile for privacy.
pub fn anchor_title(anchor_place: &str) -> String {
    if let Some(pos) = WildernessPos::parse(anchor_place) {
        format!("outskirts birth ({}, {})", pos.identity.origin_x, pos.identity.origin_y)
    } else {
        "unknown ground".into()
    }
}

pub fn world_id_from_key(raw: &str) -> Option<WorldId> {
    WorldId::parse(raw)
}

/// Index helpers used by discovery.
pub fn settlements_by_region_key<'a>(
    state: &'a SettlementState,
    region_key: &str,
) -> Vec<&'a Settlement> {
    state
        .settlements
        .iter()
        .filter(|settle| {
            region_key_for_wild_place(&settle.anchor_place).as_deref() == Some(region_key)
        })
        .collect()
}

pub fn member_names(
    state: &SettlementState,
    settlement_id: Uuid,
    names: &HashMap<PlayerId, String>,
) -> Vec<String> {
    let mut rows: Vec<(MemberRole, String)> = state
        .members_of(settlement_id)
        .into_iter()
        .map(|member| {
            let name = names.get(&member.operator_id).cloned().unwrap_or_else(|| "someone".into());
            (member.role, name)
        })
        .collect();
    rows.sort_by(|a, b| {
        let rank = |role: MemberRole| match role {
            MemberRole::Owner => 0,
            MemberRole::Member => 1,
        };
        rank(a.0)
            .cmp(&rank(b.0))
            .then_with(|| a.1.to_ascii_lowercase().cmp(&b.1.to_ascii_lowercase()))
    });
    rows.into_iter()
        .map(|(role, name)| match role {
            MemberRole::Owner => format!("{name} (owner)"),
            MemberRole::Member => name,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geography::gate_identity;

    #[test]
    fn settlement_names_reject_reserved_and_short() {
        assert!(validate_settlement_name("a").is_err());
        assert!(validate_settlement_name("north").is_err());
        assert!(validate_settlement_name("1haven").is_err());
        assert!(validate_settlement_name("Haven").is_ok());
        assert_eq!(validate_settlement_name("Haven").unwrap().1, "haven");
    }

    #[test]
    fn region_keys_are_stable() {
        let id = gate_identity();
        assert_eq!(region_key_for_identity(id), "r:undernet:1:0:0");
        assert_eq!(region_key_for_static("coffee-shop"), "u:coffee-shop");
    }

    #[test]
    fn one_membership_lookup() {
        let mut state = SettlementState::default();
        let sid = Uuid::new_v4();
        let a = PlayerId::new();
        state.settlements.push(Settlement {
            id: sid,
            name: "Haven".into(),
            name_key: "haven".into(),
            description: String::new(),
            visibility: SettlementVisibility::Public,
            anchor_place: "w:undernet:1:0:0:1:8:9".into(),
            owner_id: a,
            created_at: 1,
        });
        state.members.push(SettlementMember {
            settlement_id: sid,
            operator_id: a,
            role: MemberRole::Owner,
            joined_at: 1,
        });
        assert!(state.is_owner(sid, a));
        assert_eq!(state.settlement_of(a).unwrap().name, "Haven");
    }
}
