//! Inspect and wilderness mark commands.

use terminus_core::{
    Audience, Channel, CommandOutcome, GameError, GameEvent, ParsedCommand, PlayerId, SessionId,
};
use uuid::Uuid;

use crate::geography::{
    closure_inspect_text, corroboration_inspect_line, is_survey_tile, landmark_sight,
    panel_after_log, stacks_after_log, story_aliases, story_finding, story_sight, survey_aliases,
    survey_sight, torn_page_text, trace_aliases, validate_marker_name, EffectiveTile,
    GenerateError, LandmarkKind, Structure, Terrain, UrbanContext, WildernessPos,
    DELISTED_FLOOR_ID, MARKER_KIND, SHARE_PRIVATE, SURVEY_CLOSURE_ID, SURVEY_MARK_FLAG,
    SURVEY_MARK_ID, SURVEY_SCRATCH_ID,
};
use crate::model::{Item, PersistOp};
use crate::state::{Transition, WorldState};
use crate::subway::{self, MetroEntrance};
use crate::undernet::{ObjectDef, READ_TERMINAL_FLAG};

impl WorldState {
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

    fn with_corroboration(&self, text: String, content_key: Option<&str>) -> String {
        let Some(key) = content_key else {
            return text;
        };
        let count = self.geography.field_note_recorder_count(key);
        let Some(line) = corroboration_inspect_line(count) else {
            return text;
        };
        format!("{text}\n{line}")
    }
    pub(crate) fn cmd_inspect(
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
        let location = self.location_id(session_id)?;
        let saw_mark =
            self.operators.get(&player).is_some_and(|op| op.flags.contains(SURVEY_MARK_FLAG));
        let content_key =
            static_inspect_content_key(&location, object.keyword, read_terminal, saw_mark);
        text = self.with_corroboration(text, content_key);
        let (event, op) = self.inspect_note(session_id, player, text);
        ops.push(op);
        Ok(Transition { outcome: CommandOutcome { events: vec![event], quit: false }, ops })
    }

    pub(crate) fn cmd_mark(
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
        let mut text = if let Some(entrance) =
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
        let tile_for_key = self.effective_at(pos).ok();
        let content_key = wild_inspect_content_key(pos, target, tile_for_key.as_ref(), &urban);
        text = self.with_corroboration(text, content_key.as_deref());
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
fn metro_inspect_target(target: &str) -> bool {
    matches!(target, "stair" | "metro" | "entrance" | "subway" | "stairs")
}

fn wild_inspect_content_key(
    pos: WildernessPos,
    target: &str,
    tile: Option<&EffectiveTile>,
    urban: &UrbanContext,
) -> Option<String> {
    if is_survey_tile(pos) && survey_aliases(target) {
        return Some(SURVEY_MARK_ID.to_string());
    }
    if let Some(site) = urban.story.filter(|site| story_aliases(*site, target)) {
        return Some(story_finding(site).content_key);
    }
    let tile = tile?;
    if let Some(kind) = tile.landmark {
        if target.eq_ignore_ascii_case(kind.keyword()) || target.eq_ignore_ascii_case(kind.name()) {
            return landmark_sight(pos, kind).finding.map(|finding| finding.content_key);
        }
    }
    None
}

fn static_inspect_content_key(
    location: &str,
    object_keyword: &str,
    read_terminal: bool,
    saw_survey_mark: bool,
) -> Option<&'static str> {
    match (location, object_keyword) {
        ("roof-array", "antenna") => Some(SURVEY_SCRATCH_ID),
        ("old-relay", "dish") if saw_survey_mark => Some(SURVEY_CLOSURE_ID),
        (
            "service-corridor" | "abandoned-exchange" | "black-archive",
            "panel" | "terminal" | "stacks",
        ) if read_terminal => Some(DELISTED_FLOOR_ID),
        _ => None,
    }
}

fn metro_entrance_inspect(entrance: &MetroEntrance) -> String {
    format!(
        "Municipal subway stair. The landing is dry dust over painted concrete. \
A faded plate names {}. `go {}` descends to the platform. \
The stair is a fixed entrance — not a random shaft.",
        entrance.station_label, entrance.down_keyword
    )
}
