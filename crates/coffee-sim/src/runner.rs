//! Simulation runner: world bootstrap, scheduling, and reporting.

use std::time::Instant;

use coffee_world::geography::{WildernessPos, OUTSKIRTS_FROM};
use coffee_world::model::STARTER_LOCATION;
use coffee_world::{WorldOutcomeSnapshot, WorldState, WORLD_GENERATION_VERSION};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use terminus_core::SessionId;
use thiserror::Error;

use crate::agent::SimAgent;
use crate::execute::{execute_line, ActionResult};
use crate::report::{
    duration_to_ms, world_report_from_snapshot, AgentStats, ExecutionStats, RunReport,
    RunReportConfig, SimVersion, SIM_CRATE_VERSION,
};
use crate::scheduler::{EventScheduler, SchedulerConfig};

pub const MS_PER_DAY: u64 = 86_400_000;

#[derive(Clone, Debug)]
pub struct RunConfig {
    pub seed: u64,
    pub agent_count: u32,
    pub virtual_days: f64,
    pub scheduler: SchedulerConfig,
    pub max_actions: Option<u64>,
    pub virtual_start_ms: u64,
}

impl RunConfig {
    pub fn virtual_duration_ms(&self) -> u64 {
        ((self.virtual_days * MS_PER_DAY as f64).round()).max(0.0) as u64
    }

    pub fn virtual_end_ms(&self) -> u64 {
        self.virtual_start_ms.saturating_add(self.virtual_duration_ms())
    }
}

#[derive(Debug, Error)]
pub enum SimulationError {
    #[error("agent count must be at least 1")]
    NoAgents,
    #[error("virtual duration must be positive")]
    ZeroDuration,
    #[error("{0}")]
    Engine(String),
}

pub struct SimulationRunner {
    config: RunConfig,
}

impl SimulationRunner {
    pub fn new(config: RunConfig) -> Result<Self, SimulationError> {
        if config.agent_count == 0 {
            return Err(SimulationError::NoAgents);
        }
        if config.virtual_duration_ms() == 0 {
            return Err(SimulationError::ZeroDuration);
        }
        Ok(Self { config })
    }

    pub fn run(&self) -> Result<RunReport, SimulationError> {
        let started = Instant::now();
        let mut rng = StdRng::seed_from_u64(self.config.seed);
        let mut world = WorldState::for_simulation(self.config.virtual_start_ms);
        let mut agents: Vec<SimAgent> = (0..self.config.agent_count)
            .map(|index| SimAgent::new(self.config.seed, index))
            .collect();
        agents.sort_by_key(|agent| agent.index);

        for agent in &agents {
            world.insert_operator(agent.operator());
            world
                .connect_player(agent.identity.clone())
                .map_err(|err| SimulationError::Engine(err.to_string()))?;
        }

        let mut scheduler = EventScheduler::new();
        for agent in &agents {
            scheduler.schedule(self.config.virtual_start_ms, agent.index);
        }

        let end_ms = self.config.virtual_end_ms();
        let mut total_attempted = 0u64;
        let mut inspect_total = 0u64;
        let mut move_total = 0u64;
        let mut locations_visited: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();

        while let Some(event) = scheduler.pop_next() {
            if event.time_ms > end_ms {
                break;
            }
            if let Some(max) = self.config.max_actions {
                if total_attempted >= max {
                    break;
                }
            }

            let agent = agents
                .iter_mut()
                .find(|a| a.index == event.agent_index)
                .expect("scheduled agent exists");

            advance_world_time(&mut world, event.time_ms);

            let (line, idle) = choose_action(&world, agent, &mut rng);
            if !line.is_empty() {
                total_attempted += 1;
                agent.attempted += 1;
                let (result, _) = execute_line(&mut world, agent.session_id, &line);
                record_outcome(agent, &result);
                if result.is_success() {
                    if line.starts_with("inspect") {
                        agent.inspect_actions += 1;
                        inspect_total += 1;
                    }
                    if is_cardinal(&line) {
                        agent.exploration_moves += 1;
                        move_total += 1;
                    }
                    if let Ok(loc) = world.location_id_for_sim(agent.session_id) {
                        locations_visited.insert(loc);
                    }
                }
            }

            let mut next_interval = self.config.scheduler.action_interval_ms;
            if idle {
                next_interval = next_interval.saturating_add(self.config.scheduler.idle_extra_ms);
            }
            let next_time = event.time_ms.saturating_add(next_interval);
            if next_time <= end_ms {
                scheduler.schedule(next_time, agent.index);
            }
        }

        world.advance_virtual_time(0); // document final now via event loop
        let snapshot = world.outcome_snapshot();
        let virtual_end_ms = world.simulation_now_ms();

        let report = assemble_report(ReportInputs {
            config: &self.config,
            agents: &agents,
            snapshot,
            unique_locations_visited: locations_visited.len(),
            inspect_total,
            move_total,
            total_attempted,
            virtual_end_ms,
            real_elapsed_ms: duration_to_ms(started.elapsed()),
        });
        Ok(report)
    }
}

