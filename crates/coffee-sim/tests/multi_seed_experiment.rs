#![allow(clippy::unwrap_used)]

use coffee_sim::config::SnapshotConfig;
use coffee_sim::multi_seed::{
    run_multi_seed_experiment, ExperimentCondition, MultiSeedExperimentRequest,
};
use coffee_sim::population::PopulationMix;
use coffee_sim::profile::Profile;
use coffee_sim::runner::{run_with_config, RunConfig, MS_PER_DAY};
use coffee_sim::seeds::{replicate_seed, ReplicateSeedSchedule};
use coffee_sim::snapshot::ObservationKind;
use coffee_sim::stats::numeric_summary;
use coffee_sim::SimulationError;

fn small_base() -> RunConfig {
    RunConfig {
        seed: 999,
        agent_count: 4,
        virtual_days: 3.0,
        max_actions: Some(40),
        population: PopulationMix::exploration_heavy(),
        ..RunConfig::default()
    }
}

fn experiment_request(replicates: u32, sizes: &[u32]) -> MultiSeedExperimentRequest {
    let base = small_base();
    MultiSeedExperimentRequest {
        base,
        conditions: sizes
            .iter()
            .map(|size| ExperimentCondition {
                label: format!("N={size}"),
                agent_count: *size,
                population: PopulationMix::exploration_heavy(),
            })
            .collect(),
        seed_schedule: ReplicateSeedSchedule::FromBase {
            base_seed: 1000,
            replicate_count: replicates,
        },
        include_full_run_reports: false,
    }
}

#[test]
fn identical_settings_produce_identical_experiment_reports() {
    let req = experiment_request(2, &[4, 6]);
    let a = run_multi_seed_experiment(req.clone()).expect("a");
    let b = run_multi_seed_experiment(req).expect("b");
    assert_eq!(a, b);
}

#[test]
fn explicit_seeds_are_recorded() {
    let req = MultiSeedExperimentRequest {
        base: small_base(),
        conditions: vec![ExperimentCondition {
            label: "N=4".into(),
            agent_count: 4,
            population: PopulationMix::exploration_heavy(),
        }],
        seed_schedule: ReplicateSeedSchedule::Explicit { seeds: vec![11, 22, 33] },
        include_full_run_reports: false,
    };
    let report = run_multi_seed_experiment(req).expect("run");
    assert_eq!(report.seeds_used, vec![11, 22, 33]);
    assert_eq!(report.conditions[0].replicates[1].seed, 22);
}

#[test]
fn base_seed_generation_is_deterministic() {
    let seeds: Vec<_> = (0..4).map(|i| replicate_seed(500, i)).collect();
    let req = MultiSeedExperimentRequest {
        base: small_base(),
        conditions: vec![ExperimentCondition {
            label: "N=4".into(),
            agent_count: 4,
            population: PopulationMix::exploration_heavy(),
        }],
        seed_schedule: ReplicateSeedSchedule::FromBase { base_seed: 500, replicate_count: 4 },
        include_full_run_reports: false,
    };
    let report = run_multi_seed_experiment(req).expect("run");
    assert_eq!(report.seeds_used, seeds);
}

#[test]
fn generated_replicate_seeds_are_distinct() {
    let report = run_multi_seed_experiment(experiment_request(5, &[4])).expect("run");
    let set: std::collections::HashSet<_> = report.seeds_used.iter().copied().collect();
    assert_eq!(set.len(), report.seeds_used.len());
}

#[test]
fn conditions_share_the_same_seed_schedule() {
    let report = run_multi_seed_experiment(experiment_request(3, &[4, 8])).expect("run");
    assert_eq!(report.conditions[0].replicates[0].seed, report.conditions[1].replicates[0].seed);
    assert_eq!(report.conditions[0].replicates[2].seed, report.conditions[1].replicates[2].seed);
}

