//! Shared hub navigation (context-based, not profile-specific scripts).

use coffee_world::geography::{WildernessPos, OUTSKIRTS_FROM};
use coffee_world::model::STARTER_LOCATION;
use coffee_world::WorldState;
use rand::rngs::StdRng;
use rand::RngExt;
use terminus_core::Game;

use crate::agent::SimAgent;
use crate::runner::SimWorldExt;

#[derive(Clone, Debug)]
pub struct ActionContext {
    pub location: String,
    pub region_key: Option<String>,
    pub home_place_key: Option<String>,
    pub others_present: usize,
    pub first_other_present: Option<String>,
    pub agent_display_name: String,
    pub cardinal_exits: Vec<String>,
}

pub fn build_context(
    world: &WorldState,
    agent: &SimAgent,
) -> Result<ActionContext, terminus_core::GameError> {
    let location = world.location_id_for_sim(agent.session_id)?;
    let region_key = wilderness_region_key(&location);
    let home_place_key = world.active_home_place_key(agent.player_id);
    let view = world.view(agent.session_id)?;
    let others_present = view.present.len();
    let first_other_present = view.present.first().map(|p| p.name.clone());
    let cardinal_exits = cardinal_exits_from_view(&view.exits);
    Ok(ActionContext {
        location,
        region_key,
        home_place_key,
        others_present,
        first_other_present,
        agent_display_name: agent.display_name.clone(),
        cardinal_exits,
    })
}

pub fn in_wilderness(location: &str) -> bool {
    WildernessPos::parse(location).is_some()
}

fn wilderness_region_key(location: &str) -> Option<String> {
    WildernessPos::parse(location).map(|pos| {
        format!(
            "w:{}:{}:{}:{}",
            pos.identity.world, pos.identity.epoch_id, pos.identity.origin_x, pos.identity.origin_y,
        )
    })
}

pub fn hub_navigation_choice(ctx: &ActionContext, rng: &mut StdRng) -> String {
    let candidates: Vec<(&str, u32)> = if ctx.location == STARTER_LOCATION {
        vec![("go up", 70), ("look", 30)]
    } else if ctx.location == OUTSKIRTS_FROM {
        vec![("outskirts", 80), ("look", 20)]
    } else {
        vec![("look", 50), ("go up", 25), ("outskirts", 25)]
    };
    pick_weighted_static(rng, &candidates)
}

fn cardinal_exits_from_view(exits: &[terminus_core::ExitView]) -> Vec<String> {
    exits
        .iter()
        .filter_map(|exit| {
            let key = exit.keyword.to_ascii_lowercase();
            matches!(key.as_str(), "north" | "south" | "east" | "west").then_some(key)
        })
        .collect()
}

fn pick_weighted_static(rng: &mut StdRng, choices: &[(&str, u32)]) -> String {
    let total: u32 = choices.iter().map(|(_, w)| w).sum();
    if total == 0 {
        return "look".into();
    }
    let mut roll = rng.random_range(0..total);
    for (cmd, weight) in choices {
        if roll < *weight {
            return (*cmd).to_string();
        }
        roll -= weight;
    }
    choices.last().map(|(c, _)| (*c).to_string()).unwrap_or_else(|| "look".into())
}
