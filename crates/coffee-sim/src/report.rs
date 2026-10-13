//! Run configuration and outcome aggregates (human text + JSON).

use std::collections::BTreeMap;
use std::time::Duration;

use coffee_world::WorldOutcomeSnapshot;
use serde::{Deserialize, Serialize};

use crate::scheduler::SchedulerConfig;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_trace: Option<ActionTrace>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunReportConfig {
    pub seed: u64,
    pub agent_count: u32,
    pub virtual_start_ms: u64,
    pub virtual_duration_ms: u64,
    pub scheduler: SchedulerConfig,
    pub max_actions: Option<u64>,
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
    /// Scheduler slot time of the last processed event, if any.
    pub last_event_slot_ms: Option<u64>,
    /// Authoritative world clock after the last processed command.
    pub final_world_ms: u64,
    pub events_processed: u64,
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
    /// Distinct place keys where agents stand at end of run.
    pub unique_locations_occupied: usize,
    /// Regions present in geography state (materialized during play).
    pub regions_materialized: usize,
    /// Discovery rows recorded (visits, notes, etc.).
    pub discoveries_recorded: usize,
    pub inspect_attempts: u64,
    pub successful_inspections: u64,
    pub exploration_move_attempts: u64,
    pub successful_exploration_moves: u64,
    pub home_claims_active: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentStats {
    pub attempted: u64,
    pub successful: u64,
    pub rejected: u64,
    pub failed: u64,
}

impl RunReport {
    /// Same report with operational-only fields cleared for seed/config equality checks.
    pub fn for_reproducibility_comparison(mut self) -> Self {
        self.execution.real_elapsed_ms = 0;
        self
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
            format!(
                "  Unique locations occupied (end state): {}",
                self.world.unique_locations_occupied
            ),
            format!("  Regions materialized: {}", self.world.regions_materialized),
            format!("  Discoveries recorded: {}", self.world.discoveries_recorded),
            format!(
                "  Inspect: {} attempted / {} successful",
                self.world.inspect_attempts, self.world.successful_inspections
            ),
            format!(
                "  Exploration moves: {} attempted / {} successful",
                self.world.exploration_move_attempts, self.world.successful_exploration_moves
            ),
            format!("  Active home claims: {}", self.world.home_claims_active),
        ];
        if !self.per_agent.is_empty() {
            lines.push(String::new());
            lines.push("Per agent".into());
            for (name, stats) in &self.per_agent {
                lines.push(format!(
                    "  {name}: ok {} / rejected {} / failed {} (attempted {})",
                    stats.successful, stats.rejected, stats.failed, stats.attempted
                ));
            }
        }
        lines.join("\n")
    }
}

pub struct WorldMetricsInput {
    pub unique_locations_visited: usize,
    pub inspect_attempts: u64,
    pub successful_inspections: u64,
    pub exploration_move_attempts: u64,
    pub successful_exploration_moves: u64,
}

pub fn world_report_from_snapshot(
    snapshot: &WorldOutcomeSnapshot,
    metrics: WorldMetricsInput,
) -> WorldReport {
    WorldReport {
        unique_locations_visited: metrics.unique_locations_visited,
        unique_locations_occupied: snapshot.unique_locations_occupied,
        regions_materialized: snapshot.region_count,
        discoveries_recorded: snapshot.discovery_count,
        inspect_attempts: metrics.inspect_attempts,
        successful_inspections: metrics.successful_inspections,
        exploration_move_attempts: metrics.exploration_move_attempts,
        successful_exploration_moves: metrics.successful_exploration_moves,
        home_claims_active: snapshot.active_home_count,
    }
}

pub fn duration_to_ms(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}
