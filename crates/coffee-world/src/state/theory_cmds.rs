//! Shared public theories — interpretations tied to a subject (content or place key).

use terminus_core::{
    Audience, Channel, CommandOutcome, GameError, ParsedCommand, PlayerId, SessionId,
};

use crate::history::{split_subject_from_body, validate_theory_subject};
use crate::mail::format_received;
use crate::model::PersistOp;
use crate::state::{quiet, Transition, WorldState};
use crate::theories::{
    clean_body, format_id, parse_entry_id, preview, TheoryEntry, TheoryKind, BODY_MAX,
    POST_INTERVAL_MS, TOPIC, TOPIC_CAP,
};

impl WorldState {
    pub(crate) fn cmd_theory(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let rest = cmd.rest.trim();
        if rest.is_empty() {
            return self.theory_help(session_id);
        }
        let mut parts = rest.splitn(2, char::is_whitespace);
        let head = parts.next().unwrap_or("").to_ascii_lowercase();
        let tail = parts.next().unwrap_or("").trim();
        match head.as_str() {
            "list" => self.theory_list(session_id, tail),
            "read" => self.theory_read(session_id, tail),
            "post" => self.theory_post(session_id, tail),
            "remove" => self.theory_remove(session_id, tail),
            _ => Err(GameError::Rejected(
                "usage: theory [list [subject]|read <id>|post <observation|theory> [subject] <text>|remove <id>]"
                    .into(),
            )),
        }
    }

    fn theory_help(&mut self, session_id: SessionId) -> Result<Transition, GameError> {
        let summaries = self.theories.topic_summaries();
        let mut lines = vec![theory_help()];
        if summaries.is_empty() {
            lines.push(String::new());
            lines.push("No public entries yet.".into());
        } else {
            lines.push(String::new());
            lines.push("Subjects with entries:".into());
            for (topic, count) in summaries {
                lines.push(format!("  {topic} ({count})"));
            }
        }
        self.set_overlay(session_id, lines.join("\n"));
        Ok(quiet())
    }

    fn theory_list(&mut self, session_id: SessionId, tail: &str) -> Result<Transition, GameError> {
        if tail.is_empty() {
            let summaries = self.theories.topic_summaries();
            if summaries.is_empty() {
                self.set_overlay(
                    session_id,
                    format!(
                        "No public theory entries yet.\n{}\n`theory post observation <text>` — at a discovery, or name a subject.",
                        "Player-authored. Not checked against the world."
                    ),
                );
                return Ok(quiet());
            }
            let mut lines = vec![
                "Public theory subjects".into(),
                "Player-authored. Not checked against the world.".into(),
            ];
            for (topic, count) in summaries {
                lines.push(format!("  {topic} ({count}) — `theory list {topic}`"));
            }
            self.set_overlay(session_id, lines.join("\n"));
            return Ok(quiet());
        }
        let subject = validate_theory_subject(tail).map_err(|m| GameError::Rejected(m.into()))?;
        self.theory_list_subject(session_id, &subject)
    }

    fn theory_list_subject(
        &mut self,
        session_id: SessionId,
        subject: &str,
    ) -> Result<Transition, GameError> {
        let rows = self.theories.for_topic(subject);
        if rows.is_empty() {
            self.set_overlay(
                session_id,
                format!(
                    "{subject}\nNo entries yet.\n{}\n`theory post observation {subject} <text>`",
                    "Player-authored. Not checked against the world."
                ),
            );
            return Ok(quiet());
        }
        let mut lines = vec![
            format!("{subject}  ({} entries)", rows.len()),
            "Player-authored. Not checked against the world.".into(),
        ];
        for entry in rows {
            lines.push(format!(
                "{}  {}  {}  {}",
                format_id(entry.id),
                entry.kind.as_str(),
                self.public_name(entry.author_id),
                preview(&entry.body)
            ));
        }
        lines.push("`theory read <id>`".into());
        self.set_overlay(session_id, lines.join("\n"));
        Ok(quiet())
    }

