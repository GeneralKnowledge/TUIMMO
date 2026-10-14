//! Rollback bundle for failed SQLite commits.

use std::collections::HashMap;

use terminus_core::{LogEntry, PlayerId, SessionId};

use crate::geography::GeographyState;
use crate::home::HomeState;
use crate::invite::InviteBook;
use crate::mail::Mailbox;
use crate::model::Operator;
use crate::settlement::SettlementState;
use crate::social::SocialState;
use crate::theories::TheoryBoard;
use crate::threads::ThreadState;
use crate::waypoint::Waypoint;

use super::WorldState;

/// In-memory world rows restored when a command commit fails.
#[derive(Clone, Debug)]
pub struct CommandRollback {
    log: Vec<LogEntry>,
    operators: HashMap<PlayerId, Operator>,
    geography: GeographyState,
    social: SocialState,
    settlements: SettlementState,
    waypoints: Vec<Waypoint>,
    homes: HomeState,
    mailbox: Mailbox,
    theories: TheoryBoard,
    threads: ThreadState,
    invites: InviteBook,
    overlays: HashMap<SessionId, String>,
}

impl WorldState {
    pub fn command_rollback_snapshot(&self) -> CommandRollback {
        CommandRollback {
            log: self.log_snapshot(),
            operators: self.operators.clone(),
            geography: self.geography.clone(),
            social: self.social.clone(),
            settlements: self.settlements.clone(),
            waypoints: self.waypoints.clone(),
            homes: self.homes.clone(),
            mailbox: self.mailbox.clone(),
            theories: self.theories.clone(),
            threads: self.threads.clone(),
            invites: self.invites.clone(),
            overlays: self.overlays.clone(),
        }
    }

    pub fn restore_command_rollback(&mut self, snap: CommandRollback) {
        self.replace_log(snap.log);
        self.operators = snap.operators;
        self.restore_geography(snap.geography);
        self.restore_social(snap.social);
        self.restore_settlements(snap.settlements);
        self.restore_waypoints(snap.waypoints);
        self.restore_homes(snap.homes);
        self.restore_mail(snap.mailbox);
        self.restore_theories(snap.theories);
        self.restore_threads(snap.threads);
        self.restore_invites(snap.invites);
        self.restore_overlays(snap.overlays);
    }
}
