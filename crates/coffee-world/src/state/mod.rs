//! In-memory Undernet. All rules run here, with no terminal and no socket.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, VecDeque};

use terminus_core::text::{clean_line, MAX_SPEECH_CHARS};
use terminus_core::{
    Audience, Channel, CommandOutcome, CommandRegistry, ConnectOutcome, ConnectionState, ExitView,
    FeedLine, Game, GameError, GameEvent, GameView, ItemView, LogEntry, ObjectView, ParseError,
    ParsedCommand, Permissions, PlayerId, PresenceView, SessionId, SessionIdentity, ViewerContext,
};
use uuid::Uuid;

use crate::catalog::coffee_registry;
use crate::geography::{
    closure_inspect_text, derive_region_seed, describe_tile, effective_tile, epoch_spec,
    format_seed, frontier_epoch, frontier_identity, is_reserved_word, is_survey_tile,
    landmark_sight, map_symbol_for_structures, marker_sights, panel_after_log, stacks_after_log,
    step_origin, story_aliases, story_sight, survey_aliases, survey_look_line, survey_sight,
    torn_page_text, trace_aliases, validate_marker_name, Cardinal, Discovery, EffectiveTile,
    GenerateError, GeneratedRegion, GeographyState, LandmarkKind, MaterializedRegion, RegionCache,
    RegionCoordKey, RegionIdentity, Structure, Terrain, Tile, UrbanContext, WildernessPos,
    WorldConnection, CITY_KEYWORD, MARKER_KIND, OUTSKIRTS_FROM, OUTSKIRTS_KEYWORD, REGION_SIZE,
    SHARE_PRIVATE, SURVEY_MARK_FLAG, WORKSHOP_KIND,
};
use crate::home::HomeState;
use crate::model::{Item, Operator, PersistOp, FEED_CAP, LOG_CAP, STARTER_LOCATION};
use crate::subway::{self, MetroEntrance};
use crate::undernet::{self, ExitDef, ObjectDef, READ_TERMINAL_FLAG};
use crate::waypoint::Waypoint;

const SQUALOR_ALIAS: &str = "squalor";

mod building_cmds;
mod guide;
mod home_cmds;
mod journal_cmds;
mod mail_cmds;
mod settlement_cmds;
mod social_cmds;
mod subway_cmds;
mod theory_cmds;
mod waypoint_cmds;

/// Result of a state transition, including the rows that must be committed.
#[derive(Clone, Debug)]
pub struct Transition {
    pub outcome: CommandOutcome,
    pub ops: Vec<PersistOp>,
}

#[derive(Clone, Debug)]
pub(crate) struct Session {
    pub(crate) identity: SessionIdentity,
}

type Handler = fn(&mut WorldState, SessionId, &ParsedCommand) -> Result<Transition, GameError>;

/// Authoritative Undernet state.
pub struct WorldState {
    pub(crate) world_name: String,
    pub(crate) registry: CommandRegistry,
    pub(crate) operators: HashMap<PlayerId, Operator>,
    pub(crate) sessions: HashMap<SessionId, Session>,
    pub(crate) by_player: HashMap<PlayerId, SessionId>,
    pub(crate) log: VecDeque<LogEntry>,
    pub(crate) overlays: HashMap<SessionId, String>,
    /// Latest inspection text, per session. Not persisted and not shared.
    pub(crate) findings: HashMap<SessionId, String>,
    pub(crate) geography: GeographyState,
    pub(crate) social: crate::social::SocialState,
    pub(crate) settlements: crate::settlement::SettlementState,
    pub(crate) waypoints: Vec<Waypoint>,
    pub(crate) homes: HomeState,
    pub(crate) mailbox: crate::mail::Mailbox,
    pub(crate) theories: crate::theories::TheoryBoard,
    pub(crate) region_cache: RefCell<RegionCache>,
    pub(crate) use_system_clock: bool,
    pub(crate) now_ms: u64,
}

struct TravelTarget {
    to: String,
    from_label: String,
    to_label: String,
    open: Option<(RegionIdentity, u8, u8)>,
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
            findings: HashMap::new(),
            geography: GeographyState::default(),
            social: crate::social::SocialState::default(),
            settlements: crate::settlement::SettlementState::default(),
            waypoints: Vec::new(),
            homes: HomeState::default(),
            mailbox: crate::mail::Mailbox::default(),
            theories: crate::theories::TheoryBoard::default(),
            region_cache: RefCell::new(RegionCache::new(64)),
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

    pub fn install_geography(&mut self, geography: GeographyState) {
        self.geography = geography;
    }

    pub fn geography_snapshot(&self) -> GeographyState {
        self.geography.clone()
    }

    pub fn restore_geography(&mut self, geography: GeographyState) {
        self.geography = geography;
    }

    pub fn install_social(&mut self, social: crate::social::SocialState) {
        self.social = social;
    }

    pub fn social_snapshot(&self) -> crate::social::SocialState {
        self.social.clone()
    }

    pub fn restore_social(&mut self, social: crate::social::SocialState) {
        self.social = social;
    }

    pub fn install_settlements(&mut self, settlements: crate::settlement::SettlementState) {
        self.settlements = settlements;
    }

    pub fn settlement_snapshot(&self) -> crate::settlement::SettlementState {
        self.settlements.clone()
    }

    pub fn restore_settlements(&mut self, settlements: crate::settlement::SettlementState) {
        self.settlements = settlements;
    }

    pub fn install_waypoints(&mut self, waypoints: Vec<Waypoint>) {
        self.waypoints = waypoints;
    }

    pub fn waypoint_snapshot(&self) -> Vec<Waypoint> {
        self.waypoints.clone()
    }

    pub fn restore_waypoints(&mut self, waypoints: Vec<Waypoint>) {
        self.waypoints = waypoints;
    }

    pub fn install_homes(&mut self, homes: HomeState) {
        self.homes = homes;
    }

    pub fn home_snapshot(&self) -> HomeState {
        self.homes.clone()
    }

    pub fn restore_homes(&mut self, homes: HomeState) {
        self.homes = homes;
    }

    pub fn install_mail(&mut self, mailbox: crate::mail::Mailbox) {
        self.mailbox = mailbox;
    }

    pub fn mail_snapshot(&self) -> crate::mail::Mailbox {
        self.mailbox.clone()
    }

    pub fn restore_mail(&mut self, mailbox: crate::mail::Mailbox) {
        self.mailbox = mailbox;
    }

    pub fn install_theories(&mut self, theories: crate::theories::TheoryBoard) {
        self.theories = theories;
    }

    pub fn theory_snapshot(&self) -> crate::theories::TheoryBoard {
        self.theories.clone()
    }

    pub fn restore_theories(&mut self, theories: crate::theories::TheoryBoard) {
        self.theories = theories;
    }

    pub fn overlay_snapshot(&self) -> HashMap<SessionId, String> {
        self.overlays.clone()
    }

    pub fn restore_overlays(&mut self, overlays: HashMap<SessionId, String>) {
        self.overlays = overlays;
    }

    /// Physical region chat key for the player's current place.
    pub fn region_key_for_location(&self, location_id: &str) -> String {
        use crate::settlement::{region_key_for_identity, region_key_for_static};
        if let Some(pos) = WildernessPos::parse(location_id) {
            return region_key_for_identity(pos.identity);
        }
        if let Some(cabin) = self.geography.structure_by_interior(location_id) {
            if let Some(region) = self.geography.region_by_id(cabin.region_id) {
                return region_key_for_identity(region.identity);
            }
        }
        region_key_for_static(location_id)
    }

    pub(crate) fn region_key_of(&self, session_id: SessionId) -> Result<String, GameError> {
        let location = self.location_id(session_id)?;
        Ok(self.region_key_for_location(&location))
    }

    pub fn insert_operator(&mut self, operator: Operator) {
        self.operators.insert(operator.id, operator);
    }

