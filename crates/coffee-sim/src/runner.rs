//! Simulation runner: world bootstrap, scheduling, and reporting.

use std::collections::BTreeSet;
use std::time::Instant;

use coffee_world::{WorldOutcomeSnapshot, WorldState, WORLD_GENERATION_VERSION};
use rand::rngs::StdRng;
use rand::SeedableRng;
use terminus_core::SessionId;
use thiserror::Error;

use crate::agent::SimAgent;
use crate::config::{
    configured_horizon_ms, virtual_duration_from_days, SnapshotConfig, ValidatedSchedule,
    DEFAULT_TRACE_LIMIT, MAX_AGENT_COUNT, MAX_SNAPSHOT_COUNT,
};
use crate::execute::{execute_line, ActionResult};
use crate::navigation::build_context;
use crate::population::{assign_profiles, PopulationMix};
use crate::profile::{choose_action, ActionChoice, Profile};
use crate::report::{
    aggregate_profile_stats, duration_to_ms, world_report_from_snapshot, AgentStats,
    ExecutionStats, RunReport, RunReportConfig, SimVersion, SimulationTiming, StopReason,
    WorldMetricsInput, SIM_CRATE_VERSION,
};
use crate::scheduler::{EventScheduler, ScheduledEventKind, SchedulerConfig};
use crate::snapshot::{MetricsSnapshotInput, SnapshotCollector, SnapshotSamplingSummary};
use crate::trace::{ActionTrace, ActionTraceEntry, TraceOutcome};

pub const MS_PER_DAY: u64 = 86_400_000;

