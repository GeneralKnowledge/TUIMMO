//! Simulation runner: world bootstrap, scheduling, and reporting.

use std::collections::BTreeSet;
use std::time::Instant;

use coffee_world::geography::{WildernessPos, OUTSKIRTS_FROM};
use coffee_world::model::STARTER_LOCATION;
use coffee_world::{WorldOutcomeSnapshot, WorldState, WORLD_GENERATION_VERSION};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use terminus_core::SessionId;
use thiserror::Error;

use crate::agent::SimAgent;
use crate::config::{
    configured_horizon_ms, virtual_duration_from_days, ValidatedSchedule, DEFAULT_TRACE_LIMIT,
    MAX_AGENT_COUNT,
};
use crate::execute::{execute_line, ActionResult};
use crate::report::{
    duration_to_ms, world_report_from_snapshot, AgentStats, ExecutionStats, RunReport,
    RunReportConfig, SimVersion, SimulationTiming, StopReason, WorldMetricsInput,
    SIM_CRATE_VERSION,
};
use crate::scheduler::{EventScheduler, SchedulerConfig};
use crate::trace::{ActionTrace, ActionTraceEntry, TraceOutcome};

pub const MS_PER_DAY: u64 = 86_400_000;

#[derive(Clone, Debug)]
pub struct RunConfig {
    pub seed: u64,
    pub agent_count: u32,
    pub virtual_days: f64,
    pub scheduler: SchedulerConfig,
    pub max_actions: Option<u64>,
    pub virtual_start_ms: u64,
    /// Record a bounded deterministic action trace in the report.
    pub record_action_trace: bool,
    pub action_trace_limit: u32,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            seed: 42,
            agent_count: 8,
            virtual_days: 7.0,
            scheduler: SchedulerConfig::default(),
            max_actions: None,
            virtual_start_ms: 1_700_000_000_000,
            record_action_trace: false,
            action_trace_limit: DEFAULT_TRACE_LIMIT,
        }
    }
}

impl RunConfig {
    pub fn virtual_duration_ms(&self) -> Result<u64, SimulationError> {
        virtual_duration_from_days(self.virtual_days)
    }

    pub fn configured_horizon_ms(&self) -> Result<u64, SimulationError> {
        let schedule = self.validate()?;
        Ok(schedule.configured_horizon_ms)
    }

