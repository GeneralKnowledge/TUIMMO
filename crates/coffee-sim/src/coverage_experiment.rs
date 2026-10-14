//! Offline coverage grid comparison (does not change production defaults).

use coffee_world::{measure_coverage_report, CoverageParams};
use serde::{Deserialize, Serialize};

use crate::population::PopulationMix;
use crate::runner::{run_with_config, RunConfig};
use crate::SimulationError;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DensitySettingResult {
    pub grid_step: u8,
    pub max_gap: u8,
    pub gap_p95_by_bucket: Vec<(String, f64)>,
    pub gap_max_by_bucket: Vec<(String, f64)>,
    pub median_time_to_first_clue_ms: Option<u64>,
    /// Sum of near-hub thread clue discoveries divided by virtual_days (not wall-clock hours).
    pub near_hub_clues_per_sim_day: f64,
    pub median_actions_to_first_thread_clue: Option<u64>,
    pub median_clues_per_100_actions: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DensityExperimentReport {
    pub seed: u64,
    pub settings: Vec<DensitySettingResult>,
    pub recommendation: String,
}

const SETTINGS: [(u8, u8); 4] = [(4, 4), (8, 8), (12, 12), (6, 6)];

pub fn run_density_experiment(seed: u64) -> Result<DensityExperimentReport, SimulationError> {
    let mut settings = Vec::new();
    for (grid_step, max_gap) in SETTINGS {
        let params = CoverageParams::new(grid_step, max_gap)
            .validate()
            .map_err(|e| SimulationError::InvalidPopulationMix(e.to_string()))?;
        let offline = measure_coverage_report(seed, params);
        let config = RunConfig {
            seed,
            agent_count: 16,
            population: PopulationMix::wanderer_vs_curious(),
            virtual_days: 3.0,
            max_actions: Some(800),
            coverage_params: params,
            include_coverage_metrics: false,
            ..RunConfig::default()
        };
        let virtual_days = config.virtual_days;
        let sim = run_with_config(config)?;
        let median_ttf = sim.clue_discovery.as_ref().and_then(|d| {
            let mut times: Vec<u64> =
                d.operators.iter().filter_map(|o| o.time_to_first_clue_ms).collect();
            times.sort();
            times.get(times.len() / 2).copied()
        });
        let near_hub_clues_per_sim_day = sim
            .clue_discovery
            .as_ref()
            .map(|d| {
                let near: u32 = d
                    .operators
                    .iter()
                    .flat_map(|o| o.clues_per_hour_by_bucket.get("0-100"))
                    .flat_map(|hours| hours.values())
                    .sum();
                near as f64 / virtual_days
            })
            .unwrap_or(0.0);

        let median_actions_to_first_thread_clue = sim.action_discovery.as_ref().and_then(|d| {
            let mut xs: Vec<u64> =
                d.operators.iter().filter_map(|o| o.actions_to_first_thread_clue).collect();
            xs.sort();
            xs.get(xs.len() / 2).copied()
        });
        let median_clues_per_100_actions = sim.action_discovery.as_ref().and_then(|d| {
            let mut xs: Vec<f64> = d.operators.iter().map(|o| o.clues_per_100_actions).collect();
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            xs.get(xs.len() / 2).copied()
        });

        settings.push(DensitySettingResult {
            grid_step,
            max_gap,
            gap_p95_by_bucket: offline
                .gap_by_bucket
                .iter()
                .map(|b| (b.bucket.label().to_string(), b.p95))
                .collect(),
            gap_max_by_bucket: offline
                .gap_by_bucket
                .iter()
                .map(|b| (b.bucket.label().to_string(), b.max))
                .collect(),
            median_time_to_first_clue_ms: median_ttf,
            near_hub_clues_per_sim_day,
            median_actions_to_first_thread_clue,
            median_clues_per_100_actions,
        });
    }

    let recommendation = recommend(&settings);
    Ok(DensityExperimentReport { seed, settings, recommendation })
}

fn recommend(settings: &[DensitySettingResult]) -> String {
    let default = settings.first();
    let sparse = settings.iter().find(|s| s.grid_step == 8);
    match (default, sparse) {
        (Some(d), Some(eight))
            if eight.near_hub_clues_per_sim_day < d.near_hub_clues_per_sim_day * 0.6 =>
        {
            format!(
                "Default (4,4) yields {:.1} near-hub thread clue discoveries/sim-day vs (8,8) at {:.1}; \
                 p95 gap remains ≤{:.0} tiles on (4,4). Consider (8,8) if inspect density feels like confetti — \
                 defaults unchanged in code.",
                d.near_hub_clues_per_sim_day,
                eight.near_hub_clues_per_sim_day,
                d.gap_p95_by_bucket.first().map(|(_, v)| *v).unwrap_or(0.0)
            )
        }
        _ => {
            "Default (4,4) balances gap guarantee with moderate discovery rates; no change recommended.".into()
        }
    }
}

pub fn format_coverage_report(seed: u64, params: CoverageParams) -> String {
    let report = measure_coverage_report(seed, params);
    let mut lines = vec![format!(
        "Coverage report seed={seed} grid={} gap={}",
        params.grid_step, params.max_gap
    )];
    for bucket in report.gap_by_bucket {
        lines.push(format!(
            "  gap {}: p50={:.1} p95={:.1} max={:.1} (n={})",
            bucket.bucket.label(),
            bucket.p50,
            bucket.p95,
            bucket.max,
            bucket.sample_tiles
        ));
    }
    for bucket in report.density_by_bucket {
        lines.push(format!(
            "  density {}: {:.1} features/1000 walkable ({} walkable, {} features)",
            bucket.bucket.label(),
            bucket.features_per_1000_walkable,
            bucket.walkable_tiles,
            bucket.feature_sites
        ));
    }
    lines.join("\n")
}
