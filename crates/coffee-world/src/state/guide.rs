//! First arrival and the short help topics.
//!
//! `seen-welcome` and `hint-inspect` remember that a note was shown. They do not
//! open a door, grant an item, or record a discovery.

use terminus_core::{
    Audience, Channel, GameError, GameEvent, ParseError, ParsedCommand, SessionId,
};

use crate::model::{PersistOp, STARTER_LOCATION};

use super::{quiet, Transition, WorldState};

pub(crate) const SEEN_WELCOME: &str = "seen-welcome";
pub(crate) const HINT_INSPECT: &str = "hint-inspect";

const ARRIVAL_LINE: &str =
    "You arrived in the Undernet. Type help, or welcome for the longer note.";
const INSPECT_HINT: &str = "Noticed keeps that. `help inspect` if you want the longer note.";

impl WorldState {
    /// Remember that this operator has been offered the arrival note.
    ///
    /// A newcomer still standing in the Coffee Shop, with only the loyalty chip,
    /// sees the note once. Anyone who has already left, kept a note, or carried
    /// something else gets the flag quietly, so a later visit does not lecture them.
    pub(crate) fn mark_arrival(
        &mut self,
        session_id: SessionId,
        player_id: terminus_core::PlayerId,
    ) -> (Vec<GameEvent>, Vec<PersistOp>) {
        let already =
            self.operators.get(&player_id).is_some_and(|op| op.flags.contains(SEEN_WELCOME));
        if already {
            return (Vec::new(), Vec::new());
        }
        let newcomer = self.is_newcomer(player_id);
        if let Some(op) = self.operators.get_mut(&player_id) {
            op.flags.insert(SEEN_WELCOME.to_string());
        }
        let mut events = Vec::new();
        let mut ops =
            vec![PersistOp::Flag { operator_id: player_id, flag: SEEN_WELCOME.to_string() }];
        if newcomer {
            self.set_overlay(session_id, welcome_page());
            let (event, op) =
                self.emit(Channel::System, Audience::Player(player_id), None, ARRIVAL_LINE);
            events.push(event);
            ops.push(op);
        }
        (events, ops)
    }

    /// One private line the first time an inspection succeeds.
    pub(crate) fn attach_inspect_hint(
        &mut self,
        session_id: SessionId,
        transition: &mut Transition,
    ) {
        let Ok(player) = self.player(session_id) else {
            return;
        };
        let fresh = self
            .operators
            .get_mut(&player)
            .is_some_and(|op| op.flags.insert(HINT_INSPECT.to_string()));
        if !fresh {
            return;
        }
        let (event, message) =
            self.emit(Channel::System, Audience::Player(player), None, INSPECT_HINT);
        transition
            .ops
            .push(PersistOp::Flag { operator_id: player, flag: HINT_INSPECT.to_string() });
        transition.ops.push(message);
        transition.outcome.events.push(event);
    }

    pub(crate) fn cmd_welcome(
        &mut self,
        session_id: SessionId,
        _cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        self.set_overlay(session_id, welcome_page());
        Ok(quiet())
    }

    pub(crate) fn cmd_help(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let text = if let Some(topic) = cmd.args.first() {
            if topic.eq_ignore_ascii_case("commands") {
                self.registry.help_all()
            } else if let Some(page) = category_page(topic) {
                page
            } else {
                self.explain_command(topic)?
            }
        } else {
            help_overview().to_string()
        };
        self.set_overlay(session_id, text);
        Ok(quiet())
    }

    fn explain_command(&self, topic: &str) -> Result<String, GameError> {
        let text = self.registry.help_topic(topic).map_err(|err| {
            GameError::Rejected(match err {
                ParseError::Unknown(name) => format!("No help for '{name}'."),
                other => other.to_string(),
            })
        })?;
        let name = text.lines().next().unwrap_or(topic);
        Ok(format!("{text}\n\n{}", command_why(name)))
    }

