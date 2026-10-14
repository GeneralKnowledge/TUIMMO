//! In-memory Undernet. All rules run here, with no terminal and no socket.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

use terminus_core::{
    Audience, Channel, CommandOutcome, CommandRegistry, ConnectOutcome, FeedLine, Game, GameError,
    GameEvent, GameView, LogEntry, ParseError, ParsedCommand, Permissions, PlayerId, SessionId,
    SessionIdentity, ViewerContext,
};
use uuid::Uuid;

use self::commands::{coffee_commands, coffee_registry, find_command, CommandAccess};
use self::movement_cmds::is_squalor_keyword;
use crate::geography::frontier_epoch;
use crate::geography::{
    derive_region_seed, effective_tile, format_seed, is_reserved_word, Cardinal, Discovery,
    GenerateError, GeneratedRegion, GeographyState, MaterializedRegion, RegionCache,
    RegionIdentity, Structure, Terrain, Tile, UrbanContext, WildernessPos, WorldConnection,
    CITY_KEYWORD, OUTSKIRTS_FROM, REGION_SIZE,
};
use crate::home::HomeState;
use crate::model::{Item, Operator, PersistOp, FEED_CAP, LOG_CAP, STARTER_LOCATION};
use crate::subway::{self};
use crate::threads::{load_default_threads, ThreadState};
use crate::undernet::{self, ExitDef};
use crate::waypoint::Waypoint;

mod building_cmds;
mod chalk_cmds;
pub(crate) mod commands;
mod guide;
mod listen_cmds;
mod home_cmds;
mod inspect_cmds;
mod inventory_cmds;
mod invite_cmds;
mod journal_cmds;
mod mail_cmds;
mod movement_cmds;
mod presence_cmds;
mod settlement_cmds;
mod survey_cmds;
mod snapshot;
mod social_cmds;
mod speech_cmds;
mod subway_cmds;
mod theory_cmds;
mod thread_inspect;
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

/// Authoritative Undernet state.
pub struct WorldState {
    pub(crate) world_name: String,
    pub(crate) commands: Vec<commands::CoffeeCommand>,
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
    pub(crate) threads: ThreadState,
    pub(crate) invites: crate::invite::InviteBook,
    pub(crate) region_cache: RefCell<RegionCache>,
    pub(crate) use_system_clock: bool,
    pub(crate) now_ms: u64,
}

