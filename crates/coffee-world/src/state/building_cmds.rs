//! Homestead, workshops, signs, trails, and Coffee Shop noticeboard commands.

use terminus_core::{
    Audience, Channel, CommandOutcome, GameError, ParsedCommand, PlayerId, SessionId,
};
use uuid::Uuid;

use crate::geography::{
    count_operator_structures_in_region, reject_permanent_trace_reason, validate_marker_name,
    Structure, WildernessPos, ACCESS_FRIENDS, ACCESS_PRIVATE, ACCESS_PUBLIC, CABIN_KIND,
    MAX_SIGNS_PER_OPERATOR_REGION, MAX_TRAILS_PER_OPERATOR_REGION, SHARE_COMMUNAL, SHARE_PRIVATE,
    SHARE_SHARED, SIGN_KIND, TRAIL_KIND, WORKSHOP_KIND,
};
use crate::model::{PersistOp, STARTER_LOCATION};
use crate::settlement::workshop_place_id;
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
        match kind.as_str() {
            "cabin" if !name_raw.is_empty() => {
                self.build_enterable(session_id, CABIN_KIND, name_raw, ACCESS_PUBLIC)
            }
            "workshop" if !name_raw.is_empty() => {
                // Workshops start private; owners opt in with `home share`.
                self.build_enterable(session_id, WORKSHOP_KIND, name_raw, ACCESS_PRIVATE)
            }
            _ => Err(GameError::Rejected("usage: build <cabin|workshop> <name>".into())),
        }
    }

    fn build_enterable(
        &mut self,
        session_id: SessionId,
        kind: &str,
        name_raw: &str,
        default_access: &str,
    ) -> Result<Transition, GameError> {
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
        if self.geography.structure_of_kind_at(region_id, pos.local_x, pos.local_y, kind).is_some()
        {
            let label = if kind == WORKSHOP_KIND { "workshop" } else { "cabin" };
            return Err(GameError::Rejected(format!("There is already a {label} here.")));
        }
        // Name collision among enterables on this tile would break go/enter keywords.
        if self
            .geography
            .structures_at(region_id, pos.local_x, pos.local_y)
            .into_iter()
            .any(|item| item.is_enterable() && item.name.eq_ignore_ascii_case(&name))
        {
            return Err(GameError::Rejected(
                "An enterable structure already uses that name on this tile.".into(),
            ));
        }
        let id = Uuid::new_v4();
        let interior = Structure::interior_key(id);
        let created_at = self.now_ms;
        let settlement_id = self.settlements.settlement_of(player).map(|s| s.id);
        let structure = Structure {
            id,
            region_id,
            local_x: pos.local_x,
            local_y: pos.local_y,
            kind: kind.into(),
            name: name.clone(),
            owner_id: Some(player),
            access: default_access.into(),
            interior_place: Some(interior.clone()),
            settlement_id,
            share: SHARE_PRIVATE.into(),
            created_at,
            expires_at: None,
        };
        self.geography.structures.push(structure.clone());
        ops.push(PersistOp::Structure {
            id,
            region_id,
            local_x: pos.local_x,
            local_y: pos.local_y,
            kind: kind.into(),
            name: name.clone(),
            owner_id: Some(player),
            access: default_access.into(),
            interior_place: Some(interior.clone()),
            settlement_id,
            share: SHARE_PRIVATE.into(),
            created_at,
            expires_at: None,
        });
        let enter_kw = name.to_ascii_lowercase();
        let enter_op = self.link_places(&location, &enter_kw, &interior, &name, player)?;
        ops.push(enter_op);
        let out_op = self.link_places(&interior, "out", &location, "outside", player)?;
        ops.push(out_op);
        self.social.last_build_at.insert(player, self.now_ms);
        let verb = if kind == WORKSHOP_KIND { "raises a workshop" } else { "raises a cabin" };
        let (event, note) = self.emit(
            Channel::System,
            Audience::Location(location),
            None,
            format!("{} {verb} named {name}.", self.name_of(session_id)?),
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
            return Err(GameError::Rejected("There is no entrance here.".into()));
        };
        let region_id = self
            .geography
            .region(pos.identity.coord_key())
            .map(|region| region.id)
            .ok_or_else(|| GameError::Rejected("There is no entrance here.".into()))?;
        let Some(structure) = self
            .geography
            .structures_at(region_id, pos.local_x, pos.local_y)
            .into_iter()
            .find(|item| item.is_enterable() && item.name.eq_ignore_ascii_case(raw))
            .cloned()
        else {
            return Err(GameError::Rejected(format!("No cabin or workshop named '{raw}' here.")));
        };
        self.ensure_structure_access_for_move(player, &structure)?;
        let keyword = structure.name.to_ascii_lowercase();
        self.move_through(session_id, &keyword)
    }

    pub(crate) fn cmd_leave(
        &mut self,
        session_id: SessionId,
        _cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let location = self.location_id(session_id)?;
        if self.geography.structure_by_interior(&location).is_none() {
            return Err(GameError::Rejected("You are not inside a cabin or workshop.".into()));
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
        if rest.is_empty() {
            return self.home_status(session_id);
        }
        let mut parts = rest.splitn(2, char::is_whitespace);
        let action = parts.next().unwrap_or("").to_ascii_lowercase();
        let tail = parts.next().unwrap_or("").trim();
        match action.as_str() {
            "help" => return self.home_help(session_id),
            "claim" => {
                if !tail.is_empty() {
                    return Err(GameError::Rejected("usage: home claim".into()));
                }
                return self.home_claim(session_id);
            }
            "move" => {
                let confirm = match tail {
                    "" => false,
                    "confirm" => true,
                    _ => {
                        return Err(GameError::Rejected(
                            "Moving abandons your current home. Type: home move confirm".into(),
                        ));
                    }
                };
                return self.home_move(session_id, confirm);
            }
            "abandon" => {
                let confirm = match tail {
                    "" => false,
                    "confirm" => true,
                    _ => {
                        return Err(GameError::Rejected(
                            "Abandoning leaves you without a home. Type: home abandon confirm"
                                .into(),
                        ));
                    }
                };
                return self.home_abandon(session_id, confirm);
            }
            "name" => {
                if tail.is_empty() {
                    return Err(GameError::Rejected(
                        "usage: home name <name> | home name clear".into(),
                    ));
                }
                return self.home_name(session_id, tail);
            }
            _ => {}
        }
        let player = self.player(session_id)?;
        if action == "inspect" {
            return self.home_inspect(session_id, player);
        }
        let cabin = self.structure_under_control(session_id, player)?;
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
            "share" => {
                let mode = tail.to_ascii_lowercase();
                let share = match mode.as_str() {
                    "private" => SHARE_PRIVATE,
                    "shared" => SHARE_SHARED,
                    "communal" => SHARE_COMMUNAL,
                    _ => {
                        return Err(GameError::Rejected(
                            "usage: home share <private|shared|communal>".into(),
                        ));
                    }
                };
                self.update_structure_share(player, cabin, share)
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
            _ => Err(GameError::Rejected(crate::state::home_cmds::home_usage().into())),
        }
    }

    pub(crate) fn cmd_workshop(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let rest = cmd.rest.trim();
        let mut parts = rest.splitn(2, char::is_whitespace);
        let action = parts.next().unwrap_or("").to_ascii_lowercase();
        let tail = parts.next().unwrap_or("").trim();
        match action.as_str() {
            "board" | "" => {
                self.workshop_board(session_id, if action.is_empty() { rest } else { tail })
            }
            "inspect" => {
                let player = self.player(session_id)?;
                self.home_inspect(session_id, player)
            }
            _ => Err(GameError::Rejected(
                "usage: workshop <inspect|board [post <text>|remove <id>]>".into(),
            )),
        }
    }

    fn home_inspect(
        &mut self,
        session_id: SessionId,
        player: PlayerId,
    ) -> Result<Transition, GameError> {
        let structure = self.structure_here_or_inside(session_id)?;
        let owner = self.owner_name(structure.owner_id);
        let settle = structure
            .settlement_id
            .and_then(|id| self.settlements.by_id(id))
            .map(|s| s.name.clone())
            .unwrap_or_else(|| "(none)".into());
        let kind = if structure.kind == WORKSHOP_KIND { "workshop" } else { "cabin" };
        let mut lines = vec![
            format!("{} \"{}\"", kind, structure.name),
            format!("Owner: {owner}"),
            format!("Visitor access: {}", structure.access),
            format!("Settlement share: {}", structure.share),
            format!("Settlement link: {settle}"),
        ];
        if structure.owner_id == Some(player) {
            lines.push(
                "Owner controls: home access … · home share <private|shared|communal>".into(),
            );
        }
        if structure.is_workshop() {
            lines.push("Workshop board: workshop board [post <text>]".into());
        }
        self.set_overlay(session_id, lines.join("\n"));
        Ok(quiet())
    }

    fn update_structure_share(
        &mut self,
        player: PlayerId,
        structure: Structure,
        share: &str,
    ) -> Result<Transition, GameError> {
        if structure.owner_id != Some(player) {
            return Err(GameError::Rejected("Only the owner can change sharing.".into()));
        }
        if matches!(share, SHARE_SHARED | SHARE_COMMUNAL) && structure.settlement_id.is_none() {
            return Err(GameError::Rejected(
                "Link this structure to a settlement first (settle claim), then set sharing."
                    .into(),
            ));
        }
        if matches!(share, SHARE_SHARED | SHARE_COMMUNAL) {
            let Some(sid) = structure.settlement_id else {
                return Err(GameError::Rejected("Settlement link missing.".into()));
            };
            if self.settlements.by_id(sid).is_none() {
                return Err(GameError::Rejected(
                    "That settlement no longer exists. Sharing stays private.".into(),
                ));
            }
            if !self.settlements.is_member(sid, player) {
                return Err(GameError::Rejected(
                    "You must belong to the linked settlement to share with it.".into(),
                ));
            }
        }
        let (name, access, interior, new_share) = {
            let Some(item) =
                self.geography.structures.iter_mut().find(|item| item.id == structure.id)
            else {
                return Err(GameError::Rejected("Structure not found.".into()));
            };
            item.share = share.to_string();
            (
                item.name.clone(),
                item.access.clone(),
                item.interior_place.clone(),
                item.share.clone(),
            )
        };
        let (event, note) = self.emit(
            Channel::System,
            Audience::Player(player),
            None,
            format!("\"{name}\" settlement share is now {new_share}."),
        );
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![
                PersistOp::UpdateStructure {
                    id: structure.id,
                    name,
                    access,
                    interior_place: interior,
                    share: new_share,
                },
                note,
            ],
        })
    }

    fn structure_under_control(
        &self,
        session_id: SessionId,
        player: PlayerId,
    ) -> Result<Structure, GameError> {
        let structure = self.structure_here_or_inside(session_id)?;
        if structure.owner_id != Some(player) {
            return Err(GameError::Rejected(
                "Only the owner can manage this cabin or workshop.".into(),
            ));
        }
        Ok(structure)
    }

    fn structure_here_or_inside(&self, session_id: SessionId) -> Result<Structure, GameError> {
        let location = self.location_id(session_id)?;
        if let Some(structure) = self.geography.structure_by_interior(&location).cloned() {
            return Ok(structure);
        }
        let Some(pos) = WildernessPos::parse(&location) else {
            return Err(GameError::Rejected(
                "Stand at your cabin or workshop, or inside it.".into(),
            ));
        };
        let region_id =
            self.geography.region(pos.identity.coord_key()).map(|region| region.id).ok_or_else(
                || GameError::Rejected("Stand at your cabin or workshop, or inside it.".into()),
            )?;
        let structures = self.geography.structures_at(region_id, pos.local_x, pos.local_y);
        let enterable: Vec<&Structure> =
            structures.into_iter().filter(|item| item.is_enterable()).collect();
        match enterable.as_slice() {
            [one] => Ok((*one).clone()),
            [] => Err(GameError::Rejected("There is no cabin or workshop here.".into())),
            many => {
                // Prefer a workshop when several enterables share the tile.
                if let Some(workshop) = many.iter().find(|item| item.kind == WORKSHOP_KIND) {
                    return Ok((*workshop).clone());
                }
                Ok(many[0].clone())
            }
        }
    }

    fn update_cabin_access(
        &mut self,
        player: PlayerId,
        cabin_id: Uuid,
        access: &str,
    ) -> Result<Transition, GameError> {
        let (name, interior, share) = {
            let Some(cabin) = self.geography.structures.iter_mut().find(|item| item.id == cabin_id)
            else {
                return Err(GameError::Rejected("Cabin not found.".into()));
            };
            cabin.access = access.to_string();
            (cabin.name.clone(), cabin.interior_place.clone(), cabin.share.clone())
        };
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
                    share,
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
            share: cabin.share,
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
        if let Some(reason) = reject_permanent_trace_reason(pos) {
            return Err(GameError::Rejected(reason.into()));
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
        let sign_count = count_operator_structures_in_region(
            &self.geography.structures,
            region_id,
            player,
            SIGN_KIND,
            self.now_ms,
        );
        if sign_count >= MAX_SIGNS_PER_OPERATOR_REGION
            && self
                .geography
                .structure_of_kind_at(region_id, pos.local_x, pos.local_y, SIGN_KIND)
                .is_none()
        {
            return Err(GameError::Rejected(
                "You already have enough signs in this region. Remove one or move farther out."
                    .into(),
            ));
        }
        if let Some(existing) =
            self.geography.structure_of_kind_at(region_id, pos.local_x, pos.local_y, SIGN_KIND)
        {
            if existing.owner_id != Some(player) {
                return Err(GameError::Rejected("Someone else's sign is already here.".into()));
            }
            // Replace own sign text.
            let id = existing.id;
            let share = existing.share.clone();
            if let Some(item) = self.geography.structures.iter_mut().find(|item| item.id == id) {
                item.name = text.clone();
            }
            ops.push(PersistOp::UpdateStructure {
                id,
                name: text.clone(),
                access: ACCESS_PUBLIC.into(),
                interior_place: None,
                share,
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
                settlement_id: None,
                share: SHARE_PRIVATE.into(),
                created_at,
                expires_at: None,
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
                settlement_id: None,
                share: SHARE_PRIVATE.into(),
                created_at,
                expires_at: None,
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
        if let Some(reason) = reject_permanent_trace_reason(pos) {
            return Err(GameError::Rejected(reason.into()));
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
        let trail_count = count_operator_structures_in_region(
            &self.geography.structures,
            region_id,
            player,
            TRAIL_KIND,
            self.now_ms,
        );
        if trail_count >= MAX_TRAILS_PER_OPERATOR_REGION {
            return Err(GameError::Rejected(
                "You already have a trail post in this region. Remove it before setting another."
                    .into(),
            ));
        }
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
            settlement_id: None,
            share: SHARE_PRIVATE.into(),
            created_at,
            expires_at: None,
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
            settlement_id: None,
            share: SHARE_PRIVATE.into(),
            created_at,
            expires_at: None,
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

    fn workshop_board(
        &mut self,
        session_id: SessionId,
        raw: &str,
    ) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let structure = self.structure_here_or_inside(session_id)?;
        if !structure.is_workshop() && structure.kind != WORKSHOP_KIND {
            return Err(GameError::Rejected(
                "Stand at a workshop (or a cabin marked communal) to use its board.".into(),
            ));
        }
        // Entering requires the same rules as the door; reading the board from outside does too.
        self.ensure_structure_access_for_move(player, &structure)?;
        let place_id = workshop_place_id(structure.id);
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("read") {
            return self.workshop_board_read(session_id, &structure, &place_id);
        }
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let action = parts.next().unwrap_or("").to_ascii_lowercase();
        let tail = parts.next().unwrap_or("").trim();
        match action.as_str() {
            "post" => self.workshop_board_post(session_id, player, &structure, &place_id, tail),
            "remove" => self.workshop_board_remove(player, &structure, &place_id, tail),
            _ => Err(GameError::Rejected("usage: workshop board [post <text>|remove <id>]".into())),
        }
    }

    fn workshop_board_read(
        &mut self,
        session_id: SessionId,
        structure: &Structure,
        place_id: &str,
    ) -> Result<Transition, GameError> {
        let _ = self.social.prune_notices(self.now_ms);
        let posts = self.social.active_notices(place_id, self.now_ms);
        let mut lines = vec![format!("Workshop \"{}\" project board:", structure.name)];
        lines.push(format!("Share: {} · Access: {}", structure.share, structure.access));
        if posts.is_empty() {
            lines.push("(empty — try workshop board post <text>)".into());
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

    fn workshop_board_post(
        &mut self,
        _session_id: SessionId,
        player: PlayerId,
        structure: &Structure,
        place_id: &str,
        raw: &str,
    ) -> Result<Transition, GameError> {
        // Poster must currently be allowed to use the workshop (membership checked live).
        self.ensure_structure_access_for_move(player, structure)?;
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
        let active = self.social.active_notices(place_id, self.now_ms);
        if active.len() >= NOTICE_PLACE_CAP {
            if let Some(oldest) = active.last() {
                let id = oldest.id;
                self.social.notices.retain(|post| post.id != id);
                ops.push(PersistOp::NoticeDelete { id });
            }
        }
        let id = Uuid::new_v4();
        let created_at = self.now_ms;
        let expires_at = created_at.saturating_add(NOTICE_RETENTION_MS);
        self.social.notices.push(NoticePost {
            id,
            place_id: place_id.into(),
            author_id: player,
            body: body.clone(),
            created_at,
            expires_at,
        });
        self.social.last_notice_at.insert(player, created_at);
        ops.push(PersistOp::NoticeUpsert {
            id,
            place_id: place_id.into(),
            author_id: player,
            body,
            created_at,
            expires_at,
        });
        let short = id.to_string();
        let id8 = &short[..8.min(short.len())];
        let (event, note) = self.emit(
            Channel::System,
            Audience::Player(player),
            None,
            format!("You pin a project note [{id8}] on \"{}\".", structure.name),
        );
        ops.push(note);
        Ok(Transition { outcome: CommandOutcome { events: vec![event], quit: false }, ops })
    }

    fn workshop_board_remove(
        &mut self,
        player: PlayerId,
        structure: &Structure,
        place_id: &str,
        raw: &str,
    ) -> Result<Transition, GameError> {
        let key = raw.trim().to_ascii_lowercase();
        if key.is_empty() {
            return Err(GameError::Rejected("usage: workshop board remove <id>".into()));
        }
        let Some(post) = self
            .social
            .notices
            .iter()
            .find(|post| post.place_id == place_id && post.id.to_string().starts_with(&key))
            .cloned()
        else {
            return Err(GameError::Rejected("No notice matches that id.".into()));
        };
        let allowed = post.author_id == player || structure.owner_id == Some(player);
        if !allowed {
            return Err(GameError::Rejected(
                "Only the author or workshop owner can remove that notice.".into(),
            ));
        }
        self.social.notices.retain(|row| row.id != post.id);
        let (event, note) =
            self.emit(Channel::System, Audience::Player(player), None, "Notice removed.");
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![PersistOp::NoticeDelete { id: post.id }, note],
        })
    }

    pub(crate) fn check_build_rate(&self, player: PlayerId) -> Result<(), GameError> {
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
