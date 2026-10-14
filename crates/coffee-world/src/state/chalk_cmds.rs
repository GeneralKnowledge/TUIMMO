//! Ephemeral chalk marks and personal trace listing.

use terminus_core::{
    Audience, Channel, CommandOutcome, GameError, ParsedCommand, SessionId,
};
use uuid::Uuid;

use crate::geography::{
    chalk_ttl_ms, structure_is_active, validate_chalk_text, WildernessPos, CHALK_KIND,
};
use crate::model::PersistOp;
use crate::state::{Transition, WorldState};

impl WorldState {
    pub(crate) fn cmd_chalk(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let rest = cmd.rest.trim();
        let lower = rest.to_ascii_lowercase();
        if lower == "refresh" {
            return self.refresh_chalk(session_id);
        }
        let player = self.player(session_id)?;
        self.check_build_rate(player)?;
        let text = validate_chalk_text(rest).map_err(|m| GameError::Rejected(m.into()))?;
        let location = self.location_id(session_id)?;
        let Some(pos) = WildernessPos::parse(&location) else {
            return Err(GameError::Rejected("Chalk only works in the Squalor.".into()));
        };
        if pos.is_gate() {
            return Err(GameError::Rejected("Keep the city gate clear.".into()));
        }
        let tile = self.effective_at(pos)?;
        if !tile.traversable() {
            return Err(GameError::Rejected("You cannot mark chalk there.".into()));
        }
        let mut ops = self.ensure_region(pos.identity, Some(player), pos.local_x, pos.local_y)?;
        let region_id = self
            .geography
            .region(pos.identity.coord_key())
            .map(|region| region.id)
            .ok_or_else(|| GameError::Rejected("This region is not open.".into()))?;
        let expires_at = self.now_ms.saturating_add(chalk_ttl_ms(pos));
        if let Some(existing) = self
            .geography
            .structure_of_kind_at(region_id, pos.local_x, pos.local_y, CHALK_KIND)
            .cloned()
        {
            if existing.owner_id != Some(player) {
                return Err(GameError::Rejected("Someone else's chalk is already here.".into()));
            }
            let id = existing.id;
            let access = existing.access.clone();
            let share = existing.share.clone();
            if let Some(item) = self.geography.structures.iter_mut().find(|item| item.id == id) {
                item.name = text.clone();
                item.created_at = self.now_ms;
                item.expires_at = Some(expires_at);
            }
            ops.push(PersistOp::UpdateStructure {
                id,
                name: text.clone(),
                access: access.clone(),
                interior_place: None,
                share: share.clone(),
            });
            ops.push(PersistOp::PatchStructureExpiry { id, expires_at });
        } else {
            let id = Uuid::new_v4();
            let created_at = self.now_ms;
            self.geography.structures.push(crate::geography::Structure {
                id,
                region_id,
                local_x: pos.local_x,
                local_y: pos.local_y,
                kind: CHALK_KIND.into(),
                name: text.clone(),
                owner_id: Some(player),
                access: crate::geography::ACCESS_PUBLIC.into(),
                interior_place: None,
                settlement_id: None,
                share: crate::geography::SHARE_PRIVATE.into(),
                created_at,
                expires_at: Some(expires_at),
            });
            ops.push(PersistOp::Structure {
                id,
                region_id,
                local_x: pos.local_x,
                local_y: pos.local_y,
                kind: CHALK_KIND.into(),
                name: text.clone(),
                owner_id: Some(player),
                access: crate::geography::ACCESS_PUBLIC.into(),
                interior_place: None,
                settlement_id: None,
                share: crate::geography::SHARE_PRIVATE.into(),
                created_at,
                expires_at: Some(expires_at),
            });
        }
        self.social.last_build_at.insert(player, self.now_ms);
        let (event, note) = self.emit(
            Channel::System,
            Audience::Location(location),
            None,
            format!("{} leaves chalk.", self.name_of(session_id)?),
        );
        ops.push(note);
        Ok(Transition { outcome: CommandOutcome { events: vec![event], quit: false }, ops })
    }

    fn refresh_chalk(&mut self, session_id: SessionId) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let location = self.location_id(session_id)?;
        let Some(pos) = WildernessPos::parse(&location) else {
            return Err(GameError::Rejected("There is no chalk here.".into()));
        };
        let region_id = self
            .geography
            .region(pos.identity.coord_key())
            .map(|region| region.id)
            .ok_or_else(|| GameError::Rejected("There is no chalk here.".into()))?;
        let Some(chalk) = self
            .geography
            .structure_of_kind_at(region_id, pos.local_x, pos.local_y, CHALK_KIND)
            .cloned()
        else {
            return Err(GameError::Rejected("There is no chalk here.".into()));
        };
        if chalk.owner_id != Some(player) {
            return Err(GameError::Rejected("Only the owner can refresh this chalk.".into()));
        }
        let expires_at = self.now_ms.saturating_add(chalk_ttl_ms(pos));
        if let Some(item) = self.geography.structures.iter_mut().find(|item| item.id == chalk.id) {
            item.expires_at = Some(expires_at);
            item.created_at = self.now_ms;
        }
        let (event, op) = self.emit(
            Channel::System,
            Audience::Player(player),
            None,
            "You refresh the chalk. It should hold a while longer.".to_string(),
        );
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![PersistOp::PatchStructureExpiry { id: chalk.id, expires_at }, op],
        })
    }

    pub(crate) fn cmd_traces(
        &mut self,
        session_id: SessionId,
        _cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let location = self.location_id(session_id)?;
        let Some(pos) = WildernessPos::parse(&location) else {
            return Err(GameError::Rejected("Traces only list your marks in this region.".into()));
        };
        let Some(region) = self.geography.region(pos.identity.coord_key()) else {
            return Err(GameError::Rejected("This region is not open yet.".into()));
        };
        let mut lines: Vec<String> = self
            .geography
            .structures
            .iter()
            .filter(|item| {
                item.region_id == region.id
                    && item.owner_id == Some(player)
                    && matches!(item.kind.as_str(), CHALK_KIND | crate::geography::SIGN_KIND | crate::geography::TRAIL_KIND)
                    && structure_is_active(item, self.now_ms)
            })
            .map(|item| {
                format!(
                    "({}, {}) {} — \"{}\"",
                    item.local_x,
                    item.local_y,
                    item.kind,
                    item.name
                )
            })
            .collect();
        lines.sort();
        let body = if lines.is_empty() {
            "You have no active signs, trail posts, or chalk in this region.".into()
        } else {
            format!("Your active traces here:\n{}", lines.join("\n"))
        };
        let (event, op) = self.emit(
            Channel::System,
            Audience::Player(player),
            None,
            body,
        );
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![op],
        })
    }

    pub(crate) fn purge_expired_chalk(&mut self) -> Vec<PersistOp> {
        let now = self.now_ms;
        let expired: Vec<Uuid> = self
            .geography
            .structures
            .iter()
            .filter(|item| item.kind == CHALK_KIND && !structure_is_active(item, now))
            .map(|item| item.id)
            .collect();
        if expired.is_empty() {
            return Vec::new();
        }
        self.geography.structures.retain(|item| !expired.contains(&item.id));
        expired
            .into_iter()
            .map(|id| PersistOp::DeleteStructure { id })
            .collect()
    }
}
