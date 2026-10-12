//! Status, inventory, and quit.

use terminus_core::{CommandOutcome, GameError, ParsedCommand, SessionId};

use crate::state::{quiet, Transition, WorldState};

impl WorldState {
    pub(crate) fn cmd_status(
        &mut self,
        session_id: SessionId,
        _cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let lines = self.status_lines(session_id)?;
        self.set_overlay(session_id, lines.join("\n"));
        Ok(quiet())
    }

    pub(crate) fn cmd_inventory(
        &mut self,
        session_id: SessionId,
        _cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let items = self.operators.get(&player).map(|op| op.items.clone()).unwrap_or_default();
        let text = if items.is_empty() {
            "You are not carrying anything.".into()
        } else {
            let mut lines = vec!["You are carrying:".to_string()];
            for item in items {
                lines.push(format!("- {} ({})", item.name, item.keyword));
            }
            lines.join("\n")
        };
        self.set_overlay(session_id, text);
        Ok(quiet())
    }

    pub(crate) fn cmd_quit(
        &mut self,
        _session_id: SessionId,
        _cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        Ok(Transition {
            outcome: CommandOutcome { events: Vec::new(), quit: true },
            ops: Vec::new(),
        })
    }
}