    fn theory_read(
        &mut self,
        session_id: SessionId,
        raw_id: &str,
    ) -> Result<Transition, GameError> {
        if raw_id.is_empty() || raw_id.split_whitespace().nth(1).is_some() {
            return Err(GameError::Rejected("usage: theory read <id>".into()));
        }
        let id = parse_entry_id(raw_id).map_err(|message| GameError::Rejected(message.into()))?;
        let Some(entry) = self.theories.entry_by_id(id).cloned() else {
            return Err(GameError::Rejected(format!("No entry {}.", format_id(id))));
        };
        self.set_overlay(session_id, self.entry_page(&entry));
        Ok(quiet())
    }

    fn theory_post(&mut self, session_id: SessionId, tail: &str) -> Result<Transition, GameError> {
        let mut parts = tail.splitn(2, char::is_whitespace);
        let kind_word = parts.next().unwrap_or("").to_ascii_lowercase();
        let raw_tail = parts.next().unwrap_or("").trim();
        let Some(kind) = TheoryKind::parse(&kind_word) else {
            return Err(GameError::Rejected(
                "usage: theory post <observation|theory> [subject] <text>".into(),
            ));
        };
        let (explicit_subject, body_raw) = if let Some(rest) = raw_tail.strip_prefix("about ") {
            let mut bits = rest.splitn(2, char::is_whitespace);
            let sub = bits.next().ok_or_else(|| {
                GameError::Rejected("usage: theory post observation about <subject> <text>".into())
            })?;
            let body = bits
                .next()
                .ok_or_else(|| GameError::Rejected("Write an observation or a theory.".into()))?;
            (Some(validate_theory_subject(sub).map_err(|m| GameError::Rejected(m.into()))?), body)
        } else {
            let (subject, body) = split_subject_from_body(raw_tail);
            (subject, body)
        };
        let body = clean_body(body_raw).map_err(|message| GameError::Rejected(message.into()))?;
        let author = self.player(session_id)?;
        let topic = if let Some(subject) = explicit_subject {
            subject
        } else if let Some(inferred) = self.theory_subject_at_location(author) {
            inferred
        } else {
            TOPIC.to_string()
        };
        if self.theories.posted_too_soon(author, self.now_ms) {
            return Err(GameError::Rejected("Wait a moment before posting another entry.".into()));
        }
        if self.theories.count_topic(&topic) >= TOPIC_CAP {
            return Err(GameError::Rejected(format!(
                "{topic} has {TOPIC_CAP} entries. Nothing was erased. An author can remove their own entry."
            )));
        }
        if self.theories.next_id == 0 || self.theories.next_id > i64::MAX as u64 {
            return Err(GameError::Rejected("This subject cannot accept another entry.".into()));
        }
        let id = self.theories.next_id;
        let created_at = self.now_ms;
        self.theories.next_id = self.theories.next_id.saturating_add(1);
        self.theories.last_post_at.insert(author, created_at);
        let entry = TheoryEntry {
            id,
            topic: topic.clone(),
            author_id: author,
            kind,
            body: body.clone(),
            created_at,
        };
        self.theories.entries.push(entry.clone());
        self.set_overlay(session_id, self.entry_page(&entry));
        let phrase = match kind {
            TheoryKind::Observation => "an observation",
            TheoryKind::Theory => "a theory",
        };
        let (event, note) = self.emit(
            Channel::System,
            Audience::Player(author),
            None,
            format!("Filed {phrase} as {} on {topic}.", format_id(id)),
        );
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![
                PersistOp::TheoryInsert { id, topic, author_id: author, kind, body, created_at },
                note,
            ],
        })
    }

    fn theory_remove(
        &mut self,
        session_id: SessionId,
        raw_id: &str,
    ) -> Result<Transition, GameError> {
        if raw_id.is_empty() || raw_id.split_whitespace().nth(1).is_some() {
            return Err(GameError::Rejected("usage: theory remove <id>".into()));
        }
        let id = parse_entry_id(raw_id).map_err(|message| GameError::Rejected(message.into()))?;
        let author = self.player(session_id)?;
        let Some(entry) = self.theories.entry_by_id(id) else {
            return Err(GameError::Rejected(format!("No entry {}.", format_id(id))));
        };
        if entry.author_id != author {
            return Err(GameError::Rejected("You can only remove your own entry.".into()));
        }
        let topic = entry.topic.clone();
        self.theories.entries.retain(|row| row.id != id);
        self.theory_list_subject(session_id, &topic)?;
        let (event, note) = self.emit(
            Channel::System,
            Audience::Player(author),
            None,
            format!("Removed {}.", format_id(id)),
        );
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![PersistOp::TheoryDelete { id, author_id: author }, note],
        })
    }

    fn entry_page(&self, entry: &TheoryEntry) -> String {
        format!(
            "Subject: {}\n{}  {}\n{}  {}\n{}\nPlayer-authored. Not checked against the world.",
            entry.topic,
            format_id(entry.id),
            entry.kind.as_str(),
            self.public_name(entry.author_id),
            format_received(entry.created_at),
            entry.body
        )
    }

    fn public_name(&self, player: PlayerId) -> String {
        self.public_name_for(player)
    }
}