#[derive(Clone, Debug)]
pub struct RunConfig {
    pub seed: u64,
    pub agent_count: u32,
    pub population: PopulationMix,
    pub virtual_days: f64,
    pub scheduler: SchedulerConfig,
    pub max_actions: Option<u64>,
    pub virtual_start_ms: u64,
    /// Record a bounded deterministic action trace in the report.
    pub record_action_trace: bool,
    pub action_trace_limit: u32,
    pub snapshots: SnapshotConfig,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            seed: 42,
            agent_count: 8,
            population: PopulationMix::uniform_wanderer(),
            virtual_days: 7.0,
            scheduler: SchedulerConfig::default(),
            max_actions: None,
            virtual_start_ms: 1_700_000_000_000,
            record_action_trace: false,
            action_trace_limit: DEFAULT_TRACE_LIMIT,
            snapshots: SnapshotConfig::disabled(),
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
        self.population.validate()?;
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
        if let Some(interval) = self.snapshots.interval_ms {
            if interval == 0 {
                return Err(SimulationError::ZeroSnapshotInterval);
            }
            if self.snapshots.max_snapshots == 0 {
                return Err(SimulationError::ZeroSnapshotLimit);
            }
            if self.snapshots.max_snapshots > MAX_SNAPSHOT_COUNT {
                return Err(SimulationError::TooManySnapshots { max: MAX_SNAPSHOT_COUNT });
            }
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
    #[error("invalid population mix: {0}")]
    InvalidPopulationMix(String),
    #[error("snapshot interval must be at least 1 ms when enabled")]
    ZeroSnapshotInterval,
    #[error("snapshot limit must be at least 1 when sampling is enabled")]
    ZeroSnapshotLimit,
    #[error("snapshot limit must be at most {max}")]
    TooManySnapshots { max: u32 },
    #[error("population scale experiment requires at least one player count")]
    EmptyScaleExperiment,
    #[error("population scale experiment supports at most {max} player counts")]
    TooManyScaleRuns { max: usize },
    #[error("multi-seed experiment requires at least one condition")]
    EmptyExperimentConditions,
    #[error("seed schedule must specify at least one replicate seed")]
    EmptySeedSchedule,
    #[error("multi-seed experiment supports at most {max} conditions")]
    TooManyExperimentConditions { max: usize },
    #[error("multi-seed experiment supports at most {max} replicates per condition")]
    TooManyExperimentReplicates { max: usize },
    #[error("multi-seed experiment workload {requested} runs exceeds limit {max}")]
    TooManyExperimentRuns { requested: usize, max: usize },
    #[error("multi-seed experiment workload overflow")]
    ExperimentWorkloadOverflow,
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
        let profiles =
            assign_profiles(self.config.seed, self.config.agent_count, &self.config.population)?;
        let allocated_profile_counts = profile_count_map(&profiles);
        let mut agents: Vec<SimAgent> = profiles
            .into_iter()
            .enumerate()
            .map(|(index, profile)| SimAgent::new(self.config.seed, index as u32, profile))
            .collect();
        agents.sort_by_key(|agent| agent.index);

        for agent in &agents {
            world.insert_operator(agent.operator());
            world
                .connect_player(agent.identity.clone())
                .map_err(|err| SimulationError::Engine(err.to_string()))?;
        }
        world.align_simulation_clock(self.schedule.virtual_start_ms);

        let mut scheduler = EventScheduler::new();
        for agent in &agents {
            scheduler.schedule(self.schedule.virtual_start_ms, agent.index);
        }

        let mut total_attempted = 0u64;
        let mut successful_commands = 0u64;
        let mut metrics = MetricsAccumulator::default();
        let horizon_ms = self.schedule.configured_horizon_ms;
        let mut snapshot_collector = SnapshotCollector::new(
            &self.config.snapshots,
            self.schedule.virtual_start_ms,
            horizon_ms,
            self.config.agent_count,
        )?;
        if let Some(collector) = snapshot_collector.as_mut() {
            collector.enqueue_interval_observations(&mut scheduler);
            collector.capture_initial(
                &world,
                &snapshot_metrics(&metrics, &agents, total_attempted, successful_commands, 0, 0),
            );
        }
        let mut trace =
            if self.config.record_action_trace { Some(ActionTrace::default()) } else { None };
        let mut inactive_by_index = vec![0u64; self.config.agent_count as usize];

        let mut events_processed = 0u64;
        let mut observation_events_processed = 0u64;
        let mut inactive_slots = 0u64;
        let mut last_event_slot_ms = None::<u64>;
        let mut last_scheduler_slot_ms = None::<u64>;
        let mut stop_reason = StopReason::QueueExhausted;

        while let Some(event) = scheduler.pop_next() {
            if event.time_ms > horizon_ms {
                stop_reason = StopReason::HorizonReached;
                break;
            }

            if action_limit_reached(&self.config, total_attempted) {
                stop_reason = StopReason::ActionLimit;
                break;
            }

            if event.kind == ScheduledEventKind::Observation {
                if should_skip_post_gameplay_observation(
                    event.time_ms,
                    last_event_slot_ms,
                    &scheduler,
                ) {
                    stop_reason = StopReason::QueueExhausted;
                    break;
                }
                advance_world_time(&mut world, event.time_ms);
                last_scheduler_slot_ms = Some(event.time_ms);
                if let Some(collector) = snapshot_collector.as_mut() {
                    collector.capture_interval(
                        event.time_ms,
                        &world,
                        &snapshot_metrics(
                            &metrics,
                            &agents,
                            total_attempted,
                            successful_commands,
                            inactive_slots,
                            events_processed,
                        ),
                    );
                }
                observation_events_processed += 1;
                continue;
            }

            let agent = &mut agents[event.agent_index as usize];

            advance_world_time(&mut world, event.time_ms);
            last_scheduler_slot_ms = Some(event.time_ms);
            let world_ms_before = world.simulation_now_ms();

            let choice = match build_context(&world, agent) {
                Ok(ctx) => choose_action(
                    agent.profile,
                    &ctx,
                    &mut agent.memory,
                    &mut agent.visitor_actions_remaining,
                    &mut rng,
                ),
                Err(_) => {
                    inactive_slots += 1;
                    inactive_by_index[event.agent_index as usize] += 1;
                    events_processed += 1;
                    last_event_slot_ms = Some(event.time_ms);
                    schedule_next(
                        &mut scheduler,
                        &self.config,
                        event.time_ms,
                        horizon_ms,
                        agent.index,
                        ActionChoice { command: String::new(), idle: true, offline_gap_ms: None },
                    );
                    continue;
                }
            };

            if choice.command.is_empty() {
                inactive_slots += 1;
                inactive_by_index[event.agent_index as usize] += 1;
                events_processed += 1;
                last_event_slot_ms = Some(event.time_ms);
                schedule_next(
                    &mut scheduler,
                    &self.config,
                    event.time_ms,
                    horizon_ms,
                    agent.index,
                    choice,
                );
                continue;
            }

            total_attempted += 1;
            agent.attempted += 1;
            let location_before = world.location_id_for_sim(agent.session_id).ok();
            let home_before = world.active_home_place_key(agent.player_id);
            let is_inspect = is_inspect_command(&choice.command);
            let is_move = is_cardinal(&choice.command);
            let is_social = is_social_command(&choice.command);
            let is_journal = choice.command.starts_with("journal record");
            if is_inspect {
                metrics.inspect_attempts += 1;
            }
            if is_move {
                metrics.exploration_move_attempts += 1;
            }
            if is_social {
                agent.memory.social_attempts += 1;
                metrics.social_attempts += 1;
            }

            let (result, _) = execute_line(&mut world, agent.session_id, &choice.command);
            record_outcome(agent, &result);
            if result.is_success() {
                successful_commands += 1;
            }

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
                        command: choice.command.clone(),
                        outcome,
                    },
                    self.config.action_trace_limit,
                );
            }

            if result.is_success() {
                if is_inspect {
                    metrics.successful_inspections += 1;
                    agent.memory.successful_inspections += 1;
                    if let Ok(loc) = world.location_id_for_sim(agent.session_id) {
                        agent.memory.record_inspection(&loc);
                    }
                }
                if is_journal {
                    agent.memory.journal_records += 1;
                    metrics.journal_records += 1;
                }
                if is_social {
                    agent.memory.social_successes += 1;
                    metrics.social_successes += 1;
                }
                if is_move {
                    metrics.successful_exploration_moves += 1;
                    if let Ok(location_after) = world.location_id_for_sim(agent.session_id) {
                        if location_before.as_deref() != Some(location_after.as_str()) {
                            metrics.locations_visited.insert(location_after.clone());
                            let region = crate::navigation::build_context(&world, agent)
                                .ok()
                                .and_then(|c| c.region_key);
                            agent.memory.record_location(&location_after, region.as_deref());
                            agent.memory.record_move(
                                location_before.as_deref().unwrap_or(""),
                                &choice.command,
                            );
                            if home_before.is_some()
                                && world.active_home_place_key(agent.player_id).as_deref()
                                    == Some(location_after.as_str())
                            {
                                agent.memory.home_returns += 1;
                                metrics.home_returns += 1;
                            }
                        }
                    }
                } else if let Ok(loc) = world.location_id_for_sim(agent.session_id) {
                    let region = crate::navigation::build_context(&world, agent)
                        .ok()
                        .and_then(|c| c.region_key);
                    agent.memory.record_location(&loc, region.as_deref());
                }
            }

            events_processed += 1;
            last_event_slot_ms = Some(event.time_ms);

            schedule_next(
                &mut scheduler,
                &self.config,
                event.time_ms,
                horizon_ms,
                agent.index,
                choice,
            );

            if action_limit_reached(&self.config, total_attempted) {
                stop_reason = StopReason::ActionLimit;
                break;
            }
        }

        if let Some(collector) = snapshot_collector.as_mut() {
            if let Some(slot) = last_event_slot_ms {
                collector.capture_terminal(
                    &world,
                    &snapshot_metrics(
                        &metrics,
                        &agents,
                        total_attempted,
                        successful_commands,
                        inactive_slots,
                        events_processed,
                    ),
                    slot,
                );
            } else {
                collector.finalize_terminal_status(false);
            }
        }

        let effective_termination_slot_ms = last_event_slot_ms;

        let snapshot = world.outcome_snapshot();
        let final_world_ms = world.simulation_now_ms();
        metrics.merge_agent_memory(&agents);

        let snapshot_sampling =
            snapshot_collector.as_ref().map(SnapshotCollector::sampling_summary);

        Ok(assemble_report(ReportInputs {
            config: &self.config,
            schedule: &self.schedule,
            agents: &agents,
            allocated_profile_counts,
            snapshot,
            metrics,
            total_attempted,
            longitudinal_snapshots: snapshot_collector.map(|c| c.snapshots),
            snapshot_sampling,
            timing: SimulationTiming {
                configured_start_ms: self.schedule.virtual_start_ms,
                configured_horizon_ms: horizon_ms,
                last_event_slot_ms,
                last_scheduler_slot_ms,
                effective_termination_slot_ms,
                final_world_ms,
                events_processed,
                observation_events_processed,
                inactive_slots,
                stop_reason,
            },
            real_elapsed_ms: duration_to_ms(started.elapsed()),
            action_trace: trace,
            inactive_by_index,
        }))
    }
}

