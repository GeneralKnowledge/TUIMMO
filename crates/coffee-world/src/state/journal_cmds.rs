//! Private field journal. Entries live in `region_discoveries` as kind `field`.
//!
//! The content key is the discovery. The row id is not an access key, so guessing
//! another operator's identifier does not open their notes.

use terminus_core::{
    Audience, Channel, CommandOutcome, GameError, ParsedCommand, PlayerId, SessionId,
};
use uuid::Uuid;

use crate::geography::{
    format_entry, format_overview, format_recent, is_survey_tile, landmark_sight, roof_scratch,
    survey_closure, survey_finding, AuthoredFinding, JournalLine, WildernessPos, FIELD_KIND,
    SURVEY_MARK_FLAG, SURVEY_MARK_ID,
};
use crate::model::PersistOp;
use crate::state::{Transition, WorldState};

const USAGE: &str = "usage: journal [recent|read <id>|record]";

impl WorldState {
    pub(crate) fn cmd_journal(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let rest = cmd.rest.trim();
        if rest.is_empty() {
            return self.journal_show(session_id, false);
        }
        let mut parts = rest.split_whitespace();
        let action = parts.next().unwrap_or("").to_ascii_lowercase();
        let arg = parts.next().unwrap_or("").to_ascii_lowercase();
        if parts.next().is_some() {
            return Err(GameError::Rejected(USAGE.into()));
        }
        match action.as_str() {
            "recent" => {
                if !arg.is_empty() {
                    return Err(GameError::Rejected(USAGE.into()));
                }
                self.journal_show(session_id, true)
            }
            "read" => {
                if arg.is_empty() {
                    return Err(GameError::Rejected("Which entry? `journal` lists them.".into()));
                }
                self.journal_read(session_id, &arg)
            }
            "record" => {
                if !arg.is_empty() {
                    return Err(GameError::Rejected(USAGE.into()));
                }
                self.journal_record(session_id)
            }
            _ => Err(GameError::Rejected(USAGE.into())),
        }
    }

