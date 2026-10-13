//! Multi-seed population experiments: conditions, replicates, aggregates.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::population::PopulationMix;
use crate::report::{RunReport, StopReason};
use crate::runner::RunConfig;
use crate::seeds::ReplicateSeedSchedule;
use crate::snapshot::ObservationKind;
use crate::stats::{numeric_summary, rate_summary, NumericSummary, RateSummary};
use crate::SimulationError;
use crate::SimulationRunner;

pub const MAX_MULTI_SEED_CONDITIONS: usize = 12;
pub const MAX_MULTI_SEED_REPLICATES: usize = 32;
pub const MAX_MULTI_SEED_TOTAL_RUNS: usize = 96;

#[derive(Clone, Debug)]
pub struct MultiSeedExperimentRequest {
    pub base: RunConfig,
    pub conditions: Vec<ExperimentCondition>,
    pub seed_schedule: ReplicateSeedSchedule,
    /// When false, only compact replicate summaries are stored (default).
    pub include_full_run_reports: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExperimentCondition {
    pub label: String,
    pub agent_count: u32,
    pub population: PopulationMix,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MultiSeedExperimentReport {
    pub experiment: ExperimentSettings,
    pub seed_schedule: ReplicateSeedSchedule,
    pub seeds_used: Vec<u64>,
    pub conditions: Vec<ConditionResult>,
    pub comparisons: Vec<ConditionMeanComparison>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExperimentSettings {
    pub virtual_days: f64,
    pub virtual_start_ms: u64,
    pub max_actions: Option<u64>,
    pub snapshot_interval_ms: Option<u64>,
    pub snapshot_max: u32,
    pub total_runs_requested: u32,
    pub total_runs_completed: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConditionResult {
    pub label: String,
    pub agent_count: u32,
    pub population: PopulationMix,
    pub allocated_profile_counts: BTreeMap<String, u32>,
    pub replicates: Vec<ReplicateRecord>,
    pub aggregate: ConditionAggregate,
    /// Longitudinal summaries for this condition only (one entry per aligned time/kind).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_series: Option<Vec<SnapshotSeriesPoint>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReplicateRecord {
    pub replicate_index: u32,
    pub seed: u64,
    pub stop_reason: StopReason,
    pub final_world_ms: u64,
    pub metrics: ReplicateMetrics,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_report: Option<RunReport>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReplicateMetrics {
    pub total_attempted: u64,
    pub total_successful: u64,
    pub attempted_commands_per_agent: f64,
    pub unique_locations_visited: f64,
    pub unique_locations_per_agent: f64,
    pub unique_regions_visited: f64,
    pub successful_exploration_moves: f64,
    pub successful_inspections: f64,
    pub repeat_visits: f64,
    pub home_returns: f64,
    pub home_claims_active: f64,
    pub social_attempts: f64,
    pub social_successes: f64,
    pub inactive_slots: f64,
    pub unique_locations_occupied: f64,
    pub max_agents_sharing_one_location: f64,
    pub hub_agent_share: Option<f64>,
    pub deep_squalor_agent_share: Option<f64>,
    pub exploration_move_success_rate: Option<f64>,
    pub inspect_success_rate: Option<f64>,
    pub social_success_rate: Option<f64>,
    pub inactive_slot_share: Option<f64>,
    pub exploration_move_attempts: u64,
    pub inspect_attempts: u64,
    pub events_processed: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConditionAggregate {
    pub total_attempted: NumericSummary,
    pub attempted_commands_per_agent: NumericSummary,
    pub unique_locations_visited: NumericSummary,
    pub unique_locations_per_agent: NumericSummary,
    pub unique_regions_visited: NumericSummary,
    pub successful_exploration_moves: NumericSummary,
    pub successful_inspections: NumericSummary,
    pub repeat_visits: NumericSummary,
    pub home_returns: NumericSummary,
    pub home_claims_active: NumericSummary,
    pub social_attempts: NumericSummary,
    pub social_successes: NumericSummary,
    pub inactive_slots: NumericSummary,
    pub unique_locations_occupied: NumericSummary,
    pub max_agents_sharing_one_location: NumericSummary,
    pub hub_agent_share: NumericSummary,
    pub deep_squalor_agent_share: NumericSummary,
    pub final_world_ms: NumericSummary,
    pub exploration_move_success_rate: RateSummary,
    pub inspect_success_rate: RateSummary,
    pub social_success_rate: RateSummary,
    pub inactive_slot_share: RateSummary,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConditionMeanComparison {
    pub metric: String,
    pub baseline_label: String,
    pub other_label: String,
    pub baseline_mean: Option<f64>,
    pub other_mean: Option<f64>,
    pub mean_delta: Option<f64>,
    pub relative_delta: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapshotSeriesPoint {
    pub elapsed_virtual_ms: u64,
    pub observation_ms: u64,
    pub kind: ObservationKind,
    pub replicate_count: u32,
    pub cumulative_unique_locations_visited: NumericSummary,
    pub cumulative_attempted_commands: NumericSummary,
    pub current_unique_locations_occupied: NumericSummary,
}

pub fn run_multi_seed_experiment(
    request: MultiSeedExperimentRequest,
) -> Result<MultiSeedExperimentReport, SimulationError> {
    if request.conditions.is_empty() {
        return Err(SimulationError::EmptyExperimentConditions);
    }
    if request.conditions.len() > MAX_MULTI_SEED_CONDITIONS {
        return Err(SimulationError::TooManyExperimentConditions {
            max: MAX_MULTI_SEED_CONDITIONS,
        });
    }
    let seeds = request.seed_schedule.resolve()?;
    if seeds.len() > MAX_MULTI_SEED_REPLICATES {
        return Err(SimulationError::TooManyExperimentReplicates {
            max: MAX_MULTI_SEED_REPLICATES,
        });
    }
    let total_runs = request
        .conditions
        .len()
        .checked_mul(seeds.len())
        .ok_or(SimulationError::ExperimentWorkloadOverflow)?;
    if total_runs > MAX_MULTI_SEED_TOTAL_RUNS {
        return Err(SimulationError::TooManyExperimentRuns {
            requested: total_runs,
            max: MAX_MULTI_SEED_TOTAL_RUNS,
        });
    }
    for condition in &request.conditions {
        condition.population.validate()?;
        let mut probe = request.base.clone();
        probe.agent_count = condition.agent_count;
        probe.population = condition.population.clone();
        probe.validate()?;
    }

    let snapshots_enabled = request.base.snapshots.interval_ms.is_some();
    let mut condition_results = Vec::with_capacity(request.conditions.len());

    for condition in &request.conditions {
        let mut replicates = Vec::with_capacity(seeds.len());
        let mut run_reports = Vec::new();
        for (replicate_index, seed) in seeds.iter().enumerate() {
            let mut config = request.base.clone();
            config.seed = *seed;
            config.agent_count = condition.agent_count;
            config.population = condition.population.clone();
            let report = SimulationRunner::new(config)?.run()?;
            let metrics = replicate_metrics_from_report(&report);
            if request.include_full_run_reports {
                replicates.push(ReplicateRecord {
                    replicate_index: replicate_index as u32,
                    seed: *seed,
                    stop_reason: report.timing.stop_reason,
                    final_world_ms: report.timing.final_world_ms,
                    metrics,
                    full_report: Some(report.clone()),
                });
            } else {
                replicates.push(ReplicateRecord {
                    replicate_index: replicate_index as u32,
                    seed: *seed,
                    stop_reason: report.timing.stop_reason,
                    final_world_ms: report.timing.final_world_ms,
                    metrics,
                    full_report: None,
                });
            }
            run_reports.push(report);
        }
        let allocated = run_reports
            .first()
            .map(|r| r.config.allocated_profile_counts.clone())
            .unwrap_or_default();
        let aggregate = aggregate_condition(&replicates);
        let snapshot_series = if snapshots_enabled {
            Some(aggregate_snapshot_series_for_condition(&run_reports))
        } else {
            None
        };
        condition_results.push(ConditionResult {
            label: condition.label.clone(),
            agent_count: condition.agent_count,
            population: condition.population.clone(),
            allocated_profile_counts: allocated,
            replicates,
            aggregate,
            snapshot_series,
        });
    }

    let comparisons = compare_condition_means(&condition_results);

    Ok(MultiSeedExperimentReport {
        experiment: ExperimentSettings {
            virtual_days: request.base.virtual_days,
            virtual_start_ms: request.base.virtual_start_ms,
            max_actions: request.base.max_actions,
            snapshot_interval_ms: request.base.snapshots.interval_ms,
            snapshot_max: request.base.snapshots.max_snapshots,
            total_runs_requested: total_runs as u32,
            total_runs_completed: total_runs as u32,
        },
        seed_schedule: request.seed_schedule.clone(),
        seeds_used: seeds,
        conditions: condition_results,
        comparisons,
    })
}

impl MultiSeedExperimentReport {
    pub fn to_pretty_text(&self) -> String {
        let mut lines = vec![
            "Multi-seed population experiment".into(),
            format!(
                "Runs: {} conditions × {} seeds = {} completed",
                self.conditions.len(),
                self.seeds_used.len(),
                self.experiment.total_runs_completed
            ),
            String::new(),
        ];
        for condition in &self.conditions {
            let a = &condition.aggregate;
            lines.push(format!(
                "Condition \"{}\" (N={}): attempted mean {:.1} (σ {}) | unique locations mean {:.1} (σ {}) | inactive share mean {}",
                condition.label,
                condition.agent_count,
                a.total_attempted.mean.unwrap_or(f64::NAN),
                fmt_opt(a.total_attempted.std_dev),
                a.unique_locations_visited.mean.unwrap_or(f64::NAN),
                fmt_opt(a.unique_locations_visited.std_dev),
                fmt_rate_summary(&a.inactive_slot_share),
            ));
        }
        if self.conditions.len() >= 2 {
            lines.push(String::new());
            lines.push("Mean comparisons vs baseline:".into());
            for cmp in &self.comparisons {
                if cmp.baseline_label == self.conditions[0].label {
                    lines.push(format!(
                        "  {} vs {} — {}: Δ mean {}",
                        cmp.baseline_label,
                        cmp.other_label,
                        cmp.metric,
                        fmt_opt(cmp.mean_delta),
                    ));
                }
            }
        }
        lines.join("\n")
    }
}

fn fmt_opt(v: Option<f64>) -> String {
    v.map(|x| format!("{x:.3}")).unwrap_or_else(|| "n/a".into())
}

fn fmt_rate_summary(r: &RateSummary) -> String {
    match r.distribution.mean {
        Some(m) => format!("{m:.3} (n={})", r.n_defined),
        None => format!("undefined (n={})", r.n_defined),
    }
}

fn replicate_metrics_from_report(report: &RunReport) -> ReplicateMetrics {
    let derived = report.derived_rates();
    let hub = report
        .longitudinal_snapshots
        .as_ref()
        .and_then(|snaps| snaps.last())
        .and_then(|s| s.occupancy.hub_agent_share);
    let squalor = report
        .longitudinal_snapshots
        .as_ref()
        .and_then(|snaps| snaps.last())
        .and_then(|s| s.occupancy.deep_squalor_agent_share);
    let max_coloc = report
        .longitudinal_snapshots
        .as_ref()
        .and_then(|snaps| snaps.last())
        .map(|s| f64::from(s.occupancy.max_agents_sharing_one_location))
        .unwrap_or(0.0);
    ReplicateMetrics {
        total_attempted: report.execution.total_attempted,
        total_successful: report.execution.successful,
        attempted_commands_per_agent: derived.attempted_commands_per_agent,
        unique_locations_visited: report.world.unique_locations_visited as f64,
        unique_locations_per_agent: derived.unique_locations_per_agent,
        unique_regions_visited: report.world.unique_regions_visited as f64,
        successful_exploration_moves: report.world.successful_exploration_moves as f64,
        successful_inspections: report.world.successful_inspections as f64,
        repeat_visits: report.world.repeat_visits as f64,
        home_returns: report.world.home_returns as f64,
        home_claims_active: report.world.home_claims_active as f64,
        social_attempts: report.world.social_attempts as f64,
        social_successes: report.world.social_successes as f64,
        inactive_slots: report.timing.inactive_slots as f64,
        unique_locations_occupied: report.world.unique_locations_occupied as f64,
        max_agents_sharing_one_location: max_coloc,
        hub_agent_share: hub,
        deep_squalor_agent_share: squalor,
        exploration_move_success_rate: derived.exploration_move_success_rate,
        inspect_success_rate: derived.inspect_success_rate,
        social_success_rate: derived.social_success_rate,
        inactive_slot_share: derived.inactive_slot_share,
        exploration_move_attempts: report.world.exploration_move_attempts,
        inspect_attempts: report.world.inspect_attempts,
        events_processed: report.timing.events_processed,
    }
}

fn aggregate_condition(replicates: &[ReplicateRecord]) -> ConditionAggregate {
    let f = |extract: fn(&ReplicateMetrics) -> f64| {
        numeric_summary(&replicates.iter().map(|r| extract(&r.metrics)).collect::<Vec<_>>())
    };
    let opt_f = |extract: fn(&ReplicateMetrics) -> Option<f64>| {
        replicates.iter().map(|r| extract(&r.metrics)).collect::<Vec<_>>()
    };
    ConditionAggregate {
        total_attempted: f(|m| m.total_attempted as f64),
        attempted_commands_per_agent: f(|m| m.attempted_commands_per_agent),
        unique_locations_visited: f(|m| m.unique_locations_visited),
        unique_locations_per_agent: f(|m| m.unique_locations_per_agent),
        unique_regions_visited: f(|m| m.unique_regions_visited),
        successful_exploration_moves: f(|m| m.successful_exploration_moves),
        successful_inspections: f(|m| m.successful_inspections),
        repeat_visits: f(|m| m.repeat_visits),
        home_returns: f(|m| m.home_returns),
        home_claims_active: f(|m| m.home_claims_active),
        social_attempts: f(|m| m.social_attempts),
        social_successes: f(|m| m.social_successes),
        inactive_slots: f(|m| m.inactive_slots),
        unique_locations_occupied: f(|m| m.unique_locations_occupied),
        max_agents_sharing_one_location: f(|m| m.max_agents_sharing_one_location),
        hub_agent_share: numeric_summary(
            &opt_f(|m| m.hub_agent_share).into_iter().flatten().collect::<Vec<_>>(),
        ),
        deep_squalor_agent_share: numeric_summary(
            &opt_f(|m| m.deep_squalor_agent_share).into_iter().flatten().collect::<Vec<_>>(),
        ),
        final_world_ms: numeric_summary(
            &replicates.iter().map(|r| r.final_world_ms as f64).collect::<Vec<_>>(),
        ),
        exploration_move_success_rate: rate_summary(
            &opt_f(|m| m.exploration_move_success_rate),
            replicates.iter().map(|r| r.metrics.successful_exploration_moves as u64).sum(),
            replicates.iter().map(|r| r.metrics.exploration_move_attempts).sum(),
        ),
        inspect_success_rate: rate_summary(
            &opt_f(|m| m.inspect_success_rate),
            replicates.iter().map(|r| r.metrics.successful_inspections as u64).sum(),
            replicates.iter().map(|r| r.metrics.inspect_attempts).sum(),
        ),
        social_success_rate: rate_summary(
            &opt_f(|m| m.social_success_rate),
            replicates.iter().map(|r| r.metrics.social_successes as u64).sum(),
            replicates.iter().map(|r| r.metrics.social_attempts as u64).sum(),
        ),
        inactive_slot_share: rate_summary(
            &opt_f(|m| m.inactive_slot_share),
            replicates.iter().map(|r| r.metrics.inactive_slots as u64).sum(),
            replicates.iter().map(|r| r.metrics.events_processed).sum(),
        ),
    }
}

fn compare_condition_means(conditions: &[ConditionResult]) -> Vec<ConditionMeanComparison> {
    let mut out = Vec::new();
    if conditions.len() < 2 {
        return out;
    }
    let baseline = &conditions[0];
    let metrics: &[(&str, &NumericSummary)] = &[
        ("total_attempted", &baseline.aggregate.total_attempted),
        ("unique_locations_visited", &baseline.aggregate.unique_locations_visited),
        ("attempted_commands_per_agent", &baseline.aggregate.attempted_commands_per_agent),
        ("unique_locations_per_agent", &baseline.aggregate.unique_locations_per_agent),
    ];
    for other in &conditions[1..] {
        for (name, base_summary) in metrics {
            let other_summary = match *name {
                "total_attempted" => &other.aggregate.total_attempted,
                "unique_locations_visited" => &other.aggregate.unique_locations_visited,
                "attempted_commands_per_agent" => &other.aggregate.attempted_commands_per_agent,
                "unique_locations_per_agent" => &other.aggregate.unique_locations_per_agent,
                _ => continue,
            };
            let (baseline_mean, other_mean, mean_delta, relative_delta) =
                mean_delta_pair(base_summary.mean, other_summary.mean);
            out.push(ConditionMeanComparison {
                metric: (*name).into(),
                baseline_label: baseline.label.clone(),
                other_label: other.label.clone(),
                baseline_mean,
                other_mean,
                mean_delta,
                relative_delta,
            });
        }
    }
    out
}

fn mean_delta_pair(
    baseline: Option<f64>,
    other: Option<f64>,
) -> (Option<f64>, Option<f64>, Option<f64>, Option<f64>) {
    let delta = match (baseline, other) {
        (Some(b), Some(o)) => Some(o - b),
        _ => None,
    };
    let relative = match (baseline, delta) {
        (Some(b), Some(d)) if b != 0.0 => Some(d / b),
        _ => None,
    };
    (baseline, other, delta, relative)
}

/// Aggregates snapshots across replicates of a **single** condition.
fn aggregate_snapshot_series_for_condition(runs: &[RunReport]) -> Vec<SnapshotSeriesPoint> {
    struct Acc {
        observation_ms: u64,
        locs: Vec<f64>,
        attempted: Vec<f64>,
        occupied: Vec<f64>,
    }
    let mut points: BTreeMap<(u64, ObservationKind), Acc> = BTreeMap::new();
    for report in runs {
        let Some(snaps) = report.longitudinal_snapshots.as_ref() else {
            continue;
        };
        for snap in snaps {
            let entry = points.entry((snap.elapsed_virtual_ms, snap.kind)).or_insert_with(|| Acc {
                observation_ms: snap.observation_ms,
                locs: Vec::new(),
                attempted: Vec::new(),
                occupied: Vec::new(),
            });
            entry.observation_ms = snap.observation_ms;
            entry.locs.push(snap.cumulative.unique_locations_visited_move_based as f64);
            entry.attempted.push(snap.cumulative.attempted_commands as f64);
            entry.occupied.push(snap.world_state.unique_locations_occupied as f64);
        }
    }
    points
        .into_iter()
        .map(|((elapsed, kind), acc)| SnapshotSeriesPoint {
            elapsed_virtual_ms: elapsed,
            observation_ms: acc.observation_ms,
            kind,
            replicate_count: acc.locs.len() as u32,
            cumulative_unique_locations_visited: numeric_summary(&acc.locs),
            cumulative_attempted_commands: numeric_summary(&acc.attempted),
            current_unique_locations_occupied: numeric_summary(&acc.occupied),
        })
        .collect()
}