fn schedule_next(
    scheduler: &mut EventScheduler,
    config: &RunConfig,
    event_time_ms: u64,
    horizon_ms: u64,
    agent_index: u32,
    choice: ActionChoice,
) {
    let next_time = if let Some(gap) = choice.offline_gap_ms {
        event_time_ms.saturating_add(gap)
    } else {
        config.scheduler.next_event_ms(event_time_ms, choice.idle)
    };
    if next_time <= horizon_ms {
        scheduler.schedule(next_time, agent_index);
    }
}

#[derive(Default)]
pub(crate) struct MetricsAccumulator {
    pub(crate) locations_visited: BTreeSet<String>,
    pub(crate) regions_visited: BTreeSet<String>,
    pub(crate) inspect_attempts: u64,
    pub(crate) successful_inspections: u64,
    pub(crate) exploration_move_attempts: u64,
    pub(crate) successful_exploration_moves: u64,
    pub(crate) journal_records: u64,
    pub(crate) social_attempts: u64,
    pub(crate) social_successes: u64,
    pub(crate) repeat_visits: u64,
    pub(crate) home_returns: u64,
}

impl MetricsAccumulator {
    fn merge_agent_memory(&mut self, agents: &[SimAgent]) {
        for agent in agents {
            self.regions_visited.extend(agent.memory.region_keys_visited.iter().cloned());
            self.repeat_visits += agent.memory.repeat_visits;
        }
    }
}

