//! A two-room game. It uses the engine traits and nothing from Coffee.shop.

use std::collections::HashMap;

use terminus_core::{
    parse_command, ArgMode, Audience, Channel, CommandOutcome, CommandRegistry, CommandSpec,
    ConnectOutcome, ConnectionState, ExitView, FeedLine, Game, GameError, GameEvent, GameView,
    LogEntry, ParsedCommand, Permissions, PresenceView, SessionId, SessionIdentity,
};

struct Room {
    name: &'static str,
    description: &'static str,
    exits: &'static [(&'static str, &'static str, &'static str)],
}

const LOBBY: &str = "lobby";
const GARDEN: &str = "garden";

const ROOMS: &[(&str, Room)] = &[
    (
        LOBBY,
        Room {
            name: "Lobby",
            description: "A quiet lobby with a door to the east.",
            exits: &[("east", GARDEN, "Garden")],
        },
    ),
    (
        GARDEN,
        Room {
            name: "Garden",
            description: "A small garden. The lobby is west.",
            exits: &[("west", LOBBY, "Lobby")],
        },
    ),
];

struct Occupant {
    identity: SessionIdentity,
    room: String,
}

/// In-memory example world.
pub struct MiniGame {
    registry: CommandRegistry,
    occupants: HashMap<SessionId, Occupant>,
    log: Vec<LogEntry>,
    overlays: HashMap<SessionId, String>,
    clock: u64,
}

impl MiniGame {
    pub fn new() -> Self {
        Self {
            registry: CommandRegistry::new(vec![
                CommandSpec {
                    name: "help",
                    aliases: &["?"],
                    summary: "List commands",
                    usage: "help [command]",
                    args: ArgMode::OptionalWord,
                },
                CommandSpec {
                    name: "look",
                    aliases: &["l"],
                    summary: "Inspect the room",
                    usage: "look",
                    args: ArgMode::None,
                },
                CommandSpec {
                    name: "go",
                    aliases: &[],
                    summary: "Move through an exit",
                    usage: "go <exit>",
                    args: ArgMode::RequiredWord,
                },
                CommandSpec {
                    name: "say",
                    aliases: &["'"],
                    summary: "Speak in the room",
                    usage: "say <text>",
                    args: ArgMode::Rest { required: true },
                },
                CommandSpec {
                    name: "who",
                    aliases: &[],
                    summary: "List people in the room",
                    usage: "who",
                    args: ArgMode::None,
                },
                CommandSpec {
                    name: "status",
                    aliases: &[],
                    summary: "Show your name and room",
                    usage: "status",
                    args: ArgMode::None,
                },
                CommandSpec {
                    name: "quit",
                    aliases: &["q"],
                    summary: "Leave",
                    usage: "quit",
                    args: ArgMode::None,
                },
            ]),
            occupants: HashMap::new(),
            log: Vec::new(),
            overlays: HashMap::new(),
            clock: 1,
        }
    }

    fn room(id: &str) -> &'static Room {
        ROOMS.iter().find(|(key, _)| *key == id).map(|(_, room)| room).expect("known room")
    }

    fn emit(&mut self, room: &str, speaker: Option<String>, text: String) -> GameEvent {
        self.clock += 1;
        let entry = LogEntry {
            at_ms: self.clock,
            channel: if speaker.is_some() { Channel::Local } else { Channel::System },
            audience: Audience::Location(room.to_string()),
            speaker,
            text,
        };
        self.log.push(entry.clone());
        GameEvent { entry }
    }
}

impl Default for MiniGame {
    fn default() -> Self {
        Self::new()
    }
}

impl Game for MiniGame {
    fn world_name(&self) -> &str {
        "Minimal"
    }

    fn registry(&self) -> &CommandRegistry {
        &self.registry
    }

    fn connect(&mut self, identity: SessionIdentity) -> Result<ConnectOutcome, GameError> {
        let name = identity.name.clone();
        let session_id = identity.session_id;
        self.occupants.insert(session_id, Occupant { identity, room: LOBBY.into() });
        let event = self.emit(LOBBY, None, format!("{name} arrives."));
        Ok(ConnectOutcome { events: vec![event], replaced: None })
    }

    fn disconnect(&mut self, session_id: SessionId) -> Result<Vec<GameEvent>, GameError> {
        let Some(occupant) = self.occupants.remove(&session_id) else {
            return Ok(Vec::new());
        };
        self.overlays.remove(&session_id);
        let event = self.emit(&occupant.room, None, format!("{} leaves.", occupant.identity.name));
        Ok(vec![event])
    }

