//! Compare two population mixes under identical simulation parameters.

use serde::{Deserialize, Serialize};

use crate::population::PopulationMix;
use crate::report::{DerivedRates, RunReport};
use crate::runner::RunConfig;
use crate::SimulationError;
use crate::SimulationRunner;

#[derive(Clone, Debug)]
pub struct ComparisonRequest {
    pub base: RunConfig,
    pub mix_a: PopulationMix,
    pub mix_b: PopulationMix,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComparisonReport {
    pub mix_a: PopulationMix,
    pub mix_b: PopulationMix,
    pub run_a: RunReport,
    pub run_b: RunReport,
    pub summary: ComparisonSummary,
    pub derived_a: DerivedRates,
    pub derived_b: DerivedRates,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComparisonSummary {
    pub unique_locations_visited_delta: i64,
    pub successful_inspections_delta: i64,
    pub successful_exploration_moves_delta: i64,
    pub active_home_claims_delta: i64,
    pub journal_records_delta: i64,
    pub social_successes_delta: i64,
    pub total_attempted_delta: i64,
}

impl ComparisonReport {
    pub fn to_pretty_text(&self) -> String {
        format!(
            "Population comparison (seed {})\n\
             Mix A: {} wanderer / {} investigator / {} homebody / {} social / {} visitor\n\
             Mix B: {} wanderer / {} investigator / {} homebody / {} social / {} visitor\n\
             \n\
             Unique locations visited: A={} B={} (Δ {})\n\
             Successful inspections: A={} B={} (Δ {})\n\
             Successful exploration moves: A={} B={} (Δ {})\n\
             Active home claims: A={} B={} (Δ {})\n\
             Journal records (sim memory): A={} B={} (Δ {})\n\
             Social successes: A={} B={} (Δ {})\n\
             Attempted actions: A={} B={} (Δ {})\n\
             {}\n\
             {}",
            self.run_a.config.seed,
            self.mix_a.wanderer,
            self.mix_a.investigator,
            self.mix_a.homebody,
            self.mix_a.social_player,
            self.mix_a.occasional_visitor,
            self.mix_b.wanderer,
            self.mix_b.investigator,
            self.mix_b.homebody,
            self.mix_b.social_player,
            self.mix_b.occasional_visitor,
            self.run_a.world.unique_locations_visited,
            self.run_b.world.unique_locations_visited,
            self.summary.unique_locations_visited_delta,
            self.run_a.world.successful_inspections,
            self.run_b.world.successful_inspections,
            self.summary.successful_inspections_delta,
            self.run_a.world.successful_exploration_moves,
            self.run_b.world.successful_exploration_moves,
            self.summary.successful_exploration_moves_delta,
            self.run_a.world.home_claims_active,
            self.run_b.world.home_claims_active,
            self.summary.active_home_claims_delta,
            self.run_a.world.journal_records,
            self.run_b.world.journal_records,
            self.summary.journal_records_delta,
            self.run_a.world.social_successes,
            self.run_b.world.social_successes,
            self.summary.social_successes_delta,
            self.run_a.execution.total_attempted,
            self.run_b.execution.total_attempted,
            self.summary.total_attempted_delta,
            self.format_derived_block("A", &self.derived_a),
            self.format_derived_block("B", &self.derived_b),
        )
    }

    fn format_derived_block(&self, label: &str, d: &DerivedRates) -> String {
        format!(
            "Derived {label}: exploration move ok rate {} | inspect ok rate {} | social ok rate {} | \
             unique locations/agent {:.2} | attempted/agent {:.1} | inactive slot share {}\n",
            fmt_rate(d.exploration_move_success_rate),
            fmt_rate(d.inspect_success_rate),
            fmt_rate(d.social_success_rate),
            d.unique_locations_per_agent,
            d.attempted_commands_per_agent,
            fmt_rate(d.inactive_slot_share),
        )
    }
}

fn fmt_rate(rate: Option<f64>) -> String {
    match rate {
        Some(v) => format!("{v:.3}"),
        None => "(undefined)".into(),
    }
}

pub fn compare_populations(
    request: ComparisonRequest,
) -> Result<ComparisonReport, SimulationError> {
    let mut config_a = request.base.clone();
    config_a.population = request.mix_a.clone();
    let mut config_b = request.base.clone();
    config_b.population = request.mix_b.clone();
    let run_a = SimulationRunner::new(config_a)?.run()?;
    let run_b = SimulationRunner::new(config_b)?.run()?;
    let summary = ComparisonSummary {
        unique_locations_visited_delta: i64::try_from(run_b.world.unique_locations_visited)
            .unwrap_or(0)
            - i64::try_from(run_a.world.unique_locations_visited).unwrap_or(0),
        successful_inspections_delta: i64::try_from(run_b.world.successful_inspections)
            .unwrap_or(0)
            - i64::try_from(run_a.world.successful_inspections).unwrap_or(0),
        successful_exploration_moves_delta: i64::try_from(run_b.world.successful_exploration_moves)
            .unwrap_or(0)
            - i64::try_from(run_a.world.successful_exploration_moves).unwrap_or(0),
        active_home_claims_delta: i64::try_from(run_b.world.home_claims_active).unwrap_or(0)
            - i64::try_from(run_a.world.home_claims_active).unwrap_or(0),
        journal_records_delta: i64::try_from(run_b.world.journal_records).unwrap_or(0)
            - i64::try_from(run_a.world.journal_records).unwrap_or(0),
        social_successes_delta: i64::try_from(run_b.world.social_successes).unwrap_or(0)
            - i64::try_from(run_a.world.social_successes).unwrap_or(0),
        total_attempted_delta: i64::try_from(run_b.execution.total_attempted).unwrap_or(0)
            - i64::try_from(run_a.execution.total_attempted).unwrap_or(0),
    };
    let derived_a = run_a.derived_rates();
    let derived_b = run_b.derived_rates();
    Ok(ComparisonReport {
        mix_a: request.mix_a,
        mix_b: request.mix_b,
        run_a,
        run_b,
        summary,
        derived_a,
        derived_b,
    })
}
