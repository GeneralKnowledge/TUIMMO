//! Personal home designation commands.
//!
//! Cabins remain under `home inspect|access|…`. Claiming a home does not build
//! a cabin and does not copy journals, waypoints, or discovery flags.

use terminus_core::{GameError, PlayerId, SessionId};
use uuid::Uuid;

use crate::geography::{describe_place, WildernessPos, ENTERED_KIND};
use crate::home::{validate_home_name, ExplorationTraits, PersonalHome, MIN_AWAY_MS};
use crate::model::PersistOp;
use crate::state::{quiet, Transition, WorldState};
use crate::subway;

const USAGE: &str =
    "usage: home [claim|move|abandon|name|inspect|access|share|allow|deny|rename|demolish] …";

const HELP: &str = "\
Personal home
Designate one place in the Squalor as your home. No building is required.

home
home claim
home name <name>
home name clear
home move
home move confirm
home abandon confirm

Cabin and workshop controls (when you stand at one you own):
home inspect · home access · home share · home allow · home deny
home rename · home demolish confirm

Rules:
You may have one active home. Claiming again does not move it.
Moving abandons the old place. Abandoning leaves you without a home.
Names are public on the street. Journals and waypoints stay private.
A home is not a teleport. Time and real returns settle it.
After ten minutes away, returning home counts as an expedition return — you will notice when it lands.

`look` returns to the place.";

