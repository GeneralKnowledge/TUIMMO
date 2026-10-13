//! Run configuration and outcome aggregates (human text + JSON).

use std::collections::BTreeMap;
use std::time::Duration;

use coffee_world::WorldOutcomeSnapshot;
use serde::{Deserialize, Serialize};

use crate::scheduler::SchedulerConfig;

pub const SIM_CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimVersion {
    pub simulator_crate: String,
    pub world_generation_version: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunReport {
    pub version: SimVersion,
    pub config: RunReportConfig,
    pub execution: ExecutionStats,
    pub world: WorldReport,
    pub per_agent: BTreeMap<String, AgentStats>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunReportConfig {
    pub seed: u64,
    pub agent_count: u32,
    pub virtual_start_ms: u64,
    pub virtual_end_ms: u64,
    pub virtual_duration_ms: u64,
    pub scheduler: SchedulerConfig,
    pub max_actions: Option<u64>,
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
    pub unique_locations_visited: usize,
    pub unique_locations_occupied: usize,
    pub regions_materialized: usize,
    pub discoveries_recorded: usize,
    pub inspect_actions: u64,
    pub exploration_moves: u64,
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
    ///
    /// Gameplay outcomes and virtual time must match; wall-clock duration may differ.
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
                "  Virtual duration: {} ms (~{:.2} days)",
                self.config.virtual_duration_ms,
                self.config.virtual_duration_ms as f64 / 86_400_000.0
            ),
            format!(
                "  Virtual window: {} .. {}",
                self.config.virtual_start_ms, self.config.virtual_end_ms
            ),
            format!("  Action interval: {} ms", self.config.scheduler.action_interval_ms),
            format!("  Total attempted actions: {}", self.execution.total_attempted),
            String::new(),
            "Execution outcomes".into(),
            format!("  Successful: {}", self.execution.successful),
            format!("  Rejected: {}", self.execution.rejected),
            format!("  Failed: {}", self.execution.failed),
            format!("  Real elapsed: {} ms", self.execution.real_elapsed_ms),
            String::new(),
            "World outcomes".into(),
            format!("  Unique locations visited: {}", self.world.unique_locations_visited),
            format!(
                "  Unique locations occupied (agents): {}",
                self.world.unique_locations_occupied
            ),
            format!("  Regions materialized: {}", self.world.regions_materialized),
            format!("  Discoveries recorded: {}", self.world.discoveries_recorded),
            format!("  Inspect actions: {}", self.world.inspect_actions),
            format!("  Exploration moves: {}", self.world.exploration_moves),
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

pub fn world_report_from_snapshot(
    snapshot: &WorldOutcomeSnapshot,
    unique_locations_visited: usize,
    inspect: u64,
    moves: u64,
) -> WorldReport {
    WorldReport {
        unique_locations_visited,
        unique_locations_occupied: snapshot.unique_locations_occupied,
        regions_materialized: snapshot.region_count,
        discoveries_recorded: snapshot.discovery_count,
        inspect_actions: inspect,
        exploration_moves: moves,
        home_claims_active: snapshot.active_home_count,
    }
}

pub fn duration_to_ms(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}
