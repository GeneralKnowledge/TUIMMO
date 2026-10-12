//! Personal waypoints. The name is the operator's. The place key is the world's.
//!
//! Showing a waypoint does not materialize a region, publish the note, or move anyone.

use terminus_core::{
    Audience, Channel, CommandOutcome, GameError, ParsedCommand, PlayerId, SessionId,
};
use uuid::Uuid;

use crate::geography::{
    cross_region_hint, describe_place, distant_cell_hint, epoch_spec, local_path, Cardinal,
    LocalPath, Structure, WildernessPos, GATE_LOCAL_X, GATE_LOCAL_Y,
};
use crate::model::PersistOp;
use crate::state::{quiet, Transition, WorldState};
use crate::undernet as rooms;
use crate::waypoint::{clean_note, parse_name, Waypoint, LIST_LIMIT};

const USAGE: &str = "usage: waypoint [add|list|show|note|remove]";

const HELP: &str = "\
Waypoints
A private mark for a place you have stood. Not a map, and not a way to travel.

waypoint
waypoint add <name>
waypoint list
waypoint show <name>
waypoint note <name> <text>
waypoint remove <name>

Examples:
waypoint add marked-stone
waypoint note old-relay Dish points toward the city gate.

Names are 2-24 characters: a letter, then letters, digits, or hyphens.
Matching ignores case. The list shows the name as you typed it.
Adding a name you already use does not move the mark. waypoint remove <name> first.

Notes stay on your waypoint. They are not posted to chat, signs, or boards.
A waypoint is not a journal entry. Removing one does not delete the other.

Directions are hints. They never move you. Another region is named, not walked.
If a saved place no longer resolves, the record stays and is not pointed somewhere else.

`look` returns to the place.";

impl WorldState {
    pub(crate) fn cmd_waypoint(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let rest = cmd.rest.trim();
        if rest.is_empty() {
            self.set_overlay(session_id, HELP);
            return Ok(quiet());
        }
        let (action, tail) = split_action(rest);
        match action.to_ascii_lowercase().as_str() {
            "add" => self.waypoint_add(session_id, tail),
            "list" => {
                if !tail.is_empty() {
                    return Err(GameError::Rejected(USAGE.into()));
                }
                self.waypoint_list(session_id)
            }
            "show" => self.waypoint_show(session_id, one_name(tail)?),
            "remove" => self.waypoint_remove(session_id, one_name(tail)?),
            "note" => self.waypoint_note(session_id, tail),
            _ => Err(GameError::Rejected(USAGE.into())),
        }
    }

    fn waypoint_add(&mut self, session_id: SessionId, tail: &str) -> Result<Transition, GameError> {
        self.require_play(session_id)?;
        let name = one_name(tail)?;
        let parsed = parse_name(name).map_err(|message| GameError::Rejected(message.into()))?;
        let player = self.player(session_id)?;
        if let Some(existing) = self.owned_waypoint(player, &parsed.key) {
            let display = existing.display_name.clone();
            return Err(GameError::Rejected(format!(
                "You already have a waypoint named {display}. `waypoint remove {display}` first. Adding it again does not move it."
            )));
        }
        let place_key = self.location_id(session_id)?;
        let summary = self.destination_summary(&place_key);
        let now = self.now_ms;
        let row = Waypoint {
            id: Uuid::new_v4(),
            operator_id: player,
            name_key: parsed.key,
            display_name: parsed.display.clone(),
            place_key: place_key.clone(),
            note: String::new(),
            created_at: now,
            updated_at: now,
        };
        let op = PersistOp::WaypointInsert {
            id: row.id,
            operator_id: row.operator_id,
            name_key: row.name_key.clone(),
            display_name: row.display_name.clone(),
            place_key: row.place_key.clone(),
            note: String::new(),
            created_at: row.created_at,
            updated_at: row.updated_at,
        };
        self.waypoints.push(row);
        self.private_line(
            player,
            format!("Saved waypoint {} at {summary}.", parsed.display),
            vec![op],
        )
    }

