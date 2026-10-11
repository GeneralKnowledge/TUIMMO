//! In-memory Undernet. All rules run here, with no terminal and no socket.

use std::collections::{HashMap, VecDeque};

use terminus_core::text::{clean_line, MAX_SPEECH_CHARS};
use terminus_core::{
    Audience, Channel, CommandOutcome, CommandRegistry, ConnectOutcome, ConnectionState, ExitView,
    FeedLine, Game, GameError, GameEvent, GameView, ItemView, LogEntry, ObjectView, ParseError,
    ParsedCommand, Permissions, PlayerId, PresenceView, SessionId, SessionIdentity,
};

use crate::catalog::coffee_registry;
use crate::model::{Item, Operator, PersistOp, FEED_CAP, LOG_CAP, STARTER_LOCATION};
use crate::undernet::{self, ExitDef, ObjectDef};

/// Result of a state transition, including the rows that must be committed.
#[derive(Clone, Debug)]
pub struct Transition {
    pub outcome: CommandOutcome,
    pub ops: Vec<PersistOp>,
}

#[derive(Clone, Debug)]
struct Session {
    identity: SessionIdentity,
}

type Handler = fn(&mut WorldState, SessionId, &ParsedCommand) -> Result<Transition, GameError>;

/// Authoritative Undernet state.
pub struct WorldState {
    world_name: String,
    registry: CommandRegistry,
    operators: HashMap<PlayerId, Operator>,
    sessions: HashMap<SessionId, Session>,
    by_player: HashMap<PlayerId, SessionId>,
    log: VecDeque<LogEntry>,
    overlays: HashMap<SessionId, String>,
    use_system_clock: bool,
    now_ms: u64,
}

impl WorldState {
    pub fn new() -> Self {
        Self {
            world_name: "Undernet".into(),
            registry: coffee_registry(),
            operators: HashMap::new(),
            sessions: HashMap::new(),
            by_player: HashMap::new(),
            log: VecDeque::new(),
            overlays: HashMap::new(),
            use_system_clock: true,
            now_ms: 0,
        }
    }

    /// Deterministic clock for tests. Each transition advances one millisecond.
    pub fn for_tests() -> Self {
        let mut world = Self::new();
        world.use_system_clock = false;
        world
    }

    pub fn install_loaded(&mut self, operators: Vec<Operator>, log: Vec<LogEntry>) {
        self.operators = operators.into_iter().map(|op| (op.id, op)).collect();
        self.log = log.into_iter().collect();
        while self.log.len() > LOG_CAP {
            self.log.pop_front();
        }
    }

    pub fn insert_operator(&mut self, operator: Operator) {
        self.operators.insert(operator.id, operator);
    }

    pub fn operator(&self, id: PlayerId) -> Option<&Operator> {
        self.operators.get(&id)
    }

    /// Attach, returning the session that was replaced, if any.
    pub fn connect_player(
        &mut self,
        identity: SessionIdentity,
    ) -> Result<(Transition, Option<SessionId>), GameError> {
        self.tick();
        if !self.operators.contains_key(&identity.player_id) {
            return Err(GameError::Rejected("no such operator".into()));
        }
        let replaced = self.by_player.remove(&identity.player_id);
        if let Some(old) = replaced {
            self.sessions.remove(&old);
            self.overlays.remove(&old);
        }
        let name = identity.name.clone();
        let player_id = identity.player_id;
        let session_id = identity.session_id;
        let text = if replaced.is_some() {
            format!("{name}'s link is replaced.")
        } else {
            format!("{name} connects.")
        };
        let (event, op) = self.emit(Channel::System, Audience::Everyone, None, text);
        let seen = PersistOp::Seen { operator_id: player_id, last_seen: self.now_ms };
        self.by_player.insert(player_id, session_id);
        self.sessions.insert(session_id, Session { identity });
        Ok((
            Transition {
                outcome: CommandOutcome { events: vec![event], quit: false },
                ops: vec![op, seen],
            },
            replaced,
        ))
    }

