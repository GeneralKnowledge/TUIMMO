//! Shared theories about the delisted floor.
//!
//! Publishing is an explicit command. Journal rows, flags, items, waypoints,
//! and mail are never copied here.

use terminus_core::{
    Audience, Channel, CommandOutcome, GameError, ParsedCommand, PlayerId, SessionId,
};

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
            "list" => {
                if !tail.is_empty() {
                    return Err(GameError::Rejected("usage: theory list".into()));
                }
                self.theory_list(session_id)
            }
            "read" => self.theory_read(session_id, tail),
            "post" => self.theory_post(session_id, tail),
            "remove" => self.theory_remove(session_id, tail),
            _ => Err(GameError::Rejected(
                "usage: theory [list|read <id>|post <observation|theory> <text>|remove <id>]"
                    .into(),
            )),
        }
    }

    fn theory_help(&mut self, session_id: SessionId) -> Result<Transition, GameError> {
        let count = self.theories.count_topic(TOPIC);
        self.set_overlay(session_id, format!("{}\n\n{TOPIC}: {count} entries.", theory_help()));
        Ok(quiet())
    }

    fn theory_list(&mut self, session_id: SessionId) -> Result<Transition, GameError> {
        let rows = self.theories.for_topic(TOPIC);
        if rows.is_empty() {
            self.set_overlay(
                session_id,
                format!(
                    "{TOPIC}\nNo entries yet.\n{}\n`theory post observation <text>`\n`theory post theory <text>`",
                    "Player-authored. Not checked against the world."
                ),
            );
            return Ok(quiet());
        }
        let mut lines = vec![
            format!("{TOPIC}  ({} entries)", rows.len()),
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
        let Some(entry) = self.theories.get(TOPIC, id).cloned() else {
            return Err(GameError::Rejected(format!("No entry {} on {TOPIC}.", format_id(id))));
        };
        self.set_overlay(session_id, self.entry_page(&entry));
        Ok(quiet())
    }

    fn theory_post(&mut self, session_id: SessionId, tail: &str) -> Result<Transition, GameError> {
        let mut parts = tail.splitn(2, char::is_whitespace);
        let kind_word = parts.next().unwrap_or("").to_ascii_lowercase();
        let raw_body = parts.next().unwrap_or("").trim();
        let Some(kind) = TheoryKind::parse(&kind_word) else {
            return Err(GameError::Rejected(
                "usage: theory post <observation|theory> <text>".into(),
            ));
        };
        let body = clean_body(raw_body).map_err(|message| GameError::Rejected(message.into()))?;
        let author = self.player(session_id)?;
        if self.theories.posted_too_soon(author, self.now_ms) {
            return Err(GameError::Rejected("Wait a moment before posting another entry.".into()));
        }
        if self.theories.count_topic(TOPIC) >= TOPIC_CAP {
            return Err(GameError::Rejected(format!(
                "{TOPIC} has {TOPIC_CAP} entries. Nothing was erased. An author can remove their own entry."
            )));
        }
        if self.theories.next_id == 0 || self.theories.next_id > i64::MAX as u64 {
            return Err(GameError::Rejected("This topic cannot accept another entry.".into()));
        }
        let id = self.theories.next_id;
        let created_at = self.now_ms;
        self.theories.next_id = self.theories.next_id.saturating_add(1);
        self.theories.last_post_at.insert(author, created_at);
        let entry = TheoryEntry {
            id,
            topic: TOPIC.to_string(),
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
            format!("Filed {phrase} as {}.", format_id(id)),
        );
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![
                PersistOp::TheoryInsert {
                    id,
                    topic: TOPIC.to_string(),
                    author_id: author,
                    kind,
                    body,
                    created_at,
                },
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
        let Some(entry) = self.theories.get(TOPIC, id) else {
            return Err(GameError::Rejected(format!("No entry {} on {TOPIC}.", format_id(id))));
        };
        if entry.author_id != author {
            return Err(GameError::Rejected("You can only remove your own entry.".into()));
        }
        self.theories.entries.retain(|row| !(row.topic == TOPIC && row.id == id));
        self.theory_list(session_id)?;
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
            "{}  {}\n{}  {}\n{}\nPlayer-authored. Not checked against the world.",
            format_id(entry.id),
            entry.kind.as_str(),
            self.public_name(entry.author_id),
            format_received(entry.created_at),
            entry.body
        )
    }

    fn public_name(&self, player: PlayerId) -> String {
        self.operators
            .get(&player)
            .map(|op| op.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "unknown".into())
    }
}

