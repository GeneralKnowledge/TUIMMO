//! Behavioural profiles as weighted action preferences (not fixed scripts).

use rand::rngs::StdRng;
use rand::RngExt;
use serde::{Deserialize, Serialize};

use crate::memory::AgentMemory;
use crate::navigation::{hub_navigation_choice, in_wilderness, ActionContext};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    Wanderer,
    Investigator,
    Homebody,
    Follower,
    SocialPlayer,
    OccasionalVisitor,
    Curious,
}

impl Profile {
    pub const ALL: [Profile; 7] = [
        Profile::Wanderer,
        Profile::Investigator,
        Profile::Homebody,
        Profile::Follower,
        Profile::SocialPlayer,
        Profile::OccasionalVisitor,
        Profile::Curious,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wanderer => "wanderer",
            Self::Investigator => "investigator",
            Self::Homebody => "homebody",
            Self::Follower => "follower",
            Self::SocialPlayer => "social_player",
            Self::OccasionalVisitor => "occasional_visitor",
            Self::Curious => "curious",
        }
    }
}

/// Offline gap between short sessions (Occasional Visitor).
pub const VISITOR_OFFLINE_MS: u64 = 2 * 24 * 60 * 60 * 1000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionChoice {
    pub command: String,
    /// Scheduler idle hint (shorter pause, still an event slot).
    pub idle: bool,
    /// Long virtual gap before the next event (between visitor sessions).
    pub offline_gap_ms: Option<u64>,
}

struct WeightedCommand {
    command: String,
    weight: u32,
}

pub fn choose_action(
    profile: Profile,
    ctx: &ActionContext,
    memory: &mut AgentMemory,
    visitor_actions_remaining: &mut u32,
    rng: &mut StdRng,
) -> ActionChoice {
    if profile == Profile::OccasionalVisitor && *visitor_actions_remaining == 0 {
        *visitor_actions_remaining = rng.random_range(2..=5);
        return ActionChoice {
            command: String::new(),
            idle: false,
            offline_gap_ms: Some(VISITOR_OFFLINE_MS),
        };
    }

    if !in_wilderness(&ctx.location) {
        let cmd = hub_navigation_choice(ctx, rng);
        return ActionChoice { command: cmd, idle: false, offline_gap_ms: None };
    }

    let mut choices = wilderness_choices(profile, ctx, memory, rng);
    if choices.is_empty() {
        choices.push(WeightedCommand { command: "look".into(), weight: 1 });
    }
    let command = pick_weighted(rng, &choices);
    if profile == Profile::OccasionalVisitor {
        *visitor_actions_remaining = visitor_actions_remaining.saturating_sub(1);
    }
    ActionChoice { command, idle: false, offline_gap_ms: None }
}

