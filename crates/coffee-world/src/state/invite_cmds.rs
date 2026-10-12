//! List unused account invite codes owned by the caller.

use terminus_core::{GameError, ParsedCommand, SessionId};

use crate::state::{quiet, Transition, WorldState};

const HELP: &str = "\
Invite codes
A code lets one friend register when this server is invite-only.
invite / invite list — show your unused codes.
Share a code out of band. It works once.
Redeemed codes are not shown again.";

impl WorldState {
    pub(crate) fn cmd_invite(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let rest = cmd.rest.trim();
        if rest.is_empty() || rest.eq_ignore_ascii_case("list") {
            return self.invite_list(session_id, player);
        }
        if rest.eq_ignore_ascii_case("help") {
            self.set_overlay(session_id, HELP);
            return Ok(quiet());
        }
        Err(GameError::Rejected("usage: invite [list]".into()))
    }

    fn invite_list(
        &mut self,
        session_id: SessionId,
        player: terminus_core::PlayerId,
    ) -> Result<Transition, GameError> {
        let unused = self.invites.unused_for(player);
        let redeemed = self.invites.redeemed_count_for(player);
        let mut lines = Vec::new();
        lines.push("Your invite codes".into());
        if unused.is_empty() {
            lines.push("None left to share.".into());
        } else {
            lines.push(format!("{} unused:", unused.len()));
            for row in unused {
                lines.push(format!("  {}", row.display_code));
            }
        }
        if redeemed > 0 {
            lines.push(format!("{redeemed} already used by friends."));
        }
        lines.push("Share a code privately. Each code registers one account.".into());
        self.set_overlay(session_id, lines.join("\n"));
        Ok(quiet())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invite::{new_invite, InviteBook};
    use crate::state::starter_operator;
    use terminus_core::{Game, Permissions, PlayerId, SessionIdentity};
    use uuid::Uuid;

    fn attach(world: &mut WorldState, name: &str) -> (PlayerId, SessionId) {
        let id = PlayerId::new();
        let op = starter_operator(id, Uuid::new_v4(), name, name);
        world.insert_operator(op);
        let identity = SessionIdentity::new(id, name, Permissions::player());
        let session = identity.session_id;
        world.connect_player(identity).unwrap();
        (id, session)
    }

    #[test]
    fn lists_only_unused_codes_for_caller() {
        let mut world = WorldState::for_tests();
        let (ada, session) = attach(&mut world, "ada");
        let bob = PlayerId::new();
        let mut used = new_invite(Some(ada), 1, "");
        used.redeemed_by = Some(bob);
        used.redeemed_at = Some(2);
        world.install_invites(InviteBook {
            codes: vec![used, new_invite(Some(ada), 3, ""), new_invite(Some(bob), 4, "")],
        });
        world.apply(session, "invite list").unwrap();
        let page = world.view(session).unwrap().overlay.unwrap_or_default();
        assert!(page.contains("1 unused"), "{page}");
        assert!(page.contains("1 already used"), "{page}");
        assert!(!page.contains(&world.invites.codes[2].display_code), "{page}");
    }
}