    pub fn detach(&mut self, session_id: SessionId) -> Result<Transition, GameError> {
        self.tick();
        let Some(session) = self.sessions.remove(&session_id) else {
            return Ok(Transition { outcome: CommandOutcome::default(), ops: Vec::new() });
        };
        self.overlays.remove(&session_id);
        self.by_player.remove(&session.identity.player_id);
        let name = session.identity.name;
        let (event, op) =
            self.emit(Channel::System, Audience::Everyone, None, format!("{name} disconnects."));
        let seen =
            PersistOp::Seen { operator_id: session.identity.player_id, last_seen: self.now_ms };
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![op, seen],
        })
    }

    pub fn apply(&mut self, session_id: SessionId, line: &str) -> Result<Transition, GameError> {
        self.tick();
        if !self.sessions.contains_key(&session_id) {
            return Err(GameError::UnknownSession);
        }
        let parsed = match self.registry.parse(line) {
            Ok(parsed) => parsed,
            Err(ParseError::Empty) => {
                return Ok(Transition { outcome: CommandOutcome::default(), ops: Vec::new() });
            }
            Err(ParseError::Unknown(word)) if self.matches_exit(session_id, &word) => {
                ParsedCommand {
                    name: "go".into(),
                    raw: line.to_string(),
                    args: vec![word],
                    rest: String::new(),
                }
            }
            Err(ParseError::Unknown(word)) => {
                return Err(GameError::Rejected(format!("Unknown command '{word}'. Try help.")));
            }
            Err(ParseError::TooLong) => {
                return Err(GameError::Rejected("That line is too long.".into()));
            }
            Err(ParseError::BadArgs { message, .. }) => {
                return Err(GameError::Rejected(message));
            }
        };
        let permissions = self.permissions(session_id)?;
        if needs_play(&parsed.name) && !permissions.can_play() {
            return Err(GameError::Rejected("You cannot do that from this session.".into()));
        }
        if needs_speak(&parsed.name) && !permissions.can_speak() {
            return Err(GameError::Rejected("You cannot speak right now.".into()));
        }
        let handler = handler(&parsed.name).ok_or_else(|| {
            GameError::Rejected(format!("Unknown command '{}'. Try help.", parsed.name))
        })?;
        handler(self, session_id, &parsed)
    }

    pub fn replace_log(&mut self, log: Vec<LogEntry>) {
        self.log = log.into_iter().collect();
    }

    pub fn log_snapshot(&self) -> Vec<LogEntry> {
        self.log.iter().cloned().collect()
    }

    fn tick(&mut self) {
        if self.use_system_clock {
            self.now_ms = system_now_ms();
        } else {
            self.now_ms = self.now_ms.saturating_add(1);
        }
    }

    fn permissions(&self, session_id: SessionId) -> Result<Permissions, GameError> {
        self.sessions
            .get(&session_id)
            .map(|session| session.identity.permissions)
            .ok_or(GameError::UnknownSession)
    }

    pub fn player_of(&self, session_id: SessionId) -> Option<PlayerId> {
        self.sessions.get(&session_id).map(|session| session.identity.player_id)
    }

    fn player(&self, session_id: SessionId) -> Result<PlayerId, GameError> {
        self.sessions
            .get(&session_id)
            .map(|session| session.identity.player_id)
            .ok_or(GameError::UnknownSession)
    }

    fn name_of(&self, session_id: SessionId) -> Result<String, GameError> {
        self.sessions
            .get(&session_id)
            .map(|session| session.identity.name.clone())
            .ok_or(GameError::UnknownSession)
    }

    fn location_id(&self, session_id: SessionId) -> Result<String, GameError> {
        let player = self.player(session_id)?;
        self.operators
            .get(&player)
            .map(|op| op.location_id.clone())
            .ok_or(GameError::UnknownSession)
    }

    fn matches_exit(&self, session_id: SessionId, word: &str) -> bool {
        let Ok(location) = self.location_id(session_id) else {
            return false;
        };
        let key = word.to_ascii_lowercase();
        self.exit_list().into_iter().any(|exit| exit.from == location && exit.keyword == key)
    }

    fn exit_list(&self) -> Vec<ExitDef> {
        undernet::all_exits()
    }

    fn emit(
        &mut self,
        channel: Channel,
        audience: Audience,
        speaker: Option<String>,
        text: impl Into<String>,
    ) -> (GameEvent, PersistOp) {
        let text = text.into();
        let (audience_kind, audience_key) = audience_parts(&audience);
        let entry = LogEntry {
            at_ms: self.now_ms,
            channel,
            audience,
            speaker: speaker.clone(),
            text: text.clone(),
        };
        let op = PersistOp::Message {
            at_ms: entry.at_ms,
            channel: channel_name(entry.channel).into(),
            audience: audience_kind.into(),
            audience_key,
            speaker,
            body: text,
        };
        self.log.push_back(entry.clone());
        while self.log.len() > LOG_CAP {
            self.log.pop_front();
        }
        (GameEvent { entry }, op)
    }

    fn set_overlay(&mut self, session_id: SessionId, text: impl Into<String>) {
        self.overlays.insert(session_id, text.into());
    }

    fn clear_overlay(&mut self, session_id: SessionId) {
        self.overlays.remove(&session_id);
    }

    fn cmd_help(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let text = if let Some(topic) = cmd.args.first() {
            self.registry.help_topic(topic).map_err(|err| {
                GameError::Rejected(match err {
                    ParseError::Unknown(name) => format!("No help for '{name}'."),
                    other => other.to_string(),
                })
            })?
        } else {
            self.registry.help_all()
        };
        self.set_overlay(session_id, text);
        Ok(quiet())
    }

    fn cmd_look(
        &mut self,
        session_id: SessionId,
        _cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        self.clear_overlay(session_id);
        Ok(quiet())
    }

    fn cmd_map(
        &mut self,
        session_id: SessionId,
        _cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let location = self.location_id(session_id)?;
        let open = self
            .operators
            .get(&player)
            .map(|op| op.flags.contains("cipher-plate"))
            .unwrap_or(false);
        self.set_overlay(session_id, undernet::map_text(&location, open));
        Ok(quiet())
    }

    fn cmd_who(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let scope =
            cmd.args.first().map(|word| word.to_ascii_lowercase()).unwrap_or_else(|| "here".into());
        let text = match scope.as_str() {
            "here" => {
                let location = self.location_id(session_id)?;
                let mine = self.name_of(session_id)?;
                let mut names = self.names_in(&location);
                names.retain(|name| name != &mine);
                if names.is_empty() {
                    "Nobody else is here.".into()
                } else {
                    format!("Here: {}", names.join(", "))
                }
            }
            "all" => {
                let mut lines = vec!["Online:".to_string()];
                let mut rows = Vec::new();
                for session in self.sessions.values() {
                    let loc = self
                        .operators
                        .get(&session.identity.player_id)
                        .map(|op| op.location_id.clone())
                        .unwrap_or_else(|| "?".into());
                    let loc_name = undernet::location(&loc).map(|l| l.name).unwrap_or(loc.as_str());
                    rows.push(format!("{} — {loc_name}", session.identity.name));
                }
                rows.sort();
                if rows.is_empty() {
                    lines.push("(nobody)".into());
                } else {
                    lines.extend(rows);
                }
                lines.join("\n")
            }
            _ => {
                return Err(GameError::Rejected("usage: who [here|all]".into()));
            }
        };
        self.set_overlay(session_id, text);
        Ok(quiet())
    }

    fn cmd_go(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let keyword = cmd.args.first().map(|word| word.to_ascii_lowercase()).unwrap_or_default();
        let player = self.player(session_id)?;
        let from = self.location_id(session_id)?;
        let from_name = place_name(&from);
        let exit =
            self.exit_list().into_iter().find(|exit| exit.from == from && exit.keyword == keyword);
        let Some(exit) = exit else {
            let visible = self.visible_exits(session_id)?;
            let list = if visible.is_empty() {
                "none".to_string()
            } else {
                visible.iter().map(|exit| exit.keyword.clone()).collect::<Vec<_>>().join(", ")
            };
            return Err(GameError::Rejected(format!("You can't go that way. Exits: {list}.")));
        };
        if let Some(flag) = exit.requires_flag {
            let has =
                self.operators.get(&player).map(|op| op.flags.contains(flag)).unwrap_or(false);
            if !has {
                return Err(GameError::Rejected(
                    "The way to the Black Archive is sealed. The lock wants a cipher plate.".into(),
                ));
            }
        }
        let dest_name = place_name(exit.to);
        let name = self.name_of(session_id)?;
        if let Some(op) = self.operators.get_mut(&player) {
            op.location_id = exit.to.to_string();
        }
        self.clear_overlay(session_id);
        let (leave, leave_op) = self.emit(
            Channel::System,
            Audience::Location(from),
            None,
            format!("{name} heads toward {dest_name}."),
        );
        let (arrive, arrive_op) = self.emit(
            Channel::System,
            Audience::Location(exit.to.to_string()),
            None,
            format!("{name} arrives from {from_name}."),
        );
        let (note, note_op) = self.emit(
            Channel::System,
            Audience::Player(player),
            None,
            format!("You step into {dest_name}."),
        );
        let moved = PersistOp::Move {
            operator_id: player,
            location_id: exit.to.to_string(),
            last_seen: self.now_ms,
        };
        Ok(Transition {
            outcome: CommandOutcome { events: vec![leave, arrive, note], quit: false },
            ops: vec![moved, leave_op, arrive_op, note_op],
        })
    }

    fn cmd_say(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        self.speak(session_id, &cmd.rest, false)
    }

    fn cmd_chat(
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

    fn cmd_status(
        &mut self,
        session_id: SessionId,
        _cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let lines = self.status_lines(session_id)?;
        self.set_overlay(session_id, lines.join("\n"));
        Ok(quiet())
    }

    fn cmd_inventory(
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

    fn cmd_inspect(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let target = cmd.args.first().map(|word| word.to_ascii_lowercase()).unwrap_or_default();
        let player = self.player(session_id)?;
        let location = self.location_id(session_id)?;
        if let Some(object) = find_object(&location, &target) {
            return self.inspect_object(session_id, player, object);
        }
        if let Some(other) =
            self.names_in(&location).into_iter().find(|name| name.eq_ignore_ascii_case(&target))
        {
            let (event, op) = self.emit(
                Channel::System,
                Audience::Player(player),
                None,
                format!("{other} is here, watching the room."),
            );
            return Ok(Transition {
                outcome: CommandOutcome { events: vec![event], quit: false },
                ops: vec![op],
            });
        }
        if let Some(item) = self.operators.get(&player).and_then(|op| {
            op.items
                .iter()
                .find(|item| {
                    item.keyword.eq_ignore_ascii_case(&target)
                        || item.name.eq_ignore_ascii_case(&target)
                })
                .cloned()
        }) {
            let (event, op) = self.emit(
                Channel::System,
                Audience::Player(player),
                None,
                format!("{}: {}", item.name, item.detail),
            );
            return Ok(Transition {
                outcome: CommandOutcome { events: vec![event], quit: false },
                ops: vec![op],
            });
        }
        Err(GameError::Rejected(format!("You see nothing called '{target}'.")))
    }

    fn inspect_object(
        &mut self,
        session_id: SessionId,
        player: PlayerId,
        object: ObjectDef,
    ) -> Result<Transition, GameError> {
        let mut ops = Vec::new();
        let mut extra = String::new();
        if let Some(grant) = object.grant {
            let already = self
                .operators
                .get(&player)
                .map(|op| op.flags.contains(grant.flag))
                .unwrap_or(false);
            if already {
                extra = grant.already.to_string();
            } else if let Some(op) = self.operators.get_mut(&player) {
                op.flags.insert(grant.flag.to_string());
                ops.push(PersistOp::Flag { operator_id: player, flag: grant.flag.to_string() });
                if !op.items.iter().any(|item| item.keyword == grant.item_key) {
                    op.items.push(Item {
                        keyword: grant.item_key.into(),
                        name: grant.item_name.into(),
                        detail: grant.item_detail.into(),
                    });
                    ops.push(PersistOp::Item {
                        operator_id: player,
                        keyword: grant.item_key.into(),
                        name: grant.item_name.into(),
                        detail: grant.item_detail.into(),
                    });
                }
                extra = grant.first.to_string();
            }
        }
        let text = if extra.is_empty() {
            object.description.to_string()
        } else {
            format!("{}\n{extra}", object.description)
        };
        let (event, op) = self.emit(Channel::System, Audience::Player(player), None, text);
        ops.push(op);
        let _ = session_id;
        Ok(Transition { outcome: CommandOutcome { events: vec![event], quit: false }, ops })
    }

    fn cmd_quit(
        &mut self,
        _session_id: SessionId,
        _cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        Ok(Transition {
            outcome: CommandOutcome { events: Vec::new(), quit: true },
            ops: Vec::new(),
        })
    }

    fn names_in(&self, location: &str) -> Vec<String> {
        let mut names = Vec::new();
        for session in self.sessions.values() {
            if let Some(op) = self.operators.get(&session.identity.player_id) {
                if op.location_id == location {
                    names.push(session.identity.name.clone());
                }
            }
        }
        names.sort();
        names
    }

    fn visible_exits(&self, session_id: SessionId) -> Result<Vec<ExitView>, GameError> {
        let player = self.player(session_id)?;
        let location = self.location_id(session_id)?;
        let flags = self.operators.get(&player).map(|op| op.flags.clone()).unwrap_or_default();
        let mut exits = Vec::new();
        for exit in self.exit_list() {
            if exit.from != location {
                continue;
            }
            if let Some(flag) = exit.requires_flag {
                if !flags.contains(flag) {
                    continue;
                }
            }
            exits.push(ExitView {
                keyword: exit.keyword.to_string(),
                label: exit.label.to_string(),
                destination: place_name(exit.to),
            });
        }
        Ok(exits)
    }

    fn status_lines(&self, session_id: SessionId) -> Result<Vec<String>, GameError> {
        let player = self.player(session_id)?;
        let name = self.name_of(session_id)?;
        let op = self.operators.get(&player).ok_or(GameError::UnknownSession)?;
        let cipher = if op.flags.contains("cipher-plate") { "yes" } else { "no" };
        Ok(vec![
            name,
            format!("place: {}", place_name(&op.location_id)),
            format!("online: {}", self.sessions.len()),
            format!("carried: {}", op.items.len()),
            format!("cipher plate: {cipher}"),
        ])
    }

    fn build_view(&self, session_id: SessionId) -> Result<GameView, GameError> {
        let player = self.player(session_id)?;
        let name = self.name_of(session_id)?;
        let op = self.operators.get(&player).ok_or(GameError::UnknownSession)?;
        let loc = undernet::location(&op.location_id)
            .ok_or_else(|| GameError::Rejected("you are standing nowhere known".into()))?;
        let mut present = self
            .names_in(&op.location_id)
            .into_iter()
            .filter(|other| other != &name)
            .map(|name| PresenceView { name })
            .collect::<Vec<_>>();
        present.sort_by(|a, b| a.name.cmp(&b.name));
        let objects = loc
            .objects
            .iter()
            .map(|object| ObjectView {
                keyword: object.keyword.to_string(),
                name: object.name.to_string(),
            })
            .collect();
        let inventory = op
            .items
            .iter()
            .map(|item| ItemView {
                keyword: item.keyword.clone(),
                name: item.name.clone(),
                detail: item.detail.clone(),
            })
            .collect();
        let mut feed = self
            .log
            .iter()
            .filter(|entry| entry.visible_to(&op.location_id, player))
            .map(LogEntry::to_feed)
            .collect::<Vec<_>>();
        if feed.len() > FEED_CAP {
            feed = feed.split_off(feed.len() - FEED_CAP);
        }
        Ok(GameView {
            world_name: self.world_name.clone(),
            connection: ConnectionState::Connected,
            operator_name: name,
            location_name: loc.name.to_string(),
            description: loc.description.to_string(),
            exits: self.visible_exits(session_id)?,
            present,
            objects,
            status: self.status_lines(session_id)?,
            inventory,
            feed,
            overlay: self.overlays.get(&session_id).cloned(),
            commands: self.registry.hints(),
        })
    }
}

impl Default for WorldState {
    fn default() -> Self {
        Self::new()
    }
}

impl Game for WorldState {
    fn world_name(&self) -> &str {
        &self.world_name
    }

    fn registry(&self) -> &CommandRegistry {
        &self.registry
    }

    fn connect(&mut self, identity: SessionIdentity) -> Result<ConnectOutcome, GameError> {
        let (transition, replaced) = self.connect_player(identity)?;
        Ok(ConnectOutcome { events: transition.outcome.events, replaced })
    }

    fn disconnect(&mut self, session_id: SessionId) -> Result<Vec<GameEvent>, GameError> {
        Ok(self.detach(session_id)?.outcome.events)
    }

    fn command(&mut self, session_id: SessionId, line: &str) -> Result<CommandOutcome, GameError> {
        Ok(self.apply(session_id, line)?.outcome)
    }

    fn view(&self, session_id: SessionId) -> Result<GameView, GameError> {
        self.build_view(session_id)
    }

    fn location_key(&self, session_id: SessionId) -> Option<String> {
        self.location_id(session_id).ok()
    }
}

fn handler(name: &str) -> Option<Handler> {
    Some(match name {
        "help" => WorldState::cmd_help,
        "look" => WorldState::cmd_look,
        "map" => WorldState::cmd_map,
        "who" => WorldState::cmd_who,
        "go" => WorldState::cmd_go,
        "say" => WorldState::cmd_say,
        "chat" => WorldState::cmd_chat,
        "status" => WorldState::cmd_status,
        "inventory" => WorldState::cmd_inventory,
        "inspect" => WorldState::cmd_inspect,
        "quit" => WorldState::cmd_quit,
        _ => return None,
    })
}

fn needs_play(name: &str) -> bool {
    matches!(name, "go" | "inspect")
}

fn needs_speak(name: &str) -> bool {
    matches!(name, "say" | "chat")
}

fn quiet() -> Transition {
    Transition { outcome: CommandOutcome::default(), ops: Vec::new() }
}

fn place_name(id: &str) -> String {
    undernet::location(id).map(|loc| loc.name.to_string()).unwrap_or_else(|| id.to_string())
}

fn find_object(location: &str, target: &str) -> Option<ObjectDef> {
    let loc = undernet::location(location)?;
    loc.objects.iter().copied().find(|object| {
        object.keyword.eq_ignore_ascii_case(target)
            || object.aliases.iter().any(|alias| alias.eq_ignore_ascii_case(target))
    })
}

fn audience_parts(audience: &Audience) -> (&'static str, String) {
    match audience {
        Audience::Everyone => ("everyone", String::new()),
        Audience::Location(id) => ("location", id.clone()),
        Audience::Player(id) => ("player", id.to_string()),
    }
}

fn channel_name(channel: Channel) -> &'static str {
    match channel {
        Channel::System => "system",
        Channel::Local => "local",
        Channel::Global => "global",
    }
}