fn wilderness_choices(
    profile: Profile,
    ctx: &ActionContext,
    memory: &AgentMemory,
    rng: &mut StdRng,
) -> Vec<WeightedCommand> {
    let mut choices = Vec::new();
    let at_home = ctx.home_place_key.as_deref().is_some_and(|home| home == ctx.location);
    let unseen_here = !memory.inspected_here(&ctx.location);
    let novel = !memory.knows_location(&ctx.location);

    match profile {
        Profile::Wanderer => {
            push_cardinals(&mut choices, ctx, memory, 40);
            if unseen_here {
                choices.push(WeightedCommand { command: "inspect ground".into(), weight: 25 });
            }
            if rng.random_ratio(1, 14) {
                choices.push(WeightedCommand {
                    command: "sign A path worth checking.".into(),
                    weight: 8,
                });
            }
            if memory.knows_location(&ctx.location) && rng.random_ratio(1, 5) {
                choices.push(WeightedCommand { command: "look".into(), weight: 15 });
            }
            if novel {
                choices.push(WeightedCommand { command: "look".into(), weight: 10 });
            }
        }
        Profile::Investigator => {
            if unseen_here {
                choices.push(WeightedCommand { command: "inspect ground".into(), weight: 40 });
                choices.push(WeightedCommand { command: "inspect mark".into(), weight: 15 });
            }
            choices.push(WeightedCommand { command: "journal record".into(), weight: 25 });
            if memory.inspected_here(&ctx.location) && rng.random_ratio(1, 6) {
                choices.push(WeightedCommand {
                    command:
                        "theory post observation survey-mark The mark still aligns with the gate."
                            .into(),
                    weight: 12,
                });
            }
            push_cardinals(&mut choices, ctx, memory, 15);
            choices.push(WeightedCommand { command: "look".into(), weight: 15 });
        }
        Profile::Follower => {
            if memory.public_trace_locations.contains(&ctx.location) {
                choices.push(WeightedCommand { command: "inspect sign".into(), weight: 30 });
                choices.push(WeightedCommand { command: "look".into(), weight: 25 });
            }
            if let Some(subject) = memory.theory_subject_hints.iter().next() {
                choices.push(WeightedCommand {
                    command: format!("theory list {subject}"),
                    weight: 35,
                });
                if rng.random_ratio(1, 4) {
                    choices.push(WeightedCommand {
                        command: format!(
                            "theory post theory {subject} A different reading than the first post."
                        ),
                        weight: 20,
                    });
                }
            }
            push_cardinals(&mut choices, ctx, memory, 20);
            choices.push(WeightedCommand { command: "inspect ground".into(), weight: 15 });
        }
        Profile::Homebody => {
            if ctx.home_place_key.is_none() {
                choices.push(WeightedCommand { command: "home claim".into(), weight: 35 });
                push_cardinals(&mut choices, ctx, memory, 20);
            } else if at_home {
                choices.push(WeightedCommand { command: "look".into(), weight: 30 });
                choices.push(WeightedCommand { command: "inspect ground".into(), weight: 25 });
                push_cardinals(&mut choices, ctx, memory, 5);
            } else {
                push_cardinals(&mut choices, ctx, memory, 35);
                choices.push(WeightedCommand { command: "look".into(), weight: 20 });
            }
        }
        Profile::SocialPlayer => {
            if ctx.others_present > 0 {
                choices.push(WeightedCommand {
                    command: format!("say hello from {}", ctx.agent_display_name),
                    weight: 45,
                });
                choices.push(WeightedCommand { command: "emote waves".into(), weight: 20 });
                if let Some(ref name) = ctx.first_other_present {
                    choices.push(WeightedCommand {
                        command: format!("whisper {name} quiet greeting"),
                        weight: 10,
                    });
                }
            }
            push_cardinals(&mut choices, ctx, memory, 15);
            choices.push(WeightedCommand { command: "look".into(), weight: 20 });
        }
        Profile::OccasionalVisitor => {
            push_cardinals(&mut choices, ctx, memory, 25);
            choices.push(WeightedCommand { command: "look".into(), weight: 30 });
            if unseen_here {
                choices.push(WeightedCommand { command: "inspect ground".into(), weight: 15 });
            }
        }
        Profile::Curious => {
            for keyword in &ctx.thread_keywords {
                if !memory.inspected_keyword_at(&ctx.location, keyword) {
                    choices.push(WeightedCommand {
                        command: format!("inspect {keyword}"),
                        weight: 50,
                    });
                }
            }
            if choices.is_empty() {
                push_cardinals_toward_novel(&mut choices, ctx, memory, 35);
                choices.push(WeightedCommand { command: "look".into(), weight: 25 });
            } else {
                push_cardinals_toward_novel(&mut choices, ctx, memory, 15);
            }
        }
    }
    choices
}

fn push_cardinals_toward_novel(
    choices: &mut Vec<WeightedCommand>,
    ctx: &ActionContext,
    memory: &AgentMemory,
    base_weight: u32,
) {
    for exit in &ctx.cardinal_exits {
        let mut weight = base_weight;
        if let Some(region) = &ctx.region_key {
            if !memory.knows_region(region) {
                weight += 15;
            }
        }
        if !memory.knows_location(&ctx.location) {
            weight += 10;
        }
        choices.push(WeightedCommand { command: exit.clone(), weight });
    }
}

fn push_cardinals(
    choices: &mut Vec<WeightedCommand>,
    ctx: &ActionContext,
    memory: &AgentMemory,
    base_weight: u32,
) {
    for exit in &ctx.cardinal_exits {
        let mut weight = base_weight;
        if memory.repeats_last_exit(&ctx.location, exit) {
            weight = weight.saturating_div(4).max(1);
        }
        if !memory.knows_location(&ctx.location) {
            weight = weight.saturating_add(10);
        }
        choices.push(WeightedCommand { command: exit.clone(), weight });
    }
}

fn pick_weighted(rng: &mut StdRng, choices: &[WeightedCommand]) -> String {
    let total: u32 = choices.iter().map(|c| c.weight).sum();
    if total == 0 {
        return "look".into();
    }
    let mut roll = rng.random_range(0..total);
    for choice in choices {
        if roll < choice.weight {
            return choice.command.clone();
        }
        roll -= choice.weight;
    }
    choices.last().map(|c| c.command.clone()).unwrap_or_else(|| "look".into())
}