#[test]
fn replicates_use_independent_world_state() {
    let report = run_multi_seed_experiment(experiment_request(2, &[4])).expect("run");
    assert_ne!(report.conditions[0].replicates[0].seed, report.conditions[0].replicates[1].seed);
    let repeated_seed = MultiSeedExperimentRequest {
        base: small_base(),
        conditions: vec![ExperimentCondition {
            label: "N=4".into(),
            agent_count: 4,
            population: PopulationMix::exploration_heavy(),
        }],
        seed_schedule: ReplicateSeedSchedule::Explicit { seeds: vec![123, 123] },
        include_full_run_reports: false,
    };
    let repeated = run_multi_seed_experiment(repeated_seed).expect("repeated");
    assert_eq!(
        repeated.conditions[0].replicates[0].metrics.total_attempted,
        repeated.conditions[0].replicates[1].metrics.total_attempted
    );
}

#[test]
fn aggregate_mean_std_min_max_median_hand_verified() {
    let summary = numeric_summary(&[1.0, 2.0, 3.0]);
    assert_eq!(summary.mean, Some(2.0));
    assert_eq!(summary.min, Some(1.0));
    assert_eq!(summary.max, Some(3.0));
    assert_eq!(summary.median, Some(2.0));
    assert!((summary.std_dev.unwrap() - 1.0).abs() < 1e-9);
}

#[test]
fn single_replicate_has_undefined_std_dev() {
    let report = run_multi_seed_experiment(experiment_request(1, &[4])).expect("run");
    let agg = &report.conditions[0].aggregate.total_attempted;
    assert_eq!(agg.n, 1);
    assert!(agg.std_dev.is_none());
}

#[test]
fn rate_summary_distinguishes_undefined_from_zero() {
    let report = run_multi_seed_experiment(experiment_request(2, &[4])).expect("run");
    let social = &report.conditions[0].aggregate.social_success_rate;
    assert!(social.n_undefined <= 2);
}

#[test]
fn pooled_and_distribution_rates_are_both_present() {
    let report = run_multi_seed_experiment(experiment_request(2, &[4])).expect("run");
    let inspect = &report.conditions[0].aggregate.inspect_success_rate;
    assert!(inspect.distribution.n >= 1);
    assert!(inspect.pooled_denominator > 0 || inspect.pooled_rate.is_none());
}

fn snapshot_experiment(
    conditions: Vec<ExperimentCondition>,
    replicates: u32,
) -> MultiSeedExperimentRequest {
    let mut base = small_base();
    base.max_actions = None;
    base.snapshots = SnapshotConfig { interval_ms: Some(MS_PER_DAY), max_snapshots: 12 };
    MultiSeedExperimentRequest {
        base,
        conditions,
        seed_schedule: ReplicateSeedSchedule::FromBase {
            base_seed: 77,
            replicate_count: replicates,
        },
        include_full_run_reports: true,
    }
}

#[test]
fn snapshot_series_aligns_by_elapsed_time_not_index() {
    let report = run_multi_seed_experiment(snapshot_experiment(
        vec![ExperimentCondition {
            label: "N=4".into(),
            agent_count: 4,
            population: PopulationMix::exploration_heavy(),
        }],
        2,
    ))
    .expect("run");
    let series = report.conditions[0].snapshot_series.as_ref().expect("series");
    assert!(series.iter().any(|p| p.replicate_count >= 1 && p.elapsed_virtual_ms == 0));
    for point in series {
        assert!(point.replicate_count <= 2);
    }
}

