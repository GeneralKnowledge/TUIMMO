//! Homestead, signs, trails, and Coffee Shop noticeboard commands.

use terminus_core::{
    Audience, Channel, CommandOutcome, GameError, ParsedCommand, PlayerId, SessionId,
};
use uuid::Uuid;

use crate::geography::{
    validate_marker_name, Structure, WildernessPos, ACCESS_FRIENDS, ACCESS_PRIVATE, ACCESS_PUBLIC,
    CABIN_KIND, SIGN_KIND, TRAIL_KIND,
};
use crate::model::{PersistOp, STARTER_LOCATION};
use crate::social::{
    validate_notice_text, validate_sign_text, NoticePost, StructureGrant, BUILD_RATE_MS,
    NOTICE_PLACE_CAP, NOTICE_RATE_MS, NOTICE_RETENTION_MS,
};
use crate::state::{quiet, Transition, WorldState};

impl WorldState {
    pub(crate) fn cmd_build(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let rest = cmd.rest.trim();
        let mut parts = rest.splitn(2, char::is_whitespace);
        let kind = parts.next().unwrap_or("").to_ascii_lowercase();
        let name_raw = parts.next().unwrap_or("").trim();
        if kind != "cabin" || name_raw.is_empty() {
            return Err(GameError::Rejected("usage: build cabin <name>".into()));
        }
        let player = self.player(session_id)?;
        self.check_build_rate(player)?;
        let location = self.location_id(session_id)?;
        let Some(pos) = WildernessPos::parse(&location) else {
            return Err(GameError::Rejected("You can only build in the outskirts.".into()));
        };
        if pos.is_gate() {
            return Err(GameError::Rejected("The city gate must stay clear.".into()));
        }
        let name = validate_marker_name(name_raw).map_err(|m| GameError::Rejected(m.into()))?;
        let tile = self.effective_at(pos)?;
        if !tile.traversable() {
            return Err(GameError::Rejected("You cannot build on impassable ground.".into()));
        }
        let mut ops = self.ensure_region(pos.identity, Some(player), pos.local_x, pos.local_y)?;
        let region_id = self
            .geography
            .region(pos.identity.coord_key())
            .map(|region| region.id)
            .ok_or_else(|| GameError::Rejected("This region is not open.".into()))?;
        if self
            .geography
            .structure_of_kind_at(region_id, pos.local_x, pos.local_y, CABIN_KIND)
            .is_some()
        {
            return Err(GameError::Rejected("There is already a cabin here.".into()));
        }
        // Unique cabin name per owner is enough for enter/inspect by name on the tile.
        let id = Uuid::new_v4();
        let interior = Structure::interior_key(id);
        let created_at = self.now_ms;
        let structure = Structure {
            id,
            region_id,
            local_x: pos.local_x,
            local_y: pos.local_y,
            kind: CABIN_KIND.into(),
            name: name.clone(),
            owner_id: Some(player),
            access: ACCESS_PUBLIC.into(),
            interior_place: Some(interior.clone()),
            created_at,
        };
        self.geography.structures.push(structure.clone());
        ops.push(PersistOp::Structure {
            id,
            region_id,
            local_x: pos.local_x,
            local_y: pos.local_y,
            kind: CABIN_KIND.into(),
            name: name.clone(),
            owner_id: Some(player),
            access: ACCESS_PUBLIC.into(),
            interior_place: Some(interior.clone()),
            created_at,
        });
        let enter_kw = name.to_ascii_lowercase();
        let enter_op = self.link_places(&location, &enter_kw, &interior, &name, player)?;
        ops.push(enter_op);
        let out_op = self.link_places(&interior, "out", &location, "outside", player)?;
        ops.push(out_op);
        self.social.last_build_at.insert(player, self.now_ms);
        let (event, note) = self.emit(
            Channel::System,
            Audience::Location(location),
            None,
            format!("{} raises a cabin named {name}.", self.name_of(session_id)?),
        );
        ops.push(note);
        Ok(Transition { outcome: CommandOutcome { events: vec![event], quit: false }, ops })
    }