    fn command(&mut self, session_id: SessionId, line: &str) -> Result<CommandOutcome, GameError> {
        if !self.occupants.contains_key(&session_id) {
            return Err(GameError::UnknownSession);
        }
        let parsed = match parse_command(&self.registry, line) {
            Ok(parsed) => parsed,
            Err(err) => return Err(GameError::Rejected(err.to_string())),
        };
        match parsed.name.as_str() {
            "help" => {
                let text = self.registry.help_all();
                self.overlays.insert(session_id, text);
                Ok(CommandOutcome::default())
            }
            "look" => {
                self.overlays.remove(&session_id);
                Ok(CommandOutcome::default())
            }
            "go" => self.go(session_id, &parsed),
            "say" => {
                let name = self
                    .occupants
                    .get(&session_id)
                    .map(|o| o.identity.name.clone())
                    .unwrap_or_default();
                let room = self
                    .occupants
                    .get(&session_id)
                    .map(|o| o.room.clone())
                    .unwrap_or_else(|| LOBBY.into());
                let event = self.emit(&room, Some(name), parsed.rest);
                Ok(CommandOutcome { events: vec![event], quit: false })
            }
            "who" | "status" => {
                let view = self.view(session_id)?;
                let text = if parsed.name == "who" {
                    if view.present.is_empty() {
                        "You are alone.".into()
                    } else {
                        view.present.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join(", ")
                    }
                } else {
                    view.status.join("\n")
                };
                self.overlays.insert(session_id, text);
                Ok(CommandOutcome::default())
            }
            "quit" => Ok(CommandOutcome { events: Vec::new(), quit: true }),
            other => Err(GameError::Rejected(format!("unknown command '{other}'"))),
        }
    }

    fn view(&self, session_id: SessionId) -> Result<GameView, GameError> {
        let occupant = self.occupants.get(&session_id).ok_or(GameError::UnknownSession)?;
        let room = Self::room(&occupant.room);
        let present = self
            .occupants
            .values()
            .filter(|other| other.room == occupant.room && other.identity.session_id != session_id)
            .map(|other| PresenceView { name: other.identity.name.clone() })
            .collect();
        let feed = self
            .log
            .iter()
            .filter(|entry| {
                entry.visible_to(&terminus_core::ViewerContext {
                    player_id: occupant.identity.player_id,
                    location_id: &occupant.room,
                    region_key: &occupant.room,
                })
            })
            .map(LogEntry::to_feed)
            .collect::<Vec<FeedLine>>();
        Ok(GameView {
            world_name: "Minimal".into(),
            connection: ConnectionState::Connected,
            operator_name: occupant.identity.name.clone(),
            location_name: room.name.into(),
            description: room.description.into(),
            exits: room
                .exits
                .iter()
                .map(|(keyword, _, label)| ExitView {
                    keyword: (*keyword).to_string(),
                    label: (*label).to_string(),
                    destination: (*label).to_string(),
                })
                .collect(),
            present,
            objects: Vec::new(),
            status: vec![
                format!("Name: {}", occupant.identity.name),
                format!("Room: {}", room.name),
            ],
            inventory: Vec::new(),
            feed,
            overlay: self.overlays.get(&session_id).cloned(),
            commands: self.registry.hints(),
        })
    }

    fn location_key(&self, session_id: SessionId) -> Option<String> {
        self.occupants.get(&session_id).map(|o| o.room.clone())
    }
}

impl MiniGame {
    fn go(
        &mut self,
        session_id: SessionId,
        parsed: &ParsedCommand,
    ) -> Result<CommandOutcome, GameError> {
        let keyword = parsed.args.first().map(|s| s.as_str()).unwrap_or("");
        let room_id = self
            .occupants
            .get(&session_id)
            .map(|o| o.room.clone())
            .ok_or(GameError::UnknownSession)?;
        let room = Self::room(&room_id);
        let Some((_, dest, label)) = room.exits.iter().find(|(word, _, _)| *word == keyword) else {
            return Err(GameError::Rejected(format!(
                "no exit '{keyword}'. Try: {}",
                room.exits.iter().map(|(word, _, _)| *word).collect::<Vec<_>>().join(", ")
            )));
        };
        let name =
            self.occupants.get(&session_id).map(|o| o.identity.name.clone()).unwrap_or_default();
        if let Some(occupant) = self.occupants.get_mut(&session_id) {
            occupant.room = (*dest).to_string();
        }
        self.overlays.remove(&session_id);
        let leave = self.emit(&room_id, None, format!("{name} goes {keyword}."));
        let arrive = self.emit(dest, None, format!("{name} arrives in {label}."));
        Ok(CommandOutcome { events: vec![leave, arrive], quit: false })
    }
}

/// Run a fixed script and return the room names visited.
pub fn scripted_demo() -> Vec<String> {
    let mut game = MiniGame::new();
    let identity =
        SessionIdentity::new(terminus_core::PlayerId::new(), "Guest", Permissions::player());
    let session = identity.session_id;
    game.connect(identity).expect("connect");
    let mut rooms = vec![game.view(session).expect("view").location_name];
    for line in ["look", "go east", "say hello", "status"] {
        game.command(session, line).expect(line);
        rooms.push(game.view(session).expect("view").location_name);
    }
    let outcome = game.command(session, "quit").expect("quit");
    assert!(outcome.quit);
    rooms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_reaches_the_garden() {
        let rooms = scripted_demo();
        assert_eq!(rooms[0], "Lobby");
        assert!(rooms.iter().any(|room| room == "Garden"));
    }

    #[test]
    fn unknown_exit_is_rejected() {
        let mut game = MiniGame::new();
        let identity =
            SessionIdentity::new(terminus_core::PlayerId::new(), "Guest", Permissions::player());
        let session = identity.session_id;
        game.connect(identity).expect("connect");
        let err = game.command(session, "go north").expect_err("bad exit");
        assert!(err.to_string().contains("no exit"));
    }
}