impl WorldState {
    pub fn new() -> Self {
        let commands = coffee_commands();
        let registry = coffee_registry(&commands);
        Self {
            world_name: "Undernet".into(),
            commands,
            registry,
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
            threads: ThreadState::new(load_default_threads(), frontier_epoch().world_seed),
            invites: crate::invite::InviteBook::default(),
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

    /// Hydrate in-memory state from a SQLite load. Prefer this over individual `install_*` calls.
    pub fn install_loaded_world(&mut self, loaded: crate::store::LoadedWorld) {
        let crate::store::LoadedWorld {
            operators,
            log,
            geography,
            social,
            settlements,
            waypoints,
            homes,
            mailbox,
            theories,
            clue_seen,
            invites,
        } = loaded;
        self.install_loaded(operators, log);
        self.install_geography(geography);
        self.install_social(social);
        self.install_settlements(settlements);
        self.install_waypoints(waypoints);
        self.install_homes(homes);
        self.install_mail(mailbox);
        self.install_theories(theories);
        self.install_clue_seen(clue_seen);
        self.install_invites(invites);
    }

    pub(crate) fn install_clue_seen(&mut self, seen: Vec<crate::threads::ClueSeen>) {
        let catalog = load_default_threads();
        self.threads = ThreadState::new(catalog, frontier_epoch().world_seed);
        self.threads.seen = seen;
    }

    pub(crate) fn restore_threads(&mut self, threads: ThreadState) {
        self.threads = threads;
    }

    pub(crate) fn install_loaded(&mut self, operators: Vec<Operator>, log: Vec<LogEntry>) {
        self.operators = operators.into_iter().map(|op| (op.id, op)).collect();
        self.log = log.into_iter().collect();
        while self.log.len() > LOG_CAP {
            self.log.pop_front();
        }
    }

    pub(crate) fn install_geography(&mut self, geography: GeographyState) {
        self.geography = geography;
    }

    #[allow(dead_code)] // command and integration tests simulate partial rollback
    pub(crate) fn geography_snapshot(&self) -> GeographyState {
        self.geography.clone()
    }

    pub(crate) fn restore_geography(&mut self, geography: GeographyState) {
        self.geography = geography;
    }

    pub(crate) fn install_social(&mut self, social: crate::social::SocialState) {
        self.social = social;
    }

    pub(crate) fn restore_social(&mut self, social: crate::social::SocialState) {
        self.social = social;
    }

    pub(crate) fn install_settlements(&mut self, settlements: crate::settlement::SettlementState) {
        self.settlements = settlements;
    }

    pub(crate) fn restore_settlements(&mut self, settlements: crate::settlement::SettlementState) {
        self.settlements = settlements;
    }

    pub(crate) fn install_waypoints(&mut self, waypoints: Vec<Waypoint>) {
        self.waypoints = waypoints;
    }

    pub(crate) fn restore_waypoints(&mut self, waypoints: Vec<Waypoint>) {
        self.waypoints = waypoints;
    }

    pub(crate) fn install_homes(&mut self, homes: HomeState) {
        self.homes = homes;
    }

    #[allow(dead_code)]
    pub(crate) fn home_snapshot(&self) -> HomeState {
        self.homes.clone()
    }

    pub(crate) fn restore_homes(&mut self, homes: HomeState) {
        self.homes = homes;
    }

    pub(crate) fn install_mail(&mut self, mailbox: crate::mail::Mailbox) {
        self.mailbox = mailbox;
    }

    pub(crate) fn restore_mail(&mut self, mailbox: crate::mail::Mailbox) {
        self.mailbox = mailbox;
    }

    pub(crate) fn install_theories(&mut self, theories: crate::theories::TheoryBoard) {
        self.theories = theories;
    }

    #[allow(dead_code)]
    pub(crate) fn theory_snapshot(&self) -> crate::theories::TheoryBoard {
        self.theories.clone()
    }

    pub(crate) fn restore_theories(&mut self, theories: crate::theories::TheoryBoard) {
        self.theories = theories;
    }

    pub(crate) fn install_invites(&mut self, invites: crate::invite::InviteBook) {
        self.invites = invites;
    }

    pub(crate) fn restore_invites(&mut self, invites: crate::invite::InviteBook) {
        self.invites = invites;
    }

    /// Apply invite rows minted or redeemed during registration.
    pub fn note_invite_redemption(
        &mut self,
        code_key: Option<&str>,
        redeemed_by: PlayerId,
        redeemed_at: u64,
        minted: Vec<crate::invite::AccountInvite>,
    ) {
        if let Some(key) = code_key {
            self.invites.mark_redeemed(key, redeemed_by, redeemed_at);
        }
        self.invites.push_many(minted);
    }

    #[allow(dead_code)]
    pub(crate) fn overlay_snapshot(&self) -> HashMap<SessionId, String> {
        self.overlays.clone()
    }

    pub(crate) fn restore_overlays(&mut self, overlays: HashMap<SessionId, String>) {
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
        let purge_ops = self.purge_expired_chalk();
        if !self.sessions.contains_key(&session_id) {
            return Err(GameError::UnknownSession);
        }
        let parsed = match self.registry.parse(line) {
            Ok(parsed) => parsed,
            Err(ParseError::Empty) => {
                return Ok(Transition {
                    outcome: CommandOutcome::default(),
                    ops: purge_ops,
                });
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
        let command = find_command(&self.commands, &parsed.name).ok_or_else(|| {
            GameError::Rejected(format!("Unknown command '{}'. Try help.", parsed.name))
        })?;
        match command.access {
            CommandAccess::Play if !permissions.can_play() => {
                return Err(GameError::Rejected("You cannot do that from this session.".into()));
            }
            CommandAccess::Speak if !permissions.can_speak() => {
                return Err(GameError::Rejected("You cannot speak right now.".into()));
            }
            _ => {}
        }
        let mut transition = (command.handler)(self, session_id, &parsed)?;
        transition.ops.extend(purge_ops);
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
                    let terrain = effective_tile(pos, baseline, &self.geography).terrain;
                    let urban = UrbanContext::at(pos);
                    format!("{} — {}", terrain.label(), urban.title_suffix())
                }
                Err(_) => "Unreadable Squalor".to_string(),
            };
        }
        crate::static_location(id).map(|loc| loc.name.to_string()).unwrap_or_else(|| id.to_string())
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

pub(crate) fn quiet() -> Transition {
    Transition { outcome: CommandOutcome::default(), ops: Vec::new() }
}

pub(crate) fn unreadable_wilderness(err: &GenerateError) -> String {
    format!(
        "This land cannot be rebuilt ({err}). It has not been deleted. Walking it is closed until its generator is available."
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
#[path = "world_rules_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "ugc_policy_tests.rs"]
mod ugc_policy_tests;