fn system_now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

pub fn starter_operator(
    id: PlayerId,
    account_id: uuid::Uuid,
    username: &str,
    name: &str,
) -> Operator {
    Operator {
        id,
        account_id,
        username: username.into(),
        name: name.into(),
        location_id: STARTER_LOCATION.into(),
        flags: Default::default(),
        items: vec![Item::chip()],
    }
}

/// Feed lines currently visible to a session. Useful for tests.
pub fn visible_feed(world: &WorldState, session_id: SessionId) -> Result<Vec<FeedLine>, GameError> {
    Ok(world.view(session_id)?.feed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use terminus_core::Permissions;

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

    fn feed_text(world: &WorldState, session: SessionId) -> String {
        world
            .view(session)
            .unwrap()
            .feed
            .iter()
            .map(|line| format!("{}:{}", line.speaker.clone().unwrap_or_default(), line.text))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn movement_chat_and_presence() {
        let (mut world, ada, bob, _, _) = pair();
        let here = world.view(ada).unwrap();
        assert_eq!(here.location_name, "The Coffee Shop");
        assert!(here.present.iter().any(|p| p.name == "bob"));

        world.apply(ada, "say hello bob").unwrap();
        assert!(feed_text(&world, bob).contains("hello bob"));

        world.apply(bob, "go alley").unwrap();
        assert_eq!(world.view(bob).unwrap().location_name, "Neon Alley");
        assert!(!world.view(ada).unwrap().present.iter().any(|p| p.name == "bob"));
        assert!(feed_text(&world, ada).contains("heads toward"));

        world.apply(ada, "say secret").unwrap();
        assert!(!feed_text(&world, bob).contains("secret"));

        world.apply(ada, "chat the wire is up").unwrap();
        assert!(feed_text(&world, bob).contains("the wire is up"));
    }

    #[test]
    fn archive_starts_sealed() {
        let (mut world, ada, _, ada_id, _) = pair();
        world.operators.get_mut(&ada_id).unwrap().location_id = "old-relay".into();
        let err = world.apply(ada, "go archive").unwrap_err();
        assert!(err.to_string().contains("sealed"));
        assert!(world.view(ada).unwrap().exits.iter().all(|exit| exit.keyword != "archive"));
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
    fn bare_exit_word_and_quit() {
        let (mut world, ada, _, _, _) = pair();
        world.apply(ada, "alley").unwrap();
        assert_eq!(world.location_key(ada).unwrap(), "neon-alley");
        let outcome = world.apply(ada, "quit").unwrap();
        assert!(outcome.outcome.quit);
        assert!(world.apply(ada, "frobnicate").is_err());
    }

    #[test]
    fn spectator_cannot_speak_or_move() {
        let mut world = WorldState::for_tests();
        let id = PlayerId::new();
        world.insert_operator(starter_operator(id, uuid::Uuid::new_v4(), "cy", "cy"));
        let identity = SessionIdentity::new(id, "cy", Permissions::spectator());
        let session = identity.session_id;
        world.connect_player(identity).unwrap();
        assert!(world.apply(session, "look").is_ok());
        assert!(world.apply(session, "say hi").is_err());
        assert!(world.apply(session, "go alley").is_err());
    }

    #[test]
    fn reconnect_keeps_location_and_replaces_session() {
        let (mut world, ada, _, ada_id, _) = pair();
        world.apply(ada, "go up").unwrap();
        let again = SessionIdentity::new(ada_id, "ada", Permissions::player());
        let new_session = again.session_id;
        let (_transition, replaced) = world.connect_player(again).unwrap();
        assert_eq!(replaced, Some(ada));
        assert!(world.view(ada).is_err());
        assert_eq!(world.location_key(new_session).unwrap(), "roof-array");
    }

    #[test]
    fn every_command_has_a_handler() {
        let world = WorldState::new();
        for spec in world.registry().specs() {
            assert!(handler(spec.name).is_some(), "{}", spec.name);
        }
    }

    #[test]
    fn disconnect_does_not_remove_the_other_player() {
        let (mut world, ada, bob, _, _) = pair();
        world.detach(ada).unwrap();
        assert!(world.view(bob).is_ok());
        assert!(feed_text(&world, bob).contains("disconnects"));
    }
}