fn theory_help() -> String {
    let seconds = POST_INTERVAL_MS / 1000;
    format!(
        "\
Shared theories
Public interpretations tied to a subject — usually the same id as a field discovery (`survey-mark`, `delisted-floor`) or a place key.
Your journal stays private. Nothing here is copied from it.
This is not a quest log, and it does not unlock a place.
An observation is something you believe you saw.
A theory is an interpretation. It may be wrong.
Neither is checked against the world. Several entries can disagree.

theory
theory list
theory list <subject>
theory read <id>
theory post observation <text>
theory post theory <text>
theory post observation <subject> <text>
theory post observation about <subject> <text>
theory remove <id>

Standing at a discovery fills in the subject when you omit it.
Without a subject and nowhere special to stand, posts go to `{TOPIC}` (legacy default).

An entry is at most {BODY_MAX} characters. Each subject holds up to {TOPIC_CAP} entries.
A full subject refuses the new post. Older entries stay.
You can post once every {seconds} seconds. That gap resets on restart.
You can remove only your own entry. Ids are not reused.
`look` returns to the place."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::starter_operator;
    use crate::theories::{BODY_MAX, POST_INTERVAL_MS, TOPIC_CAP};
    use crate::undernet::READ_TERMINAL_FLAG;
    use terminus_core::{Game, Permissions, SessionIdentity};

    fn pair() -> (WorldState, SessionId, SessionId, PlayerId, PlayerId) {
        let mut world = WorldState::for_tests();
        let ada_id = PlayerId::new();
        let bob_id = PlayerId::new();
        world.insert_operator(starter_operator(ada_id, uuid::Uuid::new_v4(), "ada", "Ada"));
        world.insert_operator(starter_operator(bob_id, uuid::Uuid::new_v4(), "bob", "Robert"));
        let ada = SessionIdentity::new(ada_id, "Ada", Permissions::player());
        let bob = SessionIdentity::new(bob_id, "Robert", Permissions::player());
        let ada_session = ada.session_id;
        let bob_session = bob.session_id;
        world.connect_player(ada).unwrap();
        world.connect_player(bob).unwrap();
        (world, ada_session, bob_session, ada_id, bob_id)
    }

    fn overlay(world: &WorldState, session: SessionId) -> String {
        world.view(session).unwrap().overlay.unwrap_or_default()
    }

    fn post(world: &mut WorldState, session: SessionId, kind: &str, text: &str) {
        world.now_ms = world.now_ms.saturating_add(POST_INTERVAL_MS);
        world.apply(session, &format!("theory post {kind} {text}")).unwrap();
    }

    #[test]
    fn an_observation_and_a_theory_are_readable_by_name() {
        let (mut world, ada, bob, ada_id, _) = pair();
        post(&mut world, ada, "observation", "The panel still shows B-4.");
        post(&mut world, ada, "theory", "The lock may still be with the relay.");
        world.detach(ada).unwrap();
        world.apply(bob, "theory list delisted-floor").unwrap();
        let list = overlay(&world, bob);
        assert!(list.contains("observation"));
        assert!(list.contains("theory"));
        assert!(list.contains("Ada"));
        assert!(!list.contains("Robert"));
        world.apply(bob, "theory read 1").unwrap();
        let page = overlay(&world, bob);
        assert!(page.contains("Subject: delisted-floor"));
        assert!(page.contains("#1"));
        world.apply(bob, "theory read #2").unwrap();
        assert!(overlay(&world, bob).contains("The lock may still be with the relay."));
        assert_eq!(world.theories.entry_by_id(1).unwrap().author_id, ada_id);
    }

    #[test]
    fn journal_is_not_copied_and_reading_grants_nothing() {
        let (mut world, ada, bob, ada_id, bob_id) = pair();
        for step in ["alley", "exchange", "service", "down", "inspect terminal", "up"] {
            world.apply(ada, step).unwrap();
        }
        world.apply(ada, "journal record").unwrap();
        world.apply(ada, "theory list delisted-floor").unwrap();
        assert!(overlay(&world, ada).contains("No entries yet"));
        post(&mut world, ada, "observation", "I wrote this on purpose.");
        world.apply(bob, "theory read 1").unwrap();
        assert!(overlay(&world, bob).contains("I wrote this on purpose."));
        assert!(world.geography.field_notes_for(bob_id).is_empty());
        assert!(!world.operator(bob_id).unwrap().flags.contains(READ_TERMINAL_FLAG));
        assert_eq!(world.geography.field_notes_for(ada_id).len(), 1);
    }

    #[test]
    fn authors_remove_only_their_own_entries_and_ids_are_not_reused() {
        let (mut world, ada, bob, _, _) = pair();
        post(&mut world, ada, "theory", "Maybe the floor was closed for the relay.");
        let err = world.apply(bob, "theory remove 1").unwrap_err();
        assert!(err.to_string().contains("your own entry"));
        world.apply(ada, "theory remove #1").unwrap();
        post(&mut world, bob, "observation", "The crossed-out line still ends in B-4.");
        assert_eq!(world.theories.entry_by_id(2).unwrap().kind, TheoryKind::Observation);
    }

    #[test]
    fn length_rate_and_capacity_are_refused_without_erasing() {
        let (mut world, ada, _, _, _) = pair();
        let too_long = "b".repeat(BODY_MAX + 1);
        let err = world.apply(ada, &format!("theory post observation {too_long}")).unwrap_err();
        assert!(err.to_string().contains("120"));
        post(&mut world, ada, "observation", "First mark on the panel.");
        let err = world.apply(ada, "theory post theory A second thought.").unwrap_err();
        assert!(err.to_string().contains("Wait a moment"));
        for n in 2..=TOPIC_CAP {
            post(&mut world, ada, "theory", &format!("guess {n}"));
        }
        world.now_ms = world.now_ms.saturating_add(POST_INTERVAL_MS);
        let err = world.apply(ada, "theory post observation one more").unwrap_err();
        assert!(err.to_string().contains("Nothing was erased"));
    }

    #[test]
    fn explicit_subject_and_competing_theories_coexist() {
        let (mut world, ada, bob, _, _) = pair();
        world.now_ms = world.now_ms.saturating_add(POST_INTERVAL_MS);
        world.apply(ada, "theory post theory survey-mark Deliberate survey closure.").unwrap();
        world.now_ms = world.now_ms.saturating_add(POST_INTERVAL_MS);
        world
            .apply(
                bob,
                "theory post observation about survey-mark Fiber scuff matches the gate bearing.",
            )
            .unwrap();
        world.apply(ada, "theory list survey-mark").unwrap();
        let list = overlay(&world, ada);
        assert!(list.contains("survey-mark"));
        assert!(list.contains("theory"));
        assert!(list.contains("observation"));
        assert!(list.contains("Deliberate"));
        assert!(list.contains("Fiber"));
    }

    #[test]
    fn help_explains_shared_theories() {
        let (mut world, ada, _, _, _) = pair();
        world.apply(ada, "theory").unwrap();
        let page = overlay(&world, ada);
        assert!(page.contains("subject"));
        assert!(page.contains("journal stays private"));
    }

    #[test]
    fn invalid_subject_and_missing_entry_are_rejected() {
        let (mut world, ada, _, _, _) = pair();
        let err = world
            .apply(ada, "theory post observation about bad! subject Still one line.")
            .unwrap_err();
        assert!(err.to_string().contains("Theory subjects"));
        let err = world.apply(ada, "theory list not-a-valid-subject!").unwrap_err();
        assert!(err.to_string().contains("Theory subjects"));
        let err = world.apply(ada, "theory read 999").unwrap_err();
        assert!(err.to_string().contains("No entry"));
        assert_eq!(world.theories.entries.len(), 0);
    }
}