    fn waypoint_list(&mut self, session_id: SessionId) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let from = self.location_id(session_id)?;
        let rows = self.waypoints_for(player);
        let text = if rows.is_empty() {
            "You have no waypoints.\n\
             `waypoint add <name>` saves where you are standing.\n\
             `look` returns to the place."
                .to_string()
        } else {
            let total = rows.len();
            let mut lines = vec![format!("Waypoints ({total})")];
            for row in rows.iter().take(LIST_LIMIT) {
                let view = self.dest_view(&from, &row.place_key);
                lines.push(format!("{}  {}  {}", row.display_name, view.title, view.locality));
                if !row.note.is_empty() {
                    lines.push(format!("  {}", clip_chars(&row.note, 48)));
                }
            }
            if total > LIST_LIMIT {
                lines.push(format!("Showing {LIST_LIMIT} of {total}."));
            }
            lines.push("`waypoint show <name>`".into());
            lines.push("`look` returns to the place.".into());
            lines.join("\n")
        };
        self.set_overlay(session_id, text);
        Ok(quiet())
    }

    fn waypoint_show(
        &mut self,
        session_id: SessionId,
        name: &str,
    ) -> Result<Transition, GameError> {
        let parsed = parse_name(name).map_err(|message| GameError::Rejected(message.into()))?;
        let player = self.player(session_id)?;
        let Some(row) = self.owned_waypoint(player, &parsed.key).cloned() else {
            return Err(GameError::Rejected(format!("No waypoint matches '{name}'.")));
        };
        let from = self.location_id(session_id)?;
        let view = self.dest_view(&from, &row.place_key);
        let journal = self.journal_keys_for_place(player, &row.place_key);
        let journal_line = if journal.is_empty() { "none".to_string() } else { journal.join(", ") };
        let note = if row.note.is_empty() { "none".to_string() } else { row.note };
        let text = format!(
            "Waypoint: {}\n\
             Destination: {}\n\
             Where: {}\n\
             Place: {}\n\
             Note: {note}\n\
             Journal: {journal_line}\n\
             Direction: {}\n\
             `look` returns to the place.",
            row.display_name, view.title, view.locality, row.place_key, view.hint
        );
        self.set_overlay(session_id, text);
        Ok(quiet())
    }

    fn waypoint_note(
        &mut self,
        session_id: SessionId,
        tail: &str,
    ) -> Result<Transition, GameError> {
        self.require_play(session_id)?;
        let Some((name, text)) = tail.split_once(char::is_whitespace) else {
            return Err(GameError::Rejected(
                "Which waypoint, and what note? `waypoint note <name> <text>`.".into(),
            ));
        };
        let parsed = parse_name(name).map_err(|message| GameError::Rejected(message.into()))?;
        let note = clean_note(text).map_err(|message| GameError::Rejected(message.into()))?;
        let player = self.player(session_id)?;
        let Some(index) = self.owned_index(player, &parsed.key) else {
            return Err(GameError::Rejected(format!("No waypoint matches '{name}'.")));
        };
        let now = self.now_ms;
        let id = self.waypoints[index].id;
        self.waypoints[index].note = note.clone();
        self.waypoints[index].updated_at = now;
        let display = self.waypoints[index].display_name.clone();
        let op = PersistOp::WaypointNote { id, operator_id: player, note, updated_at: now };
        self.private_line(player, format!("Noted waypoint {display}."), vec![op])
    }

    fn waypoint_remove(
        &mut self,
        session_id: SessionId,
        name: &str,
    ) -> Result<Transition, GameError> {
        self.require_play(session_id)?;
        let parsed = parse_name(name).map_err(|message| GameError::Rejected(message.into()))?;
        let player = self.player(session_id)?;
        let Some(index) = self.owned_index(player, &parsed.key) else {
            return Err(GameError::Rejected(format!("No waypoint matches '{name}'.")));
        };
        let row = self.waypoints.remove(index);
        let op = PersistOp::WaypointDelete { id: row.id, operator_id: player };
        self.private_line(player, format!("Removed waypoint {}.", row.display_name), vec![op])
    }

    fn require_play(&self, session_id: SessionId) -> Result<(), GameError> {
        if self.permissions(session_id)?.can_play() {
            Ok(())
        } else {
            Err(GameError::Rejected("You cannot do that from this session.".into()))
        }
    }

    fn private_line(
        &mut self,
        player: PlayerId,
        text: String,
        mut ops: Vec<PersistOp>,
    ) -> Result<Transition, GameError> {
        let (event, op) = self.emit(Channel::System, Audience::Player(player), None, text);
        ops.push(op);
        Ok(Transition { outcome: CommandOutcome { events: vec![event], quit: false }, ops })
    }

    fn owned_waypoint(&self, operator: PlayerId, name_key: &str) -> Option<&Waypoint> {
        self.waypoints.iter().find(|item| item.operator_id == operator && item.name_key == name_key)
    }

    fn owned_index(&self, operator: PlayerId, name_key: &str) -> Option<usize> {
        self.waypoints
            .iter()
            .position(|item| item.operator_id == operator && item.name_key == name_key)
    }

    fn waypoints_for(&self, operator: PlayerId) -> Vec<&Waypoint> {
        let mut rows: Vec<&Waypoint> =
            self.waypoints.iter().filter(|item| item.operator_id == operator).collect();
        rows.sort_by(|left, right| {
            left.name_key.cmp(&right.name_key).then_with(|| left.id.cmp(&right.id))
        });
        rows
    }

    fn journal_keys_for_place(&self, operator: PlayerId, place_key: &str) -> Vec<String> {
        let mut keys: Vec<String> = self
            .geography
            .field_notes_for(operator)
            .into_iter()
            .filter(|note| note.place_key.as_deref() == Some(place_key))
            .filter_map(|note| note.content_key.clone())
            .collect();
        keys.sort();
        keys.dedup();
        keys
    }

    fn destination_summary(&self, place_key: &str) -> String {
        let view = self.dest_view(place_key, place_key);
        if view.kept {
            format!("{} ({})", view.title, view.locality)
        } else if view.locality == "Undernet" || view.locality == "interior" {
            view.title
        } else {
            format!("{}, {}", view.title, view.locality)
        }
    }

    fn dest_view(&self, from: &str, place_key: &str) -> DestView {
        if let Some(pos) = WildernessPos::parse(place_key) {
            return self.wild_dest(from, pos);
        }
        if let Some(structure) = self.geography.structure_by_interior(place_key) {
            let title = format!("{} (interior)", structure.name);
            let hint = self.toward_interior(from, place_key);
            let locality = self
                .geography
                .region_by_id(structure.region_id)
                .map(|region| {
                    format!(
                        "door birth ({}, {}), tile {},{}",
                        region.identity.origin_x,
                        region.identity.origin_y,
                        structure.local_x,
                        structure.local_y
                    )
                })
                .unwrap_or_else(|| "interior".to_string());
            return DestView { title, locality, hint, kept: false };
        }
        if rooms::location(place_key).is_some() {
            let (title, _) = describe_place(place_key);
            return DestView {
                title,
                locality: "Undernet".into(),
                hint: self.named_hint(from, place_key),
                kept: false,
            };
        }
        if Structure::parse_interior_key(place_key).is_some() {
            return DestView {
                title: "unavailable".into(),
                locality: "kept".into(),
                hint: "This interior is no longer here. The record was kept and not moved.".into(),
                kept: true,
            };
        }
        DestView {
            title: "unavailable".into(),
            locality: "kept".into(),
            hint: "That place is not on the current map. The record was kept and not moved.".into(),
            kept: true,
        }
    }

    fn wild_dest(&self, from: &str, pos: WildernessPos) -> DestView {
        let locality = format!(
            "birth ({}, {}), tile {},{}",
            pos.identity.origin_x, pos.identity.origin_y, pos.local_x, pos.local_y
        );
        if let Err(reason) = wild_is_fresh(&self.geography, pos) {
            return DestView {
                title: "unavailable".into(),
                locality: "kept".into(),
                hint: reason,
                kept: true,
            };
        }
        let title = self.place_title(&pos.key());
        if title == "Unreadable wilderness" {
            return DestView {
                title: "unavailable".into(),
                locality: "kept".into(),
                hint: "This place cannot be read. The record was kept and not moved.".into(),
                kept: true,
            };
        }
        DestView { title, locality, hint: self.wild_hint(from, pos), kept: false }
    }

    fn wild_hint(&self, from: &str, dest: WildernessPos) -> String {
        if let Some(src) = WildernessPos::parse(from) {
            if src == dest {
                return "You are at this waypoint.".into();
            }
            if src.identity == dest.identity {
                return self.direct_hint(src, dest);
            }
            return cross_region_hint(src, dest);
        }
        self.named_to_wild(dest)
    }

    fn named_hint(&self, from: &str, dest: &str) -> String {
        if from == dest {
            return "You are at this waypoint.".into();
        }
        if let Some(src) = WildernessPos::parse(from) {
            return self.wild_to_named(src, dest);
        }
        describe_place(dest).1
    }

    fn named_to_wild(&self, dest: WildernessPos) -> String {
        if dest.identity != crate::geography::gate_identity() {
            return distant_cell_hint(dest);
        }
        let path = self.path_in_cell(
            crate::geography::gate_identity(),
            (GATE_LOCAL_X, GATE_LOCAL_Y),
            (dest.local_x, dest.local_y),
        );
        match path {
            Some(LocalPath::Here) => {
                "From the Roof Array, go outskirts. That tile is the city gate.".into()
            }
            Some(LocalPath::Blocked) => {
                "From the Roof Array, go outskirts. No open path from the city gate stays inside this birth cell. Water or rock lies between the gate and that tile.".into()
            }
            Some(LocalPath::Open(steps)) => format!(
                "From the Roof Array, go outskirts. From the city gate: {}.",
                step_words(&steps)
            ),
            None => "The outskirts cannot be read. The record was kept and not moved.".into(),
        }
    }

    fn wild_to_named(&self, src: WildernessPos, dest: &str) -> String {
        let hint = describe_place(dest).1;
        if src.identity != crate::geography::gate_identity() {
            return format!(
                "Return to birth (0, 0), tile {GATE_LOCAL_X},{GATE_LOCAL_Y}, then city. This is not a route across regions. {hint}"
            );
        }
        let back = self.direct_hint(
            src,
            WildernessPos { identity: src.identity, local_x: GATE_LOCAL_X, local_y: GATE_LOCAL_Y },
        );
        if dest == "roof-array" {
            return format!("{back} city returns to the Roof Array.");
        }
        format!("{back} Then city. {hint}")
    }

    fn toward_interior(&self, from: &str, place_key: &str) -> String {
        if from == place_key {
            return "You are at this waypoint.".into();
        }
        let Some(structure) = self.geography.structure_by_interior(place_key) else {
            return "This interior is no longer here. The record was kept and not moved.".into();
        };
        let local_x = structure.local_x;
        let local_y = structure.local_y;
        let Some(region) = self.geography.region_by_id(structure.region_id) else {
            return "The door's region is not open. The record was kept and not moved.".into();
        };
        let door = WildernessPos { identity: region.identity, local_x, local_y };
        let Some(src) = WildernessPos::parse(from) else {
            return format!(
                "The door is birth ({}, {}), tile {},{}. This is not a route.",
                door.identity.origin_x, door.identity.origin_y, local_x, local_y
            );
        };
        if src.identity != door.identity {
            return format!(
                "The door is birth ({}, {}), tile {},{}. This is not a route.",
                door.identity.origin_x, door.identity.origin_y, local_x, local_y
            );
        }
        match self.direct_hint(src, door).as_str() {
            "You are at this waypoint." => "You are at the door. Then enter.".into(),
            other => format!("{other} Then enter."),
        }
    }

    fn direct_hint(&self, src: WildernessPos, dest: WildernessPos) -> String {
        if src == dest {
            return "You are at this waypoint.".into();
        }
        match self.path_in_cell(src.identity, (src.local_x, src.local_y), (dest.local_x, dest.local_y))
        {
            Some(LocalPath::Here) => "You are at this waypoint.".into(),
            Some(LocalPath::Blocked) => format!(
                "No open path stays inside this birth cell. Water or rock lies between you and tile {},{}.",
                dest.local_x, dest.local_y
            ),
            Some(LocalPath::Open(steps)) => head_steps(&steps),
            None => "This place cannot be read. The record was kept and not moved.".into(),
        }
    }

    fn path_in_cell(
        &self,
        identity: crate::geography::RegionIdentity,
        from: (u8, u8),
        to: (u8, u8),
    ) -> Option<LocalPath> {
        let generated = self.generated(identity).ok()?;
        Some(local_path(&generated, &self.geography, identity, from, to))
    }
}