    pub(crate) fn cmd_enter(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let raw = cmd.args.first().map(String::as_str).unwrap_or("");
        let player = self.player(session_id)?;
        let location = self.location_id(session_id)?;
        let Some(pos) = WildernessPos::parse(&location) else {
            return Err(GameError::Rejected("There is no cabin entrance here.".into()));
        };
        let region_id = self
            .geography
            .region(pos.identity.coord_key())
            .map(|region| region.id)
            .ok_or_else(|| GameError::Rejected("There is no cabin here.".into()))?;
        let Some(cabin) = self
            .geography
            .structures_at(region_id, pos.local_x, pos.local_y)
            .into_iter()
            .find(|item| item.kind == CABIN_KIND && item.name.eq_ignore_ascii_case(raw))
            .cloned()
        else {
            return Err(GameError::Rejected(format!("No cabin named '{raw}' here.")));
        };
        self.ensure_cabin_access_for_move(player, &cabin)?;
        let keyword = cabin.name.to_ascii_lowercase();
        self.move_through(session_id, &keyword)
    }

    pub(crate) fn cmd_leave(
        &mut self,
        session_id: SessionId,
        _cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let location = self.location_id(session_id)?;
        if self.geography.structure_by_interior(&location).is_none() {
            return Err(GameError::Rejected("You are not inside a cabin.".into()));
        }
        if self.geography.connection(&location, "out").is_none() {
            return Err(GameError::Rejected("The door is gone. You cannot leave that way.".into()));
        }
        self.move_through(session_id, "out")
    }

    pub(crate) fn cmd_home(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let rest = cmd.rest.trim();
        let mut parts = rest.splitn(2, char::is_whitespace);
        let action = parts.next().unwrap_or("").to_ascii_lowercase();
        let tail = parts.next().unwrap_or("").trim();
        let player = self.player(session_id)?;
        let cabin = self.cabin_under_control(session_id, player)?;
        match action.as_str() {
            "access" => {
                let mode = tail.to_ascii_lowercase();
                let access = match mode.as_str() {
                    "public" => ACCESS_PUBLIC,
                    "private" => ACCESS_PRIVATE,
                    "friends" => ACCESS_FRIENDS,
                    _ => {
                        return Err(GameError::Rejected(
                            "usage: home access <public|private|friends>".into(),
                        ));
                    }
                };
                self.update_cabin_access(player, cabin.id, access)
            }
            "allow" => {
                if tail.is_empty() {
                    return Err(GameError::Rejected("usage: home allow <player>".into()));
                }
                let grantee = self.resolve_player_name(tail)?;
                self.grant_cabin(player, cabin.id, grantee)
            }
            "deny" => {
                if tail.is_empty() {
                    return Err(GameError::Rejected("usage: home deny <player>".into()));
                }
                let grantee = self.resolve_player_name(tail)?;
                self.revoke_cabin(player, cabin.id, grantee)
            }
            "rename" => {
                if tail.is_empty() {
                    return Err(GameError::Rejected("usage: home rename <name>".into()));
                }
                let name = validate_marker_name(tail).map_err(|m| GameError::Rejected(m.into()))?;
                self.rename_cabin(session_id, player, cabin, name)
            }
            "demolish" => {
                if tail != "confirm" {
                    return Err(GameError::Rejected(
                        "Demolition is permanent. Type: home demolish confirm".into(),
                    ));
                }
                self.demolish_cabin(session_id, player, cabin)
            }
            _ => Err(GameError::Rejected(
                "usage: home <access|allow|deny|rename|demolish> ...".into(),
            )),
        }
    }