    fn is_newcomer(&self, player: terminus_core::PlayerId) -> bool {
        let Some(op) = self.operators.get(&player) else {
            return false;
        };
        op.location_id == STARTER_LOCATION
            && op.flags.is_empty()
            && op.items.iter().all(|item| item.keyword == "chip")
            && self.geography.field_notes_for(player).is_empty()
            && !self.waypoints.iter().any(|mark| mark.operator_id == player)
    }
}

fn welcome_page() -> &'static str {
    // Short lines, important ones first. The notes pane on an 80x24 terminal
    // shows about seven rows and clips the rest.
    "You have arrived in the Undernet.\n\
     The Coffee Shop is your start. It stays.\n\
     You can explore alone. Others share it.\n\
     look — this room. go alley — a door nearby.\n\
     inspect counter. help — what you can do.\n\
     say is this room. chat reaches the Undernet.\n\
     journal and waypoint are private.\n\
     welcome shows this again. look clears this page."
}

fn help_overview() -> &'static str {
    "Help\n\
     Nothing here is a required task. look returns to the place.\n\
     \n\
     help around — look, move, inspect, map\n\
     help people — say, chat, whisper, mail\n\
     help explore — journal, waypoint, theory\n\
     help commands — the full list\n\
     help stuck — when you are lost\n\
     help <name> — one command"
}

fn category_page(topic: &str) -> Option<String> {
    let page = match topic.to_ascii_lowercase().as_str() {
        "around" => around_page(),
        "people" => people_page(),
        "explore" => explore_page(),
        "stuck" => stuck_page(),
        _ => return None,
    };
    Some(page.to_string())
}

fn around_page() -> &'static str {
    "Getting around\n\
     look (l) — the room, the exits, and what is here.\n\
     go <exit> — walk. A bare exit word works: alley.\n\
     map — the city, or the chart once you are outside.\n\
     inspect <thing> — read an object, a person, or an item you carry.\n\
     status — your name and place. who here — who shares the room.\n\
     \n\
     You cannot walk a word that is not an exit."
}

fn people_page() -> &'static str {
    "People\n\
     You do not have to talk to anyone to explore.\n\
     Online means connected right now. An empty room is still a room.\n\
     \n\
     say <text> — this room, if anyone is here.\n\
     chat <text> — operators who are online, in any room.\n\
     whisper <player> <text> — one online operator. It is not saved.\n\
     mail send <player> <text> — a letter that waits if they are offline.\n\
     who here / who all — this room, or everyone online.\n\
     friends, profile, ignore — optional. help <name> for each."
}

fn explore_page() -> &'static str {
    "Exploration\n\
     These are optional. The world does not score them.\n\
     \n\
     journal — a private notebook. Only you can read it.\n\
     journal record — only when a place offers a note.\n\
     waypoint add <name> — a private mark where you stand. It does not move you.\n\
     home claim — designate this Squalor place as your only home. No building required.\n\
     inspect <thing> — read what is in front of you.\n\
     theory list — shared notes about the delisted floor.\n\
     A theory does not unlock anything, and it is not the journal.\n\
     \n\
     welcome — the arrival note again."
}

fn stuck_page() -> &'static str {
    "If you are lost\n\
     look — back to the place.\n\
     map — where the doors go.\n\
     help around, help people, help explore.\n\
     \n\
     The Coffee Shop is down from the Roof Array, or shop from Neon Alley.\n\
     quit closes the session. The same account returns you to the place you left.\n\
     There is no required finish."
}

