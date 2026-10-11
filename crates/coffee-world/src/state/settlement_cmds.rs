//! Settlement membership, discovery, regional chat, and settlement noticeboards.

use terminus_core::{
    Audience, Channel, CommandOutcome, GameError, ParsedCommand, PlayerId, SessionId,
};
use uuid::Uuid;

use crate::geography::{Structure, WildernessPos, CABIN_KIND, SHARE_PRIVATE, WORKSHOP_KIND};
use crate::model::PersistOp;
use crate::settlement::{
    anchor_title, member_names, region_key_for_wild_place, settlement_place_id,
    settlements_by_region_key, validate_settlement_description, validate_settlement_name,
    InviteStatus, MemberRole, Settlement, SettlementInvite, SettlementMember, SettlementVisibility,
    INVITE_EXPIRY_MS,
};
use crate::social::{
    validate_notice_text, NoticePost, NOTICE_PLACE_CAP, NOTICE_RATE_MS, NOTICE_RETENTION_MS,
};
use crate::state::{quiet, Transition, WorldState};

impl WorldState {
    pub(crate) fn cmd_region(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let text =
            terminus_core::text::clean_line(&cmd.rest, terminus_core::text::MAX_SPEECH_CHARS);
        if text.is_empty() {
            return Err(GameError::Rejected("Region say what?".into()));
        }
        let name = self.name_of(session_id)?;
        let key = self.region_key_of(session_id)?;
        let (event, op) = self.emit(Channel::Region, Audience::Region(key), Some(name), text);
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![op],
        })
    }

    pub(crate) fn cmd_settlements(
        &mut self,
        session_id: SessionId,
        _cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let location = self.location_id(session_id)?;
        let region_key = self.region_key_for_location(&location);
        let mut lines = vec!["Settlements here:".to_string()];
        let mut listed = std::collections::BTreeSet::new();

        for settle in settlements_by_region_key(&self.settlements, &region_key) {
            if self.can_discover_settlement(player, settle) {
                listed.insert(settle.name_key.clone());
                lines.push(format_settlement_list_line(settle, &self.settlements));
            }
        }

        // Linked structures on the current tile (or cabin interior).
        for structure in self.structures_touching_location(&location) {
            let Some(sid) = structure.settlement_id else {
                continue;
            };
            let Some(settle) = self.settlements.by_id(sid) else {
                continue;
            };
            if listed.contains(&settle.name_key) {
                continue;
            }
            if self.can_discover_settlement(player, settle) {
                listed.insert(settle.name_key.clone());
                lines.push(format_settlement_list_line(settle, &self.settlements));
            }
        }

        if listed.is_empty() {
            lines.push("(none visible from this ground)".into());
        }
        if let Some(mine) = self.settlements.settlement_of(player) {
            if !listed.contains(&mine.name_key) {
                lines.push(format!(
                    "You belong to {} ({}) — not in this footprint.",
                    mine.name,
                    mine.visibility.as_str()
                ));
            }
        }
        self.set_overlay(session_id, lines.join("\n"));
        Ok(quiet())
    }

    pub(crate) fn cmd_settle(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let rest = cmd.rest.trim();
        if rest.is_empty() {
            return Err(GameError::Rejected(
                "usage: settle <create|inspect|invite|invites|accept|decline|members|leave|kick|transfer|disband|describe|access|claim|unclaim|board> …"
                    .into(),
            ));
        }
        let mut parts = rest.splitn(2, char::is_whitespace);
        let action = parts.next().unwrap_or("").to_ascii_lowercase();
        let tail = parts.next().unwrap_or("").trim();
        match action.as_str() {
            "create" => self.settle_create(session_id, tail),
            "inspect" => self.settle_inspect(session_id, tail),
            "invite" => self.settle_invite(session_id, tail),
            "invites" => self.settle_invites(session_id),
            "accept" => self.settle_accept(session_id, tail),
            "decline" => self.settle_decline(session_id, tail),
            "members" => self.settle_members(session_id),
            "leave" => self.settle_leave(session_id),
            "kick" => self.settle_kick(session_id, tail),
            "transfer" => self.settle_transfer(session_id, tail),
            "disband" => self.settle_disband(session_id, tail),
            "describe" => self.settle_describe(session_id, tail),
            "access" => self.settle_access(session_id, tail),
            "claim" => self.settle_claim(session_id),
            "unclaim" => self.settle_unclaim(session_id),
            "board" => self.settle_board(session_id, tail),
            _ => Err(GameError::Rejected(
                "usage: settle <create|inspect|invite|invites|accept|decline|members|leave|kick|transfer|disband|describe|access|claim|unclaim|board> …"
                    .into(),
            )),
        }
    }

    fn settle_create(&mut self, session_id: SessionId, raw: &str) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        if self.settlements.membership_of(player).is_some() {
            return Err(GameError::Rejected(
                "You already belong to a settlement. Leave or disband it first.".into(),
            ));
        }
        let (name, name_key) =
            validate_settlement_name(raw).map_err(|m| GameError::Rejected(m.into()))?;
        if self.settlements.by_name_key(&name_key).is_some() {
            return Err(GameError::Rejected("That settlement name is taken.".into()));
        }
        let location = self.location_id(session_id)?;
        let anchor = self.settle_anchor_place(&location)?;
        let pos = WildernessPos::parse(&anchor)
            .ok_or_else(|| GameError::Rejected("Settlements need outskirts ground.".into()))?;
        if pos.is_gate() {
            return Err(GameError::Rejected("The city gate cannot host a settlement.".into()));
        }
        let tile = self.effective_at(pos)?;
        if !tile.traversable() {
            return Err(GameError::Rejected(
                "You cannot found a settlement on impassable ground.".into(),
            ));
        }
        let mut ops = self.ensure_region(pos.identity, Some(player), pos.local_x, pos.local_y)?;
        let id = Uuid::new_v4();
        let created_at = self.now_ms;
        let settle = Settlement {
            id,
            name: name.clone(),
            name_key,
            description: String::new(),
            visibility: SettlementVisibility::Public,
            anchor_place: anchor.clone(),
            owner_id: player,
            created_at,
        };
        self.settlements.settlements.push(settle.clone());
        self.settlements.members.push(SettlementMember {
            settlement_id: id,
            operator_id: player,
            role: MemberRole::Owner,
            joined_at: created_at,
        });
        ops.push(PersistOp::SettlementUpsert {
            id,
            name: settle.name.clone(),
            name_key: settle.name_key.clone(),
            description: settle.description.clone(),
            visibility: settle.visibility,
            anchor_place: settle.anchor_place.clone(),
            owner_id: player,
            created_at,
        });
        ops.push(PersistOp::SettlementMemberUpsert {
            settlement_id: id,
            operator_id: player,
            role: MemberRole::Owner,
            joined_at: created_at,
        });

        // Auto-link owned enterables on the anchor tile when present.
        if let Some(region) = self.geography.region(pos.identity.coord_key()) {
            let owned: Vec<Uuid> = self
                .geography
                .structures_at(region.id, pos.local_x, pos.local_y)
                .into_iter()
                .filter(|item| {
                    item.owner_id == Some(player)
                        && item.settlement_id.is_none()
                        && matches!(item.kind.as_str(), CABIN_KIND | WORKSHOP_KIND)
                })
                .map(|item| item.id)
                .collect();
            for structure_id in owned {
                if let Some(item) =
                    self.geography.structures.iter_mut().find(|item| item.id == structure_id)
                {
                    item.settlement_id = Some(id);
                }
                ops.push(PersistOp::LinkStructureSettlement {
                    id: structure_id,
                    settlement_id: Some(id),
                });
            }
        }

        let (event, note) = self.emit(
            Channel::System,
            Audience::Player(player),
            None,
            format!("You found the settlement {name}."),
        );
        ops.push(note);
        Ok(Transition { outcome: CommandOutcome { events: vec![event], quit: false }, ops })
    }

    fn settle_inspect(
        &mut self,
        session_id: SessionId,
        raw: &str,
    ) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let settle = if raw.is_empty() {
            self.settlements
                .settlement_of(player)
                .cloned()
                .ok_or_else(|| GameError::Rejected("You do not belong to a settlement.".into()))?
        } else {
            let key = raw.trim().to_ascii_lowercase();
            let settle = self
                .settlements
                .by_name_key(&key)
                .cloned()
                .ok_or_else(|| GameError::Rejected("No settlement by that name.".into()))?;
            if !self.can_discover_settlement(player, &settle)
                && !self.settlements.is_member(settle.id, player)
            {
                return Err(GameError::Rejected("That settlement is private.".into()));
            }
            settle
        };
        let owner = self.owner_name(Some(settle.owner_id));
        let count = self.settlements.member_count(settle.id);
        let is_member = self.settlements.is_member(settle.id, player);
        let anchor = if is_member || settle.visibility == SettlementVisibility::Public {
            anchor_title(&settle.anchor_place)
        } else {
            "hidden".into()
        };
        let desc = if settle.description.is_empty() {
            "(no description)".into()
        } else {
            settle.description.clone()
        };
        let lines = [
            format!("{} ({})", settle.name, settle.visibility.as_str()),
            format!("Owner: {owner}"),
            format!("Members: {count}"),
            format!("Anchor: {anchor}"),
            format!("Description: {desc}"),
        ];
        self.set_overlay(session_id, lines.join("\n"));
        Ok(quiet())
    }

    fn settle_invite(&mut self, session_id: SessionId, raw: &str) -> Result<Transition, GameError> {
        let actor = self.player(session_id)?;
        let settle = self
            .settlements
            .settlement_of(actor)
            .cloned()
            .ok_or_else(|| GameError::Rejected("You do not belong to a settlement.".into()))?;
        if !self.settlements.is_member(settle.id, actor) {
            return Err(GameError::Rejected("You are not a member of that settlement.".into()));
        }
        let target = self.resolve_player_name(raw)?;
        if target == actor {
            return Err(GameError::Rejected("You are already in your settlement.".into()));
        }
        if self.social.blocked_either_way(actor, target) {
            return Err(GameError::Rejected("That operator is blocked.".into()));
        }
        if self.settlements.membership_of(target).is_some() {
            return Err(GameError::Rejected(
                "That operator already belongs to a settlement.".into(),
            ));
        }
        // Reject before any mutation so Err paths cannot diverge from SQLite.
        if self.settlements.pending_invite(settle.id, target, self.now_ms).is_some() {
            return Err(GameError::Rejected("An invite is already pending for them.".into()));
        }
        let mut ops = self.prune_settlement_invites();
        let id = Uuid::new_v4();
        let created_at = self.now_ms;
        let expires_at = created_at.saturating_add(INVITE_EXPIRY_MS);
        let invite = SettlementInvite {
            id,
            settlement_id: settle.id,
            inviter_id: actor,
            invitee_id: target,
            status: InviteStatus::Pending,
            created_at,
            expires_at,
        };
        self.settlements.invites.push(invite);
        ops.push(PersistOp::SettlementInviteUpsert {
            id,
            settlement_id: settle.id,
            inviter_id: actor,
            invitee_id: target,
            status: InviteStatus::Pending,
            created_at,
            expires_at,
        });
        let target_name = self.operators.get(&target).map(|op| op.name.clone()).unwrap_or_default();
        let (event, note) = self.emit(
            Channel::System,
            Audience::Player(actor),
            None,
            format!("Invite sent to {target_name} for {}.", settle.name),
        );
        ops.push(note);
        if self.is_online(target) {
            let (notify, n2) = self.emit(
                Channel::System,
                Audience::Player(target),
                None,
                format!(
                    "{} invites you to join settlement {}. Use settle invites, then settle accept {}.",
                    self.name_of(session_id)?,
                    settle.name,
                    settle.name
                ),
            );
            ops.push(n2);
            return Ok(Transition {
                outcome: CommandOutcome { events: vec![event, notify], quit: false },
                ops,
            });
        }
        Ok(Transition { outcome: CommandOutcome { events: vec![event], quit: false }, ops })
    }

    fn settle_invites(&mut self, session_id: SessionId) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let ops = self.prune_settlement_invites();
        let pending = self.settlements.pending_for(player, self.now_ms);
        let mut lines = vec!["Pending settlement invitations:".to_string()];
        if pending.is_empty() {
            lines.push("(none — you have no pending settlement invites)".into());
        } else {
            for invite in pending {
                let Some(settle) = self.settlements.by_id(invite.settlement_id) else {
                    continue;
                };
                let inviter = self.owner_name(Some(invite.inviter_id));
                let expiry = format_invite_expiry(invite.expires_at, self.now_ms);
                lines.push(format!("- {} from {inviter} ({expiry})", settle.name));
                lines.push(format!("  settle accept {}", settle.name));
                lines.push(format!("  settle decline {}", settle.name));
            }
            if lines.len() == 1 {
                lines.push("(none — you have no pending settlement invites)".into());
            }
        }
        self.set_overlay(session_id, lines.join("\n"));
        Ok(Transition { outcome: CommandOutcome::default(), ops })
    }

    fn settle_accept(&mut self, session_id: SessionId, raw: &str) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        if self.settlements.membership_of(player).is_some() {
            return Err(GameError::Rejected(
                "You already belong to a settlement. Leave it before accepting.".into(),
            ));
        }
        let key = raw.trim().to_ascii_lowercase();
        // Time-expired matches: expire durably via Ok, without joining.
        if self.find_invite_for_player(player, &key, true).is_some() {
            let mut ops = self.prune_settlement_invites();
            let (event, note) = self.emit(
                Channel::System,
                Audience::Player(player),
                None,
                "That settlement invite has expired. Use settle invites to review others.",
            );
            ops.push(note);
            return Ok(Transition {
                outcome: CommandOutcome { events: vec![event], quit: false },
                ops,
            });
        }
        let invite = self.find_invite_for_player(player, &key, false).ok_or_else(|| {
            GameError::Rejected(
                "No matching settlement invite. Use settle invites to list yours.".into(),
            )
        })?;
        if self.social.blocked_either_way(player, invite.inviter_id) {
            return Err(GameError::Rejected("That operator is blocked.".into()));
        }
        let settle = self
            .settlements
            .by_id(invite.settlement_id)
            .cloned()
            .ok_or_else(|| GameError::Rejected("That settlement no longer exists.".into()))?;
        let mut ops = self.prune_settlement_invites();
        let joined_at = self.now_ms;
        self.settlements.members.push(SettlementMember {
            settlement_id: settle.id,
            operator_id: player,
            role: MemberRole::Member,
            joined_at,
        });
        if let Some(row) = self.settlements.invites.iter_mut().find(|row| row.id == invite.id) {
            row.status = InviteStatus::Accepted;
        }
        ops.push(PersistOp::SettlementMemberUpsert {
            settlement_id: settle.id,
            operator_id: player,
            role: MemberRole::Member,
            joined_at,
        });
        ops.push(PersistOp::SettlementInviteUpsert {
            id: invite.id,
            settlement_id: invite.settlement_id,
            inviter_id: invite.inviter_id,
            invitee_id: invite.invitee_id,
            status: InviteStatus::Accepted,
            created_at: invite.created_at,
            expires_at: invite.expires_at,
        });
        let (event, note) = self.emit(
            Channel::System,
            Audience::Player(player),
            None,
            format!("You join settlement {}.", settle.name),
        );
        ops.push(note);
        Ok(Transition { outcome: CommandOutcome { events: vec![event], quit: false }, ops })
    }

    fn settle_decline(
        &mut self,
        session_id: SessionId,
        raw: &str,
    ) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let key = raw.trim().to_ascii_lowercase();
        if self.find_invite_for_player(player, &key, true).is_some() {
            let mut ops = self.prune_settlement_invites();
            let (event, note) = self.emit(
                Channel::System,
                Audience::Player(player),
                None,
                "That settlement invite has already expired.",
            );
            ops.push(note);
            return Ok(Transition {
                outcome: CommandOutcome { events: vec![event], quit: false },
                ops,
            });
        }
        let invite = self.find_invite_for_player(player, &key, false).ok_or_else(|| {
            GameError::Rejected(
                "No matching settlement invite. Use settle invites to list yours.".into(),
            )
        })?;
        let mut ops = self.prune_settlement_invites();
        if let Some(row) = self.settlements.invites.iter_mut().find(|row| row.id == invite.id) {
            row.status = InviteStatus::Declined;
        }
        ops.push(PersistOp::SettlementInviteUpsert {
            id: invite.id,
            settlement_id: invite.settlement_id,
            inviter_id: invite.inviter_id,
            invitee_id: invite.invitee_id,
            status: InviteStatus::Declined,
            created_at: invite.created_at,
            expires_at: invite.expires_at,
        });
        let (event, note) =
            self.emit(Channel::System, Audience::Player(player), None, "Invite declined.");
        ops.push(note);
        Ok(Transition { outcome: CommandOutcome { events: vec![event], quit: false }, ops })
    }

    /// Find an invite addressed to `player` by settlement name key.
    /// When `expired_only`, only time-stale pending rows are considered.
    fn find_invite_for_player(
        &self,
        player: PlayerId,
        name_key: &str,
        expired_only: bool,
    ) -> Option<SettlementInvite> {
        self.settlements
            .invites
            .iter()
            .filter(|invite| invite.invitee_id == player && invite.status == InviteStatus::Pending)
            .filter(|invite| {
                let stale = invite.expires_at <= self.now_ms;
                if expired_only {
                    stale
                } else {
                    !stale
                }
            })
            .find(|invite| {
                self.settlements
                    .by_id(invite.settlement_id)
                    .is_some_and(|s| name_key.is_empty() || s.name_key == name_key)
            })
            .cloned()
    }

    fn settle_members(&mut self, session_id: SessionId) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let settle = self
            .settlements
            .settlement_of(player)
            .cloned()
            .ok_or_else(|| GameError::Rejected("You do not belong to a settlement.".into()))?;
        let names: std::collections::HashMap<PlayerId, String> =
            self.operators.iter().map(|(id, op)| (*id, op.name.clone())).collect();
        let mut lines = vec![format!("{} members:", settle.name)];
        for line in member_names(&self.settlements, settle.id, &names) {
            lines.push(format!("- {line}"));
        }
        self.set_overlay(session_id, lines.join("\n"));
        Ok(quiet())
    }

    fn settle_leave(&mut self, session_id: SessionId) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let member = self
            .settlements
            .membership_of(player)
            .cloned()
            .ok_or_else(|| GameError::Rejected("You do not belong to a settlement.".into()))?;
        let settle = self
            .settlements
            .by_id(member.settlement_id)
            .cloned()
            .ok_or_else(|| GameError::Rejected("Settlement missing.".into()))?;
        let count = self.settlements.member_count(settle.id);
        if member.role == MemberRole::Owner && count > 1 {
            return Err(GameError::Rejected(
                "Transfer ownership or disband before leaving.".into(),
            ));
        }
        if count == 1 {
            return self.disband_settlement(
                session_id,
                player,
                settle,
                "You leave; the settlement disbands.",
            );
        }
        self.settlements
            .members
            .retain(|row| !(row.settlement_id == settle.id && row.operator_id == player));
        let (event, note) = self.emit(
            Channel::System,
            Audience::Player(player),
            None,
            format!("You leave settlement {}.", settle.name),
        );
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![
                PersistOp::SettlementMemberDelete { settlement_id: settle.id, operator_id: player },
                note,
            ],
        })
    }

    fn settle_kick(&mut self, session_id: SessionId, raw: &str) -> Result<Transition, GameError> {
        let actor = self.player(session_id)?;
        let settle = self
            .settlements
            .settlement_of(actor)
            .cloned()
            .ok_or_else(|| GameError::Rejected("You do not belong to a settlement.".into()))?;
        if !self.settlements.is_owner(settle.id, actor) {
            return Err(GameError::Rejected("Only the owner can kick members.".into()));
        }
        let target = self.resolve_player_name(raw)?;
        if target == actor {
            return Err(GameError::Rejected("Use leave or disband for yourself.".into()));
        }
        if !self.settlements.is_member(settle.id, target) {
            return Err(GameError::Rejected("They are not in your settlement.".into()));
        }
        self.settlements
            .members
            .retain(|row| !(row.settlement_id == settle.id && row.operator_id == target));
        let target_name = self.operators.get(&target).map(|op| op.name.clone()).unwrap_or_default();
        let mut ops = vec![PersistOp::SettlementMemberDelete {
            settlement_id: settle.id,
            operator_id: target,
        }];
        let (event, note) = self.emit(
            Channel::System,
            Audience::Player(actor),
            None,
            format!("You remove {target_name} from {}.", settle.name),
        );
        ops.push(note);
        if self.is_online(target) {
            let (notify, n2) = self.emit(
                Channel::System,
                Audience::Player(target),
                None,
                format!("You were removed from settlement {}.", settle.name),
            );
            ops.push(n2);
            return Ok(Transition {
                outcome: CommandOutcome { events: vec![event, notify], quit: false },
                ops,
            });
        }
        Ok(Transition { outcome: CommandOutcome { events: vec![event], quit: false }, ops })
    }

    fn settle_transfer(
        &mut self,
        session_id: SessionId,
        raw: &str,
    ) -> Result<Transition, GameError> {
        let actor = self.player(session_id)?;
        let settle = self
            .settlements
            .settlement_of(actor)
            .cloned()
            .ok_or_else(|| GameError::Rejected("You do not belong to a settlement.".into()))?;
        if !self.settlements.is_owner(settle.id, actor) {
            return Err(GameError::Rejected("Only the owner can transfer leadership.".into()));
        }
        let target = self.resolve_player_name(raw)?;
        if target == actor {
            return Err(GameError::Rejected("You already own it.".into()));
        }
        if !self.settlements.is_member(settle.id, target) {
            return Err(GameError::Rejected("Transfer target must be a member.".into()));
        }
        let actor_joined =
            self.settlements.membership_of(actor).map(|m| m.joined_at).unwrap_or(self.now_ms);
        let target_joined = self
            .settlements
            .members
            .iter()
            .find(|m| m.settlement_id == settle.id && m.operator_id == target)
            .map(|m| m.joined_at)
            .unwrap_or(self.now_ms);
        for member in &mut self.settlements.members {
            if member.settlement_id != settle.id {
                continue;
            }
            if member.operator_id == actor {
                member.role = MemberRole::Member;
            } else if member.operator_id == target {
                member.role = MemberRole::Owner;
            }
        }
        if let Some(row) = self.settlements.settlements.iter_mut().find(|row| row.id == settle.id) {
            row.owner_id = target;
        }
        let target_name = self.operators.get(&target).map(|op| op.name.clone()).unwrap_or_default();
        let mut ops = vec![
            PersistOp::SettlementMemberUpsert {
                settlement_id: settle.id,
                operator_id: actor,
                role: MemberRole::Member,
                joined_at: actor_joined,
            },
            PersistOp::SettlementMemberUpsert {
                settlement_id: settle.id,
                operator_id: target,
                role: MemberRole::Owner,
                joined_at: target_joined,
            },
            PersistOp::SettlementUpsert {
                id: settle.id,
                name: settle.name.clone(),
                name_key: settle.name_key.clone(),
                description: settle.description.clone(),
                visibility: settle.visibility,
                anchor_place: settle.anchor_place.clone(),
                owner_id: target,
                created_at: settle.created_at,
            },
        ];
        let (event, note) = self.emit(
            Channel::System,
            Audience::Player(actor),
            None,
            format!("You transfer ownership of {} to {target_name}.", settle.name),
        );
        ops.push(note);
        Ok(Transition { outcome: CommandOutcome { events: vec![event], quit: false }, ops })
    }

    fn settle_disband(
        &mut self,
        session_id: SessionId,
        raw: &str,
    ) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        if !raw.eq_ignore_ascii_case("confirm") {
            return Err(GameError::Rejected("usage: settle disband confirm".into()));
        }
        let settle = self
            .settlements
            .settlement_of(player)
            .cloned()
            .ok_or_else(|| GameError::Rejected("You do not belong to a settlement.".into()))?;
        if !self.settlements.is_owner(settle.id, player) {
            return Err(GameError::Rejected("Only the owner can disband the settlement.".into()));
        }
        self.disband_settlement(
            session_id,
            player,
            settle,
            "Settlement disbanded. Cabins and signs remain.",
        )
    }

    fn settle_describe(
        &mut self,
        session_id: SessionId,
        raw: &str,
    ) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let settle = self
            .settlements
            .settlement_of(player)
            .cloned()
            .ok_or_else(|| GameError::Rejected("You do not belong to a settlement.".into()))?;
        if !self.settlements.is_owner(settle.id, player) {
            return Err(GameError::Rejected("Only the owner can set the description.".into()));
        }
        let description =
            validate_settlement_description(raw).map_err(|m| GameError::Rejected(m.into()))?;
        if let Some(row) = self.settlements.settlements.iter_mut().find(|row| row.id == settle.id) {
            row.description = description.clone();
        }
        let (event, note) = self.emit(
            Channel::System,
            Audience::Player(player),
            None,
            "Settlement description updated.",
        );
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![
                PersistOp::SettlementUpsert {
                    id: settle.id,
                    name: settle.name,
                    name_key: settle.name_key,
                    description,
                    visibility: settle.visibility,
                    anchor_place: settle.anchor_place,
                    owner_id: settle.owner_id,
                    created_at: settle.created_at,
                },
                note,
            ],
        })
    }

    fn settle_access(&mut self, session_id: SessionId, raw: &str) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let settle = self
            .settlements
            .settlement_of(player)
            .cloned()
            .ok_or_else(|| GameError::Rejected("You do not belong to a settlement.".into()))?;
        if !self.settlements.is_owner(settle.id, player) {
            return Err(GameError::Rejected("Only the owner can set visibility.".into()));
        }
        let visibility = match raw.trim().to_ascii_lowercase().as_str() {
            "public" => SettlementVisibility::Public,
            "private" => SettlementVisibility::Private,
            _ => {
                return Err(GameError::Rejected("usage: settle access public|private".into()));
            }
        };
        if let Some(row) = self.settlements.settlements.iter_mut().find(|row| row.id == settle.id) {
            row.visibility = visibility;
        }
        let (event, note) = self.emit(
            Channel::System,
            Audience::Player(player),
            None,
            format!("Settlement is now {}.", visibility.as_str()),
        );
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![
                PersistOp::SettlementUpsert {
                    id: settle.id,
                    name: settle.name,
                    name_key: settle.name_key,
                    description: settle.description,
                    visibility,
                    anchor_place: settle.anchor_place,
                    owner_id: settle.owner_id,
                    created_at: settle.created_at,
                },
                note,
            ],
        })
    }

    fn settle_claim(&mut self, session_id: SessionId) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let settle = self
            .settlements
            .settlement_of(player)
            .cloned()
            .ok_or_else(|| GameError::Rejected("You do not belong to a settlement.".into()))?;
        let location = self.location_id(session_id)?;
        let Some(pos) = WildernessPos::parse(&location) else {
            return Err(GameError::Rejected(
                "Stand on an outskirts tile to claim a structure.".into(),
            ));
        };
        let region_id = self
            .geography
            .region(pos.identity.coord_key())
            .map(|region| region.id)
            .ok_or_else(|| GameError::Rejected("This region is not open.".into()))?;
        let structure = self
            .geography
            .structures_at(region_id, pos.local_x, pos.local_y)
            .into_iter()
            .find(|item| item.owner_id == Some(player) && item.settlement_id.is_none())
            .cloned()
            .ok_or_else(|| {
                GameError::Rejected("No unclaimed structure you own is on this tile.".into())
            })?;
        if let Some(item) =
            self.geography.structures.iter_mut().find(|item| item.id == structure.id)
        {
            item.settlement_id = Some(settle.id);
        }
        let (event, note) = self.emit(
            Channel::System,
            Audience::Player(player),
            None,
            format!("Linked {} to settlement {}.", structure.name, settle.name),
        );
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![
                PersistOp::LinkStructureSettlement {
                    id: structure.id,
                    settlement_id: Some(settle.id),
                },
                note,
            ],
        })
    }

    fn settle_unclaim(&mut self, session_id: SessionId) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let settle = self
            .settlements
            .settlement_of(player)
            .cloned()
            .ok_or_else(|| GameError::Rejected("You do not belong to a settlement.".into()))?;
        let location = self.location_id(session_id)?;
        let Some(pos) = WildernessPos::parse(&location) else {
            return Err(GameError::Rejected(
                "Stand on an outskirts tile to unclaim a structure.".into(),
            ));
        };
        let region_id = self
            .geography
            .region(pos.identity.coord_key())
            .map(|region| region.id)
            .ok_or_else(|| GameError::Rejected("This region is not open.".into()))?;
        let structure = self
            .geography
            .structures_at(region_id, pos.local_x, pos.local_y)
            .into_iter()
            .find(|item| item.settlement_id == Some(settle.id))
            .cloned()
            .ok_or_else(|| {
                GameError::Rejected("No structure linked to your settlement is here.".into())
            })?;
        let allowed =
            structure.owner_id == Some(player) || self.settlements.is_owner(settle.id, player);
        if !allowed {
            return Err(GameError::Rejected(
                "Only the structure owner or settlement owner can unclaim.".into(),
            ));
        }
        let mut ops =
            vec![PersistOp::LinkStructureSettlement { id: structure.id, settlement_id: None }];
        if let Some(item) =
            self.geography.structures.iter_mut().find(|item| item.id == structure.id)
        {
            item.settlement_id = None;
            if item.share != SHARE_PRIVATE {
                item.share = SHARE_PRIVATE.into();
                ops.push(PersistOp::UpdateStructure {
                    id: item.id,
                    name: item.name.clone(),
                    access: item.access.clone(),
                    interior_place: item.interior_place.clone(),
                    share: SHARE_PRIVATE.into(),
                });
            }
        }
        let (event, note) = self.emit(
            Channel::System,
            Audience::Player(player),
            None,
            format!("Unlinked {} from settlement {}.", structure.name, settle.name),
        );
        ops.push(note);
        Ok(Transition { outcome: CommandOutcome { events: vec![event], quit: false }, ops })
    }

    fn settle_board(&mut self, session_id: SessionId, raw: &str) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let location = self.location_id(session_id)?;
        let settle = self.resolve_board_settlement(player, &location, raw)?;
        if !self.can_access_settlement_board(player, &settle, &location) {
            return Err(GameError::Rejected(
                "You must be in the settlement's footprint to use its board.".into(),
            ));
        }
        let place_id = settlement_place_id(settle.id);
        // Strip optional settlement name prefix for named public boards.
        let action_tail = strip_board_name_prefix(raw, &settle.name_key);
        if action_tail.is_empty() || action_tail.eq_ignore_ascii_case("read") {
            return self.settle_board_read(session_id, &settle, &place_id);
        }
        let mut parts = action_tail.splitn(2, char::is_whitespace);
        let action = parts.next().unwrap_or("").to_ascii_lowercase();
        let tail = parts.next().unwrap_or("").trim();
        match action.as_str() {
            "post" => self.settle_board_post(session_id, player, &settle, &place_id, tail),
            "remove" => self.settle_board_remove(session_id, player, &settle, &place_id, tail),
            _ => Err(GameError::Rejected("usage: settle board [post <text>|remove <id>]".into())),
        }
    }

    fn settle_board_read(
        &mut self,
        session_id: SessionId,
        settle: &Settlement,
        place_id: &str,
    ) -> Result<Transition, GameError> {
        let _ = self.social.prune_notices(self.now_ms);
        let posts = self.social.active_notices(place_id, self.now_ms);
        let mut lines = vec![format!("{} noticeboard:", settle.name)];
        if posts.is_empty() {
            lines.push("(empty — try settle board post <text>)".into());
        } else {
            for post in posts.into_iter().take(NOTICE_PLACE_CAP) {
                let author = self
                    .operators
                    .get(&post.author_id)
                    .map(|op| op.name.clone())
                    .unwrap_or_else(|| "someone".into());
                let short = post.id.to_string();
                let id8 = &short[..8.min(short.len())];
                lines.push(format!("[{id8}] {author}: {}", post.body));
            }
        }
        self.set_overlay(session_id, lines.join("\n"));
        Ok(quiet())
    }

    fn settle_board_post(
        &mut self,
        _session_id: SessionId,
        player: PlayerId,
        settle: &Settlement,
        place_id: &str,
        raw: &str,
    ) -> Result<Transition, GameError> {
        if !self.settlements.is_member(settle.id, player) {
            return Err(GameError::Rejected("Only members can post on this board.".into()));
        }
        if self.use_system_clock {
            if let Some(last) = self.social.last_notice_at.get(&player).copied() {
                if self.now_ms.saturating_sub(last) < NOTICE_RATE_MS {
                    return Err(GameError::Rejected(
                        "Wait a moment before posting another notice.".into(),
                    ));
                }
            }
        }
        let body = validate_notice_text(raw).map_err(|m| GameError::Rejected(m.into()))?;
        let mut ops = Vec::new();
        for expired in self.social.prune_notices(self.now_ms) {
            ops.push(PersistOp::NoticeDelete { id: expired });
        }
        let active = self.social.active_notices(place_id, self.now_ms);
        if active.len() >= NOTICE_PLACE_CAP {
            if let Some(oldest) = active.last() {
                let id = oldest.id;
                self.social.notices.retain(|post| post.id != id);
                ops.push(PersistOp::NoticeDelete { id });
            }
        }
        let id = Uuid::new_v4();
        let created_at = self.now_ms;
        let expires_at = created_at.saturating_add(NOTICE_RETENTION_MS);
        self.social.notices.push(NoticePost {
            id,
            place_id: place_id.into(),
            author_id: player,
            body: body.clone(),
            created_at,
            expires_at,
        });
        self.social.last_notice_at.insert(player, created_at);
        ops.push(PersistOp::NoticeUpsert {
            id,
            place_id: place_id.into(),
            author_id: player,
            body,
            created_at,
            expires_at,
        });
        let short = id.to_string();
        let id8 = &short[..8.min(short.len())];
        let (event, note) = self.emit(
            Channel::System,
            Audience::Player(player),
            None,
            format!("You pin a notice [{id8}] on {}.", settle.name),
        );
        ops.push(note);
        Ok(Transition { outcome: CommandOutcome { events: vec![event], quit: false }, ops })
    }

    fn settle_board_remove(
        &mut self,
        _session_id: SessionId,
        player: PlayerId,
        settle: &Settlement,
        place_id: &str,
        raw: &str,
    ) -> Result<Transition, GameError> {
        let key = raw.trim().to_ascii_lowercase();
        if key.is_empty() {
            return Err(GameError::Rejected("usage: settle board remove <id>".into()));
        }
        let Some(post) = self
            .social
            .notices
            .iter()
            .find(|post| post.place_id == place_id && post.id.to_string().starts_with(&key))
            .cloned()
        else {
            return Err(GameError::Rejected("No notice matches that id.".into()));
        };
        let allowed = post.author_id == player || self.settlements.is_owner(settle.id, player);
        if !allowed {
            return Err(GameError::Rejected(
                "Only the author or settlement owner can remove that notice.".into(),
            ));
        }
        self.social.notices.retain(|row| row.id != post.id);
        let (event, note) =
            self.emit(Channel::System, Audience::Player(player), None, "Notice removed.");
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![PersistOp::NoticeDelete { id: post.id }, note],
        })
    }

    fn disband_settlement(
        &mut self,
        session_id: SessionId,
        player: PlayerId,
        settle: Settlement,
        message: &str,
    ) -> Result<Transition, GameError> {
        let _ = session_id;
        let place_id = settlement_place_id(settle.id);
        let mut ops = Vec::new();
        // Clear structure links; revert settlement-derived share to private. Never delete builds.
        for structure in &mut self.geography.structures {
            if structure.settlement_id == Some(settle.id) {
                structure.settlement_id = None;
                let share_changed = structure.share != SHARE_PRIVATE;
                if share_changed {
                    structure.share = SHARE_PRIVATE.into();
                }
                ops.push(PersistOp::LinkStructureSettlement {
                    id: structure.id,
                    settlement_id: None,
                });
                if share_changed {
                    ops.push(PersistOp::UpdateStructure {
                        id: structure.id,
                        name: structure.name.clone(),
                        access: structure.access.clone(),
                        interior_place: structure.interior_place.clone(),
                        share: SHARE_PRIVATE.into(),
                    });
                }
            }
        }
        let notice_ids: Vec<Uuid> = self
            .social
            .notices
            .iter()
            .filter(|post| post.place_id == place_id)
            .map(|post| post.id)
            .collect();
        for id in notice_ids {
            self.social.notices.retain(|post| post.id != id);
            ops.push(PersistOp::NoticeDelete { id });
        }
        self.settlements.invites.retain(|invite| invite.settlement_id != settle.id);
        self.settlements.members.retain(|member| member.settlement_id != settle.id);
        self.settlements.settlements.retain(|row| row.id != settle.id);
        ops.push(PersistOp::SettlementDelete { id: settle.id });
        let (event, note) =
            self.emit(Channel::System, Audience::Player(player), None, message.to_string());
        ops.push(note);
        Ok(Transition { outcome: CommandOutcome { events: vec![event], quit: false }, ops })
    }

    fn prune_settlement_invites(&mut self) -> Vec<PersistOp> {
        let expired = self.settlements.prune_invites(self.now_ms);
        let mut ops = Vec::new();
        for id in expired {
            if let Some(invite) = self.settlements.invites.iter().find(|row| row.id == id).cloned()
            {
                ops.push(PersistOp::SettlementInviteUpsert {
                    id: invite.id,
                    settlement_id: invite.settlement_id,
                    inviter_id: invite.inviter_id,
                    invitee_id: invite.invitee_id,
                    status: InviteStatus::Expired,
                    created_at: invite.created_at,
                    expires_at: invite.expires_at,
                });
            }
        }
        ops
    }

    fn settle_anchor_place(&self, location: &str) -> Result<String, GameError> {
        if let Some(pos) = WildernessPos::parse(location) {
            return Ok(pos.key());
        }
        if let Some(cabin) = self.geography.structure_by_interior(location) {
            let region = self
                .geography
                .region_by_id(cabin.region_id)
                .ok_or_else(|| GameError::Rejected("Cabin exterior is missing.".into()))?;
            return Ok(WildernessPos {
                identity: region.identity,
                local_x: cabin.local_x,
                local_y: cabin.local_y,
            }
            .key());
        }
        Err(GameError::Rejected(
            "Found a settlement on walkable outskirts ground (or inside a cabin there).".into(),
        ))
    }

    fn can_discover_settlement(&self, player: PlayerId, settle: &Settlement) -> bool {
        match settle.visibility {
            SettlementVisibility::Public => true,
            SettlementVisibility::Private => self.settlements.is_member(settle.id, player),
        }
    }

    fn can_access_settlement_board(
        &self,
        player: PlayerId,
        settle: &Settlement,
        location: &str,
    ) -> bool {
        let _ = player;
        if let Some(anchor_key) = region_key_for_wild_place(&settle.anchor_place) {
            if self.region_key_for_location(location) == anchor_key {
                return true;
            }
        }
        // Linked structure tile or its cabin interior.
        for structure in
            self.geography.structures.iter().filter(|s| s.settlement_id == Some(settle.id))
        {
            if let Some(region) = self.geography.region_by_id(structure.region_id) {
                let exterior = WildernessPos {
                    identity: region.identity,
                    local_x: structure.local_x,
                    local_y: structure.local_y,
                }
                .key();
                if location == exterior {
                    return true;
                }
            }
            if structure.interior_place.as_deref() == Some(location) {
                return true;
            }
        }
        false
    }

    fn resolve_board_settlement(
        &self,
        player: PlayerId,
        location: &str,
        raw: &str,
    ) -> Result<Settlement, GameError> {
        let first = raw.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
        if !first.is_empty() && !matches!(first.as_str(), "read" | "post" | "remove") {
            let settle = self
                .settlements
                .by_name_key(&first)
                .cloned()
                .ok_or_else(|| GameError::Rejected("No settlement by that name.".into()))?;
            if settle.visibility == SettlementVisibility::Private
                && !self.settlements.is_member(settle.id, player)
            {
                return Err(GameError::Rejected("That settlement is private.".into()));
            }
            if !self.can_access_settlement_board(player, &settle, location) {
                return Err(GameError::Rejected(
                    "You must be in that settlement's footprint to use its board.".into(),
                ));
            }
            return Ok(settle);
        }
        self.settlements.settlement_of(player).cloned().ok_or_else(|| {
            GameError::Rejected(
                "You have no settlement. Name a public settlement in its region: settle board <name> …"
                    .into(),
            )
        })
    }

    fn structures_touching_location(&self, location: &str) -> Vec<&Structure> {
        if let Some(pos) = WildernessPos::parse(location) {
            if let Some(region) = self.geography.region(pos.identity.coord_key()) {
                return self.geography.structures_at(region.id, pos.local_x, pos.local_y);
            }
        }
        if let Some(cabin) = self.geography.structure_by_interior(location) {
            return vec![cabin];
        }
        Vec::new()
    }
}

fn format_settlement_list_line(
    settle: &Settlement,
    state: &crate::settlement::SettlementState,
) -> String {
    format!(
        "- {} ({}, {} members)",
        settle.name,
        settle.visibility.as_str(),
        state.member_count(settle.id)
    )
}

fn strip_board_name_prefix(raw: &str, name_key: &str) -> String {
    let trimmed = raw.trim();
    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let first = parts.next().unwrap_or("").to_ascii_lowercase();
    if first == name_key {
        parts.next().unwrap_or("").trim().to_string()
    } else {
        trimmed.to_string()
    }
}

fn format_invite_expiry(expires_at: u64, now_ms: u64) -> String {
    if expires_at <= now_ms {
        return "expired".into();
    }
    let remain = expires_at.saturating_sub(now_ms);
    let days = remain / (24 * 60 * 60 * 1000);
    let hours = (remain % (24 * 60 * 60 * 1000)) / (60 * 60 * 1000);
    if days > 0 {
        format!("expires in {days}d {hours}h")
    } else if hours > 0 {
        format!("expires in {hours}h")
    } else {
        let mins = remain / (60 * 1000);
        format!("expires in {mins}m")
    }
}