    /// If an operator is stuck in a demolished interior, move them to the door tile or the Coffee Shop.
    pub(crate) fn repair_operator_place(&mut self, player: PlayerId) -> Option<PersistOp> {
        let location = self.operators.get(&player)?.location_id.clone();
        Structure::parse_interior_key(&location)?;
        if self.geography.structure_by_interior(&location).is_some() {
            return None;
        }
        let fallback = self
            .geography
            .connections
            .iter()
            .find(|link| link.from_place == location)
            .map(|link| link.to_place.clone())
            .filter(|place| {
                WildernessPos::parse(place).is_some() || crate::static_location(place).is_some()
            })
            .unwrap_or_else(|| STARTER_LOCATION.to_string());
        if let Some(op) = self.operators.get_mut(&player) {
            op.location_id = fallback.clone();
        }
        Some(PersistOp::Move { operator_id: player, location_id: fallback, last_seen: self.now_ms })
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
        let repair = self.repair_operator_place(identity.player_id);
        let replaced = self.by_player.remove(&identity.player_id);
        if let Some(old) = replaced {
            self.sessions.remove(&old);
            self.overlays.remove(&old);
            self.findings.remove(&old);
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
        let mut ops = vec![op, seen];
        if let Some(repair_op) = repair {
            ops.push(repair_op);
        }
        let (arrival_events, arrival_ops) = self.mark_arrival(session_id, player_id);
        ops.extend(arrival_ops);
        let mut events = vec![event];
        events.extend(arrival_events);
        Ok((Transition { outcome: CommandOutcome { events, quit: false }, ops }, replaced))
    }

    pub fn detach(&mut self, session_id: SessionId) -> Result<Transition, GameError> {
        self.tick();
        let Some(session) = self.sessions.remove(&session_id) else {
            return Ok(Transition { outcome: CommandOutcome::default(), ops: Vec::new() });
        };
        self.overlays.remove(&session_id);
        self.findings.remove(&session_id);
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
        let mut transition = handler(self, session_id, &parsed)?;
        if parsed.name == "inspect" {
            self.attach_inspect_hint(session_id, &mut transition);
        }
        Ok(transition)
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

    pub(crate) fn player(&self, session_id: SessionId) -> Result<PlayerId, GameError> {
        self.sessions
            .get(&session_id)
            .map(|session| session.identity.player_id)
            .ok_or(GameError::UnknownSession)
    }

    pub(crate) fn name_of(&self, session_id: SessionId) -> Result<String, GameError> {
        self.sessions
            .get(&session_id)
            .map(|session| session.identity.name.clone())
            .ok_or(GameError::UnknownSession)
    }

    pub(crate) fn location_id(&self, session_id: SessionId) -> Result<String, GameError> {
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
        if let Some(pos) = WildernessPos::parse(&location) {
            if Cardinal::parse(&key).is_some() || (pos.is_gate() && key == CITY_KEYWORD) {
                return true;
            }
        }
        if location == OUTSKIRTS_FROM && is_squalor_keyword(&key) {
            return true;
        }
        if let Some(pos) = WildernessPos::parse(&location) {
            if let Some(entrance) = subway::entrance_at(pos) {
                if key == entrance.down_keyword || key == "down" {
                    return true;
                }
            }
        }
        if let Some(entrance) = subway::entrance_for_station(&location) {
            if key == entrance.up_keyword || key == "up" {
                return true;
            }
        }
        if self.exit_list().iter().any(|exit| exit.from == location && exit.keyword == key) {
            return true;
        }
        self.geography.connection(&location, &key).is_some()
    }

    fn exit_list(&self) -> Vec<ExitDef> {
        let mut exits = undernet::all_exits();
        exits.extend(subway::station_exits().iter().copied());
        exits
    }

    pub(crate) fn emit(
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

    pub(crate) fn set_overlay(&mut self, session_id: SessionId, text: impl Into<String>) {
        self.overlays.insert(session_id, text.into());
    }

    pub(crate) fn clear_overlay(&mut self, session_id: SessionId) {
        self.overlays.remove(&session_id);
    }

    fn remember_finding(&mut self, session_id: SessionId, text: &str) {
        if text.is_empty() {
            self.findings.remove(&session_id);
        } else {
            self.findings.insert(session_id, text.to_string());
        }
    }

    fn inspect_note(
        &mut self,
        session_id: SessionId,
        player: PlayerId,
        text: String,
    ) -> (GameEvent, PersistOp) {
        self.remember_finding(session_id, &text);
        self.emit(Channel::System, Audience::Player(player), None, text)
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
        if let Some(pos) = WildernessPos::parse(&location) {
            let text = match self.wild_scene(pos, Some(player)) {
                Ok(scene) => scene.map,
                Err(err) => unreadable_wilderness(&err),
            };
            self.set_overlay(session_id, text);
            return Ok(quiet());
        }
        let open = self
            .operators
            .get(&player)
            .map(|op| op.flags.contains("cipher-plate"))
            .unwrap_or(false);
        if subway::is_station(&location) {
            self.set_overlay(session_id, subway_map_text(&location));
            return Ok(quiet());
        }
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
                    "Nobody else is here.\n\
                     `say` stays in this room. `chat` reaches the Undernet."
                        .into()
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
                    let loc_name = self.place_title(&loc);
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
        self.move_through(session_id, &keyword)
    }

    pub(crate) fn move_through(
        &mut self,
        session_id: SessionId,
        keyword: &str,
    ) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let from = self.location_id(session_id)?;
        let flags = self.operator_flags(player);
        let target = self.resolve_travel(&from, keyword, &flags)?;
        if let Some(cabin) = self.geography.structure_by_interior(&target.to).cloned() {
            // Server-side door check; clients cannot forge destination keys.
            self.ensure_cabin_access_for_move(player, &cabin)?;
        }
        let mut ops = Vec::new();
        if let Some((identity, local_x, local_y)) = target.open {
            ops.extend(self.ensure_region(identity, Some(player), local_x, local_y)?);
        }
        let name = self.name_of(session_id)?;
        if let Some(op) = self.operators.get_mut(&player) {
            op.location_id.clone_from(&target.to);
        }
        ops.extend(self.note_home_travel(player, &from, &target.to));
        self.clear_overlay(session_id);
        let (leave, leave_op) = self.emit(
            Channel::System,
            Audience::Location(from),
            None,
            format!("{name} heads toward {}.", target.to_label),
        );
        let (arrive, arrive_op) = self.emit(
            Channel::System,
            Audience::Location(target.to.clone()),
            None,
            format!("{name} arrives from {}.", target.from_label),
        );
        let (note, note_op) = self.emit(
            Channel::System,
            Audience::Player(player),
            None,
            format!("You step into {}.", target.to_label),
        );
        ops.push(PersistOp::Move {
            operator_id: player,
            location_id: target.to,
            last_seen: self.now_ms,
        });
        ops.extend([leave_op, arrive_op, note_op]);
        Ok(Transition {
            outcome: CommandOutcome { events: vec![leave, arrive, note], quit: false },
            ops,
        })
    }

    pub(crate) fn ensure_cabin_access_for_move(
        &self,
        player: PlayerId,
        cabin: &Structure,
    ) -> Result<(), GameError> {
        self.ensure_structure_access_for_move(player, cabin)
    }

    /// Authoritative entry check for cabins and workshops.
    ///
    /// Precedence:
    /// 1. Structure must be enterable.
    /// 2. Owner always enters.
    /// 3. Explicit visitor grant.
    /// 4. Settlement share (`shared` / `communal`) for current members of the linked settlement.
    /// 5. Visitor access mode (`public` / `friends` / `private`).
    /// 6. Otherwise deny.
    pub(crate) fn ensure_structure_access_for_move(
        &self,
        player: PlayerId,
        structure: &Structure,
    ) -> Result<(), GameError> {
        if !structure.is_enterable() {
            return Err(GameError::Rejected("There is no entrance here.".into()));
        }
        if structure.owner_id == Some(player) {
            return Ok(());
        }
        if self.social.has_grant(structure.id, player) {
            return Ok(());
        }
        if self.settlement_share_allows(player, structure) {
            return Ok(());
        }
        let label = if structure.kind == WORKSHOP_KIND { "workshop" } else { "cabin" };
        match structure.access.as_str() {
            crate::geography::ACCESS_PUBLIC => Ok(()),
            crate::geography::ACCESS_FRIENDS => {
                let owner = structure.owner_id.ok_or_else(|| {
                    GameError::Rejected(format!("This {label} has no owner and stays closed."))
                })?;
                if self.social.are_friends(player, owner) {
                    Ok(())
                } else {
                    Err(GameError::Rejected(format!(
                        "Friends of the owner may enter this {label}. You may not."
                    )))
                }
            }
            crate::geography::ACCESS_PRIVATE => {
                Err(GameError::Rejected(format!("This {label} is private.")))
            }
            _ => Err(GameError::Rejected(format!("This {label} is closed."))),
        }
    }

    /// Settlement-derived entry for shared/communal structures. Membership is checked live.
    pub(crate) fn settlement_share_allows(&self, player: PlayerId, structure: &Structure) -> bool {
        use crate::geography::{SHARE_COMMUNAL, SHARE_SHARED};
        match structure.share.as_str() {
            SHARE_SHARED | SHARE_COMMUNAL => {}
            _ => return false,
        }
        let Some(settle_id) = structure.settlement_id else {
            return false;
        };
        if self.settlements.by_id(settle_id).is_none() {
            return false;
        }
        self.settlements.is_member(settle_id, player)
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
        if let Some(pos) = WildernessPos::parse(&location) {
            return self.inspect_wild(session_id, player, pos, &target);
        }
        if let Some(object) = find_object(&location, &target) {
            return self.inspect_object(session_id, player, object);
        }
        if let Some(other_id) = self
            .operators
            .values()
            .find(|op| {
                op.location_id == location
                    && (op.name.eq_ignore_ascii_case(&target)
                        || op.username.eq_ignore_ascii_case(&target))
            })
            .map(|op| op.id)
        {
            let text = self.profile_text(other_id, true);
            let (event, op) = self.inspect_note(session_id, player, text);
            return Ok(Transition {
                outcome: CommandOutcome { events: vec![event], quit: false },
                ops: vec![op],
            });
        }
        if let Some(item) = self
            .operators
            .get(&player)
            .and_then(|op| op.items.iter().find(|item| item_answers(item, &target)).cloned())
        {
            let (event, op) =
                self.inspect_note(session_id, player, format!("{}: {}", item.name, item.detail));
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
        let mut fresh_grant = false;
        if let Some(grant) = object.grant {
            let already = self
                .operators
                .get(&player)
                .map(|op| op.flags.contains(grant.flag))
                .unwrap_or(false);
            if already {
                extra = grant.already.to_string();
            } else if let Some(op) = self.operators.get_mut(&player) {
                fresh_grant = true;
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
        let mut text = if extra.is_empty() {
            object.description.to_string()
        } else {
            format!("{}\n{extra}", object.description)
        };
        if object.keyword == "terminal" && (fresh_grant || !extra.is_empty()) {
            text = if fresh_grant { torn_page_text() } else { extra };
        }
        if object.keyword == "dish" {
            let saw_mark =
                self.operators.get(&player).is_some_and(|op| op.flags.contains(SURVEY_MARK_FLAG));
            if saw_mark {
                text.push('\n');
                text.push_str(&closure_inspect_text());
            }
        }
        let read_terminal =
            self.operators.get(&player).is_some_and(|op| op.flags.contains(READ_TERMINAL_FLAG));
        if read_terminal && object.keyword == "panel" {
            text = panel_after_log();
        }
        if read_terminal && object.keyword == "stacks" {
            text = format!("{}\n{}", stacks_after_log(), object.description);
        }
        let (event, op) = self.inspect_note(session_id, player, text);
        ops.push(op);
        Ok(Transition { outcome: CommandOutcome { events: vec![event], quit: false }, ops })
    }

    fn cmd_mark(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let location = self.location_id(session_id)?;
        let Some(pos) = WildernessPos::parse(&location) else {
            return Err(GameError::Rejected("You can only mark a place in the outskirts.".into()));
        };
        let raw = cmd.args.first().map(String::as_str).unwrap_or("");
        let name =
            validate_marker_name(raw).map_err(|message| GameError::Rejected(message.into()))?;
        let mut ops = self.ensure_region(pos.identity, Some(player), pos.local_x, pos.local_y)?;
        let region_id = self
            .geography
            .region(pos.identity.coord_key())
            .map(|region| region.id)
            .ok_or_else(|| GameError::Rejected("This region is not open.".into()))?;
        if self
            .geography
            .structure_of_kind_at(region_id, pos.local_x, pos.local_y, MARKER_KIND)
            .is_some()
        {
            return Err(GameError::Rejected("This spot already has a marker.".into()));
        }
        let id = Uuid::new_v4();
        let created_at = self.now_ms;
        self.geography.structures.push(Structure {
            id,
            region_id,
            local_x: pos.local_x,
            local_y: pos.local_y,
            kind: MARKER_KIND.into(),
            name: name.clone(),
            owner_id: Some(player),
            access: crate::geography::ACCESS_PUBLIC.into(),
            interior_place: None,
            settlement_id: None,
            share: SHARE_PRIVATE.into(),
            created_at,
        });
        ops.push(PersistOp::Structure {
            id,
            region_id,
            local_x: pos.local_x,
            local_y: pos.local_y,
            kind: MARKER_KIND.into(),
            name: name.clone(),
            owner_id: Some(player),
            access: crate::geography::ACCESS_PUBLIC.into(),
            interior_place: None,
            settlement_id: None,
            share: SHARE_PRIVATE.into(),
            created_at,
        });
        let (event, op) = self.emit(
            Channel::System,
            Audience::Location(location),
            None,
            format!("{} sets a marker named {name}.", self.name_of(session_id)?),
        );
        ops.push(op);
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

    fn operator_flags(&self, player: PlayerId) -> BTreeSet<String> {
        self.operators.get(&player).map(|op| op.flags.clone()).unwrap_or_default()
    }

    fn generated(&self, identity: RegionIdentity) -> Result<GeneratedRegion, GenerateError> {
        self.region_cache.borrow_mut().get_or_generate(identity)
    }

    pub(crate) fn place_title(&self, id: &str) -> String {
        if let Some(structure) = self.geography.structure_by_interior(id) {
            return format!("{} (interior)", structure.name);
        }
        if let Some(pos) = WildernessPos::parse(id) {
            if pos.is_gate() {
                return "The Squalor".to_string();
            }
            if let Some(entrance) = subway::entrance_at(pos) {
                return entrance.surface_label.to_string();
            }
            return match self.generated(pos.identity) {
                Ok(region) => {
                    let baseline = region
                        .tile(pos.local_x, pos.local_y)
                        .unwrap_or(Tile { terrain: Terrain::Rock, landmark: None });
                    effective_tile(pos, baseline, &self.geography).terrain.label().to_string()
                }
                Err(_) => "Unreadable Squalor".to_string(),
            };
        }
        crate::static_location(id).map(|loc| loc.name.to_string()).unwrap_or_else(|| id.to_string())
    }

    fn identity_for_birth(
        &self,
        world: crate::geography::WorldId,
        epoch_id: u32,
        origin_x: i32,
        origin_y: i32,
    ) -> Result<RegionIdentity, String> {
        let key = RegionCoordKey { world, epoch_id, origin_x, origin_y };
        if let Some(existing) = self.geography.region(key) {
            return Ok(existing.identity);
        }
        if frontier_epoch().id == epoch_id {
            return Ok(frontier_identity(world, origin_x, origin_y));
        }
        let epoch = epoch_spec(epoch_id).ok_or_else(|| format!("Epoch {epoch_id} is not open."))?;
        Ok(RegionIdentity {
            world,
            epoch_id,
            origin_x,
            origin_y,
            generation_version: epoch.generation_version,
        })
    }

    fn resolve_travel(
        &self,
        from: &str,
        keyword: &str,
        flags: &BTreeSet<String>,
    ) -> Result<TravelTarget, GameError> {
        let key = keyword.to_ascii_lowercase();
        if let Some(pos) = WildernessPos::parse(from) {
            if let Some(dir) = Cardinal::parse(&key) {
                return self.resolve_cardinal(pos, dir);
            }
            if pos.is_gate() && key == CITY_KEYWORD {
                return Ok(TravelTarget {
                    to: OUTSKIRTS_FROM.to_string(),
                    from_label: self.place_title(from),
                    to_label: "Roof Array".to_string(),
                    open: None,
                });
            }
        }
        if from == OUTSKIRTS_FROM && is_squalor_keyword(&key) {
            let gate = crate::geography::gate_position();
            return Ok(TravelTarget {
                to: gate.key(),
                from_label: self.place_title(from),
                to_label: "The Squalor".to_string(),
                open: Some((gate.identity, gate.local_x, gate.local_y)),
            });
        }
        if let Some(pos) = WildernessPos::parse(from) {
            if let Some(entrance) = subway::entrance_at(pos) {
                if key == entrance.down_keyword || key == "down" {
                    return self.finish_static(from, entrance.station_id, entrance.station_label);
                }
            }
        }
        if let Some(entrance) = subway::entrance_for_station(from) {
            if key == entrance.up_keyword || key == "up" {
                return self.finish_static(from, &entrance.surface.key(), entrance.surface_label);
            }
        }
        if let Some(exit) =
            self.exit_list().into_iter().find(|exit| exit.from == from && exit.keyword == key)
        {
            if let Some(flag) = exit.requires_flag {
                if !flags.contains(flag) {
                    return Err(GameError::Rejected(
                        "The way to the Black Archive is sealed. The lock wants a cipher plate."
                            .into(),
                    ));
                }
            }
            // Subway hops never open sealed Hub rooms; stations only link to stations here.
            return self.finish_static(from, exit.to, exit.label);
        }
        if let Some(link) = self.geography.connection(from, &key).cloned() {
            return self.follow_link(from, &link);
        }
        Err(GameError::Rejected(self.cannot_go(from, flags)))
    }

    fn resolve_cardinal(
        &self,
        pos: WildernessPos,
        dir: Cardinal,
    ) -> Result<TravelTarget, GameError> {
        let (origin_x, origin_y, local_x, local_y) = step_origin(pos, dir)
            .map_err(|_| GameError::Rejected("The wilderness ends here.".into()))?;
        let identity = self
            .identity_for_birth(pos.identity.world, pos.identity.epoch_id, origin_x, origin_y)
            .map_err(GameError::Rejected)?;
        let dest = WildernessPos { identity, local_x, local_y };
        let tile = self.effective_at(dest)?;
        if let Some(reason) = tile.block_reason() {
            return Err(GameError::Rejected(format!(
                "{reason} {}.",
                dir.label().to_ascii_lowercase()
            )));
        }
        Ok(TravelTarget {
            to: dest.key(),
            from_label: self.place_title(&pos.key()),
            to_label: self.place_title(&dest.key()),
            open: Some((identity, local_x, local_y)),
        })
    }

    fn finish_static(&self, from: &str, to: &str, label: &str) -> Result<TravelTarget, GameError> {
        if let Some(pos) = WildernessPos::parse(to) {
            let tile = self.effective_at(pos)?;
            if !tile.traversable() {
                let reason = tile.block_reason().unwrap_or("You cannot go that way");
                return Err(GameError::Rejected(format!("{reason}.")));
            }
            return Ok(TravelTarget {
                to: pos.key(),
                from_label: self.place_title(from),
                to_label: label.to_string(),
                open: Some((pos.identity, pos.local_x, pos.local_y)),
            });
        }
        Ok(TravelTarget {
            to: to.to_string(),
            from_label: self.place_title(from),
            to_label: label.to_string(),
            open: None,
        })
    }

    fn follow_link(&self, from: &str, link: &WorldConnection) -> Result<TravelTarget, GameError> {
        self.finish_static(from, &link.to_place, &link.label)
    }

    pub(crate) fn effective_at(
        &self,
        pos: WildernessPos,
    ) -> Result<crate::geography::EffectiveTile, GameError> {
        let generated = self
            .generated(pos.identity)
            .map_err(|err| GameError::Rejected(unreadable_wilderness(&err)))?;
        let baseline = generated
            .tile(pos.local_x, pos.local_y)
            .unwrap_or(Tile { terrain: Terrain::Rock, landmark: None });
        Ok(effective_tile(pos, baseline, &self.geography))
    }

    fn cannot_go(&self, from: &str, flags: &BTreeSet<String>) -> String {
        let exits = self.list_exits(from, flags);
        let list = if exits.is_empty() {
            "none".to_string()
        } else {
            exits.iter().map(|exit| exit.keyword.clone()).collect::<Vec<_>>().join(", ")
        };
        format!("You can't go that way. Exits: {list}.")
    }

    fn list_exits(&self, location: &str, flags: &BTreeSet<String>) -> Vec<ExitView> {
        let mut exits = Vec::new();
        if let Some(pos) = WildernessPos::parse(location) {
            for dir in [Cardinal::North, Cardinal::South, Cardinal::East, Cardinal::West] {
                if let Ok(target) = self.resolve_cardinal(pos, dir) {
                    exits.push(ExitView {
                        keyword: dir.keyword().to_string(),
                        label: dir.label().to_string(),
                        destination: target.to_label,
                    });
                }
            }
            if pos.is_gate() {
                exits.push(ExitView {
                    keyword: CITY_KEYWORD.to_string(),
                    label: "Roof Array".to_string(),
                    destination: "Roof Array".to_string(),
                });
            }
            if let Some(entrance) = subway::entrance_at(pos) {
                exits.push(ExitView {
                    keyword: entrance.down_keyword.to_string(),
                    label: entrance.station_label.to_string(),
                    destination: entrance.station_label.to_string(),
                });
            }
        } else {
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
                    destination: self.place_title(exit.to),
                });
            }
            if location == OUTSKIRTS_FROM {
                exits.push(ExitView {
                    keyword: OUTSKIRTS_KEYWORD.to_string(),
                    label: "The Squalor".to_string(),
                    destination: "The Squalor".to_string(),
                });
            }
            if let Some(entrance) = subway::entrance_for_station(location) {
                exits.push(ExitView {
                    keyword: entrance.up_keyword.to_string(),
                    label: entrance.surface_label.to_string(),
                    destination: entrance.surface_label.to_string(),
                });
            }
        }
        for link in &self.geography.connections {
            if link.from_place != location {
                continue;
            }
            if exits.iter().any(|exit| exit.keyword == link.keyword) {
                continue;
            }
            if self.follow_link(location, link).is_ok() {
                exits.push(ExitView {
                    keyword: link.keyword.clone(),
                    label: link.label.clone(),
                    destination: self.place_title(&link.to_place),
                });
            }
        }
        exits
    }

    pub(crate) fn ensure_region(
        &mut self,
        identity: RegionIdentity,
        actor: Option<PlayerId>,
        local_x: u8,
        local_y: u8,
    ) -> Result<Vec<PersistOp>, GameError> {
        if self.geography.region(identity.coord_key()).is_some() {
            return Ok(Vec::new());
        }
        let seed =
            derive_region_seed(identity).map_err(|err| GameError::Rejected(err.to_string()))?;
        let id = Uuid::new_v4();
        let discovered_at = self.now_ms;
        self.geography.regions.insert(
            identity.coord_key(),
            MaterializedRegion {
                id,
                identity,
                region_seed: seed,
                placement_x: identity.origin_x,
                placement_y: identity.origin_y,
                discovered_at,
            },
        );
        let discovery_id = Uuid::new_v4();
        self.geography.discoveries.push(Discovery {
            id: discovery_id,
            region_id: Some(id),
            local_x,
            local_y,
            kind: crate::geography::ENTERED_KIND.into(),
            note: "Entered the region.".into(),
            actor_id: actor,
            created_at: discovered_at,
            content_key: None,
            title: None,
            place_key: None,
        });
        Ok(vec![
            PersistOp::MaterializeRegion {
                id,
                world_id: identity.world.as_str().to_string(),
                epoch_id: identity.epoch_id,
                origin_x: identity.origin_x,
                origin_y: identity.origin_y,
                generation_version: identity.generation_version,
                region_seed_hex: format_seed(seed),
                placement_x: identity.origin_x,
                placement_y: identity.origin_y,
                discovered_at,
            },
            PersistOp::Discovery {
                id: discovery_id,
                region_id: Some(id),
                local_x,
                local_y,
                kind: crate::geography::ENTERED_KIND.into(),
                note: "Entered the region.".into(),
                actor_id: actor,
                created_at: discovered_at,
                content_key: None,
                title: None,
                place_key: None,
            },
        ])
    }

    /// Server-side terrain edit. Not a player command. The actor is the caller, never a client field.
    pub fn apply_terrain_mod(
        &mut self,
        identity: RegionIdentity,
        local_x: u8,
        local_y: u8,
        terrain: Terrain,
        reason: &str,
        actor: PlayerId,
    ) -> Result<PersistOp, GameError> {
        self.require_local(local_x, local_y)?;
        let region_id = {
            let region = self
                .geography
                .region(identity.coord_key())
                .ok_or_else(|| GameError::Rejected("That region has not been opened.".into()))?;
            if region.identity != identity {
                return Err(GameError::Rejected(
                    "That region keeps its original generator. It cannot be rewritten as another version."
                        .into(),
                ));
            }
            region.id
        };
        let created_at = self.now_ms;
        self.geography.terrain_mods.retain(|item| {
            !(item.region_id == region_id && item.local_x == local_x && item.local_y == local_y)
        });
        self.geography.terrain_mods.push(crate::geography::TerrainMod {
            region_id,
            local_x,
            local_y,
            terrain,
            reason: reason.to_string(),
            actor_id: Some(actor),
            created_at,
        });
        Ok(PersistOp::TerrainMod {
            region_id,
            local_x,
            local_y,
            terrain: terrain.as_str().to_string(),
            reason: reason.to_string(),
            actor_id: Some(actor),
            created_at,
        })
    }

    /// Move the atlas label. Birth coordinates, seeds, and place keys stay.
    pub fn relocate_region(
        &mut self,
        identity: RegionIdentity,
        placement_x: i32,
        placement_y: i32,
    ) -> Result<PersistOp, GameError> {
        let region = self
            .geography
            .regions
            .get_mut(&identity.coord_key())
            .ok_or_else(|| GameError::Rejected("That region has not been opened.".into()))?;
        if region.identity != identity {
            return Err(GameError::Rejected(
                "Relocation cannot change a region's birth identity.".into(),
            ));
        }
        region.placement_x = placement_x;
        region.placement_y = placement_y;
        Ok(PersistOp::RelocateRegion { id: region.id, placement_x, placement_y })
    }

    /// Author a persistent link. Rejects forged destinations and reserved movement words.
    pub fn link_places(
        &mut self,
        from_place: &str,
        keyword: &str,
        to_place: &str,
        label: &str,
        actor: PlayerId,
    ) -> Result<PersistOp, GameError> {
        let keyword = keyword.trim().to_ascii_lowercase();
        if keyword.is_empty() || keyword.split_whitespace().nth(1).is_some() {
            return Err(GameError::Rejected("A connection needs one keyword.".into()));
        }
        if is_reserved_word(&keyword) {
            return Err(GameError::Rejected("That keyword is reserved.".into()));
        }
        if !keyword.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-') {
            return Err(GameError::Rejected("A connection keyword must be plain text.".into()));
        }
        if from_place == to_place {
            return Err(GameError::Rejected("A connection cannot lead to itself.".into()));
        }
        self.require_place(from_place)?;
        self.require_place(to_place)?;
        if let Some(pos) = WildernessPos::parse(to_place) {
            let tile = self.effective_at(pos)?;
            if !tile.traversable() {
                return Err(GameError::Rejected("That destination cannot be walked.".into()));
            }
        }
        if self.geography.connection(from_place, &keyword).is_some()
            || self
                .exit_list()
                .iter()
                .any(|exit| exit.from == from_place && exit.keyword == keyword)
        {
            return Err(GameError::Rejected("That way already exists.".into()));
        }
        let id = Uuid::new_v4();
        let created_at = self.now_ms;
        self.geography.connections.push(WorldConnection {
            id,
            from_place: from_place.to_string(),
            keyword: keyword.clone(),
            to_place: to_place.to_string(),
            label: label.to_string(),
            kind: "player".into(),
            created_by: Some(actor),
            created_at,
        });
        Ok(PersistOp::Connection {
            id,
            from_place: from_place.to_string(),
            keyword,
            to_place: to_place.to_string(),
            label: label.to_string(),
            kind: "player".into(),
            created_by: Some(actor),
            created_at,
        })
    }

    fn require_place(&self, place: &str) -> Result<(), GameError> {
        if WildernessPos::parse(place).is_some() || crate::static_location(place).is_some() {
            return Ok(());
        }
        if self.geography.structure_by_interior(place).is_some() {
            return Ok(());
        }
        Err(GameError::Rejected("That place is not in the world.".into()))
    }

    fn require_local(&self, local_x: u8, local_y: u8) -> Result<(), GameError> {
        let limit = u32::try_from(REGION_SIZE).unwrap_or(16);
        if u32::from(local_x) >= limit || u32::from(local_y) >= limit {
            return Err(GameError::Rejected("That tile is outside the region.".into()));
        }
        Ok(())
    }

    pub(crate) fn owner_name(&self, id: Option<PlayerId>) -> String {
        id.and_then(|id| self.operators.get(&id).map(|op| op.name.clone()))
            .unwrap_or_else(|| "someone".to_string())
    }

    fn wild_scene(
        &self,
        pos: WildernessPos,
        viewer: Option<PlayerId>,
    ) -> Result<crate::geography::TileScene, GenerateError> {
        let generated = self.generated(pos.identity)?;
        let region_id = self.geography.region(pos.identity.coord_key()).map(|region| region.id);
        let here = region_id
            .map(|id| self.geography.structures_at(id, pos.local_x, pos.local_y))
            .unwrap_or_default();
        let sights = marker_sights(
            here,
            |id| self.owner_name(id),
            |structure| self.settlement_note_for_structure(structure, viewer),
        );
        let marks = region_id
            .map(|id| {
                let mut by_tile: std::collections::BTreeMap<(u8, u8), Vec<&Structure>> =
                    std::collections::BTreeMap::new();
                for item in &self.geography.structures {
                    if item.region_id == id {
                        by_tile.entry((item.local_x, item.local_y)).or_default().push(item);
                    }
                }
                by_tile
                    .into_iter()
                    .filter_map(|((x, y), items)| {
                        map_symbol_for_structures(&items).map(|symbol| (x, y, symbol))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut scene = describe_tile(pos, &generated, &self.geography, &sights, &marks);
        if is_survey_tile(pos) {
            scene.description.push('\n');
            scene.description.push_str(survey_look_line());
            scene.objects.push(("mark".to_string(), "repeated mark".to_string()));
        }
        let place_key = pos.key();
        if let Some(home) = self.homes.presentation_at(&place_key, self.now_ms) {
            let viewer_is_owner = viewer == Some(home.operator_id);
            // Exploration traits for look prose: owner traits for distinctive public
            // variants. Visitors never receive owner-only status lists.
            let traits = if home.is_active() {
                self.exploration_traits_for(home.operator_id)
            } else {
                crate::home::ExplorationTraits::default()
            };
            if let Some(line) = home.look_line(self.now_ms, viewer_is_owner, traits) {
                scene.description.push('\n');
                scene.description.push_str(&line);
                scene.objects.push(("home".to_string(), "personal home".to_string()));
            }
        }
        Ok(scene)
    }

    #[cfg(test)]
    pub(crate) fn wild_scene_for_tests(
        &self,
        pos: WildernessPos,
        viewer: Option<PlayerId>,
    ) -> String {
        self.wild_scene(pos, viewer).map(|scene| scene.description).unwrap_or_default()
    }

    pub(crate) fn settlement_note_for_structure(
        &self,
        structure: &Structure,
        viewer: Option<PlayerId>,
    ) -> Option<String> {
        use crate::settlement::SettlementVisibility;
        let settle_id = structure.settlement_id?;
        let settle = self.settlements.by_id(settle_id)?;
        let can_see = match settle.visibility {
            SettlementVisibility::Public => true,
            SettlementVisibility::Private => {
                viewer.is_some_and(|player| self.settlements.is_member(settle_id, player))
            }
        };
        if can_see {
            Some(format!("Part of settlement {}.", settle.name))
        } else {
            None
        }
    }

    fn wilderness_view(
        &self,
        session_id: SessionId,
        player: PlayerId,
        name: &str,
        pos: WildernessPos,
    ) -> Result<GameView, GameError> {
        let (inventory, location_id) = {
            let op = self.operators.get(&player).ok_or(GameError::UnknownSession)?;
            let inventory = op
                .items
                .iter()
                .map(|item| ItemView {
                    keyword: item.keyword.clone(),
                    name: item.name.clone(),
                    detail: item.detail.clone(),
                })
                .collect();
            (inventory, op.location_id.clone())
        };
        let mut present = self
            .names_in(&location_id)
            .into_iter()
            .filter(|other| other != name)
            .map(|name| PresenceView { name })
            .collect::<Vec<_>>();
        present.sort_by(|a, b| a.name.cmp(&b.name));
        let feed = self.visible_feed_lines(player, &location_id);
        let (location_name, description, objects, mut status) =
            match self.wild_scene(pos, Some(player)) {
                Ok(scene) => {
                    let objects = scene
                        .objects
                        .into_iter()
                        .map(|(keyword, name)| ObjectView { keyword, name })
                        .collect();
                    (scene.title, scene.description, objects, scene.status)
                }
                Err(err) => (
                    "Unreadable wilderness".to_string(),
                    unreadable_wilderness(&err),
                    Vec::new(),
                    vec![format!("generator v{}", pos.identity.generation_version)],
                ),
            };
        let mut lines = self.status_lines(session_id)?;
        lines.append(&mut status);
        Ok(GameView {
            world_name: self.world_name.clone(),
            connection: ConnectionState::Connected,
            operator_name: name.to_string(),
            location_name,
            description,
            exits: self.visible_exits(session_id)?,
            present,
            objects,
            status: lines,
            inventory,
            feed,
            overlay: self.overlays.get(&session_id).cloned(),
            finding: self.findings.get(&session_id).cloned(),
            commands: self.registry.hints(),
        })
    }

    fn inspect_wild(
        &mut self,
        session_id: SessionId,
        player: PlayerId,
        pos: WildernessPos,
        target: &str,
    ) -> Result<Transition, GameError> {
        let scene = self
            .wild_scene(pos, Some(player))
            .map_err(|err| GameError::Rejected(unreadable_wilderness(&err)))?;
        let urban = UrbanContext::at(pos);
        let matched = scene.objects.iter().find(|(keyword, name)| {
            keyword.eq_ignore_ascii_case(target) || name.eq_ignore_ascii_case(target)
        });
        let asks_ground = target == "ground";
        let asks_urban_detail = urban.story.is_some_and(|site| story_aliases(site, target))
            || urban.trace.is_some_and(|kind| trace_aliases(kind, target));
        if matched.is_none()
            && !asks_ground
            && !asks_urban_detail
            && !(is_survey_tile(pos) && survey_aliases(target))
        {
            if let Some(other) =
                self.names_in(&pos.key()).into_iter().find(|name| name.eq_ignore_ascii_case(target))
            {
                let (event, op) = self.inspect_note(
                    session_id,
                    player,
                    format!("{other} is here, watching the land."),
                );
                return Ok(Transition {
                    outcome: CommandOutcome { events: vec![event], quit: false },
                    ops: vec![op],
                });
            }
            return self.inspect_carried(session_id, player, target);
        }
        let tile = self.effective_at(pos)?;
        let mut ops = Vec::new();
        let text = if let Some(entrance) =
            subway::entrance_at(pos).filter(|_| metro_inspect_target(target))
        {
            metro_entrance_inspect(entrance)
        } else if is_survey_tile(pos) && survey_aliases(target) {
            self.note_survey_mark(player, &mut ops);
            survey_sight().text
        } else if let Some(site) = urban.story.filter(|site| story_aliases(*site, target)) {
            story_sight(site).text
        } else if let Some(kind) = urban.trace.filter(|kind| trace_aliases(*kind, target)) {
            kind.inspect_text(urban.band, urban.district)
        } else if let Some(kind) = tile.landmark {
            if target.eq_ignore_ascii_case(kind.keyword())
                || target.eq_ignore_ascii_case(kind.name())
            {
                landmark_sight(pos, kind).text
            } else if target == "ground"
                || target.eq_ignore_ascii_case(tile.terrain.keyword())
                || target.eq_ignore_ascii_case(tile.terrain.label())
            {
                ground_inspect(tile, kind)
            } else {
                scene.description
            }
        } else if target.eq_ignore_ascii_case(tile.terrain.keyword())
            || target.eq_ignore_ascii_case(tile.terrain.label())
            || target == "ground"
        {
            if is_survey_tile(pos) {
                format!(
                    "{}\nA repeated mark lies in the grass. `inspect mark`.",
                    terrain_inspect_line(tile)
                )
            } else if let Some(detail) = urban.detail_look_line() {
                format!("{}\n{detail}", terrain_inspect_line(tile))
            } else {
                format!("{}\nNothing unusual marks this ground.", terrain_inspect_line(tile))
            }
        } else {
            scene.description
        };
        let (event, op) = self.inspect_note(session_id, player, text);
        ops.push(op);
        Ok(Transition { outcome: CommandOutcome { events: vec![event], quit: false }, ops })
    }

    pub(crate) fn note_survey_mark(&mut self, player: PlayerId, ops: &mut Vec<PersistOp>) {
        let Some(operator) = self.operators.get_mut(&player) else {
            return;
        };
        if operator.flags.insert(SURVEY_MARK_FLAG.to_string()) {
            ops.push(PersistOp::Flag { operator_id: player, flag: SURVEY_MARK_FLAG.to_string() });
        }
    }

    fn inspect_carried(
        &mut self,
        session_id: SessionId,
        player: PlayerId,
        target: &str,
    ) -> Result<Transition, GameError> {
        if let Some(item) = self
            .operators
            .get(&player)
            .and_then(|op| op.items.iter().find(|item| item_answers(item, target)).cloned())
        {
            let (event, op) =
                self.inspect_note(session_id, player, format!("{}: {}", item.name, item.detail));
            return Ok(Transition {
                outcome: CommandOutcome { events: vec![event], quit: false },
                ops: vec![op],
            });
        }
        Err(GameError::Rejected(format!("You see nothing called '{target}'.")))
    }

    pub(crate) fn visible_feed_lines(&self, player: PlayerId, location_id: &str) -> Vec<FeedLine> {
        let ignored = self.social.ignored_ids(player);
        let ignored_names: std::collections::HashSet<String> = ignored
            .iter()
            .filter_map(|id| self.operators.get(id).map(|op| op.name.clone()))
            .collect();
        let region_key = self.region_key_for_location(location_id);
        let viewer = ViewerContext { player_id: player, location_id, region_key: &region_key };
        let mut feed = self
            .log
            .iter()
            .filter(|entry| {
                if !entry.visible_to(&viewer) {
                    return false;
                }
                match entry.channel {
                    Channel::Local | Channel::Global | Channel::Region | Channel::Private => {
                        !matches!(&entry.speaker, Some(speaker) if ignored_names.contains(speaker))
                    }
                    Channel::System => true,
                }
            })
            .map(LogEntry::to_feed)
            .collect::<Vec<_>>();
        if feed.len() > FEED_CAP {
            feed = feed.split_off(feed.len() - FEED_CAP);
        }
        feed
    }

    pub(crate) fn resolve_player_name(&self, raw: &str) -> Result<PlayerId, GameError> {
        let key = raw.trim().to_ascii_lowercase();
        if key.is_empty() {
            return Err(GameError::Rejected("Who?".into()));
        }
        let mut matches: Vec<PlayerId> = self
            .operators
            .values()
            .filter(|op| {
                op.name.eq_ignore_ascii_case(&key) || op.username.eq_ignore_ascii_case(&key)
            })
            .map(|op| op.id)
            .collect();
        matches.sort_by_key(|id| id.to_string());
        matches.dedup();
        match matches.as_slice() {
            [id] => Ok(*id),
            [] => Err(GameError::Rejected(format!("No operator named '{raw}'."))),
            _ => {
                Err(GameError::Rejected(format!("Several operators match '{raw}'. Be more exact.")))
            }
        }
    }

    pub(crate) fn is_online(&self, player: PlayerId) -> bool {
        self.by_player.contains_key(&player)
    }

    fn interior_view(
        &self,
        session_id: SessionId,
        player: PlayerId,
        name: &str,
        structure: &Structure,
    ) -> Result<GameView, GameError> {
        let op = self.operators.get(&player).ok_or(GameError::UnknownSession)?;
        let location_id = op.location_id.clone();
        let owner = self.owner_name(structure.owner_id);
        let mut present = self
            .names_in(&location_id)
            .into_iter()
            .filter(|other| other != name)
            .map(|name| PresenceView { name })
            .collect::<Vec<_>>();
        present.sort_by(|a, b| a.name.cmp(&b.name));
        let inventory = op
            .items
            .iter()
            .map(|item| ItemView {
                keyword: item.keyword.clone(),
                name: item.name.clone(),
                detail: item.detail.clone(),
            })
            .collect();
        let feed = self.visible_feed_lines(player, &location_id);
        let kind = if structure.kind == WORKSHOP_KIND { "workshop" } else { "cabin" };
        let mut description = if structure.kind == WORKSHOP_KIND {
            format!(
                "Inside \"{}\", a shared workshop. Tools wait on the benches. {} keeps this place (share: {}).",
                structure.name, owner, structure.share
            )
        } else {
            format!(
                "Inside \"{}\", a small cabin. Light slips through the door cracks. {} keeps this place (share: {}).",
                structure.name, owner, structure.share
            )
        };
        if let Some(note) = self.settlement_note_for_structure(structure, Some(player)) {
            description.push(' ');
            description.push_str(&note);
        }
        Ok(GameView {
            world_name: self.world_name.clone(),
            connection: ConnectionState::Connected,
            operator_name: name.to_string(),
            location_name: format!("{} ({kind})", structure.name),
            description,
            exits: self.visible_exits(session_id)?,
            present,
            objects: vec![ObjectView { keyword: "door".into(), name: format!("{kind} door") }],
            status: self.status_lines(session_id)?,
            inventory,
            feed,
            overlay: self.overlays.get(&session_id).cloned(),
            finding: self.findings.get(&session_id).cloned(),
            commands: self.registry.hints(),
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
        let flags = self.operator_flags(player);
        Ok(self.list_exits(&location, &flags))
    }

    fn status_lines(&self, session_id: SessionId) -> Result<Vec<String>, GameError> {
        let player = self.player(session_id)?;
        let name = self.name_of(session_id)?;
        let op = self.operators.get(&player).ok_or(GameError::UnknownSession)?;
        let cipher = if op.flags.contains("cipher-plate") { "yes" } else { "no" };
        let mut lines = vec![
            name,
            format!("place: {}", self.place_title(&op.location_id)),
            format!("online: {}", self.sessions.len()),
            format!("carried: {}", op.items.len()),
            format!("cipher plate: {cipher}"),
        ];
        if WildernessPos::parse(&op.location_id).is_some() {
            lines.push("layer: surface".into());
        } else if subway::is_station(&op.location_id) {
            lines.push("layer: underground".into());
        } else if crate::static_location(&op.location_id).is_some() {
            lines.push("layer: Hub".into());
        }
        if !op.status.is_empty() {
            lines.push(format!("status: {}", op.status));
        }
        let unread = self.mailbox.unread_for(player);
        if unread > 0 {
            lines.push(format!("mail: {unread} unread"));
        }
        Ok(lines)
    }

    fn build_view(&self, session_id: SessionId) -> Result<GameView, GameError> {
        let player = self.player(session_id)?;
        let name = self.name_of(session_id)?;
        let op = self.operators.get(&player).ok_or(GameError::UnknownSession)?;
        if let Some(pos) = WildernessPos::parse(&op.location_id) {
            return self.wilderness_view(session_id, player, &name, pos);
        }
        if let Some(structure) = self.geography.structure_by_interior(&op.location_id) {
            return self.interior_view(session_id, player, &name, structure);
        }
        let loc = crate::static_location(&op.location_id)
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
        let feed = self.visible_feed_lines(player, &op.location_id);
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
            finding: self.findings.get(&session_id).cloned(),
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
        "welcome" => WorldState::cmd_welcome,
        "look" => WorldState::cmd_look,
        "map" => WorldState::cmd_map,
        "who" => WorldState::cmd_who,
        "go" => WorldState::cmd_go,
        "say" => WorldState::cmd_say,
        "chat" => WorldState::cmd_chat,
        "whisper" => WorldState::cmd_whisper,
        "emote" => WorldState::cmd_emote,
        "profile" => WorldState::cmd_profile,
        "friend" => WorldState::cmd_friend,
        "friends" => WorldState::cmd_friends,
        "ignore" => WorldState::cmd_ignore,
        "unignore" => WorldState::cmd_unignore,
        "ignores" => WorldState::cmd_ignores,
        "status" => WorldState::cmd_status,
        "inventory" => WorldState::cmd_inventory,
        "inspect" => WorldState::cmd_inspect,
        "journal" => WorldState::cmd_journal,
        "theory" => WorldState::cmd_theory,
        "waypoint" => WorldState::cmd_waypoint,
        "mail" => WorldState::cmd_mail,
        "mark" => WorldState::cmd_mark,
        "build" => WorldState::cmd_build,
        "enter" => WorldState::cmd_enter,
        "leave" => WorldState::cmd_leave,
        "home" => WorldState::cmd_home,
        "workshop" => WorldState::cmd_workshop,
        "sign" => WorldState::cmd_sign,
        "trail" => WorldState::cmd_trail,
        "board" => WorldState::cmd_board,
        "region" => WorldState::cmd_region,
        "settle" => WorldState::cmd_settle,
        "settlements" => WorldState::cmd_settlements,
        "quit" => WorldState::cmd_quit,
        _ => return None,
    })
}

fn needs_play(name: &str) -> bool {
    matches!(
        name,
        "go" | "inspect"
            | "mark"
            | "build"
            | "enter"
            | "leave"
            | "home"
            | "workshop"
            | "sign"
            | "trail"
            | "board"
            | "friend"
            | "friends"
            | "ignore"
            | "unignore"
            | "ignores"
            | "profile"
            | "settle"
            | "settlements"
    )
}

fn needs_speak(name: &str) -> bool {
    matches!(name, "say" | "chat" | "whisper" | "emote" | "region")
}

pub(crate) fn quiet() -> Transition {
    Transition { outcome: CommandOutcome::default(), ops: Vec::new() }
}

fn terrain_inspect_line(tile: EffectiveTile) -> String {
    if tile.gate {
        return "Packed earth and a break in the fencing. The roof array is still behind you."
            .to_string();
    }
    match tile.terrain {
        Terrain::Grass => "Open grass. The weather still smells like the city.",
        Terrain::Forest => "Trees stand close. The wood would take a saw.",
        Terrain::Scrub => "Low scrub and scattered wire-grass.",
        Terrain::Water => "Dark water. There is no footing.",
        Terrain::Rock => "Broken rock. There is no footing.",
    }
    .to_string()
}

fn ground_inspect(tile: EffectiveTile, kind: LandmarkKind) -> String {
    format!(
        "{}\nA {} is here. `inspect {}`.",
        terrain_inspect_line(tile),
        kind.name(),
        kind.keyword()
    )
}

fn unreadable_wilderness(err: &GenerateError) -> String {
    format!(
        "This land cannot be rebuilt ({err}). It has not been deleted. Walking it is closed until its generator is available."
    )
}

/// One-word inspect. The sidebar shows the item name, which may be two words,
/// and the stored keyword may be hyphenated.
fn item_answers(item: &Item, target: &str) -> bool {
    if target.is_empty() {
        return false;
    }
    item.keyword.eq_ignore_ascii_case(target)
        || item.name.eq_ignore_ascii_case(target)
        || item.keyword.split('-').any(|part| part.eq_ignore_ascii_case(target))
        || item.name.split_whitespace().any(|part| part.eq_ignore_ascii_case(target))
}

fn find_object(location: &str, target: &str) -> Option<ObjectDef> {
    let loc = crate::static_location(location)?;
    loc.objects.iter().copied().find(|object| {
        object.keyword.eq_ignore_ascii_case(target)
            || object.aliases.iter().any(|alias| alias.eq_ignore_ascii_case(target))
    })
}

fn is_squalor_keyword(key: &str) -> bool {
    key == OUTSKIRTS_KEYWORD || key == SQUALOR_ALIAS
}

fn metro_inspect_target(target: &str) -> bool {
    matches!(target, "stair" | "metro" | "entrance" | "subway" | "stairs")
}

fn metro_entrance_inspect(entrance: &MetroEntrance) -> String {
    format!(
        "Municipal subway stair. The landing is dry dust over painted concrete. \
A faded plate names {}. `go {}` descends to the platform. \
The stair is a fixed entrance — not a random shaft.",
        entrance.station_label, entrance.down_keyword
    )
}

fn subway_map_text(current_id: &str) -> String {
    let mark = |id: &str, label: &str| -> String {
        if id == current_id {
            format!("*{label}*")
        } else {
            label.to_string()
        }
    };
    format!(
        "\
Subway (Line 8 fragment)
{} --- tunnel --- {}
   |                    |
 surface              surface
 (Ash Plaza)        (Ward West)

One hop covers {} surface tiles.
`surface` returns up. `map` in the Hub shows Hub rooms.

* marks where you are standing.",
        mark(subway::ASH_STATION, "Ash Plaza"),
        mark(subway::WARD_STATION, "Ward West"),
        subway::SUBWAY_SURFACE_SCALE,
    )
}

fn audience_parts(audience: &Audience) -> (&'static str, String) {
    match audience {
        Audience::Everyone => ("everyone", String::new()),
        Audience::Location(id) => ("location", id.clone()),
        Audience::Region(key) => ("region", key.clone()),
        Audience::Player(id) => ("player", id.to_string()),
    }
}

fn channel_name(channel: Channel) -> &'static str {
    match channel {
        Channel::System => "system",
        Channel::Local => "local",
        Channel::Global => "global",
        Channel::Region => "region",
        Channel::Private => "private",
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
        bio: String::new(),
        status: String::new(),
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

    #[test]
    fn outskirts_can_be_walked_marked_and_left() {
        let (mut world, ada, bob, ada_id, _) = pair();
        world.apply(ada, "go up").unwrap();
        let roof = world.view(ada).unwrap();
        assert!(roof.exits.iter().any(|exit| exit.keyword == "outskirts"));
        world.apply(ada, "outskirts").unwrap();
        let arrived = world.view(ada).unwrap();
        assert_eq!(arrived.location_name, "The Squalor");
        assert!(arrived.description.contains("Roof Array"));
        assert!(arrived.exits.iter().any(|exit| exit.keyword == "city"));
        assert!(world.location_key(ada).unwrap().starts_with("w:undernet:1:0:0:1:"));
        assert_eq!(world.geography.regions.len(), 1);
        let identity = world.geography.regions.values().next().unwrap().identity;
        assert_eq!(identity.generation_version, 1);
        assert_eq!(identity.origin_x, 0);
        assert_eq!(identity.origin_y, 0);

        world.apply_terrain_mod(identity, 8, 9, Terrain::Grass, "trail", ada_id).unwrap();
        world.apply(ada, "north").unwrap();
        let moved = world.location_key(ada).unwrap();
        assert!(moved.ends_with(":8:9"), "{moved}");
        world.apply(ada, "mark camp").unwrap();
        let marked = world.view(ada).unwrap();
        assert!(marked.objects.iter().any(|object| object.keyword == "camp"));
        world.apply(ada, "map").unwrap();
        let mapped = world.view(ada).unwrap().overlay.unwrap_or_default();
        assert!(mapped.contains("birth (0, 0)"));
        assert!(mapped.contains('@'));

        world.apply(bob, "go up").unwrap();
        world.apply(bob, "outskirts").unwrap();
        assert!(!world.view(ada).unwrap().present.iter().any(|person| person.name == "bob"));
        world.apply(bob, "north").unwrap();
        assert!(world.view(ada).unwrap().present.iter().any(|person| person.name == "bob"));
        world.apply(ada, "say quiet").unwrap();
        assert!(feed_text(&world, bob).contains("quiet"));

        world.apply(ada, "south").unwrap();
        world.apply(ada, "city").unwrap();
        assert_eq!(world.view(ada).unwrap().location_name, "Roof Array");
        assert_eq!(world.geography.structures.len(), 1);
        assert_eq!(world.geography.structures[0].owner_id, Some(ada_id));
    }

    #[test]
    fn gate_description_matches_movement_and_the_return_path() {
        let (mut world, ada, _, _, _) = pair();
        let region = crate::geography::generate_region(crate::geography::gate_identity()).unwrap();
        assert_eq!(crate::geography::fingerprint(&region), 0x8812_F1DB_241C_39A1);
        assert_eq!(region.tile(8, 8).unwrap().terrain, Terrain::Water);

        world.apply(ada, "go up").unwrap();
        world.apply(ada, "outskirts").unwrap();
        let arrived = world.view(ada).unwrap();
        let exits: Vec<&str> = arrived.exits.iter().map(|exit| exit.keyword.as_str()).collect();
        assert_eq!(exits, ["west", "city"]);
        assert!(arrived.description.contains("Open: west"), "{}", arrived.description);
        assert!(
            arrived.description.contains("Floodwater blocks the way north, east, and south"),
            "{}",
            arrived.description
        );
        assert!(arrived.objects.iter().any(|object| object.keyword == "ground"));
        assert!(arrived.objects.iter().all(|object| object.keyword != "water"));
        for dir in ["north", "east", "south"] {
            let err = world.apply(ada, dir).unwrap_err().to_string();
            assert!(err.contains("Floodwater blocks"), "{dir}: {err}");
            assert_eq!(world.location_key(ada).unwrap(), gate_position_key());
        }
        world.apply(ada, "west").unwrap();
        assert_eq!(world.location_key(ada).unwrap(), crate::geography::survey_position().key());
        world.apply(ada, "east").unwrap();
        world.apply(ada, "city").unwrap();
        assert_eq!(world.view(ada).unwrap().location_name, "Roof Array");
        world.apply(ada, "outskirts").unwrap();
        assert_eq!(world.view(ada).unwrap().location_name, "The Squalor");
    }

    fn gate_position_key() -> String {
        crate::geography::gate_position().key()
    }

    #[test]
    fn inspection_remains_beside_the_place() {
        let (mut world, ada, bob, _, _) = pair();
        world.apply(ada, "inspect counter").unwrap();
        let noticed = world.view(ada).unwrap();
        assert!(noticed.description.contains("Steam"));
        assert!(noticed.finding.as_deref().unwrap_or("").contains("scarred laminate"));
        world.apply(ada, "say still here").unwrap();
        let after_say = world.view(ada).unwrap();
        assert!(after_say.description.contains("Steam"));
        assert!(after_say.finding.as_deref().unwrap_or("").contains("scarred laminate"));
        assert!(world.view(bob).unwrap().finding.is_none());
        assert!(!feed_text(&world, bob).contains("scarred laminate"));
        world.apply(ada, "go alley").unwrap();
        let moved = world.view(ada).unwrap();
        assert!(moved.description.contains("service alley"));
        assert!(moved.finding.as_deref().unwrap_or("").contains("scarred laminate"));
        world.apply(ada, "look").unwrap();
        assert!(world.view(ada).unwrap().finding.as_deref().unwrap_or("").contains("laminate"));
        world.apply(ada, "journal").unwrap();
        let journal = world.view(ada).unwrap();
        assert!(journal.overlay.as_deref().unwrap_or("").contains("empty"));
        assert!(journal.description.contains("service alley"));
        assert_eq!(journal.location_name, "Neon Alley");
        assert!(journal.finding.as_deref().unwrap_or("").contains("laminate"));
    }

    #[test]
    fn wilderness_rejects_invalid_actions() {
        let (mut world, ada, _, ada_id, _) = pair();
        let err = world.apply(ada, "mark camp").unwrap_err();
        assert!(err.to_string().contains("outskirts"));
        let err = world.apply(ada, "go north").unwrap_err();
        assert!(err.to_string().contains("can't go"));
        let err = world.apply(ada, "go w:undernet:1:5:5:1:0:0").unwrap_err();
        assert!(err.to_string().contains("can't go"));

        world.apply(ada, "go up").unwrap();
        world.apply(ada, "outskirts").unwrap();
        assert!(world.apply(ada, "mark n").is_err());
        assert!(world.apply(ada, "mark north").is_err());
        let before = world.geography_snapshot();
        world.apply(ada, "mark camp").unwrap();
        assert!(world.apply(ada, "mark camp").unwrap_err().to_string().contains("already"));
        assert_eq!(world.geography.structures[0].owner_id, Some(ada_id));
        let marked = world.geography_snapshot();
        world.restore_geography(before);
        assert!(world.geography.structures.is_empty());
        world.install_geography(marked);
        assert_eq!(world.geography.structures.len(), 1);
    }

    #[test]
    fn water_blocks_movement_and_a_mod_can_open_it() {
        let (mut world, ada, _, ada_id, _) = pair();
        let region = crate::geography::generate_region(crate::geography::gate_identity()).unwrap();
        let mut water = None;
        let mut stand = None;
        for y in 0..16u8 {
            for x in 0..16u8 {
                if (x, y) == (crate::geography::GATE_LOCAL_X, crate::geography::GATE_LOCAL_Y) {
                    continue;
                }
                let tile = region.tile(x, y).unwrap();
                if tile.terrain != Terrain::Water {
                    continue;
                }
                for (dx, dy, dir) in
                    [(1, 0, "west"), (-1, 0, "east"), (0, 1, "south"), (0, -1, "north")]
                {
                    let nx = i32::from(x) + dx;
                    let ny = i32::from(y) + dy;
                    if !(0..16).contains(&nx) || !(0..16).contains(&ny) {
                        continue;
                    }
                    let Ok(nx) = u8::try_from(nx) else { continue };
                    let Ok(ny) = u8::try_from(ny) else { continue };
                    let neighbor = region.tile(nx, ny).unwrap();
                    if neighbor.terrain.traversable() {
                        water = Some((x, y, dir));
                        stand = Some((nx, ny));
                    }
                }
            }
        }
        let (wx, wy, back) = water.expect("region 0,0 has water beside walkable ground");
        let (sx, sy) = stand.unwrap();
        let pos =
            WildernessPos { identity: crate::geography::gate_identity(), local_x: sx, local_y: sy };
        world.operators.get_mut(&ada_id).unwrap().location_id = pos.key();
        let err = world.apply(ada, back).unwrap_err();
        assert!(
            err.to_string().contains("Floodwater") || err.to_string().contains("Water"),
            "{err}"
        );

        world.ensure_region(pos.identity, Some(ada_id), sx, sy).unwrap();
        world.apply_terrain_mod(pos.identity, wx, wy, Terrain::Grass, "ford", ada_id).unwrap();
        world.apply(ada, back).unwrap();
        let arrived = WildernessPos::parse(&world.location_key(ada).unwrap()).unwrap();
        assert_eq!((arrived.local_x, arrived.local_y), (wx, wy));
    }

    #[test]
    fn crossing_a_region_edge_keeps_identity_stable() {
        let (mut world, ada, _, ada_id, _) = pair();
        let identity = crate::geography::gate_identity();
        let edge = WildernessPos { identity, local_x: 15, local_y: 1 };
        world.operators.get_mut(&ada_id).unwrap().location_id = edge.key();
        world.ensure_region(identity, Some(ada_id), 15, 1).unwrap();
        let next = RegionIdentity { origin_x: 1, ..identity };
        world.ensure_region(next, Some(ada_id), 0, 1).unwrap();
        world.apply_terrain_mod(identity, 15, 1, Terrain::Grass, "edge", ada_id).unwrap();
        world.apply_terrain_mod(next, 0, 1, Terrain::Grass, "edge", ada_id).unwrap();
        world.apply(ada, "east").unwrap();
        let arrived = WildernessPos::parse(&world.location_key(ada).unwrap()).unwrap();
        assert_eq!(arrived.identity.origin_x, 1);
        assert_eq!(arrived.local_x, 0);
        assert_eq!(arrived.identity.generation_version, 1);
        assert_eq!(world.geography.regions.len(), 2);

        let birth = world.geography.region(identity.coord_key()).unwrap().identity;
        let op = world.relocate_region(identity, 80, -4).unwrap();
        assert!(matches!(op, PersistOp::RelocateRegion { placement_x: 80, placement_y: -4, .. }));
        let stored = world.geography.region(identity.coord_key()).unwrap();
        assert_eq!(stored.identity, birth);
        assert_eq!((stored.placement_x, stored.placement_y), (80, -4));
        assert_eq!(stored.region_seed, derive_region_seed(identity).unwrap());
    }

    #[test]
    fn links_reject_reserved_words_and_unknown_places() {
        let (mut world, _, _, ada_id, _) = pair();
        let err =
            world.link_places("coffee-shop", "north", "neon-alley", "North", ada_id).unwrap_err();
        assert!(err.to_string().contains("reserved"));
        let err =
            world.link_places("coffee-shop", "path", "nowhere", "Nowhere", ada_id).unwrap_err();
        assert!(err.to_string().contains("not in the world"));
        let err =
            world.link_places("coffee-shop", "path", "coffee-shop", "Loop", ada_id).unwrap_err();
        assert!(err.to_string().contains("itself"));
    }

    #[test]
    fn unknown_generation_version_stays_addressable() {
        let (mut world, ada, _, ada_id, _) = pair();
        let mut identity = crate::geography::gate_identity();
        identity.origin_x = 3;
        identity.origin_y = -4;
        identity.generation_version = 9;
        let id = uuid::Uuid::new_v4();
        world.install_geography(GeographyState {
            regions: HashMap::from([(
                identity.coord_key(),
                MaterializedRegion {
                    id,
                    identity,
                    region_seed: 99,
                    placement_x: identity.origin_x,
                    placement_y: identity.origin_y,
                    discovered_at: 1,
                },
            )]),
            ..GeographyState::default()
        });
        let pos = WildernessPos { identity, local_x: 1, local_y: 1 };
        world.operators.get_mut(&ada_id).unwrap().location_id = pos.key();
        let view = world.view(ada).unwrap();
        assert!(view.description.contains("version 9"), "{}", view.description);
        assert_eq!(world.geography.regions.len(), 1);
        assert!(world.apply(ada, "north").is_err());
        assert_eq!(world.location_key(ada).unwrap(), pos.key());
    }

    #[test]
    fn spectator_cannot_mark() {
        let mut world = WorldState::for_tests();
        let id = PlayerId::new();
        world.insert_operator(starter_operator(id, uuid::Uuid::new_v4(), "cy", "cy"));
        let identity = SessionIdentity::new(id, "cy", Permissions::spectator());
        let session = identity.session_id;
        world.connect_player(identity).unwrap();
        world.operators.get_mut(&id).unwrap().location_id = crate::geography::gate_position().key();
        assert!(world.apply(session, "mark camp").is_err());
        assert!(world.geography.structures.is_empty());
    }

    fn to_buildable_tile(world: &mut WorldState, session: SessionId, player: PlayerId) {
        world.apply(session, "go up").unwrap();
        world.apply(session, "outskirts").unwrap();
        let identity = crate::geography::gate_identity();
        world.apply_terrain_mod(identity, 8, 9, Terrain::Grass, "camp", player).unwrap();
        world.apply(session, "north").unwrap();
    }

    #[test]
    fn whisper_is_private_and_offline_is_refused() {
        let (mut world, ada, bob, _, bob_id) = pair();
        world.apply(ada, "whisper bob secret-ping").unwrap();
        assert!(feed_text(&world, bob).contains("secret-ping"));
        assert!(feed_text(&world, ada).contains("You whisper to bob"));
        // A third party must not see private contents in their feed.
        world.apply(bob, "go alley").unwrap();
        let bob_feed = feed_text(&world, bob);
        assert!(bob_feed.contains("secret-ping"));
        world.detach(bob).unwrap();
        let err = world.apply(ada, "whisper bob later").unwrap_err();
        assert!(err.to_string().contains("offline"), "{err}");
        let _ = bob_id;
    }

    #[test]
    fn friendship_requires_accept_and_persists_in_memory() {
        let (mut world, ada, bob, ada_id, bob_id) = pair();
        world.apply(ada, "friend add bob").unwrap();
        assert!(!world.social.are_friends(ada_id, bob_id));
        world.apply(bob, "friend accept ada").unwrap();
        assert!(world.social.are_friends(ada_id, bob_id));
        world.apply(ada, "friends").unwrap();
        let overlay = world.view(ada).unwrap().overlay.unwrap_or_default();
        assert!(overlay.contains("bob"));
        assert!(overlay.contains("online"));
        world.apply(bob, "friend remove ada").unwrap();
        assert!(!world.social.are_friends(ada_id, bob_id));
    }

    #[test]
    fn ignore_filters_chat_and_blocks_whisper() {
        let (mut world, ada, bob, _, _) = pair();
        world.apply(ada, "ignore bob").unwrap();
        world.apply(bob, "say ignored-local").unwrap();
        world.apply(bob, "chat ignored-global").unwrap();
        let ada_feed = feed_text(&world, ada);
        assert!(!ada_feed.contains("ignored-local"), "{ada_feed}");
        assert!(!ada_feed.contains("ignored-global"), "{ada_feed}");
        let err = world.apply(bob, "whisper ada hi").unwrap_err();
        assert!(err.to_string().contains("ignore"), "{err}");
        let err = world.apply(bob, "friend add ada").unwrap_err();
        assert!(err.to_string().contains("ignore"), "{err}");
    }

    #[test]
    fn cabin_build_enter_visit_and_private_access() {
        let (mut world, ada, bob, ada_id, _) = pair();
        to_buildable_tile(&mut world, ada, ada_id);
        world.apply(ada, "build cabin Haven").unwrap();
        assert_eq!(world.geography.structures.iter().filter(|s| s.kind == "cabin").count(), 1);
        world.apply(ada, "enter Haven").unwrap();
        assert!(world.view(ada).unwrap().location_name.contains("Haven"));
        world.apply(ada, "leave").unwrap();
        assert!(!world.location_key(ada).unwrap().starts_with("i:"));

        // Bob can visit a public cabin.
        world.apply(bob, "go up").unwrap();
        world.apply(bob, "outskirts").unwrap();
        world.apply(bob, "north").unwrap();
        world.apply(bob, "enter Haven").unwrap();
        assert!(world.view(bob).unwrap().location_name.contains("Haven"));
        world.apply(bob, "leave").unwrap();

        world.apply(ada, "home access private").unwrap();
        let err = world.apply(bob, "enter Haven").unwrap_err();
        assert!(err.to_string().contains("private"), "{err}");
        world.apply(ada, "home allow bob").unwrap();
        world.apply(bob, "enter Haven").unwrap();
        world.apply(bob, "leave").unwrap();

        world.apply(ada, "sign Welcome friends").unwrap();
        world.apply(ada, "sign read").unwrap();
        assert!(feed_text(&world, ada).contains("Welcome friends"));
        world.apply(ada, "trail Ridge").unwrap();
        assert!(world.geography.structures.iter().any(|s| s.kind == "trail"));
        // Marker can coexist with cabin/sign/trail on the same tile.
        world.apply(ada, "mark Camp").unwrap();
        assert!(
            world
                .geography
                .structures_at(
                    world
                        .geography
                        .structures
                        .iter()
                        .find(|s| s.kind == "cabin")
                        .unwrap()
                        .region_id,
                    WildernessPos::parse(&world.location_key(ada).unwrap()).unwrap().local_x,
                    WildernessPos::parse(&world.location_key(ada).unwrap()).unwrap().local_y,
                )
                .len()
                >= 4
        );
    }

    #[test]
    fn cabin_demolish_evacuates_occupants() {
        let (mut world, ada, bob, ada_id, _) = pair();
        to_buildable_tile(&mut world, ada, ada_id);
        world.apply(ada, "build cabin Nest").unwrap();
        world.apply(bob, "go up").unwrap();
        world.apply(bob, "outskirts").unwrap();
        world.apply(bob, "north").unwrap();
        world.apply(bob, "enter Nest").unwrap();
        assert!(world.location_key(bob).unwrap().starts_with("i:"));
        world.apply(ada, "home demolish confirm").unwrap();
        assert!(!world.location_key(bob).unwrap().starts_with("i:"));
        assert!(!world.geography.structures.iter().any(|s| s.kind == "cabin"));
        assert!(world.geography.connections.is_empty());
    }

    #[test]
    fn noticeboard_posts_and_owner_removal() {
        let (mut world, ada, bob, _, _) = pair();
        world.apply(ada, "board post Meet at Haven").unwrap();
        world.apply(bob, "board").unwrap();
        let overlay = world.view(bob).unwrap().overlay.unwrap_or_default();
        assert!(overlay.contains("Meet at Haven"));
        let id8 = world.social.notices[0].id.to_string()[..8].to_string();
        let err = world.apply(bob, &format!("board remove {id8}")).unwrap_err();
        assert!(err.to_string().contains("your own"), "{err}");
        world.apply(ada, &format!("board remove {id8}")).unwrap();
        assert!(world.social.notices.is_empty());
    }

    #[test]
    fn profile_bio_is_public_without_account_secrets() {
        let (mut world, ada, bob, _, _) = pair();
        world.apply(ada, "profile bio Likes rain and relays").unwrap();
        world.apply(ada, "profile status exploring").unwrap();
        world.apply(bob, "inspect ada").unwrap();
        let feed = feed_text(&world, bob);
        assert!(feed.contains("Likes rain and relays"));
        assert!(feed.contains("exploring"));
        assert!(!feed.contains("password"));
        assert!(!feed.contains('@'));
    }

    #[test]
    fn settlement_create_invite_accept_and_duplicate_name() {
        let (mut world, ada, bob, ada_id, bob_id) = pair();
        to_buildable_tile(&mut world, ada, ada_id);
        world.apply(ada, "settle create Haven").unwrap();
        assert!(world.settlements.settlement_of(ada_id).is_some());
        let err = world.apply(ada, "settle create Other").unwrap_err();
        assert!(err.to_string().contains("already belong"), "{err}");

        // Bob on same tile tries duplicate name.
        world.apply(bob, "go up").unwrap();
        world.apply(bob, "outskirts").unwrap();
        world.apply(bob, "north").unwrap();
        let err = world.apply(bob, "settle create Haven").unwrap_err();
        assert!(err.to_string().contains("taken"), "{err}");

        world.apply(ada, "settle invite bob").unwrap();
        world.apply(bob, "settle accept Haven").unwrap();
        assert!(world
            .settlements
            .is_member(world.settlements.settlement_of(ada_id).unwrap().id, bob_id));
        world.apply(ada, "settle members").unwrap();
        let overlay = world.view(ada).unwrap().overlay.unwrap_or_default();
        assert!(overlay.contains("bob"));
    }

    #[test]
    fn settlement_invite_blocked_by_ignore_and_decline() {
        let (mut world, ada, bob, ada_id, bob_id) = pair();
        to_buildable_tile(&mut world, ada, ada_id);
        world.apply(ada, "settle create Ridge").unwrap();
        world.apply(bob, "ignore ada").unwrap();
        let err = world.apply(ada, "settle invite bob").unwrap_err();
        assert!(err.to_string().contains("blocked"), "{err}");
        world.apply(bob, "unignore ada").unwrap();
        world.apply(ada, "settle invite bob").unwrap();
        world.apply(bob, "settle decline Ridge").unwrap();
        assert!(world.settlements.membership_of(bob_id).is_none());
    }

    #[test]
    fn settlement_kick_transfer_leave_and_disband_preserve_structures() {
        let (mut world, ada, bob, ada_id, bob_id) = pair();
        to_buildable_tile(&mut world, ada, ada_id);
        world.apply(ada, "build cabin Nest").unwrap();
        world.apply(ada, "settle create Nestland").unwrap();
        let settle_id = world.settlements.settlement_of(ada_id).unwrap().id;
        assert!(world
            .geography
            .structures
            .iter()
            .any(|s| s.kind == "cabin" && s.settlement_id == Some(settle_id)));

        world.apply(bob, "go up").unwrap();
        world.apply(bob, "outskirts").unwrap();
        world.apply(bob, "north").unwrap();
        world.apply(ada, "settle invite bob").unwrap();
        world.apply(bob, "settle accept Nestland").unwrap();

        world.apply(ada, "settle kick bob").unwrap();
        assert!(!world.settlements.is_member(settle_id, bob_id));
        world.apply(ada, "settle invite bob").unwrap();
        world.apply(bob, "settle accept Nestland").unwrap();
        world.apply(ada, "settle transfer bob").unwrap();
        assert!(world.settlements.is_owner(settle_id, bob_id));
        assert!(!world.settlements.is_owner(settle_id, ada_id));

        // Former owner can leave as member.
        world.apply(ada, "settle leave").unwrap();
        assert!(world.settlements.membership_of(ada_id).is_none());

        // Sole owner leave disbands; cabin remains.
        world.apply(bob, "settle leave").unwrap();
        assert!(world.settlements.settlements.is_empty());
        assert!(world.geography.structures.iter().any(|s| s.kind == "cabin" && s.name == "Nest"));
        assert!(world
            .geography
            .structures
            .iter()
            .filter(|s| s.kind == "cabin")
            .all(|s| s.settlement_id.is_none()));
    }

    #[test]
    fn regional_chat_shares_exterior_and_cabin_isolates_rooms() {
        let (mut world, ada, bob, ada_id, _) = pair();
        to_buildable_tile(&mut world, ada, ada_id);
        world.apply(ada, "build cabin Lookout").unwrap();
        world.apply(bob, "go up").unwrap();
        world.apply(bob, "outskirts").unwrap();
        world.apply(bob, "north").unwrap();

        world.apply(ada, "region ridge-call").unwrap();
        assert!(feed_text(&world, bob).contains("ridge-call"));
        world.apply(ada, "enter Lookout").unwrap();
        world.apply(ada, "region inside-call").unwrap();
        assert!(feed_text(&world, bob).contains("inside-call"));

        // Different Undernet rooms do not share region chat.
        world.apply(bob, "south").unwrap();
        let bob_loc = world.location_key(bob).unwrap();
        assert!(bob_loc.contains(":8:8") || bob_loc.ends_with(":8:8"), "bob at {bob_loc}");
        world.apply(bob, "city").unwrap();
        world.apply(bob, "go down").unwrap();
        world.apply(ada, "leave").unwrap();
        world.apply(ada, "south").unwrap();
        world.apply(ada, "city").unwrap();
        world.apply(ada, "go down").unwrap();
        world.apply(ada, "go alley").unwrap();
        world.apply(ada, "region alley-only").unwrap();
        let bob_feed = feed_text(&world, bob);
        assert!(!bob_feed.contains("alley-only"), "{bob_feed}");
        world.apply(bob, "region shop-only").unwrap();
        assert!(!feed_text(&world, ada).contains("shop-only"));
    }

    #[test]
    fn regional_chat_respects_ignore_and_leaves_global() {
        let (mut world, ada, bob, _, _) = pair();
        world.apply(ada, "ignore bob").unwrap();
        world.apply(bob, "region ignored-region").unwrap();
        assert!(!feed_text(&world, ada).contains("ignored-region"));
        world.apply(bob, "chat still-global").unwrap();
        assert!(!feed_text(&world, ada).contains("still-global"));
        world.apply(ada, "unignore bob").unwrap();
        world.apply(bob, "chat heard-global").unwrap();
        assert!(feed_text(&world, ada).contains("heard-global"));
    }

    #[test]
    fn settlement_discovery_and_board_permissions() {
        let (mut world, ada, bob, ada_id, _) = pair();
        to_buildable_tile(&mut world, ada, ada_id);
        world.apply(ada, "settle create PublicHold").unwrap();
        world.apply(ada, "settle describe A windy ridge").unwrap();
        world.apply(bob, "go up").unwrap();
        world.apply(bob, "outskirts").unwrap();
        world.apply(bob, "north").unwrap();
        world.apply(bob, "settlements").unwrap();
        let overlay = world.view(bob).unwrap().overlay.unwrap_or_default();
        assert!(overlay.contains("PublicHold"), "{overlay}");

        world.apply(ada, "settle access private").unwrap();
        world.apply(bob, "settlements").unwrap();
        let overlay = world.view(bob).unwrap().overlay.unwrap_or_default();
        assert!(!overlay.contains("PublicHold"), "{overlay}");
        let err = world.apply(bob, "settle inspect PublicHold").unwrap_err();
        assert!(err.to_string().contains("private"), "{err}");

        world.apply(ada, "settle access public").unwrap();
        world.apply(ada, "settle board post Meet at dusk").unwrap();
        world.apply(bob, "settle board PublicHold").unwrap();
        let overlay = world.view(bob).unwrap().overlay.unwrap_or_default();
        assert!(overlay.contains("Meet at dusk"), "{overlay}");
        let err = world.apply(bob, "settle board PublicHold post outsider").unwrap_err();
        assert!(err.to_string().contains("members"), "{err}");
    }

    #[test]
    fn settle_invites_inbox_and_offline_accept() {
        let (mut world, ada, bob, ada_id, bob_id) = pair();
        to_buildable_tile(&mut world, ada, ada_id);
        world.apply(ada, "settle create Haven").unwrap();
        world.apply(ada, "settle invite bob").unwrap();

        // Ada must not see Bob's inbox contents.
        world.apply(ada, "settle invites").unwrap();
        let ada_overlay = world.view(ada).unwrap().overlay.unwrap_or_default();
        assert!(ada_overlay.contains("none"), "{ada_overlay}");

        world.apply(bob, "settle invites").unwrap();
        let overlay = world.view(bob).unwrap().overlay.unwrap_or_default();
        assert!(overlay.contains("Haven"), "{overlay}");
        assert!(overlay.contains("settle accept Haven"), "{overlay}");
        assert!(overlay.contains("settle decline Haven"), "{overlay}");

        world.apply(bob, "settle accept Haven").unwrap();
        assert!(world
            .settlements
            .is_member(world.settlements.settlement_of(ada_id).unwrap().id, bob_id));
        world.apply(bob, "settle invites").unwrap();
        let overlay = world.view(bob).unwrap().overlay.unwrap_or_default();
        assert!(overlay.contains("none"), "{overlay}");
    }

    #[test]
    fn expired_invite_cannot_join_and_reject_does_not_prune() {
        let (mut world, ada, bob, ada_id, bob_id) = pair();
        to_buildable_tile(&mut world, ada, ada_id);
        world.apply(ada, "settle create Haven").unwrap();
        world.apply(ada, "settle invite bob").unwrap();
        let invite_id = world.settlements.invites[0].id;
        // Force expiry without pruning.
        world.now_ms = world.settlements.invites[0].expires_at;

        // Rejected accept of a wrong name must not prune/expire in memory alone.
        let err = world.apply(bob, "settle accept NoSuchHold").unwrap_err();
        assert!(err.to_string().contains("invite"), "{err}");
        let invite = world.settlements.invites.iter().find(|i| i.id == invite_id).unwrap();
        assert_eq!(invite.status, crate::settlement::InviteStatus::Pending);

        // Accept of expired invite expires it durably (Ok path) without joining.
        world.apply(bob, "settle accept Haven").unwrap();
        assert!(world.settlements.membership_of(bob_id).is_none());
        let invite = world.settlements.invites.iter().find(|i| i.id == invite_id).unwrap();
        assert_eq!(invite.status, crate::settlement::InviteStatus::Expired);

        // Inbox stays empty for actionable invites.
        world.apply(bob, "settle invites").unwrap();
        let overlay = world.view(bob).unwrap().overlay.unwrap_or_default();
        assert!(overlay.contains("none"), "{overlay}");
        assert!(!overlay.contains("settle accept Haven"), "{overlay}");
        let _ = ada_id;
    }

    #[test]
    fn private_cabin_stays_private_until_shared_then_leave_revokes() {
        let (mut world, ada, bob, ada_id, bob_id) = pair();
        to_buildable_tile(&mut world, ada, ada_id);
        world.apply(ada, "build cabin Nest").unwrap();
        world.apply(ada, "home access private").unwrap();
        world.apply(ada, "settle create Nestland").unwrap();
        world.apply(bob, "go up").unwrap();
        world.apply(bob, "outskirts").unwrap();
        world.apply(bob, "north").unwrap();
        world.apply(ada, "settle invite bob").unwrap();
        world.apply(bob, "settle accept Nestland").unwrap();

        // Membership alone does not open a private cabin.
        let err = world.apply(bob, "enter Nest").unwrap_err();
        assert!(err.to_string().contains("private"), "{err}");
        let err = world.apply(bob, "go nest").unwrap_err();
        assert!(err.to_string().contains("private"), "{err}");

        // Non-owner cannot change sharing.
        let err = world.apply(bob, "home share shared").unwrap_err();
        assert!(err.to_string().contains("owner"), "{err}");

        world.apply(ada, "home share shared").unwrap();
        world.apply(bob, "enter Nest").unwrap();
        world.apply(bob, "leave").unwrap();

        world.apply(bob, "settle leave").unwrap();
        assert!(world.settlements.membership_of(bob_id).is_none());
        let err = world.apply(bob, "enter Nest").unwrap_err();
        assert!(err.to_string().contains("private"), "{err}");
        let _ = ada_id;
    }

    #[test]
    fn communal_workshop_board_and_disband_fallback() {
        let (mut world, ada, bob, ada_id, bob_id) = pair();
        to_buildable_tile(&mut world, ada, ada_id);
        world.apply(ada, "settle create Forgehold").unwrap();
        world.apply(ada, "build workshop Anvil").unwrap();
        let workshop =
            world.geography.structures.iter().find(|s| s.kind == "workshop").cloned().unwrap();
        assert_eq!(workshop.share, "private");
        assert_eq!(workshop.settlement_id, world.settlements.settlement_of(ada_id).map(|s| s.id));

        world.apply(ada, "home share communal").unwrap();
        world.apply(bob, "go up").unwrap();
        world.apply(bob, "outskirts").unwrap();
        world.apply(bob, "north").unwrap();
        let err = world.apply(bob, "enter Anvil").unwrap_err();
        assert!(err.to_string().contains("private"), "{err}");

        world.apply(ada, "settle invite bob").unwrap();
        world.apply(bob, "settle accept Forgehold").unwrap();
        world.apply(bob, "enter Anvil").unwrap();
        world.apply(bob, "workshop board post Fit new hinges").unwrap();
        world.apply(bob, "workshop board").unwrap();
        let overlay = world.view(bob).unwrap().overlay.unwrap_or_default();
        assert!(overlay.contains("Fit new hinges"), "{overlay}");
        world.apply(bob, "leave").unwrap();

        // Disband preserves the workshop and reverts share to private.
        world.apply(ada, "settle disband confirm").unwrap();
        assert!(world.settlements.settlements.is_empty());
        let workshop = world
            .geography
            .structures
            .iter()
            .find(|s| s.kind == "workshop" && s.name == "Anvil")
            .unwrap();
        assert!(workshop.settlement_id.is_none());
        assert_eq!(workshop.share, "private");
        let err = world.apply(bob, "enter Anvil").unwrap_err();
        assert!(err.to_string().contains("private"), "{err}");
        // Owner still enters.
        world.apply(ada, "enter Anvil").unwrap();
        let _ = bob_id;
    }

    #[test]
    fn structure_access_precedence_owner_grant_share() {
        let (mut world, ada, bob, ada_id, bob_id) = pair();
        to_buildable_tile(&mut world, ada, ada_id);
        world.apply(ada, "build cabin Keep").unwrap();
        world.apply(ada, "home access private").unwrap();
        let cabin = world.geography.structures.iter().find(|s| s.kind == "cabin").cloned().unwrap();
        assert!(world.ensure_structure_access_for_move(ada_id, &cabin).is_ok());
        assert!(world.ensure_structure_access_for_move(bob_id, &cabin).is_err());

        world.apply(ada, "home allow bob").unwrap();
        let cabin = world.geography.structures.iter().find(|s| s.kind == "cabin").cloned().unwrap();
        assert!(world.ensure_structure_access_for_move(bob_id, &cabin).is_ok());
        world.apply(ada, "home deny bob").unwrap();

        world.apply(ada, "settle create Keepstead").unwrap();
        world.apply(ada, "settle invite bob").unwrap();
        world.apply(bob, "go up").unwrap();
        world.apply(bob, "outskirts").unwrap();
        world.apply(bob, "north").unwrap();
        world.apply(bob, "settle accept Keepstead").unwrap();
        world.apply(ada, "home share shared").unwrap();
        let cabin = world.geography.structures.iter().find(|s| s.kind == "cabin").cloned().unwrap();
        assert!(world.ensure_structure_access_for_move(bob_id, &cabin).is_ok());
        assert!(world.settlement_share_allows(bob_id, &cabin));
        assert!(world.settlement_share_allows(ada_id, &cabin));
    }
}
