//! Social command handlers: whisper, emote, profile, friends, ignore.

use terminus_core::{
    Audience, Channel, CommandOutcome, GameError, ParsedCommand, PlayerId, SessionId,
};

use crate::model::PersistOp;
use crate::social::{
    validate_bio, validate_status, Friendship, FriendshipStatus, IgnoreEdge, MAX_BIO_CHARS,
    MAX_STATUS_CHARS,
};
use crate::state::{quiet, Transition, WorldState};

impl WorldState {
    pub(crate) fn cmd_whisper(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let target_raw = cmd.args.first().map(String::as_str).unwrap_or("");
        let text =
            terminus_core::text::clean_line(&cmd.rest, terminus_core::text::MAX_SPEECH_CHARS);
        if text.is_empty() {
            return Err(GameError::Rejected("Whisper what?".into()));
        }
        let sender = self.player(session_id)?;
        let sender_name = self.name_of(session_id)?;
        let target = self.resolve_player_name(target_raw)?;
        if target == sender {
            return Err(GameError::Rejected("You cannot whisper to yourself.".into()));
        }
        if self.social.blocked_either_way(sender, target) {
            return Err(GameError::Rejected(
                "You cannot message that operator (ignore/block is in effect).".into(),
            ));
        }
        if !self.is_online(target) {
            return Err(GameError::Rejected(format!(
                "{} is offline. Private messages are not stored.",
                self.operators.get(&target).map(|op| op.name.as_str()).unwrap_or("They")
            )));
        }
        let target_name = self.operators.get(&target).map(|op| op.name.clone()).unwrap_or_default();
        let mut events = Vec::new();
        let mut ops = Vec::new();
        let (to_target, op_target) = self.emit(
            Channel::Private,
            Audience::Player(target),
            Some(sender_name.clone()),
            text.clone(),
        );
        events.push(to_target);
        ops.push(op_target);
        let (to_self, op_self) = self.emit(
            Channel::System,
            Audience::Player(sender),
            None,
            format!("You whisper to {target_name}: {text}"),
        );
        events.push(to_self);
        ops.push(op_self);
        Ok(Transition { outcome: CommandOutcome { events, quit: false }, ops })
    }

