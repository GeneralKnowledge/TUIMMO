//! Read-only survey aggregation and publish-to-board (archive channel).

use terminus_core::{Audience, Channel, CommandOutcome, GameError, ParsedCommand, SessionId};
use uuid::Uuid;

use crate::geography::WildernessPos;
use crate::history::chart_hint_flag;
use crate::model::PersistOp;
use crate::model::STARTER_LOCATION;
use crate::social::{validate_notice_text, NOTICE_RETENTION_MS};
use crate::state::{Transition, WorldState};

impl WorldState {
    pub(crate) fn cmd_survey(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let rest = cmd.rest.trim();
        let mut parts = rest.splitn(2, char::is_whitespace);
        let action = parts.next().unwrap_or("").to_ascii_lowercase();
        let tail = parts.next().unwrap_or("").trim();
        match action.as_str() {
            "" | "status" => self.survey_status(session_id),
            "publish" => self.survey_publish(session_id, tail),
            _ => Err(GameError::Rejected("usage: survey status | survey publish <text>".into())),
        }
    }

    fn survey_status(&mut self, session_id: SessionId) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let location = self.location_id(session_id)?;
        let mut lines = vec!["Survey (private summary)".to_string()];
        let journal_count = self.geography.field_notes_for(player).len();
        lines.push(format!("Journal entries (you): {journal_count}"));
        let clue_count = self.threads.seen.iter().filter(|row| row.operator_id == player).count();
        lines.push(format!("Thread clues noticed (you): {clue_count}"));
        if let Some(pos) = WildernessPos::parse(&location) {
            let flag = chart_hint_flag(&pos.identity);
            if self.operators.get(&player).is_some_and(|op| op.flags.contains(&flag)) {
                lines.push("You have charted this birth cell before.".into());
            } else if self.charting_inspect_line(player, pos).is_some() {
                lines.push("Someone else charted this birth cell first.".into());
            }
            lines.push(format!("Place key: {}", pos.key()));
        } else {
            lines.push(format!("Place: {location}"));
        }
        if let Some(subject) = self.theory_subject_at_location(player) {
            if let Some(summary) = self.theories_summary_line(&subject) {
                lines.push(summary);
            }
        }
        lines.push(
            "Publish durable notes with `survey publish …` (Coffee Shop board). \
Tile traces (chalk, signs) fade or stay local."
                .into(),
        );
        let body = lines.join("\n");
        let (event, op) = self.emit(Channel::System, Audience::Player(player), None, body);
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![op],
        })
    }

    fn survey_publish(
        &mut self,
        session_id: SessionId,
        raw: &str,
    ) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let location = self.location_id(session_id)?;
        if location != STARTER_LOCATION {
            return Err(GameError::Rejected(
                "Survey publish posts to the Coffee Shop board. Go there to publish.".into(),
            ));
        }
        let body = validate_notice_text(raw).map_err(|m| GameError::Rejected(m.into()))?;
        let created_at = self.now_ms;
        let id = Uuid::new_v4();
        let expires_at = created_at.saturating_add(NOTICE_RETENTION_MS);
        let place_id = STARTER_LOCATION.to_string();
        self.social.notices.push(crate::social::NoticePost {
            id,
            place_id: place_id.clone(),
            author_id: player,
            body: body.clone(),
            created_at,
            expires_at,
        });
        let (event, note) = self.emit(
            Channel::System,
            Audience::Location(location),
            None,
            format!("{} publishes a survey note to the board.", self.name_of(session_id)?),
        );
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![
                PersistOp::NoticeUpsert {
                    id,
                    place_id,
                    author_id: player,
                    body,
                    created_at,
                    expires_at,
                },
                note,
            ],
        })
    }
}