struct DestView {
    title: String,
    locality: String,
    hint: String,
    kept: bool,
}

fn wild_is_fresh(geo: &crate::geography::GeographyState, pos: WildernessPos) -> Result<(), String> {
    if let Some(existing) = geo.region(pos.identity.coord_key()) {
        if existing.identity != pos.identity {
            return Err(
                "This birth cell is recorded under a different generation version. The record was kept and not moved."
                    .into(),
            );
        }
        return Ok(());
    }
    match epoch_spec(pos.identity.epoch_id) {
        None => Err(format!(
            "Epoch {} is not open. The record was kept and not moved.",
            pos.identity.epoch_id
        )),
        Some(epoch) if epoch.generation_version != pos.identity.generation_version => {
            Err("That generation version is not open. The record was kept and not moved.".into())
        }
        Some(_) => Ok(()),
    }
}

fn split_action(rest: &str) -> (&str, &str) {
    match rest.split_once(char::is_whitespace) {
        Some((action, tail)) => (action, tail.trim()),
        None => (rest, ""),
    }
}

fn one_name(tail: &str) -> Result<&str, GameError> {
    let tail = tail.trim();
    if tail.is_empty() {
        return Err(GameError::Rejected("Which name?".into()));
    }
    if tail.split_whitespace().nth(1).is_some() {
        return Err(GameError::Rejected(
            "Waypoint names are one word. Use letters, digits, and hyphens.".into(),
        ));
    }
    Ok(tail)
}