    pub fn validate(&self) -> Result<ValidatedSchedule, SimulationError> {
        if self.agent_count == 0 {
            return Err(SimulationError::NoAgents);
        }
        if self.agent_count > MAX_AGENT_COUNT {
            return Err(SimulationError::TooManyAgents { max: MAX_AGENT_COUNT });
        }
        let virtual_duration_ms = virtual_duration_from_days(self.virtual_days)?;
        if virtual_duration_ms == 0 {
            return Err(SimulationError::ZeroDuration);
        }
        if self.scheduler.action_interval_ms == 0 {
            return Err(SimulationError::ZeroActionInterval);
        }
        if let Some(max) = self.max_actions {
            if max == 0 {
                return Err(SimulationError::ZeroActionLimit);
            }
        }
        if self.record_action_trace && self.action_trace_limit == 0 {
            return Err(SimulationError::ZeroTraceLimit);
        }
        let configured_horizon_ms =
            configured_horizon_ms(self.virtual_start_ms, virtual_duration_ms)?;
        Ok(ValidatedSchedule {
            virtual_start_ms: self.virtual_start_ms,
            virtual_duration_ms,
            configured_horizon_ms,
        })
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SimulationError {
    #[error("agent count must be at least 1")]
    NoAgents,
    #[error("agent count must be at most {max}")]
    TooManyAgents { max: u32 },
    #[error("virtual duration must be finite and positive")]
    InvalidDuration,
    #[error("virtual duration overflows milliseconds")]
    DurationOverflow,
    #[error("virtual duration must be positive after conversion")]
    ZeroDuration,
    #[error("action interval must be at least 1 ms")]
    ZeroActionInterval,
    #[error("max action limit must be at least 1 when set")]
    ZeroActionLimit,
    #[error("action trace limit must be at least 1 when tracing is enabled")]
    ZeroTraceLimit,
    #[error("timestamp arithmetic overflow")]
    TimestampOverflow,
    #[error("{0}")]
    Engine(String),
}

pub struct SimulationRunner {
    config: RunConfig,
    schedule: ValidatedSchedule,
}

impl SimulationRunner {
    pub fn new(config: RunConfig) -> Result<Self, SimulationError> {
        let schedule = config.validate()?;
        Ok(Self { config, schedule })
    }

    pub fn run(&self) -> Result<RunReport, SimulationError> {
        let started = Instant::now();
        let mut rng = StdRng::seed_from_u64(self.config.seed);
        let mut world = WorldState::for_simulation(self.schedule.virtual_start_ms);
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
        // Registration/connect ticks advance the test clock; realign before scheduled play.
        world.align_simulation_clock(self.schedule.virtual_start_ms);

        let mut scheduler = EventScheduler::new();
        for agent in &agents {
            scheduler.schedule(self.schedule.virtual_start_ms, agent.index);
        }

        let horizon_ms = self.schedule.configured_horizon_ms;
        let mut total_attempted = 0u64;
        let mut metrics = MetricsAccumulator::default();
        let mut trace =
            if self.config.record_action_trace { Some(ActionTrace::default()) } else { None };

        let mut events_processed = 0u64;
        let mut last_event_slot_ms = None::<u64>;
        let mut stop_reason = StopReason::QueueExhausted;

        while let Some(event) = scheduler.pop_next() {
            if event.time_ms > horizon_ms {
                stop_reason = StopReason::HorizonReached;
                break;
            }
            if let Some(max) = self.config.max_actions {
                if total_attempted >= max {
                    stop_reason = StopReason::ActionLimit;
                    break;
                }
            }

            let agent = &mut agents[event.agent_index as usize];

            advance_world_time(&mut world, event.time_ms);
            let world_ms_before = world.simulation_now_ms();

            let (line, idle) = choose_action(&world, agent, &mut rng);
            if !line.is_empty() {
                total_attempted += 1;
                agent.attempted += 1;
                let location_before = world.location_id_for_sim(agent.session_id).ok();
                let is_inspect = line.starts_with("inspect");
                let is_move = is_cardinal(&line);
                if is_inspect {
                    metrics.inspect_attempts += 1;
                }
                if is_move {
                    metrics.exploration_move_attempts += 1;
                }

                let (result, _) = execute_line(&mut world, agent.session_id, &line);
                record_outcome(agent, &result);

                let world_ms_after = world.simulation_now_ms();
                if let Some(trace) = trace.as_mut() {
                    let outcome = match &result {
                        ActionResult::Success { .. } => TraceOutcome::Success,
                        ActionResult::Rejected { .. } => TraceOutcome::Rejected,
                        ActionResult::Failed { .. } => TraceOutcome::Failed,
                    };
                    trace.push_bounded(
                        ActionTraceEntry {
                            event_slot_ms: event.time_ms,
                            world_ms_before_command: world_ms_before,
                            world_ms_after_command: world_ms_after,
                            agent_index: agent.index,
                            command: line.clone(),
                            outcome,
                        },
                        self.config.action_trace_limit,
                    );
                }

                if result.is_success() {
                    if is_inspect {
                        metrics.successful_inspections += 1;
                    }
                    if is_move {
                        metrics.successful_exploration_moves += 1;
                    }
                    if let Ok(location_after) = world.location_id_for_sim(agent.session_id) {
                        if location_before.as_deref() != Some(location_after.as_str()) {
                            metrics.locations_visited.insert(location_after);
                        }
                    }
                }
            }

            events_processed += 1;
            last_event_slot_ms = Some(event.time_ms);

            let next_time = self.config.scheduler.next_event_ms(event.time_ms, idle);
            if next_time <= horizon_ms {
                scheduler.schedule(next_time, agent.index);
            }
        }

        let snapshot = world.outcome_snapshot();
        let final_world_ms = world.simulation_now_ms();

        Ok(assemble_report(ReportInputs {
            config: &self.config,
            schedule: &self.schedule,
            agents: &agents,
            snapshot,
            metrics,
            total_attempted,
            timing: SimulationTiming {
                configured_start_ms: self.schedule.virtual_start_ms,
                configured_horizon_ms: horizon_ms,
                last_event_slot_ms,
                final_world_ms,
                events_processed,
                stop_reason,
            },
            real_elapsed_ms: duration_to_ms(started.elapsed()),
            action_trace: trace,
        }))
    }
}

#[derive(Default)]
struct MetricsAccumulator {
    locations_visited: BTreeSet<String>,
    inspect_attempts: u64,
    successful_inspections: u64,
    exploration_move_attempts: u64,
    successful_exploration_moves: u64,
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
    schedule: &'a ValidatedSchedule,
    agents: &'a [SimAgent],
    snapshot: WorldOutcomeSnapshot,
    metrics: MetricsAccumulator,
    total_attempted: u64,
    timing: SimulationTiming,
    real_elapsed_ms: u64,
    action_trace: Option<ActionTrace>,
}

fn assemble_report(input: ReportInputs<'_>) -> RunReport {
    let ReportInputs {
        config,
        schedule,
        agents,
        snapshot,
        metrics,
        total_attempted,
        timing,
        real_elapsed_ms,
        action_trace,
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
            virtual_start_ms: schedule.virtual_start_ms,
            virtual_duration_ms: schedule.virtual_duration_ms,
            scheduler: config.scheduler.clone(),
            max_actions: config.max_actions,
        },
        timing,
        execution: ExecutionStats {
            total_attempted,
            successful,
            rejected,
            failed,
            real_elapsed_ms,
        },
        world: world_report_from_snapshot(
            &snapshot,
            WorldMetricsInput {
                unique_locations_visited: metrics.locations_visited.len(),
                inspect_attempts: metrics.inspect_attempts,
                successful_inspections: metrics.successful_inspections,
                exploration_move_attempts: metrics.exploration_move_attempts,
                successful_exploration_moves: metrics.successful_exploration_moves,
            },
        ),
        per_agent,
        action_trace,
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
