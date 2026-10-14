//! Multi-seed discovery benchmarks with action-based metrics.
//!
//! ## Metric definitions (canonical)
//!
//! - **actions_to_first_thread_clue**: simulator action attempts before the first successful
//!   `inspect` that records `clue_seen` for a **thread clue** (authored or procedural; excludes
//!   coverage flavor tagged `coverage`).
//! - **clues_per_100_actions**: `(thread clues found / actions attempted) × 100` per operator.
//! - **placement inventory** (offline): inspectable feature sites vs thread-clue sites per hub
//!   bucket; does not include discoveries.
//!
//! ## Legacy rate mismatch (PR #50)
//!
//! The density experiment printed `clues/hr(near)` as `sum(near-hub clue events) / virtual_days`,
//! which is **clues per simulated day**, not per hour. Long-run comparisons that divided by
//! `days × 24` produced ~0.07/hour. Both describe the same sparse discovery; only the divisor differed.

use coffee_world::{measure_placement_inventory, CoverageParams, PlacementInventoryReport};
use serde::{Deserialize, Serialize};

use crate::population::PopulationMix;
use crate::profile::Profile;
use crate::runner::{run_with_config, RunConfig};
use crate::stats::{numeric_summary, NumericSummary};
use crate::SimulationError;

pub const DISCOVERY_BENCHMARK_SEEDS: [u64; 5] = [42, 1000, 4242, 7777, 31_415];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProfileDiscoveryAggregate {
    pub profile: String,
    pub seeds: Vec<u64>,
    pub actions_to_first_thread_clue: NumericSummary,
    pub clues_per_100_actions: NumericSummary,
    pub share_found_any_thread_clue: f64,
    pub long_thread_chapter_1: NumericSummary,
    pub long_thread_chapter_2: NumericSummary,
    pub long_thread_chapter_3: NumericSummary,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiscoveryBenchmarkReport {
    pub metric_definitions: String,
    pub placement_inventory: PlacementInventoryReport,
    pub profiles: Vec<ProfileDiscoveryAggregate>,
}

pub fn run_discovery_benchmark(
    seeds: &[u64],
    agent_count: u32,
) -> Result<DiscoveryBenchmarkReport, SimulationError> {
    let world_seed = seeds.first().copied().unwrap_or(42);
    let placement_inventory = measure_placement_inventory(world_seed, CoverageParams::default());

    let configs: [(Profile, PopulationMix, Option<u64>); 4] = [
        (Profile::Wanderer, PopulationMix::monolithic(Profile::Wanderer), Some(800)),
        (Profile::Curious, PopulationMix::monolithic(Profile::Curious), Some(800)),
        (Profile::Newcomer, PopulationMix::monolithic_newcomer(), None),
        (Profile::Tracker, PopulationMix::monolithic_tracker(), Some(2000)),
    ];

    let mut profiles_out = Vec::new();
    for (profile, population, max_actions) in configs {
        let mut first_actions = Vec::new();
        let mut rates = Vec::new();
        let mut found_share = 0_u32;
        let mut ch1 = Vec::new();
        let mut ch2 = Vec::new();
        let mut ch3 = Vec::new();

        for &seed in seeds {
            let config = RunConfig {
                seed,
                agent_count,
                population: population.clone(),
                virtual_days: if profile == Profile::Newcomer { 3.0 } else { 7.0 },
                max_actions,
                include_coverage_metrics: false,
                ..RunConfig::default()
            };
            let report = run_with_config(config)?;
            let action = report.action_discovery.as_ref().expect("action discovery");
            for op in &action.operators {
                if op.found_any_thread_clue {
                    found_share += 1;
                }
                if let Some(a) = op.actions_to_first_thread_clue {
                    first_actions.push(a as f64);
                }
                rates.push(op.clues_per_100_actions);
            }
            if let Some(lp) = report.long_thread_progress {
                ch1.push(lp.operators_with_chapter_1 as f64);
                ch2.push(lp.operators_with_chapter_2 as f64);
                ch3.push(lp.operators_with_chapter_3 as f64);
            }
        }

        let n_agents = agent_count * u32::try_from(seeds.len()).unwrap_or(1);
        profiles_out.push(ProfileDiscoveryAggregate {
            profile: profile.as_str().to_string(),
            seeds: seeds.to_vec(),
            actions_to_first_thread_clue: numeric_summary(&first_actions),
            clues_per_100_actions: numeric_summary(&rates),
            share_found_any_thread_clue: if n_agents == 0 {
                0.0
            } else {
                f64::from(found_share) / f64::from(n_agents)
            },
            long_thread_chapter_1: numeric_summary(&ch1),
            long_thread_chapter_2: numeric_summary(&ch2),
            long_thread_chapter_3: numeric_summary(&ch3),
        });
    }

    Ok(DiscoveryBenchmarkReport {
        metric_definitions: METRIC_DEFINITIONS.to_string(),
        placement_inventory,
        profiles: profiles_out,
    })
}

const METRIC_DEFINITIONS: &str = "actions_to_first_thread_clue and clues_per_100_actions use thread clues only (not coverage flavor). placement_inventory counts sites placed, not found.";

pub fn format_discovery_benchmark(report: &DiscoveryBenchmarkReport) -> String {
    let mut lines = vec![report.metric_definitions.clone(), String::new()];
    lines.push("Placement (thread clue sites per 1000 walkable):".into());
    for bucket in &report.placement_inventory.by_bucket {
        lines.push(format!(
            "  {}: inspectable={} thread_sites={} thread/1000={:.2}",
            bucket.bucket.label(),
            bucket.inspectable_features,
            bucket.thread_clue_sites,
            bucket.thread_clues_per_1000_walkable
        ));
    }
    lines.push(String::new());
    for profile in &report.profiles {
        lines.push(format!("Profile: {}", profile.profile));
        lines.push(format!(
            "  share found ≥1 thread clue: {:.1}%",
            profile.share_found_any_thread_clue * 100.0
        ));
        if let Some(med) = profile.actions_to_first_thread_clue.median {
            lines.push(format!(
                "  actions to first thread clue: median={med:.0} mean={:.1} (n={})",
                profile.actions_to_first_thread_clue.mean.unwrap_or(0.0),
                profile.actions_to_first_thread_clue.n
            ));
        }
        if let Some(med) = profile.clues_per_100_actions.median {
            lines.push(format!(
                "  clues per 100 actions: median={med:.2} mean={:.2}",
                profile.clues_per_100_actions.mean.unwrap_or(0.0)
            ));
        }
        if profile.profile == "tracker" {
            lines.push(format!(
                "  long-procession reach (mean ops ch1/ch2/ch3): {:.1}/{:.1}/{:.1}",
                profile.long_thread_chapter_1.mean.unwrap_or(0.0),
                profile.long_thread_chapter_2.mean.unwrap_or(0.0),
                profile.long_thread_chapter_3.mean.unwrap_or(0.0)
            ));
        }
        lines.push(String::new());
    }
    lines.join("\n")
}