fn action_limit_reached(config: &RunConfig, total_attempted: u64) -> bool {
    config.max_actions.is_some_and(|max| total_attempted >= max)
}

/// Observations after the last gameplay slot must not run once no agent actions remain.
fn should_skip_post_gameplay_observation(
    observation_time_ms: u64,
    last_gameplay_slot_ms: Option<u64>,
    scheduler: &EventScheduler,
) -> bool {
    let Some(last_gp) = last_gameplay_slot_ms else {
        return false;
    };
    observation_time_ms > last_gp && !scheduler.has_pending_agent_action()
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

fn is_inspect_command(line: &str) -> bool {
    line.starts_with("inspect")
}

fn is_social_command(line: &str) -> bool {
    line.starts_with("say ") || line.starts_with("emote ") || line.starts_with("whisper ")
}

struct ReportInputs<'a> {
    config: &'a RunConfig,
    schedule: &'a ValidatedSchedule,
    agents: &'a [SimAgent],
    allocated_profile_counts: std::collections::BTreeMap<String, u32>,
    snapshot: WorldOutcomeSnapshot,
    metrics: MetricsAccumulator,
    total_attempted: u64,
    longitudinal_snapshots: Option<Vec<crate::snapshot::LongitudinalSnapshot>>,
    snapshot_sampling: Option<SnapshotSamplingSummary>,
    timing: SimulationTiming,
    real_elapsed_ms: u64,
    action_trace: Option<ActionTrace>,
    inactive_by_index: Vec<u64>,
}

fn assemble_report(input: ReportInputs<'_>) -> RunReport {
    let ReportInputs {
        config,
        schedule,
        agents,
        allocated_profile_counts,
        snapshot,
        metrics,
        total_attempted,
        longitudinal_snapshots,
        snapshot_sampling,
        timing,
        real_elapsed_ms,
        action_trace,
        inactive_by_index,
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
                profile: agent.profile.as_str().to_string(),
                attempted: agent.attempted,
                successful: agent.successful,
                rejected: agent.rejected,
                failed: agent.failed,
                unique_locations_visited: agent.memory.visited_locations.len(),
                unique_regions_visited: agent.memory.region_keys_visited.len(),
                successful_inspections: agent.memory.successful_inspections,
                journal_records: agent.memory.journal_records,
                social_successes: agent.memory.social_successes,
                repeat_visits: agent.memory.repeat_visits,
                home_returns: agent.memory.home_returns,
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
            population: config.population.clone(),
            virtual_start_ms: schedule.virtual_start_ms,
            virtual_duration_ms: schedule.virtual_duration_ms,
            scheduler: config.scheduler.clone(),
            max_actions: config.max_actions,
            allocated_profile_counts,
            snapshots: config.snapshots.clone(),
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
                unique_regions_visited: metrics.regions_visited.len(),
                inspect_attempts: metrics.inspect_attempts,
                successful_inspections: metrics.successful_inspections,
                exploration_move_attempts: metrics.exploration_move_attempts,
                successful_exploration_moves: metrics.successful_exploration_moves,
                journal_records: metrics.journal_records,
                social_attempts: metrics.social_attempts,
                social_successes: metrics.social_successes,
                repeat_visits: metrics.repeat_visits,
                home_returns: metrics.home_returns,
            },
        ),
        per_agent,
        per_profile: aggregate_profile_stats(agents, &inactive_by_index),
        action_trace,
        longitudinal_snapshots,
        snapshot_sampling,
    }
}

fn profile_count_map(profiles: &[Profile]) -> std::collections::BTreeMap<String, u32> {
    let mut map = std::collections::BTreeMap::new();
    for profile in profiles {
        *map.entry(profile.as_str().to_string()).or_default() += 1;
    }
    map
}

fn snapshot_metrics<'a>(
    metrics: &'a MetricsAccumulator,
    agents: &'a [SimAgent],
    total_attempted: u64,
    successful_commands: u64,
    inactive_slots: u64,
    events_processed: u64,
) -> MetricsSnapshotInput<'a> {
    MetricsSnapshotInput {
        metrics,
        agents,
        total_attempted,
        successful_commands,
        inactive_slots,
        events_processed,
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
        self.location_key_for_sim(session_id)
    }
}