fn command_why(name: &str) -> &'static str {
    match name {
        "help" => "Use it when you are not sure what is possible from here.",
        "welcome" => "Use it to read the arrival note again. look clears the page.",
        "look" => "Use it to see the room, and to clear a notes page.",
        "map" => "Use it when you want the doors, not a route you must follow.",
        "who" => "Use it to see who is actually here, or who is online.",
        "go" => "Use it to take an exit you can see. A word that is not an exit does nothing.",
        "say" => "Use it to speak in this room. Nobody has to answer.",
        "chat" => "Use it to reach operators who are online, in every room.",
        "whisper" => "Use it for one online operator. The line is not stored.",
        "emote" => "Use it to describe an action people in this room can see.",
        "profile" => "Use it to read a public profile, or to set a short bio or status.",
        "friend" => "Use it to request, accept, decline, or end a friendship.",
        "friends" => "Use it to see friends and requests. It does not show where they stand.",
        "ignore" => "Use it to stop seeing someone's chat. It does not hide the world.",
        "unignore" => "Use it to hear someone you had ignored.",
        "ignores" => "Use it to list the people you are ignoring.",
        "status" => "Use it to confirm your name, your place, and how many are online.",
        "inventory" => "Use it to see what you are carrying.",
        "inspect" => "Use it to read something nearby. Noticed keeps the last reading.",
        "journal" => {
            "Use it for a private note. Only you can read it, and nothing is filed for you."
        }
        "theory" => {
            "Use it to share a thought about the delisted floor. It does not unlock a door."
        }
        "waypoint" => "Use it to name the place you are standing. It does not move you.",
        "mail" => "Use it to leave a letter for someone who may be offline.",
        "mark" => "Use it to name an outskirts tile you are standing on.",
        "build" => "Use it to raise a cabin or workshop on walkable outskirts ground.",
        "enter" => "Use it to step into a cabin or workshop on this tile.",
        "leave" => "Use it to step back out of an interior.",
        "home" => "Use it to claim one place as your home, or to manage a cabin you own.",
        "workshop" => "Use it to read a workshop or its project board.",
        "sign" => "Use it to leave, read, or remove a short roadside note.",
        "trail" => "Use it to leave a trail post on this tile.",
        "board" => "Use it for the Coffee Shop noticeboard, when you are in the shop.",
        "region" => "Use it to speak across this physical region, not the whole Undernet.",
        "settle" => "Use it to found, join, or manage a settlement.",
        "settlements" => "Use it to list settlements you can see from here.",
        "quit" => "Use it to leave. The same account returns you to this place.",
        _ => "Type the usage line. look returns to the place.",
    }
}

#[cfg(test)]
mod tests {
    use terminus_core::{Audience, Game, Permissions, PlayerId, SessionIdentity};
    use uuid::Uuid;

    use crate::geography::{Discovery, FIELD_KIND};
    use crate::model::{Item, PersistOp};
    use crate::state::guide::{HINT_INSPECT, SEEN_WELCOME};
    use crate::undernet::READ_TERMINAL_FLAG;
    use crate::waypoint::Waypoint;
    use crate::{starter_operator, WorldState};

    fn connect_fresh() -> (WorldState, terminus_core::SessionId, PlayerId) {
        let mut world = WorldState::for_tests();
        let id = PlayerId::new();
        world.insert_operator(starter_operator(id, Uuid::new_v4(), "ada", "Ada"));
        let identity = SessionIdentity::new(id, "Ada", Permissions::player());
        let session = identity.session_id;
        world.connect_player(identity).unwrap();
        (world, session, id)
    }

    fn overlay(world: &WorldState, session: terminus_core::SessionId) -> String {
        world.view(session).unwrap().overlay.unwrap_or_default()
    }

