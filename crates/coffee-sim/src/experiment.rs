//! Multi-run population-scale experiments.

use serde::{Deserialize, Serialize};

use crate::report::{DerivedRates, RunReport};
use crate::runner::RunConfig;
use crate::SimulationError;
use crate::SimulationRunner;

/// Maximum number of player counts in one scale experiment.
pub const MAX_SCALE_EXPERIMENT_RUNS: usize = 6;

#[derive(Clone, Debug)]
pub struct PopulationScaleRequest {
    pub base: RunConfig,
    pub player_counts: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PopulationScaleReport {
    pub player_counts: Vec<u32>,
    pub allocated_profiles: Vec<PopulationAllocation>,
    pub runs: Vec<RunReport>,
    pub derived: Vec<DerivedRates>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PopulationAllocation {
    pub agent_count: u32,
    pub counts_by_profile: std::collections::BTreeMap<String, u32>,
}

impl PopulationScaleReport {
    pub fn to_pretty_text(&self) -> String {
        let mut lines = vec!["Population scale experiment".into(), String::new()];
        for (idx, run) in self.runs.iter().enumerate() {
            let derived = &self.derived[idx];
            lines.push(format!(
                "N={}: attempted {} ({:.1}/agent) | unique locations {} ({:.2}/agent) | \
                 occupied {} | inactive share {}",
                run.config.agent_count,
                run.execution.total_attempted,
                derived.attempted_commands_per_agent,
                run.world.unique_locations_visited,
                derived.unique_locations_per_agent,
                run.world.unique_locations_occupied,
                derived
                    .inactive_slot_share
                    .map(|v| format!("{v:.3}"))
                    .unwrap_or_else(|| "(undefined)".into()),
            ));
        }
        lines.join("\n")
    }
}

pub fn run_population_scale_experiment(
    request: PopulationScaleRequest,
) -> Result<PopulationScaleReport, SimulationError> {
    if request.player_counts.is_empty() {
        return Err(SimulationError::EmptyScaleExperiment);
    }
    if request.player_counts.len() > MAX_SCALE_EXPERIMENT_RUNS {
        return Err(SimulationError::TooManyScaleRuns { max: MAX_SCALE_EXPERIMENT_RUNS });
    }
    let mut runs = Vec::with_capacity(request.player_counts.len());
    let mut derived = Vec::with_capacity(request.player_counts.len());
    let mut allocated_profiles = Vec::with_capacity(request.player_counts.len());
    for count in &request.player_counts {
        let mut config = request.base.clone();
        config.agent_count = *count;
        let report = SimulationRunner::new(config)?.run()?;
        derived.push(report.derived_rates());
        allocated_profiles.push(PopulationAllocation {
            agent_count: report.config.agent_count,
            counts_by_profile: report.config.allocated_profile_counts.clone(),
        });
        runs.push(report);
    }
    Ok(PopulationScaleReport {
        player_counts: request.player_counts,
        allocated_profiles,
        runs,
        derived,
    })
}
