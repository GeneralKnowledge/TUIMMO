//! Local and global speech commands.

use terminus_core::text::{clean_line, MAX_SPEECH_CHARS};
use terminus_core::{Audience, Channel, CommandOutcome, GameError, ParsedCommand, SessionId};

use crate::state::{Transition, WorldState};

impl WorldState {
    pub(crate) fn cmd_say(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        self.speak(session_id, &cmd.rest, false)
    }

    pub(crate) fn cmd_chat(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        self.speak(session_id, &cmd.rest, true)
    }

    fn speak(
        &mut self,
        session_id: SessionId,
        text: &str,
        global: bool,
    ) -> Result<Transition, GameError> {
        let text = clean_line(text, MAX_SPEECH_CHARS);
        if text.is_empty() {
            return Err(GameError::Rejected(if global {
                "Chat what?".into()
            } else {
                "Say what?".into()
            }));
        }
        let name = self.name_of(session_id)?;
        let (audience, channel) = if global {
            (Audience::Everyone, Channel::Global)
        } else {
            (Audience::Location(self.location_id(session_id)?), Channel::Local)
        };
        let (event, op) = self.emit(channel, audience, Some(name), text);
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![op],
        })
    }
}