    fn feed(world: &WorldState, session: terminus_core::SessionId) -> String {
        world
            .view(session)
            .unwrap()
            .feed
            .iter()
            .map(|line| line.text.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn a_new_operator_gets_the_arrival_note_once() {
        let mut world = WorldState::for_tests();
        let id = PlayerId::new();
        world.insert_operator(starter_operator(id, Uuid::new_v4(), "ada", "Ada"));
        let identity = SessionIdentity::new(id, "Ada", Permissions::player());
        let session = identity.session_id;
        let (arrived, _) = world.connect_player(identity).unwrap();
        assert!(arrived.ops.iter().any(|op| matches!(
            op,
            PersistOp::Flag { flag, .. } if flag == SEEN_WELCOME
        )));
        let page = overlay(&world, session);
        assert!(page.contains("arrived in the Undernet"), "{page}");
        assert!(page.contains("Coffee Shop"), "{page}");
        assert!(page.contains("go alley"), "{page}");
        assert!(page.contains("help"), "{page}");
        assert!(page.contains("explore alone"), "{page}");
        assert!(feed(&world, session).contains("You arrived"), "{}", feed(&world, session));
        let ada = world.operator(id).unwrap();
        assert!(ada.flags.contains(SEEN_WELCOME));
        assert!(!ada.flags.contains("cipher-plate"));
        assert!(!ada.flags.contains(READ_TERMINAL_FLAG));
        assert!(world.geography.field_notes_for(id).is_empty());
        assert!(world.theories.entries.is_empty());

        world.detach(session).unwrap();
        let again = SessionIdentity::new(id, "Ada", Permissions::player());
        let next = again.session_id;
        world.connect_player(again).unwrap();
        assert!(world.view(next).unwrap().overlay.is_none());
        assert_eq!(feed(&world, next).matches("You arrived").count(), 1);
        assert_eq!(world.location_key(next).unwrap(), "coffee-shop");
    }

    #[test]
    fn an_operator_who_already_left_is_not_lectured() {
        let mut world = WorldState::for_tests();
        let id = PlayerId::new();
        let mut op = starter_operator(id, Uuid::new_v4(), "ada", "Ada");
        op.location_id = "roof-array".into();
        world.insert_operator(op);
        let identity = SessionIdentity::new(id, "Ada", Permissions::player());
        let session = identity.session_id;
        world.connect_player(identity).unwrap();
        assert!(world.view(session).unwrap().overlay.is_none());
        assert!(!feed(&world, session).contains("You arrived"));
        assert!(world.operator(id).unwrap().flags.contains(SEEN_WELCOME));

        world.detach(session).unwrap();
        world.operators.get_mut(&id).unwrap().location_id = "coffee-shop".into();
        let back = SessionIdentity::new(id, "Ada", Permissions::player());
        let home = back.session_id;
        world.connect_player(back).unwrap();
        assert!(world.view(home).unwrap().overlay.is_none());
        assert!(!feed(&world, home).contains("You arrived"));
    }

    #[test]
    fn prior_progress_skips_the_welcome() {
        let cases = [
            ("flag", true, false, false, false),
            ("item", false, true, false, false),
            ("journal", false, false, true, false),
            ("waypoint", false, false, false, true),
        ];
        for (label, flag, item, journal, waypoint) in cases {
            let mut world = WorldState::for_tests();
            let id = PlayerId::new();
            let mut op = starter_operator(id, Uuid::new_v4(), "ada", "Ada");
            if flag {
                op.flags.insert("cipher-plate".into());
            }
            if item {
                op.items.push(Item {
                    keyword: "plate".into(),
                    name: "cipher plate".into(),
                    detail: "already carried".into(),
                });
            }
            world.insert_operator(op);
            if journal {
                world.geography.discoveries.push(Discovery {
                    id: Uuid::new_v4(),
                    region_id: None,
                    local_x: 0,
                    local_y: 0,
                    kind: FIELD_KIND.into(),
                    note: "private-zinc-note".into(),
                    actor_id: Some(id),
                    created_at: 1,
                    content_key: Some("kept".into()),
                    title: Some("Kept".into()),
                    place_key: Some("coffee-shop".into()),
                });
            }
            if waypoint {
                world.waypoints.push(Waypoint {
                    id: Uuid::new_v4(),
                    operator_id: id,
                    name_key: "shop".into(),
                    display_name: "shop".into(),
                    place_key: "coffee-shop".into(),
                    note: "private-waypoint-note".into(),
                    created_at: 1,
                    updated_at: 1,
                });
            }
            let identity = SessionIdentity::new(id, "Ada", Permissions::player());
            let session = identity.session_id;
            world.connect_player(identity).unwrap();
            assert!(world.view(session).unwrap().overlay.is_none(), "{label} should not welcome");
            assert!(!feed(&world, session).contains("You arrived"), "{label}");
            assert!(world.operator(id).unwrap().flags.contains(SEEN_WELCOME), "{label}");
        }
    }

    #[test]
    fn look_clears_the_welcome_and_welcome_brings_it_back() {
        let (mut world, session, _) = connect_fresh();
        assert_eq!(feed(&world, session).matches("You arrived").count(), 1);
        world.apply(session, "look").unwrap();
        assert!(world.view(session).unwrap().overlay.is_none());
        world.apply(session, "welcome").unwrap();
        let page = overlay(&world, session);
        assert!(page.contains("arrived in the Undernet"), "{page}");
        assert!(page.contains("go alley"), "{page}");
        assert_eq!(feed(&world, session).matches("You arrived").count(), 1);
        world.apply(session, "welcome").unwrap();
        assert_eq!(feed(&world, session).matches("You arrived").count(), 1);
    }

    #[test]
    fn help_topics_explain_real_commands_and_unknown_topics_fail() {
        let (mut world, session, _) = connect_fresh();
        world.apply(session, "help").unwrap();
        let overview = overlay(&world, session);
        assert!(overview.contains("help around"), "{overview}");
        assert!(overview.contains("help people"), "{overview}");
        assert!(overview.contains("help explore"), "{overview}");
        assert!(overview.contains("help commands"), "{overview}");
        assert!(overview.contains("required task"), "{overview}");

        world.apply(session, "help explore").unwrap();
        let explore = overlay(&world, session);
        assert!(explore.contains("journal"), "{explore}");
        assert!(explore.contains("waypoint"), "{explore}");
        assert!(explore.contains("does not unlock"), "{explore}");
        assert!(explore.contains("not the journal"), "{explore}");
        assert!(!explore.contains("B-4"), "{explore}");
        assert!(!explore.contains("cipher"), "{explore}");

        world.apply(session, "help chat").unwrap();
        let chat = overlay(&world, session);
        assert!(chat.starts_with("chat\n"), "{chat}");
        assert!(chat.contains("usage: chat <message>"), "{chat}");
        assert!(chat.contains("online"), "{chat}");

        world.apply(session, "help move").unwrap();
        assert!(overlay(&world, session).starts_with("go\n"));

        world.apply(session, "help journal").unwrap();
        let journal = overlay(&world, session);
        assert!(journal.contains("journal"));
        assert!(journal.contains("record"));

        world.apply(session, "help theory").unwrap();
        assert!(overlay(&world, session).contains("observation"));

        world.apply(session, "help mail").unwrap();
        assert!(overlay(&world, session).contains("mail [list|read|send|delete]"));

        world.apply(session, "help commands").unwrap();
        let commands = overlay(&world, session);
        assert!(commands.contains("look"), "{commands}");
        assert!(commands.contains("welcome"), "{commands}");

        let missing = world.apply(session, "help nosuch").unwrap_err();
        assert!(missing.to_string().contains("No help for 'nosuch'."));
    }

    #[test]
    fn the_inspect_hint_is_private_and_does_not_repeat() {
        let mut world = WorldState::for_tests();
        let ada_id = PlayerId::new();
        let bob_id = PlayerId::new();
        world.insert_operator(starter_operator(ada_id, Uuid::new_v4(), "ada", "Ada"));
        let mut bob = starter_operator(bob_id, Uuid::new_v4(), "bob", "Bob");
        bob.location_id = "roof-array".into();
        world.insert_operator(bob);
        let ada_ident = SessionIdentity::new(ada_id, "Ada", Permissions::player());
        let bob_ident = SessionIdentity::new(bob_id, "Bob", Permissions::player());
        let ada = ada_ident.session_id;
        let bob_session = bob_ident.session_id;
        world.connect_player(ada_ident).unwrap();
        world.connect_player(bob_ident).unwrap();

        let refused = world.apply(ada, "inspect nosuch").unwrap_err();
        assert!(refused.to_string().contains("nothing called"));
        assert!(!world.operator(ada_id).unwrap().flags.contains(HINT_INSPECT));

        world.apply(ada, "inspect counter").unwrap();
        let ada_feed = feed(&world, ada);
        assert!(ada_feed.contains("Noticed keeps that."), "{ada_feed}");
        assert!(ada_feed.contains("scarred laminate"), "{ada_feed}");
        assert!(world.operator(ada_id).unwrap().flags.contains(HINT_INSPECT));
        world.apply(ada, "inspect counter").unwrap();
        assert_eq!(feed(&world, ada).matches("Noticed keeps that.").count(), 1);

        let bob_feed = feed(&world, bob_session);
        assert!(!bob_feed.contains("Noticed keeps that."), "{bob_feed}");
        assert!(!bob_feed.contains("scarred laminate"), "{bob_feed}");
        assert!(!bob_feed.contains("You arrived"), "{bob_feed}");
        assert!(!overlay(&world, bob_session).contains("arrived in the Undernet"));
        assert!(world.geography.field_notes_for(ada_id).is_empty());
        assert!(world.geography.field_notes_for(bob_id).is_empty());
        assert!(!world.operator(bob_id).unwrap().flags.contains(HINT_INSPECT));
    }

    #[test]
    fn an_empty_room_explains_speech_without_inventing_company() {
        let (mut world, session, id) = connect_fresh();
        world.apply(session, "who").unwrap();
        let page = overlay(&world, session);
        assert!(page.contains("Nobody else is here."), "{page}");
        assert!(page.contains("`say`"), "{page}");
        assert!(page.contains("`chat`"), "{page}");
        assert!(!page.contains("is online"), "{page}");
        assert!(!page.contains("are online"), "{page}");

        world.apply(session, "go archive").unwrap_err();
        let ada = world.operator(id).unwrap();
        assert!(!ada.flags.contains("cipher-plate"));
        assert!(!ada.flags.contains(READ_TERMINAL_FLAG));
        assert!(world.theories.entries.is_empty());
        assert!(world.geography.field_notes_for(id).is_empty());
    }

    #[test]
    fn private_notes_stay_with_their_operator() {
        let mut world = WorldState::for_tests();
        let ada_id = PlayerId::new();
        let bob_id = PlayerId::new();
        world.insert_operator(starter_operator(ada_id, Uuid::new_v4(), "ada", "Ada"));
        world.insert_operator(starter_operator(bob_id, Uuid::new_v4(), "bob", "Bob"));
        let ada_ident = SessionIdentity::new(ada_id, "Ada", Permissions::player());
        let bob_ident = SessionIdentity::new(bob_id, "Bob", Permissions::player());
        let ada = ada_ident.session_id;
        let bob = bob_ident.session_id;
        world.connect_player(ada_ident).unwrap();
        world.connect_player(bob_ident).unwrap();

        world.apply(ada, "waypoint add shop").unwrap();
        world.apply(ada, "waypoint note shop carved-cup-private").unwrap();
        world.apply(bob, "waypoint list").unwrap();
        let bob_page = overlay(&world, bob);
        assert!(bob_page.contains("no waypoints"), "{bob_page}");
        assert!(!bob_page.contains("carved-cup-private"), "{bob_page}");
        assert!(!feed(&world, bob).contains("carved-cup-private"));
        assert!(world.log.iter().any(|entry| {
            entry.text.contains("You arrived")
                && matches!(entry.audience, Audience::Player(player) if player == ada_id)
        }));
        assert!(world.log.iter().all(|entry| {
            !entry.text.contains("You arrived") || matches!(entry.audience, Audience::Player(_))
        }));
    }
}