    fn cabin_under_control(
        &self,
        session_id: SessionId,
        player: PlayerId,
    ) -> Result<Structure, GameError> {
        let location = self.location_id(session_id)?;
        if let Some(cabin) = self.geography.structure_by_interior(&location).cloned() {
            if cabin.owner_id != Some(player) {
                return Err(GameError::Rejected("Only the owner can manage this cabin.".into()));
            }
            return Ok(cabin);
        }
        let Some(pos) = WildernessPos::parse(&location) else {
            return Err(GameError::Rejected("Stand at your cabin or inside it.".into()));
        };
        let region_id = self
            .geography
            .region(pos.identity.coord_key())
            .map(|region| region.id)
            .ok_or_else(|| GameError::Rejected("Stand at your cabin or inside it.".into()))?;
        let cabin = self
            .geography
            .structure_of_kind_at(region_id, pos.local_x, pos.local_y, CABIN_KIND)
            .cloned()
            .ok_or_else(|| GameError::Rejected("There is no cabin here.".into()))?;
        if cabin.owner_id != Some(player) {
            return Err(GameError::Rejected("Only the owner can manage this cabin.".into()));
        }
        Ok(cabin)
    }

    fn update_cabin_access(
        &mut self,
        player: PlayerId,
        cabin_id: Uuid,
        access: &str,
    ) -> Result<Transition, GameError> {
        let Some(cabin) = self.geography.structures.iter_mut().find(|item| item.id == cabin_id)
        else {
            return Err(GameError::Rejected("Cabin not found.".into()));
        };
        cabin.access = access.to_string();
        let name = cabin.name.clone();
        let interior = cabin.interior_place.clone();
        let (event, note) = self.emit(
            Channel::System,
            Audience::Player(player),
            None,
            format!("\"{name}\" access is now {access}."),
        );
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![
                PersistOp::UpdateStructure {
                    id: cabin_id,
                    name,
                    access: access.to_string(),
                    interior_place: interior,
                },
                note,
            ],
        })
    }

    fn grant_cabin(
        &mut self,
        player: PlayerId,
        cabin_id: Uuid,
        grantee: PlayerId,
    ) -> Result<Transition, GameError> {
        if grantee == player {
            return Err(GameError::Rejected("You already own this cabin.".into()));
        }
        if self.social.has_grant(cabin_id, grantee) {
            return Err(GameError::Rejected("They already have access.".into()));
        }
        let now = self.now_ms;
        self.social.grants.push(StructureGrant {
            structure_id: cabin_id,
            grantee_id: grantee,
            created_at: now,
        });
        let name = self.operators.get(&grantee).map(|op| op.name.clone()).unwrap_or_default();
        let (event, note) = self.emit(
            Channel::System,
            Audience::Player(player),
            None,
            format!("{name} may enter your cabin."),
        );
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![
                PersistOp::StructureGrantUpsert {
                    structure_id: cabin_id,
                    grantee_id: grantee,
                    created_at: now,
                },
                note,
            ],
        })
    }

    fn revoke_cabin(
        &mut self,
        player: PlayerId,
        cabin_id: Uuid,
        grantee: PlayerId,
    ) -> Result<Transition, GameError> {
        if !self.social.has_grant(cabin_id, grantee) {
            return Err(GameError::Rejected("They do not have a special grant.".into()));
        }
        self.social
            .grants
            .retain(|grant| !(grant.structure_id == cabin_id && grant.grantee_id == grantee));
        let name = self.operators.get(&grantee).map(|op| op.name.clone()).unwrap_or_default();
        let (event, note) = self.emit(
            Channel::System,
            Audience::Player(player),
            None,
            format!("{name}'s cabin grant is revoked."),
        );
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![
                PersistOp::StructureGrantDelete { structure_id: cabin_id, grantee_id: grantee },
                note,
            ],
        })
    }

    fn rename_cabin(
        &mut self,
        session_id: SessionId,
        player: PlayerId,
        cabin: Structure,
        name: String,
    ) -> Result<Transition, GameError> {
        let exterior = self.exterior_of(&cabin)?;
        let old_kw = cabin.name.to_ascii_lowercase();
        let new_kw = name.to_ascii_lowercase();
        // Replace exterior enter connection keyword when the name changes.
        let mut ops = Vec::new();
        if old_kw != new_kw {
            if let Some(link) = self.geography.connection(&exterior, &old_kw).cloned() {
                self.geography.connections.retain(|item| item.id != link.id);
                ops.push(PersistOp::DeleteConnection { id: link.id });
                let enter = self.link_places(
                    &exterior,
                    &new_kw,
                    cabin.interior_place.as_deref().unwrap_or(""),
                    &name,
                    player,
                )?;
                ops.push(enter);
            }
        }
        if let Some(item) = self.geography.structures.iter_mut().find(|item| item.id == cabin.id) {
            item.name = name.clone();
        }
        ops.push(PersistOp::UpdateStructure {
            id: cabin.id,
            name: name.clone(),
            access: cabin.access,
            interior_place: cabin.interior_place,
        });
        let (event, note) = self.emit(
            Channel::System,
            Audience::Location(self.location_id(session_id)?),
            None,
            format!("{} renames the cabin to {name}.", self.name_of(session_id)?),
        );
        ops.push(note);
        Ok(Transition { outcome: CommandOutcome { events: vec![event], quit: false }, ops })
    }

    fn demolish_cabin(
        &mut self,
        session_id: SessionId,
        player: PlayerId,
        cabin: Structure,
    ) -> Result<Transition, GameError> {
        let interior = cabin.interior_place.clone().unwrap_or_default();
        let exterior = self.exterior_of(&cabin)?;
        let mut ops = Vec::new();
        // Move anyone inside out.
        let displaced: Vec<PlayerId> = self
            .operators
            .values()
            .filter(|op| op.location_id == interior)
            .map(|op| op.id)
            .collect();
        for id in displaced {
            if let Some(op) = self.operators.get_mut(&id) {
                op.location_id = exterior.clone();
            }
            ops.push(PersistOp::Move {
                operator_id: id,
                location_id: exterior.clone(),
                last_seen: self.now_ms,
            });
        }
        let touching: Vec<_> = self
            .geography
            .connections
            .iter()
            .filter(|link| link.from_place == interior || link.to_place == interior)
            .cloned()
            .collect();
        for link in touching {
            self.geography.connections.retain(|item| item.id != link.id);
            ops.push(PersistOp::DeleteConnection { id: link.id });
        }
        self.social.grants.retain(|grant| grant.structure_id != cabin.id);
        self.geography.structures.retain(|item| item.id != cabin.id);
        ops.push(PersistOp::DeleteStructure { id: cabin.id });
        let (event, note) = self.emit(
            Channel::System,
            Audience::Location(exterior),
            None,
            format!("{} demolishes the cabin \"{}\".", self.name_of(session_id)?, cabin.name),
        );
        ops.push(note);
        let _ = player;
        Ok(Transition { outcome: CommandOutcome { events: vec![event], quit: false }, ops })
    }

    fn exterior_of(&self, cabin: &Structure) -> Result<String, GameError> {
        let region = self
            .geography
            .region_by_id(cabin.region_id)
            .ok_or_else(|| GameError::Rejected("Cabin region is missing.".into()))?;
        Ok(WildernessPos {
            identity: region.identity,
            local_x: cabin.local_x,
            local_y: cabin.local_y,
        }
        .key())
    }

    pub(crate) fn cmd_sign(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let rest = cmd.rest.trim();
        let lower = rest.to_ascii_lowercase();
        if lower == "read" {
            return self.read_sign(session_id);
        }
        if lower == "remove" {
            return self.remove_sign(session_id);
        }
        let player = self.player(session_id)?;
        self.check_build_rate(player)?;
        let text = validate_sign_text(rest).map_err(|m| GameError::Rejected(m.into()))?;
        let location = self.location_id(session_id)?;
        let Some(pos) = WildernessPos::parse(&location) else {
            return Err(GameError::Rejected("You can only place signs in the outskirts.".into()));
        };
        if pos.is_gate() {
            return Err(GameError::Rejected("Keep the city gate clear of signs.".into()));
        }
        let tile = self.effective_at(pos)?;
        if !tile.traversable() {
            return Err(GameError::Rejected("You cannot plant a sign there.".into()));
        }
        let mut ops = self.ensure_region(pos.identity, Some(player), pos.local_x, pos.local_y)?;
        let region_id = self
            .geography
            .region(pos.identity.coord_key())
            .map(|region| region.id)
            .ok_or_else(|| GameError::Rejected("This region is not open.".into()))?;
        if let Some(existing) =
            self.geography.structure_of_kind_at(region_id, pos.local_x, pos.local_y, SIGN_KIND)
        {
            if existing.owner_id != Some(player) {
                return Err(GameError::Rejected("Someone else's sign is already here.".into()));
            }
            // Replace own sign text.
            let id = existing.id;
            if let Some(item) = self.geography.structures.iter_mut().find(|item| item.id == id) {
                item.name = text.clone();
            }
            ops.push(PersistOp::UpdateStructure {
                id,
                name: text.clone(),
                access: ACCESS_PUBLIC.into(),
                interior_place: None,
            });
        } else {
            let id = Uuid::new_v4();
            let created_at = self.now_ms;
            self.geography.structures.push(Structure {
                id,
                region_id,
                local_x: pos.local_x,
                local_y: pos.local_y,
                kind: SIGN_KIND.into(),
                name: text.clone(),
                owner_id: Some(player),
                access: ACCESS_PUBLIC.into(),
                interior_place: None,
                created_at,
            });
            ops.push(PersistOp::Structure {
                id,
                region_id,
                local_x: pos.local_x,
                local_y: pos.local_y,
                kind: SIGN_KIND.into(),
                name: text.clone(),
                owner_id: Some(player),
                access: ACCESS_PUBLIC.into(),
                interior_place: None,
                created_at,
            });
        }
        self.social.last_build_at.insert(player, self.now_ms);
        let (event, note) = self.emit(
            Channel::System,
            Audience::Location(location),
            None,
            format!("{} posts a sign.", self.name_of(session_id)?),
        );
        ops.push(note);
        Ok(Transition { outcome: CommandOutcome { events: vec![event], quit: false }, ops })
    }

    fn read_sign(&mut self, session_id: SessionId) -> Result<Transition, GameError> {
        let location = self.location_id(session_id)?;
        let player = self.player(session_id)?;
        let Some(pos) = WildernessPos::parse(&location) else {
            return Err(GameError::Rejected("There is no sign here.".into()));
        };
        let region_id = self
            .geography
            .region(pos.identity.coord_key())
            .map(|region| region.id)
            .ok_or_else(|| GameError::Rejected("There is no sign here.".into()))?;
        let Some(sign) =
            self.geography.structure_of_kind_at(region_id, pos.local_x, pos.local_y, SIGN_KIND)
        else {
            return Err(GameError::Rejected("There is no sign here.".into()));
        };
        let owner = self.owner_name(sign.owner_id);
        let (event, op) = self.emit(
            Channel::System,
            Audience::Player(player),
            None,
            format!("Sign by {owner}: \"{}\"", sign.name),
        );
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![op],
        })
    }

    fn remove_sign(&mut self, session_id: SessionId) -> Result<Transition, GameError> {
        let location = self.location_id(session_id)?;
        let player = self.player(session_id)?;
        let Some(pos) = WildernessPos::parse(&location) else {
            return Err(GameError::Rejected("There is no sign here.".into()));
        };
        let region_id = self
            .geography
            .region(pos.identity.coord_key())
            .map(|region| region.id)
            .ok_or_else(|| GameError::Rejected("There is no sign here.".into()))?;
        let Some(sign) = self
            .geography
            .structure_of_kind_at(region_id, pos.local_x, pos.local_y, SIGN_KIND)
            .cloned()
        else {
            return Err(GameError::Rejected("There is no sign here.".into()));
        };
        if sign.owner_id != Some(player) {
            return Err(GameError::Rejected("Only the owner can remove this sign.".into()));
        }
        self.geography.structures.retain(|item| item.id != sign.id);
        let (event, note) = self.emit(
            Channel::System,
            Audience::Location(location),
            None,
            format!("{} removes a sign.", self.name_of(session_id)?),
        );
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![PersistOp::DeleteStructure { id: sign.id }, note],
        })
    }

    pub(crate) fn cmd_trail(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        self.check_build_rate(player)?;
        let raw = cmd.args.first().map(String::as_str).unwrap_or("");
        let name = validate_marker_name(raw).map_err(|m| GameError::Rejected(m.into()))?;
        let location = self.location_id(session_id)?;
        let Some(pos) = WildernessPos::parse(&location) else {
            return Err(GameError::Rejected("Trails belong in the outskirts.".into()));
        };
        if pos.is_gate() {
            return Err(GameError::Rejected("Keep the city gate clear.".into()));
        }
        let tile = self.effective_at(pos)?;
        if !tile.traversable() {
            return Err(GameError::Rejected(
                "You cannot mark a trail on impassable ground.".into(),
            ));
        }
        let mut ops = self.ensure_region(pos.identity, Some(player), pos.local_x, pos.local_y)?;
        let region_id = self
            .geography
            .region(pos.identity.coord_key())
            .map(|region| region.id)
            .ok_or_else(|| GameError::Rejected("This region is not open.".into()))?;
        if self
            .geography
            .structure_of_kind_at(region_id, pos.local_x, pos.local_y, TRAIL_KIND)
            .is_some()
        {
            return Err(GameError::Rejected("A trail post is already here.".into()));
        }
        let id = Uuid::new_v4();
        let created_at = self.now_ms;
        self.geography.structures.push(Structure {
            id,
            region_id,
            local_x: pos.local_x,
            local_y: pos.local_y,
            kind: TRAIL_KIND.into(),
            name: name.clone(),
            owner_id: Some(player),
            access: ACCESS_PUBLIC.into(),
            interior_place: None,
            created_at,
        });
        ops.push(PersistOp::Structure {
            id,
            region_id,
            local_x: pos.local_x,
            local_y: pos.local_y,
            kind: TRAIL_KIND.into(),
            name: name.clone(),
            owner_id: Some(player),
            access: ACCESS_PUBLIC.into(),
            interior_place: None,
            created_at,
        });
        self.social.last_build_at.insert(player, self.now_ms);
        let (event, note) = self.emit(
            Channel::System,
            Audience::Location(location),
            None,
            format!("{} sets a trail post named {name}.", self.name_of(session_id)?),
        );
        ops.push(note);
        Ok(Transition { outcome: CommandOutcome { events: vec![event], quit: false }, ops })
    }

    pub(crate) fn cmd_board(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let location = self.location_id(session_id)?;
        if location != STARTER_LOCATION {
            return Err(GameError::Rejected(
                "The noticeboard is in the Coffee Shop. Go there to use it.".into(),
            ));
        }
        let rest = cmd.rest.trim();
        if rest.is_empty() || rest.eq_ignore_ascii_case("read") {
            return self.board_read(session_id);
        }
        let mut parts = rest.splitn(2, char::is_whitespace);
        let action = parts.next().unwrap_or("").to_ascii_lowercase();
        let tail = parts.next().unwrap_or("").trim();
        match action.as_str() {
            "post" => self.board_post(session_id, tail),
            "remove" => self.board_remove(session_id, tail),
            _ => Err(GameError::Rejected("usage: board [post <text>|remove <id>]".into())),
        }
    }

    fn board_read(&mut self, session_id: SessionId) -> Result<Transition, GameError> {
        let _ = self.social.prune_notices(self.now_ms);
        let posts = self.social.active_notices(STARTER_LOCATION, self.now_ms);
        let mut lines = vec!["Coffee Shop noticeboard:".to_string()];
        if posts.is_empty() {
            lines.push("(empty — try board post <text>)".into());
        } else {
            for post in posts.into_iter().take(NOTICE_PLACE_CAP) {
                let author = self
                    .operators
                    .get(&post.author_id)
                    .map(|op| op.name.clone())
                    .unwrap_or_else(|| "someone".into());
                let short = post.id.to_string();
                let id8 = &short[..8.min(short.len())];
                lines.push(format!("[{id8}] {author}: {}", post.body));
            }
        }
        self.set_overlay(session_id, lines.join("\n"));
        Ok(quiet())
    }

    fn board_post(&mut self, session_id: SessionId, raw: &str) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        if self.use_system_clock {
            if let Some(last) = self.social.last_notice_at.get(&player).copied() {
                if self.now_ms.saturating_sub(last) < NOTICE_RATE_MS {
                    return Err(GameError::Rejected(
                        "Wait a moment before posting another notice.".into(),
                    ));
                }
            }
        }
        let body = validate_notice_text(raw).map_err(|m| GameError::Rejected(m.into()))?;
        let mut ops = Vec::new();
        for expired in self.social.prune_notices(self.now_ms) {
            ops.push(PersistOp::NoticeDelete { id: expired });
        }
        let active = self.social.active_notices(STARTER_LOCATION, self.now_ms);
        if active.len() >= NOTICE_PLACE_CAP {
            // Drop oldest retained post for this place.
            if let Some(oldest) = active.last() {
                let id = oldest.id;
                self.social.notices.retain(|post| post.id != id);
                ops.push(PersistOp::NoticeDelete { id });
            }
        }
        let id = Uuid::new_v4();
        let created_at = self.now_ms;
        let expires_at = created_at.saturating_add(NOTICE_RETENTION_MS);
        let post = NoticePost {
            id,
            place_id: STARTER_LOCATION.into(),
            author_id: player,
            body: body.clone(),
            created_at,
            expires_at,
        };
        self.social.notices.push(post);
        self.social.last_notice_at.insert(player, created_at);
        ops.push(PersistOp::NoticeUpsert {
            id,
            place_id: STARTER_LOCATION.into(),
            author_id: player,
            body,
            created_at,
            expires_at,
        });
        let short = id.to_string();
        let id8 = &short[..8.min(short.len())];
        let (event, note) = self.emit(
            Channel::System,
            Audience::Location(STARTER_LOCATION.into()),
            None,
            format!("{} pins a notice [{id8}].", self.name_of(session_id)?),
        );
        ops.push(note);
        Ok(Transition { outcome: CommandOutcome { events: vec![event], quit: false }, ops })
    }

    fn board_remove(&mut self, session_id: SessionId, raw: &str) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let key = raw.trim().to_ascii_lowercase();
        if key.is_empty() {
            return Err(GameError::Rejected("usage: board remove <id>".into()));
        }
        let Some(post) = self.social.notices.iter().find(|post| {
            post.place_id == STARTER_LOCATION && post.id.to_string().starts_with(&key)
        }) else {
            return Err(GameError::Rejected("No notice matches that id.".into()));
        };
        if post.author_id != player {
            return Err(GameError::Rejected("You can only remove your own notices.".into()));
        }
        let id = post.id;
        self.social.notices.retain(|post| post.id != id);
        let (event, note) =
            self.emit(Channel::System, Audience::Player(player), None, "Notice removed.");
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![PersistOp::NoticeDelete { id }, note],
        })
    }

    fn check_build_rate(&self, player: PlayerId) -> Result<(), GameError> {
        if !self.use_system_clock {
            return Ok(());
        }
        if let Some(last) = self.social.last_build_at.get(&player).copied() {
            if self.now_ms.saturating_sub(last) < BUILD_RATE_MS {
                return Err(GameError::Rejected(
                    "Give the land a moment before building again.".into(),
                ));
            }
        }
        Ok(())
    }
}