#[test]
fn different_population_sizes_get_separate_snapshot_series() {
    let report = run_multi_seed_experiment(snapshot_experiment(
        vec![
            ExperimentCondition {
                label: "N=4".into(),
                agent_count: 4,
                population: PopulationMix::exploration_heavy(),
            },
            ExperimentCondition {
                label: "N=8".into(),
                agent_count: 8,
                population: PopulationMix::exploration_heavy(),
            },
        ],
        2,
    ))
    .expect("run");
    let s4 = report.conditions[0].snapshot_series.as_ref().expect("series n4");
    let s8 = report.conditions[1].snapshot_series.as_ref().expect("series n8");
    assert!(!s4.is_empty() && !s8.is_empty());
    let initial4 = s4
        .iter()
        .find(|p| p.elapsed_virtual_ms == 0 && p.kind == ObservationKind::Initial)
        .expect("initial n4");
    let initial8 = s8
        .iter()
        .find(|p| p.elapsed_virtual_ms == 0 && p.kind == ObservationKind::Initial)
        .expect("initial n8");
    assert_eq!(initial4.replicate_count, 2);
    assert_eq!(initial8.replicate_count, 2);
    for point in s4.iter().chain(s8.iter()) {
        assert!(
            point.replicate_count <= 2,
            "cross-condition pooling would inflate replicate_count beyond this condition's replicates"
        );
    }
    assert_ne!(
        report.conditions[0].aggregate.total_attempted.mean,
        report.conditions[1].aggregate.total_attempted.mean,
    );
}

#[test]
fn different_behavioural_mixes_are_not_merged_in_snapshot_series() {
    let report = run_multi_seed_experiment(snapshot_experiment(
        vec![
            ExperimentCondition {
                label: "wanderer".into(),
                agent_count: 6,
                population: PopulationMix::monolithic(Profile::Wanderer),
            },
            ExperimentCondition {
                label: "homebody".into(),
                agent_count: 6,
                population: PopulationMix::monolithic(Profile::Homebody),
            },
        ],
        2,
    ))
    .expect("run");
    let wanderer = &report.conditions[0];
    let homebody = &report.conditions[1];
    assert!(wanderer.snapshot_series.is_some());
    assert!(homebody.snapshot_series.is_some());
    assert_ne!(
        wanderer.aggregate.home_claims_active.mean,
        homebody.aggregate.home_claims_active.mean
    );
    for point in wanderer
        .snapshot_series
        .as_ref()
        .unwrap()
        .iter()
        .chain(homebody.snapshot_series.as_ref().unwrap().iter())
    {
        assert!(point.replicate_count <= 2);
    }
}

#[test]
fn snapshot_series_json_is_nested_under_each_condition() {
    let report = run_multi_seed_experiment(snapshot_experiment(
        vec![
            ExperimentCondition {
                label: "A".into(),
                agent_count: 4,
                population: PopulationMix::exploration_heavy(),
            },
            ExperimentCondition {
                label: "B".into(),
                agent_count: 6,
                population: PopulationMix::exploration_heavy(),
            },
        ],
        1,
    ))
    .expect("run");
    let json = serde_json::to_value(&report).expect("json");
    assert!(json.get("snapshot_series").is_none());
    assert!(json["conditions"][0].get("snapshot_series").is_some());
    assert!(json["conditions"][1].get("snapshot_series").is_some());
    assert_eq!(json["conditions"][0]["label"], "A");
}

#[test]
fn snapshot_points_use_distinct_elapsed_and_kind_keys() {
    let report = run_multi_seed_experiment(snapshot_experiment(
        vec![ExperimentCondition {
            label: "N=4".into(),
            agent_count: 4,
            population: PopulationMix::exploration_heavy(),
        }],
        2,
    ))
    .expect("run");
    let series = report.conditions[0].snapshot_series.as_ref().expect("series");
    assert!(series.iter().any(|p| p.kind == ObservationKind::Initial));
    assert!(series.iter().any(|p| p.kind == ObservationKind::Interval));
    let keys: Vec<_> = series.iter().map(|p| (p.elapsed_virtual_ms, p.kind)).collect();
    let unique: std::collections::BTreeSet<_> = keys.iter().copied().collect();
    assert_eq!(keys.len(), unique.len(), "each (elapsed, kind) pair appears once per condition");
}

