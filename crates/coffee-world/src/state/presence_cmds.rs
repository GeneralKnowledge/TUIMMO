//! Look, map, who, and GameView assembly.

use std::collections::BTreeSet;

use terminus_core::{
    ConnectionState, ExitView, GameError, GameView, ItemView, ObjectView, ParsedCommand, PlayerId,
    PresenceView, SessionId,
};

use crate::geography::{
    describe_tile, is_survey_tile, map_symbol_for_structures, marker_sights, survey_look_line,
    GenerateError, Structure, WildernessPos, WORKSHOP_KIND,
};
use crate::state::{quiet, Transition, WorldState};
use crate::subway::{self};
use crate::undernet::{self};

impl WorldState {
    pub(crate) fn cmd_look(
        &mut self,
        session_id: SessionId,
        _cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        self.clear_overlay(session_id);
        Ok(quiet())
    }

    pub(crate) fn cmd_map(
        &mut self,
        session_id: SessionId,
        _cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let location = self.location_id(session_id)?;
        if let Some(pos) = WildernessPos::parse(&location) {
            let text = match self.wild_scene(pos, Some(player)) {
                Ok(scene) => scene.map,
                Err(err) => crate::state::unreadable_wilderness(&err),
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

    pub(crate) fn cmd_who(
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

    pub(crate) fn operator_flags(&self, player: PlayerId) -> BTreeSet<String> {
        self.operators.get(&player).map(|op| op.flags.clone()).unwrap_or_default()
    }
    pub(crate) fn wild_scene(
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
        for (keyword, label) in self.thread_look_objects(&place_key, Some(pos)) {
            scene.objects.push((keyword, label));
        }
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
                    crate::state::unreadable_wilderness(&err),
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

    pub(crate) fn names_in(&self, location: &str) -> Vec<String> {
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

    pub(crate) fn visible_exits(&self, session_id: SessionId) -> Result<Vec<ExitView>, GameError> {
        let player = self.player(session_id)?;
        let location = self.location_id(session_id)?;
        let flags = self.operator_flags(player);
        Ok(self.list_exits(&location, &flags))
    }

    pub(crate) fn status_lines(&self, session_id: SessionId) -> Result<Vec<String>, GameError> {
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

    pub(crate) fn build_view(&self, session_id: SessionId) -> Result<GameView, GameError> {
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
        let mut objects: Vec<ObjectView> = loc
            .objects
            .iter()
            .map(|object| ObjectView {
                keyword: object.keyword.to_string(),
                name: object.name.to_string(),
            })
            .collect();
        for (keyword, label) in self.thread_look_objects(&op.location_id, None) {
            objects.push(ObjectView { keyword, name: label });
        }
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