    pub(crate) fn cmd_emote(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let action =
            terminus_core::text::clean_line(&cmd.rest, terminus_core::text::MAX_SPEECH_CHARS);
        if action.is_empty() {
            return Err(GameError::Rejected("Emote what?".into()));
        }
        let name = self.name_of(session_id)?;
        let location = self.location_id(session_id)?;
        let (event, op) = self.emit(
            Channel::Local,
            Audience::Location(location),
            None,
            format!("{name} {action}"),
        );
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![op],
        })
    }

    pub(crate) fn cmd_profile(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let rest = cmd.rest.trim();
        if rest.is_empty() {
            let player = self.player(session_id)?;
            self.set_overlay(session_id, self.profile_text(player, false));
            return Ok(quiet());
        }
        let mut parts = rest.splitn(2, char::is_whitespace);
        let head = parts.next().unwrap_or("").to_ascii_lowercase();
        let tail = parts.next().unwrap_or("").trim();
        match head.as_str() {
            "bio" => {
                let bio = validate_bio(tail).map_err(|m| GameError::Rejected(m.into()))?;
                let player = self.player(session_id)?;
                if let Some(op) = self.operators.get_mut(&player) {
                    op.bio = bio.clone();
                }
                let status =
                    self.operators.get(&player).map(|op| op.status.clone()).unwrap_or_default();
                let (event, note) = self.emit(
                    Channel::System,
                    Audience::Player(player),
                    None,
                    if bio.is_empty() {
                        "Your biography is cleared.".into()
                    } else {
                        format!("Biography set ({}/{}).", bio.chars().count(), MAX_BIO_CHARS)
                    },
                );
                Ok(Transition {
                    outcome: CommandOutcome { events: vec![event], quit: false },
                    ops: vec![PersistOp::Profile { operator_id: player, bio, status }, note],
                })
            }
            "status" => {
                let status = validate_status(tail).map_err(|m| GameError::Rejected(m.into()))?;
                let player = self.player(session_id)?;
                if let Some(op) = self.operators.get_mut(&player) {
                    op.status = status.clone();
                }
                let bio = self.operators.get(&player).map(|op| op.bio.clone()).unwrap_or_default();
                let (event, note) = self.emit(
                    Channel::System,
                    Audience::Player(player),
                    None,
                    if status.is_empty() {
                        "Your status is cleared.".into()
                    } else {
                        format!("Status set ({}/{}).", status.chars().count(), MAX_STATUS_CHARS)
                    },
                );
                Ok(Transition {
                    outcome: CommandOutcome { events: vec![event], quit: false },
                    ops: vec![PersistOp::Profile { operator_id: player, bio, status }, note],
                })
            }
            _ => {
                let target = self.resolve_player_name(rest)?;
                self.set_overlay(session_id, self.profile_text(target, false));
                Ok(quiet())
            }
        }
    }

    pub(crate) fn profile_text(&self, player: PlayerId, colocated: bool) -> String {
        let Some(op) = self.operators.get(&player) else {
            return "No such operator.".into();
        };
        let online = if self.is_online(player) { "online" } else { "offline" };
        let mut lines = vec![format!("{} ({online})", op.name)];
        if !op.status.is_empty() {
            lines.push(format!("status: {}", op.status));
        }
        if !op.bio.is_empty() {
            lines.push(format!("bio: {}", op.bio));
        } else {
            lines.push("bio: (none)".into());
        }
        if colocated {
            lines.push("They are here with you.".into());
        }
        lines.join("\n")
    }

    pub(crate) fn cmd_friend(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let rest = cmd.rest.trim();
        let mut parts = rest.splitn(2, char::is_whitespace);
        let action = parts.next().unwrap_or("").to_ascii_lowercase();
        let target_raw = parts.next().unwrap_or("").trim();
        if target_raw.is_empty() {
            return Err(GameError::Rejected(
                "usage: friend <add|accept|decline|remove> <player>".into(),
            ));
        }
        let actor = self.player(session_id)?;
        let target = self.resolve_player_name(target_raw)?;
        if target == actor {
            return Err(GameError::Rejected("You cannot friend yourself.".into()));
        }
        let target_name = self.operators.get(&target).map(|op| op.name.clone()).unwrap_or_default();
        match action.as_str() {
            "add" => self.friend_add(actor, target, &target_name),
            "accept" => self.friend_accept(actor, target, &target_name),
            "decline" => self.friend_decline(actor, target, &target_name),
            "remove" | "rm" => self.friend_remove(actor, target, &target_name),
            _ => Err(GameError::Rejected(
                "usage: friend <add|accept|decline|remove> <player>".into(),
            )),
        }
    }

    fn friend_add(
        &mut self,
        actor: PlayerId,
        target: PlayerId,
        target_name: &str,
    ) -> Result<Transition, GameError> {
        if self.social.blocked_either_way(actor, target) {
            return Err(GameError::Rejected(
                "You cannot send that request (ignore/block is in effect).".into(),
            ));
        }
        if self.social.are_friends(actor, target) {
            return Err(GameError::Rejected(format!(
                "You are already friends with {target_name}."
            )));
        }
        if self.social.pending_from(actor, target).is_some() {
            return Err(GameError::Rejected(format!(
                "You already asked {target_name}. Waiting for a reply."
            )));
        }
        if let Some(incoming) = self.social.pending_from(target, actor).cloned() {
            return self.friend_accept_inner(actor, target, target_name, incoming);
        }
        let now = self.now_ms;
        let row = Friendship {
            requester_id: actor,
            recipient_id: target,
            status: FriendshipStatus::Pending,
            created_at: now,
            updated_at: now,
        };
        self.social.friendships.push(row.clone());
        let mut events = Vec::new();
        let mut ops = vec![PersistOp::FriendshipUpsert {
            requester_id: row.requester_id,
            recipient_id: row.recipient_id,
            status: row.status,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }];
        let (note, note_op) = self.emit(
            Channel::System,
            Audience::Player(actor),
            None,
            format!("Friend request sent to {target_name}."),
        );
        events.push(note);
        ops.push(note_op);
        if self.is_online(target) {
            let actor_name =
                self.operators.get(&actor).map(|op| op.name.clone()).unwrap_or_default();
            let (ping, ping_op) = self.emit(
                Channel::System,
                Audience::Player(target),
                None,
                format!("{actor_name} wants to be friends. Type: friend accept {actor_name}"),
            );
            events.push(ping);
            ops.push(ping_op);
        }
        Ok(Transition { outcome: CommandOutcome { events, quit: false }, ops })
    }

    fn friend_accept(
        &mut self,
        actor: PlayerId,
        target: PlayerId,
        target_name: &str,
    ) -> Result<Transition, GameError> {
        let Some(pending) = self.social.pending_from(target, actor).cloned() else {
            return Err(GameError::Rejected(format!(
                "There is no pending request from {target_name}."
            )));
        };
        self.friend_accept_inner(actor, target, target_name, pending)
    }

    fn friend_accept_inner(
        &mut self,
        actor: PlayerId,
        target: PlayerId,
        target_name: &str,
        pending: Friendship,
    ) -> Result<Transition, GameError> {
        let now = self.now_ms;
        if let Some(row) = self.social.friendships.iter_mut().find(|row| {
            row.requester_id == pending.requester_id && row.recipient_id == pending.recipient_id
        }) {
            row.status = FriendshipStatus::Accepted;
            row.updated_at = now;
        }
        let mut events = Vec::new();
        let mut ops = vec![PersistOp::FriendshipUpsert {
            requester_id: pending.requester_id,
            recipient_id: pending.recipient_id,
            status: FriendshipStatus::Accepted,
            created_at: pending.created_at,
            updated_at: now,
        }];
        let (note, note_op) = self.emit(
            Channel::System,
            Audience::Player(actor),
            None,
            format!("You and {target_name} are now friends."),
        );
        events.push(note);
        ops.push(note_op);
        if self.is_online(target) {
            let actor_name =
                self.operators.get(&actor).map(|op| op.name.clone()).unwrap_or_default();
            let (ping, ping_op) = self.emit(
                Channel::System,
                Audience::Player(target),
                None,
                format!("{actor_name} accepted your friend request."),
            );
            events.push(ping);
            ops.push(ping_op);
        }
        Ok(Transition { outcome: CommandOutcome { events, quit: false }, ops })
    }

    fn friend_decline(
        &mut self,
        actor: PlayerId,
        target: PlayerId,
        target_name: &str,
    ) -> Result<Transition, GameError> {
        let Some(pending) = self.social.pending_from(target, actor).cloned() else {
            return Err(GameError::Rejected(format!(
                "There is no pending request from {target_name}."
            )));
        };
        self.social.friendships.retain(|row| {
            !(row.requester_id == pending.requester_id && row.recipient_id == pending.recipient_id)
        });
        let (event, op) = self.emit(
            Channel::System,
            Audience::Player(actor),
            None,
            format!("Declined {target_name}'s friend request."),
        );
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![
                PersistOp::FriendshipDelete {
                    requester_id: pending.requester_id,
                    recipient_id: pending.recipient_id,
                },
                op,
            ],
        })
    }

    fn friend_remove(
        &mut self,
        actor: PlayerId,
        target: PlayerId,
        target_name: &str,
    ) -> Result<Transition, GameError> {
        let Some(existing) = self.social.friendship_between(actor, target).cloned() else {
            return Err(GameError::Rejected(format!("You have no friendship with {target_name}.")));
        };
        self.social.friendships.retain(|row| {
            !(row.requester_id == existing.requester_id
                && row.recipient_id == existing.recipient_id)
        });
        let (event, op) = self.emit(
            Channel::System,
            Audience::Player(actor),
            None,
            format!("Friendship with {target_name} removed."),
        );
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![
                PersistOp::FriendshipDelete {
                    requester_id: existing.requester_id,
                    recipient_id: existing.recipient_id,
                },
                op,
            ],
        })
    }

    pub(crate) fn cmd_friends(
        &mut self,
        session_id: SessionId,
        _cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let mut lines = vec!["Friends:".to_string()];
        let mut rows = Vec::new();
        for other in self.social.friends_of(player) {
            let name = self
                .operators
                .get(&other)
                .map(|op| op.name.clone())
                .unwrap_or_else(|| other.to_string());
            // Privacy: online status only; never reveal exact place here.
            let presence = if self.social.blocked_either_way(player, other) {
                "hidden"
            } else if self.is_online(other) {
                "online"
            } else {
                "offline"
            };
            rows.push(format!("- {name} ({presence})"));
        }
        rows.sort();
        if rows.is_empty() {
            lines.push("(none)".into());
        } else {
            lines.extend(rows);
        }
        let incoming = self.social.pending_incoming(player);
        if !incoming.is_empty() {
            lines.push("Incoming requests:".into());
            for row in incoming {
                let name = self
                    .operators
                    .get(&row.requester_id)
                    .map(|op| op.name.clone())
                    .unwrap_or_else(|| row.requester_id.to_string());
                lines.push(format!("- {name}"));
            }
        }
        let outgoing = self.social.pending_outgoing(player);
        if !outgoing.is_empty() {
            lines.push("Outgoing requests:".into());
            for row in outgoing {
                let name = self
                    .operators
                    .get(&row.recipient_id)
                    .map(|op| op.name.clone())
                    .unwrap_or_else(|| row.recipient_id.to_string());
                lines.push(format!("- {name}"));
            }
        }
        self.set_overlay(session_id, lines.join("\n"));
        Ok(quiet())
    }

    pub(crate) fn cmd_ignore(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let raw = cmd.args.first().map(String::as_str).unwrap_or("");
        let actor = self.player(session_id)?;
        let target = self.resolve_player_name(raw)?;
        if target == actor {
            return Err(GameError::Rejected("You cannot ignore yourself.".into()));
        }
        if self.social.ignores(actor, target) {
            return Err(GameError::Rejected("You already ignore that operator.".into()));
        }
        let now = self.now_ms;
        self.social.ignores.push(IgnoreEdge {
            operator_id: actor,
            ignored_id: target,
            created_at: now,
        });
        // Drop pending friendship either direction.
        let mut ops = vec![PersistOp::IgnoreUpsert {
            operator_id: actor,
            ignored_id: target,
            created_at: now,
        }];
        let doomed: Vec<_> = self
            .social
            .friendships
            .iter()
            .filter(|row| {
                (row.requester_id == actor && row.recipient_id == target)
                    || (row.requester_id == target && row.recipient_id == actor)
            })
            .cloned()
            .collect();
        for row in doomed {
            self.social.friendships.retain(|item| {
                !(item.requester_id == row.requester_id && item.recipient_id == row.recipient_id)
            });
            ops.push(PersistOp::FriendshipDelete {
                requester_id: row.requester_id,
                recipient_id: row.recipient_id,
            });
        }
        let name = self.operators.get(&target).map(|op| op.name.clone()).unwrap_or_default();
        let (event, note) = self.emit(
            Channel::System,
            Audience::Player(actor),
            None,
            format!(
                "You ignore {name}. Their say, chat, emote, whisper, and friend requests are filtered."
            ),
        );
        ops.push(note);
        Ok(Transition { outcome: CommandOutcome { events: vec![event], quit: false }, ops })
    }

    pub(crate) fn cmd_unignore(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let raw = cmd.args.first().map(String::as_str).unwrap_or("");
        let actor = self.player(session_id)?;
        let target = self.resolve_player_name(raw)?;
        if !self.social.ignores(actor, target) {
            return Err(GameError::Rejected("You are not ignoring that operator.".into()));
        }
        self.social
            .ignores
            .retain(|edge| !(edge.operator_id == actor && edge.ignored_id == target));
        let name = self.operators.get(&target).map(|op| op.name.clone()).unwrap_or_default();
        let (event, note) = self.emit(
            Channel::System,
            Audience::Player(actor),
            None,
            format!("You no longer ignore {name}."),
        );
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![PersistOp::IgnoreDelete { operator_id: actor, ignored_id: target }, note],
        })
    }

    pub(crate) fn cmd_ignores(
        &mut self,
        session_id: SessionId,
        _cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let mut names: Vec<String> = self
            .social
            .ignored_ids(player)
            .into_iter()
            .map(|id| {
                self.operators.get(&id).map(|op| op.name.clone()).unwrap_or_else(|| id.to_string())
            })
            .collect();
        names.sort();
        let text = if names.is_empty() {
            "You are not ignoring anyone.".into()
        } else {
            format!("Ignoring:\n- {}", names.join("\n- "))
        };
        self.set_overlay(session_id, text);
        Ok(quiet())
    }
}