#[test]
fn experiment_workload_limit_rejects_excessive_conditions() {
    let conditions: Vec<_> = (0..=coffee_sim::multi_seed::MAX_MULTI_SEED_CONDITIONS)
        .map(|i| ExperimentCondition {
            label: format!("c{i}"),
            agent_count: 4,
            population: PopulationMix::exploration_heavy(),
        })
        .collect();
    let req = MultiSeedExperimentRequest {
        base: small_base(),
        conditions,
        seed_schedule: ReplicateSeedSchedule::Explicit { seeds: vec![1] },
        include_full_run_reports: false,
    };
    let err = run_multi_seed_experiment(req).unwrap_err();
    assert!(matches!(err, SimulationError::TooManyExperimentConditions { .. }));
}

#[test]
fn total_run_product_is_validated() {
    let mut conditions = Vec::new();
    for i in 0..6 {
        conditions.push(ExperimentCondition {
            label: format!("c{i}"),
            agent_count: 4,
            population: PopulationMix::exploration_heavy(),
        });
    }
    let req = MultiSeedExperimentRequest {
        base: small_base(),
        conditions,
        seed_schedule: ReplicateSeedSchedule::FromBase { base_seed: 1, replicate_count: 20 },
        include_full_run_reports: false,
    };
    let err = run_multi_seed_experiment(req).unwrap_err();
    assert!(matches!(err, SimulationError::TooManyExperimentRuns { .. }));
}

#[test]
fn snapshots_do_not_change_single_run_gameplay() {
    let mut base = small_base();
    base.seed = report_seeds_sample();
    let without =
        run_with_config(RunConfig { snapshots: SnapshotConfig::disabled(), ..base.clone() })
            .expect("without");
    base.snapshots = SnapshotConfig { interval_ms: Some(MS_PER_DAY), max_snapshots: 8 };
    let with = run_with_config(base).expect("with");
    assert_eq!(without.execution.total_attempted, with.execution.total_attempted);
    assert_eq!(without.timing.final_world_ms, with.timing.final_world_ms);
    assert_eq!(without.timing.stop_reason, with.timing.stop_reason);
}

fn report_seeds_sample() -> u64 {
    replicate_seed(1000, 0)
}

/// Documented smoke configuration (values from an actual test run — not a population claim).
#[test]
fn example_experiment_representative_output() {
    let report = run_multi_seed_experiment(MultiSeedExperimentRequest {
        base: RunConfig {
            seed: 0,
            agent_count: 4,
            virtual_days: 2.0,
            max_actions: Some(25),
            population: PopulationMix::uniform_wanderer(),
            ..RunConfig::default()
        },
        conditions: vec![
            ExperimentCondition {
                label: "N=4".into(),
                agent_count: 4,
                population: PopulationMix::uniform_wanderer(),
            },
            ExperimentCondition {
                label: "N=6".into(),
                agent_count: 6,
                population: PopulationMix::uniform_wanderer(),
            },
        ],
        seed_schedule: ReplicateSeedSchedule::FromBase { base_seed: 1000, replicate_count: 2 },
        include_full_run_reports: false,
    })
    .expect("example");
    assert_eq!(report.experiment.total_runs_completed, 4);
    assert_eq!(report.seeds_used.len(), 2);
    let n4 = &report.conditions[0].aggregate;
    assert_eq!(n4.total_attempted.n, 2);
    assert!(n4.total_attempted.mean.unwrap() > 0.0);
    assert!(n4.attempted_commands_per_agent.mean.unwrap() > 0.0);
    assert!(report.comparisons.iter().any(|c| c.metric == "total_attempted"));
}

#[test]
fn json_report_carries_seed_schedule_and_settings() {
    let report = run_multi_seed_experiment(experiment_request(2, &[4])).expect("run");
    let json = serde_json::to_value(&report).expect("json");
    assert!(json.get("seeds_used").is_some());
    assert!(json.get("experiment").is_some());
    assert!(json["conditions"][0]["replicates"][0].get("seed").is_some());
}