impl WorldState {
    pub(crate) fn home_status(&mut self, session_id: SessionId) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let text = if let Some(home) = self.homes.active_for(player) {
            let title = describe_place(&home.place_key).0;
            let here = self.location_id(session_id)?;
            let traits = self.exploration_traits_for(player);
            let mut lines = vec!["Your home".into(), format!("Place: {title}")];
            lines.extend(home.owner_status_lines(self.now_ms, traits));
            if here == home.place_key {
                lines.push("You are standing at your home.".into());
            } else {
                lines.push(format!("A way back: {}", describe_place(&home.place_key).1));
            }
            lines.push(String::new());
            lines.push("home name <name> · home name clear".into());
            lines.push(
                "home move — make another eligible place your home (abandons this one).".into(),
            );
            lines.push("home abandon confirm — leave this home with no replacement.".into());
            lines.push("`look` returns to the place.".into());
            lines.join("\n")
        } else {
            "You have no home.\n\
             Stand on eligible ground in the Squalor and type `home claim`.\n\
             The Hub, the city gate, subway stairs, and another player's active home cannot be claimed.\n\
             \n\
             Cabin controls still use home inspect|access|share|… when you stand at a cabin you own.\n\
             `look` returns to the place."
                .to_string()
        };
        self.set_overlay(session_id, text);
        Ok(quiet())
    }

    pub(crate) fn home_help(&mut self, session_id: SessionId) -> Result<Transition, GameError> {
        self.set_overlay(session_id, HELP);
        Ok(quiet())
    }

    pub(crate) fn home_claim(&mut self, session_id: SessionId) -> Result<Transition, GameError> {
        self.require_play(session_id)?;
        let player = self.player(session_id)?;
        let place_key = self.location_id(session_id)?;
        if let Some(existing) = self.homes.active_for(player) {
            if existing.place_key == place_key {
                return Err(GameError::Rejected("This is already your home.".into()));
            }
            let title = describe_place(&existing.place_key).0;
            return Err(GameError::Rejected(format!(
                "You already have a home at {title}. Moving abandons it. Type: home move confirm"
            )));
        }
        self.validate_home_destination(player, &place_key)?;
        let title = describe_place(&place_key).0;
        let mut ops = self.ensure_home_region(&place_key, player)?;
        let now = self.now_ms;
        let home = fresh_home(player, place_key.clone(), now);
        ops.push(claim_op(&home));
        self.homes.insert(home);
        self.private_home_line(player, format!("You make a home at {title}."), ops)
    }

    pub(crate) fn home_name(
        &mut self,
        session_id: SessionId,
        tail: &str,
    ) -> Result<Transition, GameError> {
        self.require_play(session_id)?;
        let player = self.player(session_id)?;
        let Some(current) = self.homes.active_for(player).cloned() else {
            return Err(GameError::Rejected(
                "You have no home to name. Stand somewhere eligible and type `home claim`.".into(),
            ));
        };
        let name = if tail.eq_ignore_ascii_case("clear") {
            String::new()
        } else {
            validate_home_name(tail).map_err(|message| GameError::Rejected(message.into()))?
        };
        if !self.homes.set_name(current.id, player, name.clone()) {
            return Err(GameError::Rejected("That home is no longer yours to name.".into()));
        }
        let op = PersistOp::HomeRename {
            id: current.id,
            operator_id: player,
            public_name: name.clone(),
        };
        let message = if name.is_empty() {
            "Your home returns to an untitled name.".into()
        } else {
            format!("Your home is now called {name}.")
        };
        self.private_home_line(player, message, vec![op])
    }

    pub(crate) fn home_move(
        &mut self,
        session_id: SessionId,
        confirm: bool,
    ) -> Result<Transition, GameError> {
        self.require_play(session_id)?;
        let player = self.player(session_id)?;
        let Some(current) = self.homes.active_for(player).cloned() else {
            return Err(GameError::Rejected(
                "You have no home to move. Stand somewhere eligible and type `home claim`.".into(),
            ));
        };
        let place_key = self.location_id(session_id)?;
        if current.place_key == place_key {
            return Err(GameError::Rejected("This is already your home.".into()));
        }
        let old_title = describe_place(&current.place_key).0;
        if !confirm {
            let new_title = describe_place(&place_key).0;
            return Err(GameError::Rejected(format!(
                "You have already made a home in {old_title}.\n\
                 If you make this place your home, {old_title} will be abandoned.\n\
                 Make this your new home? Type: home move confirm\n\
                 (Standing at: {new_title})"
            )));
        }
        self.validate_home_destination(player, &place_key)?;
        // Re-check ownership after validation; another failure path must not clear the old home.
        let Some(owned) = self.homes.active_for(player).cloned() else {
            return Err(GameError::Rejected("Your previous home is no longer active.".into()));
        };
        if owned.id != current.id || owned.place_key != current.place_key {
            return Err(GameError::Rejected(
                "Your home changed before the move finished. Nothing was moved.".into(),
            ));
        }
        let new_title = describe_place(&place_key).0;
        let mut ops = self.ensure_home_region(&place_key, player)?;
        let now = self.now_ms;
        if !self.homes.abandon(owned.id, now) {
            return Err(GameError::Rejected(
                "Your previous home could not be abandoned. Nothing was moved.".into(),
            ));
        }
        ops.push(PersistOp::HomeAbandon { id: owned.id, operator_id: player, abandoned_at: now });
        let home = fresh_home(player, place_key.clone(), now);
        ops.push(claim_op(&home));
        self.homes.insert(home);
        self.private_home_line(
            player,
            format!("You leave {old_title} behind and make a home at {new_title}."),
            ops,
        )
    }

    pub(crate) fn home_abandon(
        &mut self,
        session_id: SessionId,
        confirm: bool,
    ) -> Result<Transition, GameError> {
        self.require_play(session_id)?;
        let player = self.player(session_id)?;
        let Some(current) = self.homes.active_for(player).cloned() else {
            return Err(GameError::Rejected("You have no home to abandon.".into()));
        };
        let title = describe_place(&current.place_key).0;
        if !confirm {
            return Err(GameError::Rejected(format!(
                "Abandoning leaves you without a home. {title} will remain as an abandoned place.\n\
                 Type: home abandon confirm"
            )));
        }
        let now = self.now_ms;
        if !self.homes.abandon(current.id, now) {
            return Err(GameError::Rejected("That home is no longer active.".into()));
        }
        let ops =
            vec![PersistOp::HomeAbandon { id: current.id, operator_id: player, abandoned_at: now }];
        self.private_home_line(
            player,
            format!("You abandon your home at {title}. You have no home now."),
            ops,
        )
    }

    fn validate_home_destination(
        &self,
        player: PlayerId,
        place_key: &str,
    ) -> Result<(), GameError> {
        let Some(pos) = WildernessPos::parse(place_key) else {
            return Err(GameError::Rejected(
                "You can only claim a home in the Squalor, not inside the Hub or a subway station."
                    .into(),
            ));
        };
        if pos.is_gate() {
            return Err(GameError::Rejected("The Hub gate must stay clear.".into()));
        }
        let tile = self.effective_at(pos)?;
        if tile.metro {
            return Err(GameError::Rejected("The subway entrance must stay clear.".into()));
        }
        if !tile.traversable() {
            return Err(GameError::Rejected("You cannot make a home on impassable ground.".into()));
        }
        if let Some(other) = self.homes.active_at(place_key) {
            if other.operator_id != player {
                return Err(GameError::Rejected(
                    "Someone else already keeps an active home here.".into(),
                ));
            }
        }
        Ok(())
    }

    fn ensure_home_region(
        &mut self,
        place_key: &str,
        player: PlayerId,
    ) -> Result<Vec<PersistOp>, GameError> {
        let pos = WildernessPos::parse(place_key)
            .ok_or_else(|| GameError::Rejected("That place is not in the Squalor.".into()))?;
        self.ensure_region(pos.identity, Some(player), pos.local_x, pos.local_y)
    }

    /// Record a departure from or meaningful return to the owner's active home.
    pub(crate) fn note_home_travel(
        &mut self,
        player: PlayerId,
        from: &str,
        to: &str,
    ) -> HomeTravelNote {
        if from == to {
            return HomeTravelNote::empty();
        }
        let Some(home) = self.homes.active_for(player).cloned() else {
            return HomeTravelNote::empty();
        };
        let now = self.now_ms;
        if from == home.place_key {
            let Some(row) = self.homes.active_for_mut(player) else {
                return HomeTravelNote::empty();
            };
            row.last_departed_at = Some(now);
            return HomeTravelNote {
                ops: vec![PersistOp::HomeHabitation {
                    id: row.id,
                    operator_id: player,
                    meaningful_returns: row.meaningful_returns,
                    last_departed_at: row.last_departed_at,
                    last_returned_at: row.last_returned_at,
                }],
                owner_message: None,
            };
        }
        if to != home.place_key {
            return HomeTravelNote::empty();
        }
        let (counted_return, habitation_op) = {
            let Some(row) = self.homes.active_for_mut(player) else {
                return HomeTravelNote::empty();
            };
            let mut counted_return = false;
            if let Some(departed) = row.last_departed_at {
                if now.saturating_sub(departed) >= MIN_AWAY_MS {
                    row.meaningful_returns = row.meaningful_returns.saturating_add(1);
                    row.last_returned_at = Some(now);
                    counted_return = true;
                }
            }
            let had_departed = row.last_departed_at.is_some();
            if had_departed {
                row.last_departed_at = None;
            }
            if !counted_return && !had_departed {
                return HomeTravelNote::empty();
            }
            let op = PersistOp::HomeHabitation {
                id: row.id,
                operator_id: player,
                meaningful_returns: row.meaningful_returns,
                last_departed_at: row.last_departed_at,
                last_returned_at: row.last_returned_at,
            };
            (counted_return, op)
        };
        let owner_message = if counted_return {
            let traits = self.exploration_traits_for(player);
            let home = self.homes.active_for(player).expect("active home");
            Some(home.meaningful_return_message(now, traits))
        } else {
            None
        };
        HomeTravelNote { ops: vec![habitation_op], owner_message }
    }

    pub(crate) fn exploration_traits_for(&self, player: PlayerId) -> ExplorationTraits {
        let journal_notes = !self.geography.field_notes_for(player).is_empty();
        let survey_interest = self.operators.get(&player).is_some_and(|op| {
            op.flags.contains(crate::geography::SURVEY_MARK_FLAG)
                || op.flags.contains(crate::undernet::READ_TERMINAL_FLAG)
        }) || self.geography.field_note(player, "delisted-floor").is_some()
            || self.geography.field_note(player, "survey-mark").is_some();
        let districts = self
            .geography
            .discoveries
            .iter()
            .filter(|row| row.kind == ENTERED_KIND && row.actor_id == Some(player))
            .count()
            >= 2;
        // Durable evidence only: a field note taken in a station, or an entered
        // birth cell that contains a metro seam (Ash Plaza / Ward West regions).
        let subway = self
            .geography
            .field_notes_for(player)
            .iter()
            .any(|note| note.place_key.as_deref().is_some_and(subway::is_station))
            || self.geography.discoveries.iter().any(|row| {
                row.kind == ENTERED_KIND
                    && row.actor_id == Some(player)
                    && row.region_id.is_some_and(|rid| {
                        self.geography.region_by_id(rid).is_some_and(|region| {
                            subway::entrances().iter().any(|entrance| {
                                entrance.surface.identity.coord_key() == region.identity.coord_key()
                            })
                        })
                    })
            });
        ExplorationTraits { journal_notes, survey_interest, districts, subway }
    }

    fn private_home_line(
        &mut self,
        player: PlayerId,
        text: String,
        mut ops: Vec<PersistOp>,
    ) -> Result<Transition, GameError> {
        let (event, op) = self.emit(
            terminus_core::Channel::System,
            terminus_core::Audience::Player(player),
            None,
            text,
        );
        ops.push(op);
        Ok(Transition {
            outcome: terminus_core::CommandOutcome { events: vec![event], quit: false },
            ops,
        })
    }
}

