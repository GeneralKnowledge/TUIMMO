//! Movement, travel resolution, and exit listing.

use std::collections::BTreeSet;

use terminus_core::{
    Audience, Channel, CommandOutcome, ExitView, GameError, ParsedCommand, PlayerId, SessionId,
};

use crate::geography::{
    effective_tile, epoch_spec, frontier_epoch, frontier_identity, step_origin, Cardinal,
    RegionCoordKey, RegionIdentity, Structure, Terrain, Tile, WildernessPos, WorldConnection,
    CITY_KEYWORD, OUTSKIRTS_FROM, OUTSKIRTS_KEYWORD, WORKSHOP_KIND,
};
use crate::model::PersistOp;
use crate::state::{Transition, WorldState};
use crate::subway;
use crate::threads::USED_SUBWAY_FLAG;

const SQUALOR_ALIAS: &str = "squalor";

struct TravelTarget {
    to: String,
    from_label: String,
    to_label: String,
    open: Option<(RegionIdentity, u8, u8)>,
}

impl WorldState {
    pub(crate) fn cmd_go(
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
            if subway::is_station(&target.to) && op.flags.insert(USED_SUBWAY_FLAG.to_string()) {
                ops.push(PersistOp::Flag {
                    operator_id: player,
                    flag: USED_SUBWAY_FLAG.to_string(),
                });
            }
        }
        let home_travel = self.note_home_travel(player, &from, &target.to);
        ops.extend(home_travel.ops);
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
        let destination = target.to.clone();
        ops.push(PersistOp::Move {
            operator_id: player,
            location_id: destination.clone(),
            last_seen: self.now_ms,
        });
        ops.extend([leave_op, arrive_op, note_op]);
        let mut events = vec![leave, arrive, note];
        if let Some(text) = home_travel.owner_message {
            let (home_note, home_op) =
                self.emit(Channel::System, Audience::Player(player), None, text);
            ops.push(home_op);
            events.push(home_note);
        }
        let mut transition = Transition { outcome: CommandOutcome { events, quit: false }, ops };
        self.attach_home_claim_hint(session_id, &destination, &mut transition);
        Ok(transition)
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
            .map_err(|err| GameError::Rejected(crate::state::unreadable_wilderness(&err)))?;
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

    pub(crate) fn list_exits(&self, location: &str, flags: &BTreeSet<String>) -> Vec<ExitView> {
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
}

pub(crate) fn is_squalor_keyword(key: &str) -> bool {
    key == OUTSKIRTS_KEYWORD || key == SQUALOR_ALIAS
}