fn advance_world_time(world: &mut WorldState, target_ms: u64) {
    let now = world.simulation_now_ms();
    if target_ms > now {
        world.advance_virtual_time(target_ms - now);
    }
}

fn record_outcome(agent: &mut SimAgent, result: &ActionResult) {
    match result {
        ActionResult::Success { .. } => agent.successful += 1,
        ActionResult::Rejected { .. } => agent.rejected += 1,
        ActionResult::Failed { .. } => agent.failed += 1,
    }
}

fn is_cardinal(line: &str) -> bool {
    matches!(line, "north" | "south" | "east" | "west")
}

/// Deterministic action selection: shared hub script, then wilderness exploration.
fn choose_action(world: &WorldState, agent: &mut SimAgent, rng: &mut StdRng) -> (String, bool) {
    const HUB_SCRIPT: &[&str] = &["look", "go up", "outskirts", "north"];
    if (agent.script_step as usize) < HUB_SCRIPT.len() {
        let line = HUB_SCRIPT[agent.script_step as usize].to_string();
        agent.script_step += 1;
        return (line, false);
    }

    let Ok(location) = world.location_id_for_sim(agent.session_id) else {
        return (String::from("look"), false);
    };

    if WildernessPos::parse(&location).is_some()
        && !agent.home_claim_attempted
        && world.active_home_base_habitation(agent.player_id).is_none()
        && rng.random_ratio(1, 4)
    {
        agent.home_claim_attempted = true;
        return (String::from("home claim"), false);
    }

    if WildernessPos::parse(&location).is_some() && rng.random_ratio(1, 3) {
        return (String::from("inspect"), false);
    }

    if WildernessPos::parse(&location).is_some() {
        let dirs = ["north", "south", "east", "west"];
        let dir = dirs[rng.random_range(0..dirs.len())];
        return (dir.to_string(), false);
    }

    if location == OUTSKIRTS_FROM {
        return (String::from("squalor"), false);
    }

    if location == STARTER_LOCATION && rng.random_ratio(1, 2) {
        return (String::from("look"), false);
    }

    if rng.random_ratio(1, 5) {
        return (String::new(), true);
    }

    (String::from("look"), false)
}

struct ReportInputs<'a> {
    config: &'a RunConfig,
    agents: &'a [SimAgent],
    snapshot: WorldOutcomeSnapshot,
    unique_locations_visited: usize,
    inspect_total: u64,
    move_total: u64,
    total_attempted: u64,
    virtual_end_ms: u64,
    real_elapsed_ms: u64,
}

fn assemble_report(input: ReportInputs<'_>) -> RunReport {
    let ReportInputs {
        config,
        agents,
        snapshot,
        unique_locations_visited,
        inspect_total,
        move_total,
        total_attempted,
        virtual_end_ms,
        real_elapsed_ms,
    } = input;
    let mut successful = 0u64;
    let mut rejected = 0u64;
    let mut failed = 0u64;
    let mut per_agent = std::collections::BTreeMap::new();
    for agent in agents {
        successful += agent.successful;
        rejected += agent.rejected;
        failed += agent.failed;
        per_agent.insert(
            agent.display_name.clone(),
            AgentStats {
                attempted: agent.attempted,
                successful: agent.successful,
                rejected: agent.rejected,
                failed: agent.failed,
            },
        );
    }

    RunReport {
        version: SimVersion {
            simulator_crate: SIM_CRATE_VERSION.to_string(),
            world_generation_version: WORLD_GENERATION_VERSION,
        },
        config: RunReportConfig {
            seed: config.seed,
            agent_count: config.agent_count,
            virtual_start_ms: config.virtual_start_ms,
            virtual_end_ms,
            virtual_duration_ms: virtual_end_ms.saturating_sub(config.virtual_start_ms),
            scheduler: config.scheduler.clone(),
            max_actions: config.max_actions,
        },
        execution: ExecutionStats {
            total_attempted,
            successful,
            rejected,
            failed,
            real_elapsed_ms,
        },
        world: world_report_from_snapshot(
            &snapshot,
            unique_locations_visited,
            inspect_total,
            move_total,
        ),
        per_agent,
    }
}

/// Test and instrumentation helper exposed for integration tests.
pub fn run_with_config(config: RunConfig) -> Result<RunReport, SimulationError> {
    SimulationRunner::new(config)?.run()
}

/// Extension for session location lookup without making `WorldState` fields public.
pub trait SimWorldExt {
    fn location_id_for_sim(
        &self,
        session_id: SessionId,
    ) -> Result<String, terminus_core::GameError>;
}

impl SimWorldExt for WorldState {
    fn location_id_for_sim(
        &self,
        session_id: SessionId,
    ) -> Result<String, terminus_core::GameError> {
        let player = self.player_of(session_id).ok_or(terminus_core::GameError::UnknownSession)?;
        self.operator(player)
            .map(|op| op.location_id.clone())
            .ok_or(terminus_core::GameError::UnknownSession)
    }
}