pub(crate) fn home_usage() -> &'static str {
    USAGE
}

/// Side effects of recording home departure or return during movement.
pub(crate) struct HomeTravelNote {
    pub ops: Vec<PersistOp>,
    /// Private system line when a meaningful return was counted.
    pub owner_message: Option<String>,
}

impl HomeTravelNote {
    pub fn empty() -> Self {
        Self { ops: Vec::new(), owner_message: None }
    }
}

fn fresh_home(player: PlayerId, place_key: String, claimed_at: u64) -> PersonalHome {
    PersonalHome {
        id: Uuid::new_v4(),
        operator_id: player,
        place_key,
        claimed_at,
        abandoned_at: None,
        public_name: String::new(),
        public_description: String::new(),
        meaningful_returns: 0,
        last_departed_at: None,
        last_returned_at: None,
    }
}

fn claim_op(home: &PersonalHome) -> PersistOp {
    PersistOp::HomeClaim {
        id: home.id,
        operator_id: home.operator_id,
        place_key: home.place_key.clone(),
        claimed_at: home.claimed_at,
        public_name: home.public_name.clone(),
        public_description: home.public_description.clone(),
        meaningful_returns: home.meaningful_returns,
        last_departed_at: home.last_departed_at,
        last_returned_at: home.last_returned_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geography::{gate_position, survey_position, WildernessPos};
    use crate::home::{
        ExplorationTraits, Habitation, ESTABLISHED_TIME_MS, FORGOTTEN_MS, MIN_AWAY_MS,
        RECENTLY_ABANDONED_MS, SETTLING_TIME_MS,
    };
    use crate::state::starter_operator;
    use crate::subway::{ash_entrance_pos, ASH_STATION};
    use terminus_core::{Game, Permissions, SessionIdentity};

    fn pair() -> (WorldState, SessionId, SessionId, PlayerId, PlayerId) {
        let mut world = WorldState::for_tests();
        let ada_id = PlayerId::new();
        let bob_id = PlayerId::new();
        world.insert_operator(starter_operator(ada_id, Uuid::new_v4(), "ada", "ada"));
        world.insert_operator(starter_operator(bob_id, Uuid::new_v4(), "bob", "bob"));
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

    fn stand_at(world: &mut WorldState, player: PlayerId, place: &str) {
        world.operators.get_mut(&player).unwrap().location_id = place.to_string();
    }

    fn walkable_near_gate(world: &mut WorldState, player: PlayerId) -> String {
        // Survey stone is one step west of the gate and walkable.
        let pos = survey_position();
        let _ = world.ensure_region(pos.identity, Some(player), pos.local_x, pos.local_y).unwrap();
        pos.key()
    }

    fn second_walkable(world: &mut WorldState, player: PlayerId) -> String {
        // Two steps west of the gate (local 6,8) is open street in the authored corridor.
        let mut pos = gate_position();
        pos.local_x = 6;
        let _ = world.ensure_region(pos.identity, Some(player), pos.local_x, pos.local_y).unwrap();
        pos.key()
    }

    #[test]
    fn claim_lists_and_rejects_duplicates_and_protected_places() {
        let (mut world, ada, bob, ada_id, bob_id) = pair();
        world.apply(ada, "home").unwrap();
        assert!(overlay(&world, ada).contains("You have no home"));

        assert!(world.apply(ada, "home claim").unwrap_err().to_string().contains("Squalor"));

        let place = walkable_near_gate(&mut world, ada_id);
        stand_at(&mut world, ada_id, &place);
        world.apply(ada, "home claim").unwrap();
        assert!(feed(&world, ada).contains("You make a home"));
        assert_eq!(world.homes.active_for(ada_id).unwrap().place_key, place);

        assert!(world
            .apply(ada, "home claim")
            .unwrap_err()
            .to_string()
            .contains("already your home"));
        assert_eq!(world.homes.homes.iter().filter(|h| h.is_active()).count(), 1);

        stand_at(&mut world, bob_id, &place);
        assert!(world
            .apply(bob, "home claim")
            .unwrap_err()
            .to_string()
            .contains("Someone else already keeps an active home"));

        stand_at(&mut world, ada_id, &gate_position().key());
        assert!(world
            .apply(ada, "home claim")
            .unwrap_err()
            .to_string()
            .contains("already have a home"));
        // Move attempt at gate is refused; original home remains.
        let err = world.apply(ada, "home move confirm").unwrap_err().to_string();
        assert!(err.contains("gate") || err.contains("clear"), "{err}");
        assert_eq!(world.homes.active_for(ada_id).unwrap().place_key, place);

        stand_at(&mut world, ada_id, ASH_STATION);
        let err = world.apply(ada, "home move confirm").unwrap_err().to_string();
        assert!(err.contains("Hub") || err.contains("subway") || err.contains("Squalor"), "{err}");
        assert_eq!(world.homes.active_for(ada_id).unwrap().place_key, place);

        stand_at(&mut world, bob_id, &ash_entrance_pos().key());
        assert!(world.apply(bob, "home claim").unwrap_err().to_string().contains("subway"));
    }

    #[test]
    fn move_requires_confirm_and_is_atomic() {
        let (mut world, ada, _, ada_id, _) = pair();
        let first = walkable_near_gate(&mut world, ada_id);
        let second = second_walkable(&mut world, ada_id);
        stand_at(&mut world, ada_id, &first);
        world.apply(ada, "home claim").unwrap();
        let first_id = world.homes.active_for(ada_id).unwrap().id;

        stand_at(&mut world, ada_id, &second);
        let prompt = world.apply(ada, "home claim").unwrap_err().to_string();
        assert!(prompt.contains("home move confirm"), "{prompt}");
        assert_eq!(world.homes.active_for(ada_id).unwrap().id, first_id);

        let prompt = world.apply(ada, "home move").unwrap_err().to_string();
        assert!(prompt.contains("will be abandoned"), "{prompt}");
        assert!(prompt.contains("home move confirm"), "{prompt}");
        assert_eq!(world.homes.active_for(ada_id).unwrap().id, first_id);

        world.apply(ada, "home move confirm").unwrap();
        let active = world.homes.active_for(ada_id).unwrap();
        assert_eq!(active.place_key, second);
        assert_ne!(active.id, first_id);
        let old = world.homes.homes.iter().find(|h| h.id == first_id).unwrap();
        assert!(old.abandoned_at.is_some());
        assert_eq!(
            world.homes.homes.iter().filter(|h| h.operator_id == ada_id && h.is_active()).count(),
            1
        );

        // Failed move preserves the current home.
        stand_at(&mut world, ada_id, &gate_position().key());
        let before = world.homes.active_for(ada_id).cloned().unwrap();
        assert!(world.apply(ada, "home move confirm").is_err());
        assert_eq!(world.homes.active_for(ada_id).unwrap().id, before.id);
        assert_eq!(world.homes.active_for(ada_id).unwrap().place_key, second);
    }

    #[test]
    fn abandon_requires_confirm_and_leaves_no_active_home() {
        let (mut world, ada, _, ada_id, _) = pair();
        let place = walkable_near_gate(&mut world, ada_id);
        stand_at(&mut world, ada_id, &place);
        world.apply(ada, "home claim").unwrap();
        let err = world.apply(ada, "home abandon").unwrap_err().to_string();
        assert!(err.contains("home abandon confirm"), "{err}");
        assert!(world.homes.active_for(ada_id).is_some());
        world.apply(ada, "home abandon confirm").unwrap();
        assert!(world.homes.active_for(ada_id).is_none());
        assert!(feed(&world, ada).contains("no home now"));
        // Returning does not restore ownership.
        stand_at(&mut world, ada_id, &place);
        world.apply(ada, "home").unwrap();
        assert!(overlay(&world, ada).contains("You have no home"));
        // Reclaim creates a new active designation.
        world.apply(ada, "home claim").unwrap();
        assert_eq!(world.homes.active_for(ada_id).unwrap().place_key, place);
        assert_eq!(world.homes.homes.iter().filter(|h| h.place_key == place).count(), 2);
    }

    #[test]
    fn lifecycle_presentation_uses_clock_and_survives_in_memory_restart_shape() {
        let (mut world, ada, _bob, ada_id, bob_id) = pair();
        let place = walkable_near_gate(&mut world, ada_id);
        stand_at(&mut world, ada_id, &place);
        world.apply(ada, "home claim").unwrap();
        let scene = world.wild_scene_for_tests(WildernessPos::parse(&place).unwrap(), Some(ada_id));
        assert!(scene.contains("You have made a home here"), "{scene}");

        let second = second_walkable(&mut world, ada_id);
        stand_at(&mut world, ada_id, &second);
        world.apply(ada, "home move confirm").unwrap();
        let abandoned_at =
            world.homes.homes.iter().find(|h| h.place_key == place).unwrap().abandoned_at.unwrap();

        stand_at(&mut world, bob_id, &place);
        world.now_ms = abandoned_at;
        let recent =
            world.wild_scene_for_tests(WildernessPos::parse(&place).unwrap(), Some(bob_id));
        assert!(
            recent.contains("inhabited until recently") || recent.contains("left behind recently"),
            "{recent}"
        );
        world.now_ms = abandoned_at + RECENTLY_ABANDONED_MS;
        let forgotten =
            world.wild_scene_for_tests(WildernessPos::parse(&place).unwrap(), Some(bob_id));
        assert!(forgotten.contains("Dust covers"), "{forgotten}");
        world.now_ms = abandoned_at + FORGOTTEN_MS;
        let absorbed =
            world.wild_scene_for_tests(WildernessPos::parse(&place).unwrap(), Some(bob_id));
        assert!(!absorbed.contains("home"), "{absorbed}");
        assert!(!absorbed.contains("inhabited"), "{absorbed}");

        // Snapshot/restore shape used by hub rollback.
        let snap = world.home_snapshot();
        world.homes.homes.clear();
        world.restore_homes(snap);
        assert!(world.homes.active_for(ada_id).is_some());
        assert_eq!(world.homes.active_for(ada_id).unwrap().place_key, second);
    }

    #[test]
    fn private_waypoints_and_journals_stay_private_across_home_claim() {
        let (mut world, ada, bob, ada_id, bob_id) = pair();
        world.apply(ada, "go up").unwrap();
        world.apply(ada, "inspect antenna").unwrap();
        world.apply(ada, "journal record").unwrap();
        world.apply(ada, "waypoint add roof-scratch").unwrap();
        let place = walkable_near_gate(&mut world, ada_id);
        stand_at(&mut world, ada_id, &place);
        world.apply(ada, "home claim").unwrap();
        assert_eq!(world.waypoints.len(), 1);
        assert_eq!(world.geography.field_notes_for(ada_id).len(), 1);
        stand_at(&mut world, bob_id, &place);
        assert!(world.apply(bob, "waypoint show roof-scratch").is_err());
        assert!(world.apply(bob, "journal read survey-scratch").is_err());
        assert!(world.geography.field_notes_for(bob_id).is_empty());
    }

    #[test]
    fn cabin_controls_remain_on_home_command_beside_personal_homes() {
        let (mut world, ada, _, ada_id, _) = pair();
        let place = walkable_near_gate(&mut world, ada_id);
        stand_at(&mut world, ada_id, &place);
        world.apply(ada, "home claim").unwrap();
        world.apply(ada, "home").unwrap();
        assert!(overlay(&world, ada).contains("Your home"));
        // Without a cabin on the tile, cabin subcommands still refuse clearly.
        let err = world.apply(ada, "home access public").unwrap_err().to_string();
        assert!(
            err.contains("cabin")
                || err.contains("workshop")
                || err.contains("own")
                || err.contains("here"),
            "{err}"
        );
        assert!(world.homes.active_for(ada_id).is_some());
        // Existing demolish confirmation wording is unchanged.
        let err = world.apply(ada, "home demolish").unwrap_err().to_string();
        assert!(
            err.contains("demolish")
                || err.contains("cabin")
                || err.contains("workshop")
                || err.contains("own"),
            "{err}"
        );
    }

    #[test]
    fn acceptance_lifecycle_across_reconnect_and_distance() {
        let (mut world, ada, _, ada_id, _) = pair();
        // ~96 tiles west of the Hub gate: birth region (-6, 0).
        let identity = crate::geography::RegionIdentity {
            world: crate::geography::WorldId::Undernet,
            epoch_id: 1,
            origin_x: -6,
            origin_y: 0,
            generation_version: 1,
        };
        let generated = crate::geography::generate_region(identity).unwrap();
        let mut far = None;
        for local_y in 0..16u8 {
            for local_x in 0..16u8 {
                let pos = WildernessPos { identity, local_x, local_y };
                if world.effective_at(pos).unwrap().traversable()
                    && !pos.is_gate()
                    && crate::subway::entrance_at(pos).is_none()
                    && generated.tile(local_x, local_y).unwrap().terrain.traversable()
                {
                    far = Some(pos);
                    break;
                }
            }
            if far.is_some() {
                break;
            }
        }
        let far = far.expect("walkable tile ~100 steps from the Hub");
        let place = far.key();
        let _ = world.ensure_region(far.identity, Some(ada_id), far.local_x, far.local_y).unwrap();
        stand_at(&mut world, ada_id, &place);
        world.apply(ada, "home claim").unwrap();

        // Disconnect / reconnect: same operator id keeps the home.
        world.detach(ada).unwrap();
        let ada2 = SessionIdentity::new(ada_id, "ada", Permissions::player());
        let ada_session = ada2.session_id;
        world.connect_player(ada2).unwrap();
        world.apply(ada_session, "home").unwrap();
        assert!(overlay(&world, ada_session).contains("Your home"));
        assert_eq!(world.homes.active_for(ada_id).unwrap().place_key, place);

        let next = second_walkable(&mut world, ada_id);
        stand_at(&mut world, ada_id, &next);
        let prompt = world.apply(ada_session, "home move").unwrap_err().to_string();
        assert!(prompt.contains("will be abandoned"), "{prompt}");
        world.apply(ada_session, "home move confirm").unwrap();
        assert_eq!(world.homes.active_for(ada_id).unwrap().place_key, next);

        // Old place is no longer theirs; look text follows the abandoned lifecycle.
        assert_ne!(world.homes.active_for(ada_id).unwrap().place_key, place);
        stand_at(&mut world, ada_id, &place);
        let scene = world.wild_scene_for_tests(far, Some(ada_id));
        assert!(
            scene.contains("inhabited until recently") || scene.contains("left behind recently"),
            "{scene}"
        );
        assert!(!scene.contains("You have made a home here"), "{scene}");
    }

    #[test]
    fn naming_persists_renames_clears_and_stays_public_only() {
        let (mut world, ada, bob, ada_id, bob_id) = pair();
        let place = walkable_near_gate(&mut world, ada_id);
        stand_at(&mut world, ada_id, &place);
        world.apply(ada, "home claim").unwrap();
        assert!(world.apply(ada, "home name").unwrap_err().to_string().contains("usage"));
        assert!(world.apply(ada, "home name x").unwrap_err().to_string().contains("2"));
        assert!(world
            .apply(ada, &format!("home name {}", "a".repeat(25)))
            .unwrap_err()
            .to_string()
            .contains("24"));
        assert!(world
            .apply(ada, "home name You leave now")
            .unwrap_err()
            .to_string()
            .contains("system"));

        world.apply(ada, "home name Dust-Shelf").unwrap();
        assert_eq!(world.homes.active_for(ada_id).unwrap().public_name, "Dust-Shelf");
        world.apply(ada, "home").unwrap();
        assert!(overlay(&world, ada).contains("Dust-Shelf"));
        assert!(
            overlay(&world, ada).contains("Unsettled")
                || overlay(&world, ada).contains("unsettled")
                || overlay(&world, ada).contains("Condition:")
        );

        // Rename replaces; clear restores default presentation.
        world.apply(ada, "home name Wire Nest").unwrap();
        assert_eq!(world.homes.active_for(ada_id).unwrap().public_name, "Wire Nest");
        world.apply(ada, "home name clear").unwrap();
        assert!(world.homes.active_for(ada_id).unwrap().public_name.is_empty());
        world.apply(ada, "home name Wire Nest").unwrap();

        // Naming does not create a second home or change place_key.
        assert_eq!(world.homes.homes.iter().filter(|h| h.is_active()).count(), 1);
        assert_eq!(world.homes.active_for(ada_id).unwrap().place_key, place);

        // Visitors see the public name; not owner status lists about journals.
        stand_at(&mut world, bob_id, &place);
        let scene = world.wild_scene_for_tests(WildernessPos::parse(&place).unwrap(), Some(bob_id));
        assert!(scene.contains("Wire Nest"), "{scene}");
        assert!(!scene.contains("field journal"), "{scene}");
        assert!(!scene.contains("only you see"), "{scene}");

        // Abandoned home keeps historical name; renaming the new home does not rewrite it.
        let second = second_walkable(&mut world, ada_id);
        stand_at(&mut world, ada_id, &second);
        world.apply(ada, "home move confirm").unwrap();
        let abandoned_id = world.homes.homes.iter().find(|h| h.place_key == place).unwrap().id;
        assert_eq!(
            world.homes.homes.iter().find(|h| h.id == abandoned_id).unwrap().public_name,
            "Wire Nest"
        );
        world.apply(ada, "home name Still Mine").unwrap();
        assert_eq!(world.homes.active_for(ada_id).unwrap().public_name, "Still Mine");
        assert_eq!(
            world.homes.homes.iter().find(|h| h.id == abandoned_id).unwrap().public_name,
            "Wire Nest"
        );
        let scene = world.wild_scene_for_tests(WildernessPos::parse(&place).unwrap(), Some(bob_id));
        assert!(scene.contains("Wire Nest") || scene.contains("once called"), "{scene}");
        let _ = bob;
    }

    #[test]
    fn abandoned_home_trace_reflects_persisted_returns_for_visitors() {
        let (mut world, ada, _bob, ada_id, bob_id) = pair();
        let place = walkable_near_gate(&mut world, ada_id);
        stand_at(&mut world, ada_id, &place);
        world.apply(ada, "home claim").unwrap();
        world.apply(ada, "home name Dust Shelf").unwrap();
        {
            let home = world.homes.active_for_mut(ada_id).unwrap();
            home.claimed_at = 0;
            home.meaningful_returns = crate::home::ESTABLISHED_RETURNS;
        }
        world.apply(ada, "home abandon confirm").unwrap();
        let abandoned =
            world.homes.homes.iter().find(|h| h.place_key == place).expect("abandoned row");
        let abandoned_at = abandoned.abandoned_at.expect("abandoned_at");
        assert_eq!(abandoned.meaningful_returns, crate::home::ESTABLISHED_RETURNS);

        stand_at(&mut world, bob_id, &place);
        world.now_ms = abandoned_at + 1;
        let scene = world.wild_scene_for_tests(WildernessPos::parse(&place).unwrap(), Some(bob_id));
        assert!(
            scene.contains("path worn") || scene.contains("had been kept"),
            "expected deep abandoned trace, got: {scene}"
        );
        assert!(scene.contains("Dust Shelf") || scene.contains("once called"));
        assert!(!scene.contains("field journal"), "{scene}");
    }

    #[test]
    fn meaningful_return_notifies_owner_on_arrival() {
        let (mut world, ada, _, ada_id, _) = pair();
        let place = walkable_near_gate(&mut world, ada_id);
        stand_at(&mut world, ada_id, &place);
        world.apply(ada, "home claim").unwrap();
        world.apply(ada, "west").unwrap();
        world.now_ms += MIN_AWAY_MS + 1;
        world.apply(ada, "east").unwrap();
        let text = feed(&world, ada);
        assert!(
            text.contains("return") && text.contains("home"),
            "expected return feedback in feed, got: {text}"
        );
        world.apply(ada, "home").unwrap();
        let page = overlay(&world, ada);
        assert!(
            page.contains("Counted returns from expeditions: 1"),
            "expected visible return count, got: {page}"
        );
    }

    #[test]
    fn habitation_advances_by_time_returns_and_exploration() {
        let (mut world, ada, _, ada_id, _) = pair();
        let place = walkable_near_gate(&mut world, ada_id);
        let elsewhere = second_walkable(&mut world, ada_id);
        stand_at(&mut world, ada_id, &place);
        world.apply(ada, "home claim").unwrap();
        let home = world.homes.active_for(ada_id).unwrap();
        assert_eq!(
            home.habitation(world.now_ms, ExplorationTraits::default()),
            Habitation::Unsettled
        );

        // Time alone settles the home after the configured threshold.
        world.now_ms = home.claimed_at + SETTLING_TIME_MS;
        assert_eq!(
            world
                .homes
                .active_for(ada_id)
                .unwrap()
                .habitation(world.now_ms, ExplorationTraits::default()),
            Habitation::SettlingIn
        );
        world.now_ms = home.claimed_at + ESTABLISHED_TIME_MS;
        assert_eq!(
            world
                .homes
                .active_for(ada_id)
                .unwrap()
                .habitation(world.now_ms, ExplorationTraits::default()),
            Habitation::Established
        );

        // Meaningful return: leave long enough, then arrive via move_through.
        world.now_ms = 1_000;
        stand_at(&mut world, ada_id, &place);
        // Reset habitation clock for return counting on a fresh claim age.
        world.homes.active_for_mut(ada_id).unwrap().claimed_at = world.now_ms;
        world.homes.active_for_mut(ada_id).unwrap().meaningful_returns = 0;
        let note = world.note_home_travel(ada_id, &place, &elsewhere);
        assert!(!note.ops.is_empty());
        stand_at(&mut world, ada_id, &elsewhere);
        // Instant return does not count.
        world.now_ms += 1;
        let note = world.note_home_travel(ada_id, &elsewhere, &place);
        assert!(
            note.ops.is_empty() || world.homes.active_for(ada_id).unwrap().meaningful_returns == 0
        );
        world.homes.active_for_mut(ada_id).unwrap().last_departed_at = Some(world.now_ms);
        world.now_ms += MIN_AWAY_MS;
        let note = world.note_home_travel(ada_id, &elsewhere, &place);
        assert!(!note.ops.is_empty());
        assert!(note.owner_message.is_some());
        assert_eq!(world.homes.active_for(ada_id).unwrap().meaningful_returns, 1);
        assert_eq!(
            world
                .homes
                .active_for(ada_id)
                .unwrap()
                .habitation(world.now_ms, ExplorationTraits::default()),
            Habitation::SettlingIn
        );

        // Stationary / same-place travel does not farm returns.
        let before = world.homes.active_for(ada_id).unwrap().meaningful_returns;
        assert!(world.note_home_travel(ada_id, &place, &place).ops.is_empty());
        assert_eq!(world.homes.active_for(ada_id).unwrap().meaningful_returns, before);

        // Exploration traits from real journal state unlock Distinctive when established.
        world.apply(ada, "go up").ok();
        // Plant a field note without inventing a second discovery system.
        stand_at(&mut world, ada_id, &place);
        world.homes.active_for_mut(ada_id).unwrap().claimed_at = 0;
        world.now_ms = ESTABLISHED_TIME_MS;
        world.geography.discoveries.push(crate::geography::Discovery {
            id: Uuid::new_v4(),
            region_id: None,
            local_x: 0,
            local_y: 0,
            actor_id: Some(ada_id),
            kind: crate::geography::FIELD_KIND.into(),
            note: "kept".into(),
            created_at: 1,
            content_key: Some("survey-mark".into()),
            title: Some("Mark".into()),
            place_key: Some(place.clone()),
        });
        let traits = world.exploration_traits_for(ada_id);
        assert!(traits.journal_notes);
        assert_eq!(
            world.homes.active_for(ada_id).unwrap().habitation(world.now_ms, traits),
            Habitation::Distinctive
        );
        world.apply(ada, "home").unwrap();
        let page = overlay(&world, ada);
        assert!(page.contains("Distinctive") || page.contains("distinctive"), "{page}");
        assert!(page.contains("field journal") || page.contains("Notes"), "{page}");
        // Public look stays free of private content keys.
        let scene = world.wild_scene_for_tests(WildernessPos::parse(&place).unwrap(), None);
        assert!(!scene.contains("survey-mark"), "{scene}");
        assert!(!scene.contains("delisted-floor"), "{scene}");
    }

    #[test]
    fn abandoned_home_cannot_be_modified_and_private_state_does_not_transfer() {
        let (mut world, ada, bob, ada_id, bob_id) = pair();
        world.apply(ada, "go up").unwrap();
        world.apply(ada, "inspect antenna").unwrap();
        world.apply(ada, "journal record").unwrap();
        world.apply(ada, "waypoint add roof-scratch").unwrap();
        let place = walkable_near_gate(&mut world, ada_id);
        stand_at(&mut world, ada_id, &place);
        world.apply(ada, "home claim").unwrap();
        world.apply(ada, "home name Quiet Alcove").unwrap();
        world.apply(ada, "home abandon confirm").unwrap();

        assert!(world.apply(ada, "home name Still").unwrap_err().to_string().contains("no home"));
        stand_at(&mut world, bob_id, &place);
        world.apply(bob, "home claim").unwrap();
        assert_eq!(world.homes.active_for(bob_id).unwrap().public_name, "");
        assert!(world.geography.field_notes_for(bob_id).is_empty());
        assert!(world.apply(bob, "waypoint show roof-scratch").is_err());
        assert!(world.apply(bob, "journal read survey-scratch").is_err());
        // Ada's private rows remain hers.
        assert_eq!(world.geography.field_notes_for(ada_id).len(), 1);
        assert_eq!(world.waypoints.iter().filter(|w| w.operator_id == ada_id).count(), 1);
    }
}