fn theory_help() -> String {
    let seconds = POST_INTERVAL_MS / 1000;
    format!(
        "\
Shared theories
A record of what operators think may be happening.
Your journal stays private. Nothing here is copied from it.
This is not a quest log, and it does not unlock a place.
The only topic is {TOPIC}.
An observation is something you believe you saw.
A theory is an interpretation. It may be wrong.
Neither is checked against the world.

theory
theory list
theory read <id>
theory post observation <text>
theory post theory <text>
theory remove <id>

An entry is at most {BODY_MAX} characters. {TOPIC} holds {TOPIC_CAP}.
A full topic refuses the new post. Older entries stay.
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

    fn feed(world: &WorldState, session: SessionId) -> String {
        world
            .view(session)
            .unwrap()
            .feed
            .iter()
            .map(|line| line.text.clone())
            .collect::<Vec<_>>()
            .join("\n")
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
        world.apply(bob, "theory list").unwrap();
        let list = overlay(&world, bob);
        assert!(list.contains("observation"));
        assert!(list.contains("theory"));
        assert!(list.contains("Ada"));
        assert!(!list.contains("Robert"));
        assert!(list.contains("Not checked against the world"));
        world.apply(bob, "theory read 1").unwrap();
        let page = overlay(&world, bob);
        assert!(page.contains("#1"));
        assert!(page.contains("observation"));
        assert!(page.contains("Ada"));
        assert!(page.contains("The panel still shows B-4."));
        world.apply(bob, "theory read #2").unwrap();
        let theory = overlay(&world, bob);
        assert!(theory.contains("theory"));
        assert!(theory.contains("The lock may still be with the relay."));
        assert_eq!(world.theories.get(TOPIC, 1).unwrap().author_id, ada_id);
        assert!(!feed(&world, bob).contains("The panel still shows B-4."));
        world.operators.get_mut(&ada_id).unwrap().name = "Adelaide".into();
        world.apply(bob, "theory list").unwrap();
        assert!(overlay(&world, bob).contains("Adelaide"));
        assert!(!overlay(&world, bob).contains("Ada  "));
    }

    #[test]
    fn journal_is_not_copied_and_reading_grants_nothing() {
        let (mut world, ada, bob, ada_id, bob_id) = pair();
        for step in ["alley", "exchange", "service", "down", "inspect terminal", "up"] {
            world.apply(ada, step).unwrap();
        }
        world.apply(ada, "journal record").unwrap();
        assert_eq!(world.geography.field_notes_for(ada_id).len(), 1);
        world.apply(ada, "theory list").unwrap();
        let empty = overlay(&world, ada);
        assert!(empty.contains("No entries yet"));
        assert!(!empty.contains("struck from the public list"));
        post(&mut world, ada, "observation", "I wrote this on purpose.");
        assert!(!overlay(&world, ada).contains("struck from the public list"));
        assert!(!feed(&world, ada).contains("I wrote this on purpose."));
        assert!(feed(&world, ada).contains("Filed an observation as #1."));
        world.apply(bob, "theory read 1").unwrap();
        assert!(overlay(&world, bob).contains("I wrote this on purpose."));
        assert!(world.geography.field_notes_for(bob_id).is_empty());
        assert!(!world.operator(bob_id).unwrap().flags.contains(READ_TERMINAL_FLAG));
        assert!(!world.operator(bob_id).unwrap().flags.contains("cipher-plate"));
        assert!(world
            .operator(bob_id)
            .unwrap()
            .items
            .iter()
            .all(|item| item.keyword != "torn-log"));
        assert!(world.operator(ada_id).unwrap().flags.contains(READ_TERMINAL_FLAG));
        assert!(world
            .operator(ada_id)
            .unwrap()
            .items
            .iter()
            .any(|item| item.keyword == "torn-log"));
        assert_eq!(world.geography.field_notes_for(ada_id).len(), 1);
        assert!(world.social.notices.is_empty());
        for step in ["alley", "exchange", "service", "down", "relay"] {
            world.apply(bob, step).unwrap();
        }
        for step in ["down", "relay"] {
            world.apply(ada, step).unwrap();
        }
        let ada_sealed = world.apply(ada, "go archive").unwrap_err();
        let bob_sealed = world.apply(bob, "go archive").unwrap_err();
        assert!(ada_sealed.to_string().contains("sealed"));
        assert!(bob_sealed.to_string().contains("cipher plate"));
        assert!(!world.operator(ada_id).unwrap().flags.contains("cipher-plate"));
        assert!(!world.operator(bob_id).unwrap().flags.contains("cipher-plate"));
        world.apply(ada, "theory remove 1").unwrap();
        assert!(world.theories.entries.is_empty());
        assert_eq!(world.geography.field_notes_for(ada_id).len(), 1);
        assert!(world.operator(ada_id).unwrap().flags.contains(READ_TERMINAL_FLAG));
        let still = world.apply(ada, "go archive").unwrap_err();
        assert!(still.to_string().contains("sealed"));
    }

    #[test]
    fn authors_remove_only_their_own_entries_and_ids_are_not_reused() {
        let (mut world, ada, bob, _, _) = pair();
        post(&mut world, ada, "theory", "Maybe the floor was closed for the relay.");
        let err = world.apply(bob, "theory remove 1").unwrap_err();
        assert!(err.to_string().contains("your own entry"));
        assert!(world.theories.get(TOPIC, 1).is_some());
        world.apply(ada, "theory remove #1").unwrap();
        assert!(world.theories.get(TOPIC, 1).is_none());
        assert_eq!(world.theories.next_id, 2);
        let missing = world.apply(bob, "theory read 1").unwrap_err();
        assert!(missing.to_string().contains("No entry #1"));
        post(&mut world, bob, "observation", "The crossed-out line still ends in B-4.");
        assert_eq!(world.theories.get(TOPIC, 2).unwrap().kind, TheoryKind::Observation);
        assert!(world.theories.get(TOPIC, 1).is_none());
        world.apply(bob, "theory list").unwrap();
        assert!(overlay(&world, bob).contains("Robert"));
        assert!(overlay(&world, bob).contains("observation"));
    }

    #[test]
    fn length_rate_and_capacity_are_refused_without_erasing() {
        let (mut world, ada, _, _, _) = pair();
        let too_long = "b".repeat(BODY_MAX + 1);
        let err = world.apply(ada, &format!("theory post observation {too_long}")).unwrap_err();
        assert!(err.to_string().contains("120"));
        assert!(world.theories.entries.is_empty());
        let err = world.apply(ada, "theory post observation \u{1b}\u{7}").unwrap_err();
        assert!(err.to_string().contains("Write an observation"));
        post(&mut world, ada, "observation", "panel\u{1b}[2J still shows B-4");
        assert_eq!(world.theories.get(TOPIC, 1).unwrap().body, "panel[2J still shows B-4");
        assert_eq!(world.location_key(ada).unwrap(), "coffee-shop");
        world.apply(ada, "theory remove 1").unwrap();
        post(&mut world, ada, "observation", "First mark on the panel.");
        let err = world.apply(ada, "theory post theory A second thought.").unwrap_err();
        assert!(err.to_string().contains("Wait a moment"));
        assert_eq!(world.theories.count_topic(TOPIC), 1);
        for n in 2..=TOPIC_CAP {
            post(&mut world, ada, "theory", &format!("guess {n}"));
        }
        assert_eq!(world.theories.count_topic(TOPIC), TOPIC_CAP);
        world.now_ms = world.now_ms.saturating_add(POST_INTERVAL_MS);
        let err = world.apply(ada, "theory post observation one more").unwrap_err();
        assert!(err.to_string().contains("Nothing was erased"));
        assert_eq!(world.theories.count_topic(TOPIC), TOPIC_CAP);
        assert!(world
            .theories
            .entries
            .iter()
            .any(|entry| entry.body == "First mark on the panel."));
        assert!(world
            .theories
            .entries
            .iter()
            .any(|entry| entry.body == format!("guess {TOPIC_CAP}")));
    }

    #[test]
    fn a_failed_theory_commit_restores_the_board() {
        let (mut world, ada, _, ada_id, _) = pair();
        let board = world.theory_snapshot();
        let overlays = world.overlay_snapshot();
        let log = world.log_snapshot();
        let operator = world.operator(ada_id).cloned().unwrap();
        post(&mut world, ada, "observation", "Should roll back.");
        assert_eq!(world.theories.count_topic(TOPIC), 1);
        world.restore_theories(board);
        world.restore_overlays(overlays);
        world.replace_log(log);
        world.insert_operator(operator);
        assert!(world.theories.entries.is_empty());
        assert_eq!(world.theories.next_id, 1);
        assert!(!feed(&world, ada).contains("Filed an observation"));
        post(&mut world, ada, "theory", "Kept after the rollback.");
        assert_eq!(world.theories.get(TOPIC, 1).unwrap().kind, TheoryKind::Theory);
        assert_eq!(world.theories.next_id, 2);
    }

    #[test]
    fn help_explains_shared_theories() {
        let (mut world, ada, _, _, _) = pair();
        world.apply(ada, "help theory").unwrap();
        let topic = overlay(&world, ada);
        assert!(topic.contains("theory"));
        assert!(topic.contains("observation"));
        world.apply(ada, "theory").unwrap();
        let page = overlay(&world, ada);
        assert!(page.contains("delisted-floor"));
        assert!(page.contains("not a quest log"));
        assert!(page.contains("journal stays private"));
        assert!(page.contains("32"));
        assert!(page.contains("120"));
        assert!(page.contains("does not unlock"));
    }
}