    fn journal_show(
        &mut self,
        session_id: SessionId,
        recent: bool,
    ) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let lines = self.journal_lines(player);
        let text = if recent { format_recent(&lines) } else { format_overview(&lines) };
        self.set_overlay(session_id, text);
        Ok(crate::state::quiet())
    }

    fn journal_read(
        &mut self,
        session_id: SessionId,
        content_key: &str,
    ) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let Some(entry) = self.geography.field_note(player, content_key) else {
            return Err(GameError::Rejected(format!("No journal entry matches '{content_key}'.")));
        };
        let line = journal_line(entry);
        self.set_overlay(session_id, format_entry(&line));
        Ok(crate::state::quiet())
    }

    fn journal_record(&mut self, session_id: SessionId) -> Result<Transition, GameError> {
        if !self.permissions(session_id)?.can_play() {
            return Err(GameError::Rejected("You cannot do that from this session.".into()));
        }
        let player = self.player(session_id)?;
        let offer = self.record_offer(player)?;
        let placed = match offer {
            RecordOffer::Ready(placed) => placed,
            RecordOffer::Waiting(message) | RecordOffer::Empty(message) => {
                return self.private_note(player, message);
            }
        };
        if self.geography.field_note(player, &placed.finding.content_key).is_some() {
            let key = placed.finding.content_key;
            return self.private_note(
                player,
                format!("Already in your journal as {key}. `journal read {key}`."),
            );
        }
        let mut ops = Vec::new();
        let (region_id, local_x, local_y) = if let Some(pos) = placed.wild {
            ops.extend(self.ensure_region(pos.identity, Some(player), pos.local_x, pos.local_y)?);
            let region_id = self
                .geography
                .region(pos.identity.coord_key())
                .map(|region| region.id)
                .ok_or_else(|| GameError::Rejected("This region is not open.".into()))?;
            (Some(region_id), pos.local_x, pos.local_y)
        } else {
            (None, 0, 0)
        };
        if placed.finding.content_key == SURVEY_MARK_ID {
            self.note_survey_mark(player, &mut ops);
        }
        let id = Uuid::new_v4();
        let created_at = self.now_ms;
        let content_key = placed.finding.content_key.clone();
        self.geography.discoveries.push(crate::geography::Discovery {
            id,
            region_id,
            local_x,
            local_y,
            kind: FIELD_KIND.into(),
            note: placed.finding.note.clone(),
            actor_id: Some(player),
            created_at,
            content_key: Some(content_key.clone()),
            title: Some(placed.finding.title.clone()),
            place_key: Some(placed.place_key.clone()),
        });
        ops.push(PersistOp::Discovery {
            id,
            region_id,
            local_x,
            local_y,
            kind: FIELD_KIND.into(),
            note: placed.finding.note,
            actor_id: Some(player),
            created_at,
            content_key: Some(content_key.clone()),
            title: Some(placed.finding.title),
            place_key: Some(placed.place_key),
        });
        let (event, op) = self.emit(
            Channel::System,
            Audience::Player(player),
            None,
            format!("Recorded in your journal as {content_key}. `journal read {content_key}`."),
        );
        ops.push(op);
        Ok(Transition { outcome: CommandOutcome { events: vec![event], quit: false }, ops })
    }

    fn private_note(&mut self, player: PlayerId, text: String) -> Result<Transition, GameError> {
        let (event, op) = self.emit(Channel::System, Audience::Player(player), None, text);
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![op],
        })
    }

    fn journal_lines(&self, player: PlayerId) -> Vec<JournalLine> {
        self.geography.field_notes_for(player).into_iter().map(journal_line).collect()
    }

    fn record_offer(&self, player: PlayerId) -> Result<RecordOffer, GameError> {
        let location = self
            .operators
            .get(&player)
            .map(|op| op.location_id.clone())
            .ok_or(GameError::UnknownSession)?;
        if let Some(pos) = WildernessPos::parse(&location) {
            if is_survey_tile(pos) {
                return Ok(RecordOffer::Ready(PlacedFinding {
                    finding: survey_finding(),
                    place_key: pos.key(),
                    wild: Some(pos),
                }));
            }
            let tile = self.effective_at(pos)?;
            if let Some(kind) = tile.landmark {
                if let Some(finding) = landmark_sight(pos, kind).finding {
                    return Ok(RecordOffer::Ready(PlacedFinding {
                        finding,
                        place_key: pos.key(),
                        wild: Some(pos),
                    }));
                }
            }
            return Ok(RecordOffer::Empty("Nothing here asks for a field note.".into()));
        }
        if location == "roof-array" {
            return Ok(RecordOffer::Ready(PlacedFinding {
                finding: roof_scratch(),
                place_key: location,
                wild: None,
            }));
        }
        if location == "old-relay" {
            let saw_mark =
                self.operators.get(&player).is_some_and(|op| op.flags.contains(SURVEY_MARK_FLAG));
            if saw_mark {
                return Ok(RecordOffer::Ready(PlacedFinding {
                    finding: survey_closure(),
                    place_key: location,
                    wild: None,
                }));
            }
            return Ok(RecordOffer::Waiting(
                "The dish is scuffed, but you have nothing yet that explains it.".into(),
            ));
        }
        Ok(RecordOffer::Empty("Nothing here asks for a field note.".into()))
    }
}

struct PlacedFinding {
    finding: AuthoredFinding,
    place_key: String,
    wild: Option<WildernessPos>,
}

enum RecordOffer {
    Ready(PlacedFinding),
    Waiting(String),
    Empty(String),
}

