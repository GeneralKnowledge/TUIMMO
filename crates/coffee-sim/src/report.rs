//! Run configuration and outcome aggregates (human text + JSON).

use std::collections::BTreeMap;
use std::time::Duration;

use coffee_world::{
    ActionDiscoveryReport, ClueDiscoveryReport, LongThreadProgressReport, PlacementInventoryReport,
    ThreadCoverageReport, ThreadSimReport, WorldOutcomeSnapshot,
};
use serde::{Deserialize, Serialize};

use crate::config::SnapshotConfig;
use crate::interaction::{InteractionMetrics, StoryTranscript};
use crate::population::PopulationMix;
use crate::profile::Profile;
use crate::scheduler::SchedulerConfig;
use crate::snapshot::{LongitudinalSnapshot, SnapshotSamplingSummary};
use crate::trace::ActionTrace;

pub const SIM_CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimVersion {
    pub simulator_crate: String,
    pub world_generation_version: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunReport {
    pub version: SimVersion,
    /// Inputs as validated for this run.
    pub config: RunReportConfig,
    pub timing: SimulationTiming,
    pub execution: ExecutionStats,
    pub world: WorldReport,
    pub per_agent: BTreeMap<String, AgentStats>,
    pub per_profile: BTreeMap<String, ProfileStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_trace: Option<ActionTrace>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longitudinal_snapshots: Option<Vec<LongitudinalSnapshot>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_sampling: Option<SnapshotSamplingSummary>,
    #[serde(default)]
    pub interaction: InteractionMetrics,
    #[serde(default)]
    pub story_transcript: StoryTranscript,
    #[serde(default)]
    pub thread_reports: Vec<ThreadSimReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage_metrics: Option<ThreadCoverageReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long_thread_progress: Option<LongThreadProgressReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clue_discovery: Option<ClueDiscoveryReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_discovery: Option<ActionDiscoveryReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement_inventory: Option<PlacementInventoryReport>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunReportConfig {
    pub seed: u64,
    pub agent_count: u32,
    pub population: PopulationMix,
    pub virtual_start_ms: u64,
    pub virtual_duration_ms: u64,
    pub scheduler: SchedulerConfig,
    pub max_actions: Option<u64>,
    /// Actual profile counts after largest-remainder allocation and shuffle.
    pub allocated_profile_counts: std::collections::BTreeMap<String, u32>,
    pub snapshots: SnapshotConfig,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// No further events were scheduled before the configured horizon.
    QueueExhausted,
    /// Next event would occur after the configured horizon.
    HorizonReached,
    /// Stopped after reaching the configured action attempt limit.
    ActionLimit,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimulationTiming {
    pub configured_start_ms: u64,
    /// Inclusive upper bound of the scheduling window (`start + duration`).
    pub configured_horizon_ms: u64,
    /// Scheduler slot time of the last processed **gameplay** (agent action) event, if any.
    pub last_event_slot_ms: Option<u64>,
    /// Slot time of the last processed scheduler event of any kind (gameplay or observation).
    #[serde(default)]
    pub last_scheduler_slot_ms: Option<u64>,
    /// Gameplay-effective stop slot (`last_event_slot_ms` when gameplay ended the run).
    #[serde(default)]
    pub effective_termination_slot_ms: Option<u64>,
    /// Authoritative world clock after the run loop (not advanced by post-termination observations).
    pub final_world_ms: u64,
    pub events_processed: u64,
    /// Observation-only scheduler events (longitudinal sampling); not included in `events_processed`.
    #[serde(default)]
    pub observation_events_processed: u64,
    /// Scheduler slots with no attempted command (idle or offline gaps).
    pub inactive_slots: u64,
    pub stop_reason: StopReason,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExecutionStats {
    pub total_attempted: u64,
    pub successful: u64,
    pub rejected: u64,
    pub failed: u64,
    pub real_elapsed_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldReport {
    /// Distinct place keys reached after a successful move (location changed).
    pub unique_locations_visited: usize,
    /// Distinct wilderness region keys visited (simulator memory union).
    pub unique_regions_visited: usize,
    /// Distinct place keys where agents stand at end of run.
    pub unique_locations_occupied: usize,
    /// Regions present in geography state (materialized during play).
    pub regions_materialized: usize,
    /// Discovery rows recorded (visits, notes, etc.).
    pub discoveries_recorded: usize,
    /// Field journal keys with at least two distinct recorders at end of run.
    pub corroborated_field_keys: usize,
    pub theory_subjects: usize,
    pub theory_entries: usize,
    pub inspect_attempts: u64,
    pub successful_inspections: u64,
    pub exploration_move_attempts: u64,
    pub successful_exploration_moves: u64,
    pub home_claims_active: usize,
    /// Successful `journal record` commands (simulator count).
    pub journal_records: u64,
    pub social_attempts: u64,
    pub social_successes: u64,
    pub repeat_visits: u64,
    pub home_returns: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentStats {
    pub profile: String,
    pub attempted: u64,
    pub successful: u64,
    pub rejected: u64,
    pub failed: u64,
    pub unique_locations_visited: usize,
    pub unique_regions_visited: usize,
    pub successful_inspections: u64,
    pub journal_records: u64,
    pub social_successes: u64,
    pub repeat_visits: u64,
    pub home_returns: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProfileStats {
    pub agent_count: u32,
    pub attempted: u64,
    pub successful: u64,
    pub rejected: u64,
    pub failed: u64,
    pub unique_locations_visited: usize,
    pub unique_regions_visited: usize,
    pub successful_inspections: u64,
    pub journal_records: u64,
    pub social_attempts: u64,
    pub social_successes: u64,
    pub repeat_visits: u64,
    pub home_returns: u64,
    pub inactive_slots: u64,
}

/// Derived rates from world counters (denominator zero → `None`, not 0.0).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DerivedRates {
    pub exploration_move_success_rate: Option<f64>,
    pub inspect_success_rate: Option<f64>,
    pub social_success_rate: Option<f64>,
    pub unique_locations_per_agent: f64,
    pub attempted_commands_per_agent: f64,
    /// Share of scheduler slots with no attempted command (`inactive / events_processed`).
    pub inactive_slot_share: Option<f64>,
}

impl RunReport {
    /// Same report with operational-only fields cleared for seed/config equality checks.
    pub fn for_reproducibility_comparison(mut self) -> Self {
        self.execution.real_elapsed_ms = 0;
        self
    }

    pub fn derived_rates(&self) -> DerivedRates {
        DerivedRates {
            exploration_move_success_rate: rate(
                self.world.successful_exploration_moves,
                self.world.exploration_move_attempts,
            ),
            inspect_success_rate: rate(
                self.world.successful_inspections,
                self.world.inspect_attempts,
            ),
            social_success_rate: rate(self.world.social_successes, self.world.social_attempts),
            unique_locations_per_agent: self.world.unique_locations_visited as f64
                / f64::from(self.config.agent_count.max(1)),
            attempted_commands_per_agent: self.execution.total_attempted as f64
                / f64::from(self.config.agent_count.max(1)),
            inactive_slot_share: rate(self.timing.inactive_slots, self.timing.events_processed),
        }
    }

    pub fn to_pretty_text(&self) -> String {
        let mut lines = vec![
            "Coffee.shop population simulation report".into(),
            "========================================".into(),
            format!("Simulator version: {}", self.version.simulator_crate),
            format!("World generation version: {}", self.version.world_generation_version),
            String::new(),
            "Run configuration".into(),
            format!("  Agents: {}", self.config.agent_count),
            format!(
                "  Population mix (weights): wanderer {} / investigator {} / homebody {} / follower {} / social {} / visitor {}",
                self.config.population.wanderer,
                self.config.population.investigator,
                self.config.population.homebody,
                self.config.population.follower,
                self.config.population.social_player,
                self.config.population.occasional_visitor,
            ),
            format!("  Seed: {}", self.config.seed),
            format!(
                "  Configured duration: {} ms (~{:.2} days)",
                self.config.virtual_duration_ms,
                self.config.virtual_duration_ms as f64 / 86_400_000.0
            ),
            format!("  Action interval: {} ms", self.config.scheduler.action_interval_ms),
            String::new(),
            "Virtual time".into(),
            format!("  Configured start: {}", self.timing.configured_start_ms),
            format!("  Configured horizon: {}", self.timing.configured_horizon_ms),
            format!(
                "  Last event slot: {}",
                self.timing
                    .last_event_slot_ms
                    .map(|ms| ms.to_string())
                    .unwrap_or_else(|| "(none)".into())
            ),
            format!("  Final world clock: {}", self.timing.final_world_ms),
            format!("  Events processed: {}", self.timing.events_processed),
            format!("  Inactive slots (no command): {}", self.timing.inactive_slots),
            format!("  Stop reason: {:?}", self.timing.stop_reason),
            String::new(),
            "Execution outcomes".into(),
            format!("  Attempted actions: {}", self.execution.total_attempted),
            format!("  Successful: {}", self.execution.successful),
            format!("  Rejected: {}", self.execution.rejected),
            format!("  Failed: {}", self.execution.failed),
            format!("  Real elapsed: {} ms", self.execution.real_elapsed_ms),
            String::new(),
            "World outcomes".into(),
            format!(
                "  Unique locations visited (successful moves): {}",
                self.world.unique_locations_visited
            ),
            format!("  Unique regions visited (sim memory): {}", self.world.unique_regions_visited),
            format!(
                "  Unique locations occupied (end state): {}",
                self.world.unique_locations_occupied
            ),
            format!("  Regions materialized: {}", self.world.regions_materialized),
            format!("  Discoveries recorded: {}", self.world.discoveries_recorded),
            format!(
                "  Corroborated field keys (≥2 recorders): {}",
                self.world.corroborated_field_keys
            ),
            format!("  Theory subjects: {}", self.world.theory_subjects),
            format!("  Theory entries: {}", self.world.theory_entries),
            format!(
                "  Inspect: {} attempted / {} successful",
                self.world.inspect_attempts, self.world.successful_inspections
            ),
            format!(
                "  Exploration moves: {} attempted / {} successful",
                self.world.exploration_move_attempts, self.world.successful_exploration_moves
            ),
            format!("  Active home claims: {}", self.world.home_claims_active),
            format!("  Journal records (successful): {}", self.world.journal_records),
            format!(
                "  Social: {} attempted / {} successful",
                self.world.social_attempts, self.world.social_successes
            ),
            format!("  Repeat visits (sim memory): {}", self.world.repeat_visits),
            format!("  Home returns (sim memory): {}", self.world.home_returns),
            String::new(),
            "Cross-operator interaction (gameplay-derived)".into(),
            format!(
                "  Public traces created: {}",
                self.interaction.public_traces_created
            ),
            format!(
                "  Traces encountered by a later operator: {}",
                self.interaction.traces_encountered_by_later_operators
            ),
            format!(
                "  Distinct operators who encountered another's trace: {}",
                self.interaction.distinct_encountering_operators
            ),
            format!(
                "  Theory subjects with ≥2 authors: {}",
                self.interaction.theory_subjects_multi_author
            ),
            format!(
                "  Connected sequences (encounter → response): {}",
                self.interaction.connected_sequences
            ),
            format!(
                "  Trace encounter rate: {}",
                self.interaction
                    .trace_encounter_rate
                    .map(|r| format!("{r:.3}"))
                    .unwrap_or_else(|| "n/a (no traces)".into())
            ),
        ];
        if !self.story_transcript.lines.is_empty() {
            lines.push(String::new());
            lines.push("Representative story sequence".into());
            for beat in &self.story_transcript.lines {
                lines.push(format!("  t={} {} — {}", beat.slot_ms, beat.agent, beat.summary));
            }
        }
        if !self.per_profile.is_empty() {
            lines.push(String::new());
            lines.push("Per profile".into());
            for (name, stats) in &self.per_profile {
                lines.push(format!(
                    "  {name} ({} agents): attempted {} ok {} / inspect ok {} / journal {} / social ok {}",
                    stats.agent_count,
                    stats.attempted,
                    stats.successful,
                    stats.successful_inspections,
                    stats.journal_records,
                    stats.social_successes,
                ));
            }
        }
        if !self.thread_reports.is_empty() {
            lines.push(String::new());
            lines.push("Thread clues (simulated)".into());
            for report in &self.thread_reports {
                lines.push(format!(
                    "  {} — final clue '{}' reached by {} operator(s)",
                    report.thread_id, report.final_clue_id, report.operators_reaching_final_clue
                ));
                for (operator, hours) in &report.clues_by_operator_hour {
                    for (hour, count) in hours {
                        lines.push(format!(
                            "    {operator} hour {hour}: {count} clue(s) first seen"
                        ));
                    }
                }
            }
        }
        if !self.per_agent.is_empty() {
            lines.push(String::new());
            lines.push("Per agent".into());
            for (name, stats) in &self.per_agent {
                lines.push(format!(
                    "  {name} [{}]: ok {} / rejected {} / failed {} (attempted {})",
                    stats.profile, stats.successful, stats.rejected, stats.failed, stats.attempted
                ));
            }
        }
        lines.join("\n")
    }
}

pub struct WorldMetricsInput {
    pub unique_locations_visited: usize,
    pub unique_regions_visited: usize,
    pub inspect_attempts: u64,
    pub successful_inspections: u64,
    pub exploration_move_attempts: u64,
    pub successful_exploration_moves: u64,
    pub journal_records: u64,
    pub social_attempts: u64,
    pub social_successes: u64,
    pub repeat_visits: u64,
    pub home_returns: u64,
}

pub fn world_report_from_snapshot(
    snapshot: &WorldOutcomeSnapshot,
    metrics: WorldMetricsInput,
) -> WorldReport {
    WorldReport {
        unique_locations_visited: metrics.unique_locations_visited,
        unique_regions_visited: metrics.unique_regions_visited,
        unique_locations_occupied: snapshot.unique_locations_occupied,
        regions_materialized: snapshot.region_count,
        discoveries_recorded: snapshot.discovery_count,
        corroborated_field_keys: snapshot.corroborated_field_keys,
        theory_subjects: snapshot.theory_subjects,
        theory_entries: snapshot.theory_entries,
        inspect_attempts: metrics.inspect_attempts,
        successful_inspections: metrics.successful_inspections,
        exploration_move_attempts: metrics.exploration_move_attempts,
        successful_exploration_moves: metrics.successful_exploration_moves,
        home_claims_active: snapshot.active_home_count,
        journal_records: metrics.journal_records,
        social_attempts: metrics.social_attempts,
        social_successes: metrics.social_successes,
        repeat_visits: metrics.repeat_visits,
        home_returns: metrics.home_returns,
    }
}

pub fn aggregate_profile_stats(
    agents: &[crate::agent::SimAgent],
    inactive_by_index: &[u64],
) -> BTreeMap<String, ProfileStats> {
    let mut map = BTreeMap::new();
    for profile in Profile::ALL {
        let key = profile.as_str().to_string();
        let subset: Vec<_> = agents.iter().filter(|a| a.profile == profile).collect();
        if subset.is_empty() {
            continue;
        }
        let mut regions = std::collections::BTreeSet::new();
        let mut locations = std::collections::BTreeSet::new();
        for agent in &subset {
            regions.extend(agent.memory.region_keys_visited.iter().cloned());
            locations.extend(agent.memory.visited_locations.iter().cloned());
        }
        let inspections: u64 = subset.iter().map(|a| a.memory.successful_inspections).sum();
        let inactive: u64 = subset
            .iter()
            .map(|a| inactive_by_index.get(a.index as usize).copied().unwrap_or(0))
            .sum();
        map.insert(
            key,
            ProfileStats {
                agent_count: subset.len() as u32,
                attempted: subset.iter().map(|a| a.attempted).sum(),
                successful: subset.iter().map(|a| a.successful).sum(),
                rejected: subset.iter().map(|a| a.rejected).sum(),
                failed: subset.iter().map(|a| a.failed).sum(),
                unique_locations_visited: locations.len(),
                unique_regions_visited: regions.len(),
                successful_inspections: inspections,
                journal_records: subset.iter().map(|a| a.memory.journal_records).sum(),
                social_attempts: subset.iter().map(|a| a.memory.social_attempts).sum(),
                social_successes: subset.iter().map(|a| a.memory.social_successes).sum(),
                repeat_visits: subset.iter().map(|a| a.memory.repeat_visits).sum(),
                home_returns: subset.iter().map(|a| a.memory.home_returns).sum(),
                inactive_slots: inactive,
            },
        );
    }
    map
}

pub fn duration_to_ms(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

fn rate(numerator: u64, denominator: u64) -> Option<f64> {
    if denominator == 0 {
        None
    } else {
        Some(numerator as f64 / denominator as f64)
    }
}