fn step_words(steps: &[Cardinal]) -> String {
    if steps.len() <= 8 {
        return steps.iter().map(|step| step.keyword()).collect::<Vec<_>>().join(", ");
    }
    format!("{}, about {} open steps", steps[0].keyword(), steps.len())
}

fn head_steps(steps: &[Cardinal]) -> String {
    if steps.len() == 1 {
        return format!("Head {}.", steps[0].keyword());
    }
    if steps.len() <= 8 {
        let words = steps.iter().map(|step| step.keyword()).collect::<Vec<_>>().join(", ");
        return format!("Head {words}.");
    }
    format!("Head {}. About {} open steps inside this birth cell.", steps[0].keyword(), steps.len())
}

fn clip_chars(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if out.chars().count() >= max_chars {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use terminus_core::{Audience, Game, Permissions, SessionIdentity};
    use uuid::Uuid;

    use super::{parse_name, *};
    use crate::geography::{
        frontier_identity, gate_identity, gate_position, generate_region, survey_position, Terrain,
        WorldId, SURVEY_CLOSURE_ID, SURVEY_MARK_ID, SURVEY_SCRATCH_ID,
    };
    use crate::state::starter_operator;
    use crate::waypoint::Waypoint;

    fn pair() -> (WorldState, SessionId, SessionId, PlayerId, PlayerId) {
        let mut world = WorldState::for_tests();
        let ada_id = PlayerId::new();
        let bob_id = PlayerId::new();
        world.insert_operator(starter_operator(ada_id, Uuid::new_v4(), "ada", "ada"));
        world.insert_operator(starter_operator(bob_id, Uuid::new_v4(), "bob", "bob"));
        let ada = SessionIdentity::new(ada_id, "ada", Permissions::player());
        let bob = SessionIdentity::new(bob_id, "bob", Permissions::player());
        let ada_session = ada.session_id;
        let bob_session = bob.session_id;
        world.connect_player(ada).unwrap();
        world.connect_player(bob).unwrap();
        (world, ada_session, bob_session, ada_id, bob_id)
    }

    fn overlay(world: &WorldState, session: SessionId) -> String {
        world.view(session).unwrap().overlay.unwrap_or_default()
    }

    fn feed(world: &WorldState, session: SessionId) -> String {
        world
            .view(session)
            .unwrap()
            .feed
            .iter()
            .map(|line| line.text.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn plant(world: &mut WorldState, operator: PlayerId, name: &str, place_key: &str, note: &str) {
        let parsed = parse_name(name).unwrap();
        world.waypoints.push(Waypoint {
            id: Uuid::new_v4(),
            operator_id: operator,
            name_key: parsed.key,
            display_name: parsed.display,
            place_key: place_key.to_string(),
            note: note.to_string(),
            created_at: 1,
            updated_at: 1,
        });
    }

    #[test]
    fn help_empty_list_and_name_rules() {
        let (mut world, ada, _, _, _) = pair();
        world.apply(ada, "waypoint").unwrap();
        let help = overlay(&world, ada);
        assert!(help.contains("waypoint add"));
        assert!(help.contains("does not move"));
        assert!(help.contains("look` returns"));
        world.apply(ada, "waypoint list").unwrap();
        assert!(overlay(&world, ada).contains("no waypoints"));
        assert!(world.apply(ada, "look").unwrap().ops.is_empty());
        assert!(world.view(ada).unwrap().overlay.is_none());
        assert_eq!(world.view(ada).unwrap().location_name, "The Coffee Shop");

        let missing = world.apply(ada, "waypoint show nowhere").unwrap_err();
        assert!(missing.to_string().contains("No waypoint matches"));
        assert!(world.apply(ada, "waypoint add").is_err());
        assert!(world
            .apply(ada, "waypoint add a")
            .unwrap_err()
            .to_string()
            .contains("2 characters"));
        assert!(world
            .apply(ada, "waypoint add old relay")
            .unwrap_err()
            .to_string()
            .contains("one word"));
        assert!(world
            .apply(ada, "waypoint add old_relay")
            .unwrap_err()
            .to_string()
            .contains("hyphens"));
        let long = "a".repeat(25);
        assert!(world.apply(ada, &format!("waypoint add {long}")).is_err());

        world.apply(ada, "waypoint add Shop-Door").unwrap();
        assert!(feed(&world, ada).contains("Saved waypoint Shop-Door"));
        assert!(feed(&world, ada).contains("Coffee Shop"));
        world.apply(ada, "go up").unwrap();
        let again = world.apply(ada, "waypoint add shop-door").unwrap_err();
        assert!(again.to_string().contains("does not move"));
        world.apply(ada, "waypoint show SHOP-DOOR").unwrap();
        let page = overlay(&world, ada);
        assert!(page.contains("Waypoint: Shop-Door"));
        assert!(page.contains("Destination: The Coffee Shop"));
        assert!(page.contains("Journal: none"));
        assert!(page.contains("Note: none"));
        assert!(!page.contains("Roof Array\n"));
        assert_eq!(world.waypoints.len(), 1);
        assert_eq!(world.waypoints[0].place_key, "coffee-shop");
    }

    #[test]
    fn notes_are_private_and_do_not_move_the_mark() {
        let (mut world, ada, bob, ada_id, bob_id) = pair();
        world.apply(ada, "waypoint add old-relay").unwrap();
        world.operators.get_mut(&ada_id).unwrap().location_id = "roof-array".into();
        let noted = world.apply(ada, "waypoint note old-relay Dish points toward the city gate.");
        assert!(noted.is_ok());
        assert!(feed(&world, ada).contains("Noted waypoint old-relay"));
        assert!(!feed(&world, ada).contains("Dish points"));
        world.apply(ada, "waypoint show old-relay").unwrap();
        let page = overlay(&world, ada);
        assert!(page.contains("Dish points toward the city gate."));
        assert!(page.contains("Place: coffee-shop"));
        assert!(world.log.iter().all(|entry| !entry.text.contains("Dish points")));
        assert!(world
            .log
            .iter()
            .filter(|entry| entry.text.contains("Noted"))
            .all(|entry| { matches!(entry.audience, Audience::Player(id) if id == ada_id) }));

        world.apply(bob, "waypoint list").unwrap();
        assert!(overlay(&world, bob).contains("no waypoints"));
        let hidden = world.apply(bob, "waypoint show old-relay").unwrap_err();
        assert!(hidden.to_string().contains("No waypoint matches"));
        let stolen = world.apply(bob, "waypoint note old-relay stolen").unwrap_err();
        assert!(stolen.to_string().contains("No waypoint matches"));
        assert!(!feed(&world, bob).contains("Dish points"));
        world.apply(bob, "waypoint add old-relay").unwrap();
        assert_eq!(world.waypoints.iter().filter(|row| row.name_key == "old-relay").count(), 2);
        assert_eq!(
            world.waypoints.iter().find(|row| row.operator_id == ada_id).unwrap().note,
            "Dish points toward the city gate."
        );
        assert!(world
            .waypoints
            .iter()
            .find(|row| row.operator_id == bob_id)
            .unwrap()
            .note
            .is_empty());
        assert!(world.apply(ada, "waypoint note old-relay").is_err());
        assert!(world.apply(ada, "waypoint note old-relay    ").is_err());
        let huge = "n".repeat(161);
        assert!(world.apply(ada, &format!("waypoint note old-relay {huge}")).is_err());
        assert_eq!(
            world.waypoints.iter().find(|row| row.operator_id == ada_id).unwrap().note,
            "Dish points toward the city gate."
        );
    }

    #[test]
    fn journal_entries_and_waypoints_have_separate_lifetimes() {
        let (mut world, ada, _, ada_id, _) = pair();
        world.apply(ada, "go up").unwrap();
        world.apply(ada, "inspect antenna").unwrap();
        world.apply(ada, "journal record").unwrap();
        assert!(world.waypoints.is_empty());
        world.apply(ada, "waypoint add roof-scratch").unwrap();
        assert_eq!(world.geography.field_notes_for(ada_id).len(), 1);
        world.apply(ada, "waypoint show roof-scratch").unwrap();
        assert!(overlay(&world, ada).contains(SURVEY_SCRATCH_ID));
        assert!(!overlay(&world, ada).contains("one step west of the gate"));
        world.apply(ada, "waypoint remove roof-scratch").unwrap();
        assert!(feed(&world, ada).contains("Removed waypoint roof-scratch"));
        assert!(world.waypoints.is_empty());
        assert_eq!(world.geography.field_notes_for(ada_id).len(), 1);
        world.apply(ada, "journal read survey-scratch").unwrap();
        assert!(overlay(&world, ada).contains("survey-scratch"));
        world.geography.discoveries.clear();
        world.apply(ada, "waypoint add roof-scratch").unwrap();
        world.apply(ada, "waypoint show roof-scratch").unwrap();
        assert!(overlay(&world, ada).contains("Journal: none"));
        assert_eq!(world.waypoints.len(), 1);
    }

    #[test]
    fn nearby_paths_are_directional_and_other_regions_are_not() {
        let (mut world, ada, _, ada_id, _) = pair();
        world.apply(ada, "go up").unwrap();
        world.apply(ada, "outskirts").unwrap();
        world.apply(ada, "waypoint add city-gap").unwrap();
        world.apply(ada, "west").unwrap();
        world.apply(ada, "waypoint show city-gap").unwrap();
        let back = overlay(&world, ada);
        assert!(back.contains("Head east."), "{back}");
        assert!(!back.contains("This is not a route."), "{back}");
        assert!(!back.contains("standing stone"), "{back}");
        world.apply(ada, "east").unwrap();
        world.apply(ada, "west").unwrap();
        world.apply(ada, "waypoint add marked-stone").unwrap();
        world.apply(ada, "east").unwrap();
        world.apply(ada, "waypoint show marked-stone").unwrap();
        let west = overlay(&world, ada);
        assert!(west.contains("Head west."), "{west}");
        assert!(!west.contains("Head north"), "{west}");
        assert!(!west.contains("archive"), "{west}");

        world.apply_terrain_mod(gate_identity(), 7, 8, Terrain::Water, "closed", ada_id).unwrap();
        world.apply(ada, "waypoint show marked-stone").unwrap();
        let blocked = overlay(&world, ada);
        assert!(blocked.contains("Water or rock"), "{blocked}");
        assert!(!blocked.contains("Head"), "{blocked}");
        assert_eq!(
            world.waypoints.iter().find(|row| row.name_key == "marked-stone").unwrap().place_key,
            survey_position().key()
        );

        let identity = frontier_identity(WorldId::Undernet, 1, 0);
        let region = generate_region(identity).unwrap();
        let mut spot = None;
        for local_y in 0..16u8 {
            for local_x in 0..16u8 {
                let tile = region.tile(local_x, local_y).unwrap();
                if tile.terrain.traversable() {
                    spot = Some((local_x, local_y));
                    break;
                }
            }
            if spot.is_some() {
                break;
            }
        }
        let (local_x, local_y) = spot.expect("a walkable tile");
        let far = WildernessPos { identity, local_x, local_y };
        world.operators.get_mut(&ada_id).unwrap().location_id = far.key();
        world.apply(ada, "waypoint add far-camp").unwrap();
        world.operators.get_mut(&ada_id).unwrap().location_id = gate_position().key();
        world.apply(ada, "waypoint show far-camp").unwrap();
        let far_page = overlay(&world, ada);
        assert!(far_page.contains("This is not a route."), "{far_page}");
        assert!(far_page.contains("one cell east"), "{far_page}");
        assert!(!far_page.contains("Head"), "{far_page}");
        assert!(!far_page.contains("standing stone"), "{far_page}");
        assert!(!far_page.contains("milepost"), "{far_page}");
        assert!(!far_page.contains("archive"), "{far_page}");
        let saved = world
            .waypoints
            .iter()
            .find(|row| row.name_key == "far-camp")
            .unwrap()
            .place_key
            .clone();
        assert_eq!(saved, far.key());
    }

    #[test]
    fn stale_records_stay_put() {
        let (mut world, ada, _, ada_id, _) = pair();
        let stale = "w:undernet:9:0:0:1:4:4";
        let mismatched = "w:undernet:1:0:0:9:8:8";
        plant(&mut world, ada_id, "old-epoch", stale, "keep me");
        plant(&mut world, ada_id, "wrong-version", mismatched, "");
        world.apply(ada, "waypoint show old-epoch").unwrap();
        let page = overlay(&world, ada);
        assert!(page.contains("Epoch 9 is not open"), "{page}");
        assert!(page.contains("not moved"), "{page}");
        assert!(page.contains("keep me"), "{page}");
        assert!(!page.contains("Head"), "{page}");
        assert!(!page.contains("The Outskirts"), "{page}");
        world.apply(ada, "waypoint show wrong-version").unwrap();
        let version = overlay(&world, ada);
        assert!(version.contains("not moved"), "{version}");
        assert!(!version.contains("Head"), "{version}");
        assert!(!version.contains("The Outskirts"), "{version}");
        assert_eq!(world.waypoints[0].place_key, stale);
        assert_eq!(world.waypoints[1].place_key, mismatched);
        world.apply(ada, "waypoint list").unwrap();
        let list = overlay(&world, ada);
        assert!(list.contains("unavailable"), "{list}");
        assert!(list.contains("old-epoch"), "{list}");
        assert!(list.contains("keep me"), "{list}");
    }

    #[test]
    fn lists_are_ordered_and_capped_and_spectators_cannot_write() {
        let (mut world, ada, _, ada_id, _) = pair();
        for index in 0..17 {
            let name = format!("n{index:02}");
            world.apply(ada, &format!("waypoint add {name}")).unwrap();
        }
        world.apply(ada, "waypoint list").unwrap();
        let list = overlay(&world, ada);
        assert!(list.contains("Waypoints (17)"), "{list}");
        assert!(list.contains("Showing 16 of 17."), "{list}");
        assert!(list.contains("n00"), "{list}");
        assert!(!list.contains("n16"), "{list}");
        let spectator = SessionIdentity::new(ada_id, "ada", Permissions::spectator());
        let watching = spectator.session_id;
        world.connect_player(spectator).unwrap();
        world.apply(watching, "waypoint list").unwrap();
        assert!(overlay(&world, watching).contains("n00"));
        let denied = world.apply(watching, "waypoint add extra-name").unwrap_err();
        assert!(denied.to_string().contains("cannot do that"));
        assert_eq!(world.waypoints.len(), 17);
    }

    #[test]
    fn survey_marks_can_be_waypoints_without_sharing_notes() {
        let (mut world, ada, bob, ada_id, bob_id) = pair();
        for step in ["go up", "inspect antenna", "journal record", "outskirts", "west"] {
            world.apply(ada, step).unwrap();
        }
        assert_eq!(world.location_key(ada).unwrap(), survey_position().key());
        world.apply(ada, "inspect mark").unwrap();
        world.apply(ada, "journal record").unwrap();
        world.apply(ada, "waypoint add marked-stone").unwrap();
        assert!(feed(&world, ada).contains("Saved waypoint marked-stone"));
        for step in ["east", "city", "down", "alley", "down", "relay"] {
            world.apply(ada, step).unwrap();
        }
        assert_eq!(world.location_key(ada).unwrap(), "old-relay");
        world.apply(ada, "inspect dish").unwrap();
        let dish = feed(&world, ada);
        assert!(dish.contains("never filed"));
        world.apply(ada, "journal record").unwrap();
        world.apply(ada, "waypoint add old-relay").unwrap();
        world.apply(ada, "waypoint note old-relay Dish points toward the city gate.").unwrap();
        for step in ["tunnels", "up", "shop"] {
            world.apply(ada, step).unwrap();
        }
        assert_eq!(world.view(ada).unwrap().location_name, "The Coffee Shop");
        world.apply(ada, "waypoint list").unwrap();
        let list = overlay(&world, ada);
        assert!(list.contains("marked-stone"), "{list}");
        assert!(list.contains("old-relay"), "{list}");
        assert!(list.contains("Dish points"), "{list}");
        world.apply(ada, "waypoint show marked-stone").unwrap();
        let stone = overlay(&world, ada);
        assert!(stone.contains("Waypoint: marked-stone"), "{stone}");
        assert!(stone.contains("survey-mark"), "{stone}");
        assert!(stone.contains("From the Roof Array, go outskirts"), "{stone}");
        assert!(stone.contains("west"), "{stone}");
        assert!(!stone.contains("relay insulation"), "{stone}");
        assert!(!stone.contains("never filed"), "{stone}");
        world.apply(ada, "waypoint show old-relay").unwrap();
        let relay = overlay(&world, ada);
        assert!(relay.contains("Destination: The Old Relay"), "{relay}");
        assert!(relay.contains("survey-closure"), "{relay}");
        assert!(relay.contains("go relay"), "{relay}");
        assert!(relay.contains("Dish points toward the city gate."), "{relay}");
        assert!(!relay.contains("never filed"), "{relay}");
        world.apply(ada, "journal").unwrap();
        let journal = overlay(&world, ada);
        assert!(journal.contains(SURVEY_SCRATCH_ID), "{journal}");
        assert!(journal.contains(SURVEY_MARK_ID), "{journal}");
        assert!(journal.contains(SURVEY_CLOSURE_ID), "{journal}");
        assert!(journal.contains("Field journal (3)"), "{journal}");

        world.detach(ada).unwrap();
        let again = SessionIdentity::new(ada_id, "ada", Permissions::player());
        let returned = again.session_id;
        world.connect_player(again).unwrap();
        world.apply(returned, "waypoint show old-relay").unwrap();
        assert!(overlay(&world, returned).contains("Dish points toward the city gate."));
        world.apply(returned, "waypoint list").unwrap();
        assert!(overlay(&world, returned).contains("marked-stone"));
        assert_eq!(world.geography.field_notes_for(ada_id).len(), 3);

        world.apply(bob, "go up").unwrap();
        world.apply(bob, "outskirts").unwrap();
        world.apply(bob, "west").unwrap();
        let shared = world.view(bob).unwrap();
        assert!(shared.description.contains("short mark"));
        world.apply(bob, "waypoint list").unwrap();
        assert!(overlay(&world, bob).contains("no waypoints"));
        let hidden = world.apply(bob, "waypoint show old-relay").unwrap_err();
        assert!(hidden.to_string().contains("No waypoint matches"));
        assert!(!feed(&world, bob).contains("Dish points"));
        assert!(world.log.iter().all(|entry| !entry.text.contains("Dish points")));
        world.apply(bob, "journal").unwrap();
        assert!(overlay(&world, bob).contains("empty"));
        assert_eq!(world.geography.field_notes_for(bob_id).len(), 0);
        assert_eq!(world.geography.field_notes_for(ada_id).len(), 3);
    }
}