fn journal_line(entry: &crate::geography::Discovery) -> JournalLine {
    let place_key = entry.place_key.clone().unwrap_or_default();
    JournalLine {
        content_key: entry.content_key.clone().unwrap_or_default(),
        title: entry.title.clone().unwrap_or_else(|| "Field note".into()),
        note: entry.note.clone(),
        place_key,
        recorded_at: entry.created_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geography::{
        gate_identity, generate_region, landmark_sight, survey_position, LandmarkKind,
        ENTERED_KIND, SURVEY_CLOSURE_ID, SURVEY_SCRATCH_ID,
    };
    use crate::state::starter_operator;
    use terminus_core::{Game, Permissions, SessionIdentity};

    fn pair() -> (WorldState, SessionId, SessionId, PlayerId, PlayerId) {
        let mut world = WorldState::for_tests();
        let ada_id = PlayerId::new();
        let bob_id = PlayerId::new();
        world.insert_operator(starter_operator(ada_id, uuid::Uuid::new_v4(), "ada", "ada"));
        world.insert_operator(starter_operator(bob_id, uuid::Uuid::new_v4(), "bob", "bob"));
        let ada = SessionIdentity::new(ada_id, "ada", Permissions::player());
        let bob = SessionIdentity::new(bob_id, "bob", Permissions::player());
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

    fn field_count(world: &WorldState, player: PlayerId) -> usize {
        world.geography.field_notes_for(player).len()
    }

    #[test]
    fn empty_journal_and_unknown_ids_are_explicit() {
        let (mut world, ada, _, _, _) = pair();
        world.apply(ada, "journal").unwrap();
        let text = overlay(&world, ada);
        assert!(text.contains("empty"));
        assert!(text.contains("journal record"));
        let err = world.apply(ada, "journal read missing").unwrap_err();
        assert!(err.to_string().contains("No journal entry matches"));
        let err = world.apply(ada, "journal read survey-scratch").unwrap_err();
        assert!(err.to_string().contains("No journal entry matches"));
        assert!(world.apply(ada, "journal nope").is_err());
    }

    #[test]
    fn survey_is_solved_by_walking_and_stays_private() {
        let (mut world, ada, bob, ada_id, bob_id) = pair();
        assert_eq!(field_count(&world, ada_id), 0);

        world.apply(ada, "go up").unwrap();
        let roof = world.view(ada).unwrap();
        assert!(roof.description.contains("scratches"));
        world.apply(ada, "inspect antenna").unwrap();
        let clue = feed(&world, ada);
        assert!(clue.contains("one step west of the gate"));
        assert!(!clue.contains("never filed"));
        let noticed = world.view(ada).unwrap();
        assert!(noticed.description.contains("scratches"));
        assert!(noticed.finding.as_deref().unwrap_or("").contains("one step west of the gate"));
        assert!(!noticed.finding.as_deref().unwrap_or("").contains("never filed"));
        assert!(world.view(bob).unwrap().finding.is_none());
        world.apply(ada, "journal record").unwrap();
        assert_eq!(field_count(&world, ada_id), 1);
        world.apply(ada, "journal record").unwrap();
        assert_eq!(field_count(&world, ada_id), 1);
        assert!(feed(&world, ada).contains("Already in your journal"));
        assert!(!feed(&world, bob).contains("Gate scratch"));
        assert!(!feed(&world, bob).contains("one step west"));

        world.apply(ada, "outskirts").unwrap();
        world.apply(ada, "west").unwrap();
        let here = world.location_key(ada).unwrap();
        assert_eq!(here, survey_position().key());
        let scene = world.view(ada).unwrap();
        assert!(scene.description.contains("short mark"));
        assert!(scene.objects.iter().any(|object| object.keyword == "mark"));
        world.apply(ada, "inspect mark").unwrap();
        let mark = feed(&world, ada);
        assert!(mark.contains("relay insulation"));
        assert!(mark.contains("does not say which dish"));
        assert!(!mark.contains("never filed"));
        let on_stone = world.view(ada).unwrap();
        assert!(on_stone.description.contains("short mark"));
        assert!(on_stone.finding.as_deref().unwrap_or("").contains("relay insulation"));
        assert!(!on_stone.finding.as_deref().unwrap_or("").contains("never filed"));
        assert!(world.operator(ada_id).unwrap().flags.contains(SURVEY_MARK_FLAG));
        world.apply(ada, "journal record").unwrap();
        world.apply(ada, "journal read survey-mark").unwrap();
        let entry = overlay(&world, ada);
        assert!(entry.contains("From the city gate: west."));
        assert!(entry.contains(&survey_position().key()));
        assert!(entry.contains("Repeated mark"));

        for step in ["east", "city", "down", "alley", "down", "relay"] {
            world.apply(ada, step).unwrap();
        }
        assert_eq!(world.location_key(ada).unwrap(), "old-relay");
        world.apply(ada, "inspect dish").unwrap();
        let dish = feed(&world, ada);
        assert!(dish.contains("never filed"));
        assert!(dish.contains("journal record"));
        let relay = world.view(ada).unwrap();
        assert!(
            relay.description.contains("scuffed")
                || relay.finding.as_deref().unwrap_or("").contains("scuffed")
        );
        assert!(relay.finding.as_deref().unwrap_or("").contains("never filed"));
        assert!(relay.exits.iter().all(|exit| exit.keyword != "archive"));
        world.apply(ada, "journal record").unwrap();
        world.apply(ada, "journal").unwrap();
        let list = overlay(&world, ada);
        assert!(list.contains(SURVEY_SCRATCH_ID));
        assert!(list.contains(SURVEY_MARK_ID));
        assert!(list.contains(SURVEY_CLOSURE_ID));
        assert!(list.contains("Field journal (3)"));
        world.apply(ada, "journal recent").unwrap();
        assert!(overlay(&world, ada).contains("Unfiled survey"));
        world.apply(ada, "journal read survey-closure").unwrap();
        let closure = overlay(&world, ada);
        assert!(closure.contains("go relay"));
        assert!(closure.contains("old-relay"));
        assert!(!closure.contains("cipher plate"));

        world.apply(bob, "journal").unwrap();
        assert!(overlay(&world, bob).contains("empty"));
        let leaked = world.apply(bob, "journal read survey-closure").unwrap_err();
        assert!(leaked.to_string().contains("No journal entry matches"));
        let guessed = Uuid::new_v4();
        let leaked = world.apply(bob, &format!("journal read {guessed}")).unwrap_err();
        assert!(leaked.to_string().contains("No journal entry matches"));
        assert_eq!(field_count(&world, bob_id), 0);

        world.apply(bob, "go up").unwrap();
        world.apply(bob, "outskirts").unwrap();
        world.apply(bob, "west").unwrap();
        world.apply(bob, "inspect stone").unwrap();
        world.apply(bob, "journal record").unwrap();
        assert_eq!(field_count(&world, bob_id), 1);
        assert_eq!(field_count(&world, ada_id), 3);
        let shared = world
            .geography
            .discoveries
            .iter()
            .filter(|row| row.content_key.as_deref() == Some(SURVEY_MARK_ID))
            .count();
        assert_eq!(shared, 2);
        assert!(world.operator(bob_id).unwrap().flags.contains(SURVEY_MARK_FLAG));
        assert!(!world.operator(bob_id).unwrap().flags.contains("cipher-plate"));
    }

    #[test]
    fn relay_closure_stays_closed_until_the_mark_is_seen() {
        let (mut world, ada, _, ada_id, _) = pair();
        world.operators.get_mut(&ada_id).unwrap().location_id = "old-relay".into();
        world.apply(ada, "inspect dish").unwrap();
        let dish = feed(&world, ada);
        assert!(dish.contains("scuffed"));
        assert!(!dish.contains("never filed"));
        world.apply(ada, "journal record").unwrap();
        assert!(feed(&world, ada).contains("nothing yet"));
        assert_eq!(field_count(&world, ada_id), 0);
        world.apply(ada, "inspect relay").unwrap();
        assert!(world
            .operator(ada_id)
            .unwrap()
            .items
            .iter()
            .any(|item| item.keyword == "cipher-plate"));
        world.apply(ada, "go archive").unwrap();
        assert_eq!(world.view(ada).unwrap().location_name, "The Black Archive");
    }

    #[test]
    fn landmark_inspections_are_deterministic_and_idempotent() {
        let region = generate_region(gate_identity()).unwrap();
        assert_eq!(LandmarkKind::Milepost.prose(), "An old milepost. The number has worn off.");
        assert_eq!(
            LandmarkKind::FallenMast.prose(),
            "A fallen antenna mast lies half-buried in the dirt."
        );
        assert_eq!(
            LandmarkKind::StandingStone.prose(),
            "A standing stone, older than the city, marks the spot."
        );
        assert_eq!(
            LandmarkKind::Spring.prose(),
            "A spring leaks clear water from a crack in the ground."
        );
        let mut notable = None;
        let mut ordinary = None;
        for local_y in 0..16u8 {
            for local_x in 0..16u8 {
                let Some(kind) = region.tile(local_x, local_y).unwrap().landmark else {
                    continue;
                };
                let pos = WildernessPos { identity: gate_identity(), local_x, local_y };
                let sight = landmark_sight(pos, kind);
                if sight.finding.is_some() && notable.is_none() {
                    notable = Some((pos, kind));
                }
                if sight.finding.is_none() && ordinary.is_none() {
                    ordinary = Some((pos, kind));
                }
            }
        }
        let (notable_pos, notable_kind) = notable.expect("a notable landmark");
        let (ordinary_pos, ordinary_kind) = ordinary.expect("an ordinary landmark");
        let (mut world, ada, bob, ada_id, bob_id) = pair();
        world.operators.get_mut(&ada_id).unwrap().location_id = ordinary_pos.key();
        world.apply(ada, &format!("inspect {}", ordinary_kind.keyword())).unwrap();
        let ordinary_text = feed(&world, ada);
        assert!(ordinary_text.to_ascii_lowercase().contains(ordinary_kind.name()));
        assert!(!ordinary_text.contains("journal record"));
        world.apply(ada, "journal record").unwrap();
        assert!(feed(&world, ada).contains("Nothing here asks"));
        assert_eq!(field_count(&world, ada_id), 0);
        world.apply(ada, "inspect ground").unwrap();
        assert!(
            feed(&world, ada).contains("Nothing unusual") || feed(&world, ada).contains("is here")
        );

        world.operators.get_mut(&ada_id).unwrap().location_id = notable_pos.key();
        world.operators.get_mut(&bob_id).unwrap().location_id = notable_pos.key();
        let first = landmark_sight(notable_pos, notable_kind);
        assert_eq!(first, landmark_sight(notable_pos, notable_kind));
        world.apply(ada, &format!("inspect {}", notable_kind.keyword())).unwrap();
        assert!(feed(&world, ada).contains("journal record"));
        world.apply(ada, "journal record").unwrap();
        world.apply(ada, "journal record").unwrap();
        assert_eq!(field_count(&world, ada_id), 1);
        world.apply(bob, "journal record").unwrap();
        assert_eq!(field_count(&world, bob_id), 1);
        let key = first.finding.unwrap().content_key;
        assert_eq!(
            world.geography.field_note(ada_id, &key).unwrap().content_key,
            world.geography.field_note(bob_id, &key).unwrap().content_key
        );
        assert_ne!(
            world.geography.field_note(ada_id, &key).unwrap().id,
            world.geography.field_note(bob_id, &key).unwrap().id
        );
        world.apply(ada, &format!("journal read {key}")).unwrap();
        let page = overlay(&world, ada);
        assert!(page.contains("Place:"));
        assert!(page.contains(&notable_pos.key()));
        let err = world.apply(bob, "journal read survey-scratch").unwrap_err();
        assert!(err.to_string().contains("No journal entry matches"));
    }

    #[test]
    fn a_failed_save_keeps_no_partial_note() {
        let (mut world, ada, _, ada_id, _) = pair();
        world.apply(ada, "go up").unwrap();
        let geography = world.geography_snapshot();
        let operator = world.operator(ada_id).cloned().unwrap();
        world.apply(ada, "journal record").unwrap();
        assert_eq!(field_count(&world, ada_id), 1);
        world.restore_geography(geography);
        world.insert_operator(operator);
        assert_eq!(field_count(&world, ada_id), 0);
        world.apply(ada, "journal").unwrap();
        assert!(overlay(&world, ada).contains("empty"));
        assert!(world
            .geography
            .discoveries
            .iter()
            .all(|row| row.kind == ENTERED_KIND || row.actor_id != Some(ada_id)));
    }

    #[test]
    fn help_lists_the_journal() {
        let (mut world, ada, _, _, _) = pair();
        world.apply(ada, "help journal").unwrap();
        let text = overlay(&world, ada);
        assert!(text.contains("journal"));
        assert!(text.contains("record"));
    }
}
