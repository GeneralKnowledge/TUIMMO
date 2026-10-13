#![allow(clippy::unwrap_used)]

use coffee_sim::config::SnapshotConfig;
use coffee_sim::experiment::{
    run_population_scale_experiment, PopulationScaleRequest, MAX_SCALE_EXPERIMENT_RUNS,
};
use coffee_sim::population::PopulationMix;
use coffee_sim::runner::{run_with_config, RunConfig, MS_PER_DAY};
use coffee_sim::scheduler::SchedulerConfig;

fn snapshot_config(interval_days: f64) -> SnapshotConfig {
    SnapshotConfig {
        interval_ms: Some((interval_days * MS_PER_DAY as f64) as u64),
        max_snapshots: 64,
    }
}

#[test]
fn snapshot_timestamps_are_monotonic() {
    let mut config = RunConfig {
        seed: 11,
        agent_count: 3,
        virtual_days: 21.0,
        snapshots: snapshot_config(7.0),
        scheduler: SchedulerConfig { action_interval_ms: MS_PER_DAY / 2, idle_extra_ms: 0 },
        max_actions: Some(120),
        ..RunConfig::default()
    };
    config.population = PopulationMix::exploration_heavy();
    let report = run_with_config(config).expect("run");
    let snaps = report.longitudinal_snapshots.expect("snapshots");
    assert!(snaps.len() >= 2);
    for window in snaps.windows(2) {
        assert!(window[0].observation_ms < window[1].observation_ms);
        assert!(window[0].elapsed_virtual_ms < window[1].elapsed_virtual_ms);
    }
}

#[test]
fn snapshots_do_not_change_gameplay_outcomes() {
    let mut base = RunConfig {
        seed: 42,
        agent_count: 4,
        virtual_days: 14.0,
        scheduler: SchedulerConfig { action_interval_ms: 45 * 60 * 1000, idle_extra_ms: 0 },
        max_actions: Some(100),
        snapshots: SnapshotConfig::disabled(),
        ..RunConfig::default()
    };
    base.population = PopulationMix::all_profiles_mixed();
    let without = run_with_config(base.clone()).expect("without");
    base.snapshots = snapshot_config(3.0);
    let with = run_with_config(base).expect("with");
    fn gameplay_core(mut report: coffee_sim::RunReport) -> coffee_sim::RunReport {
        report.execution.real_elapsed_ms = 0;
        report.longitudinal_snapshots = None;
        report.config.snapshots = SnapshotConfig::disabled();
        report
    }
    assert_eq!(gameplay_core(without), gameplay_core(with));
}

#[test]
fn final_snapshot_cumulative_matches_report_when_present() {
    let config = RunConfig {
        seed: 5,
        agent_count: 2,
        virtual_days: 10.0,
        snapshots: snapshot_config(5.0),
        max_actions: Some(80),
        ..RunConfig::default()
    };
    let report = run_with_config(config).expect("run");
    let snaps = report.longitudinal_snapshots.expect("snapshots");
    let last = snaps.last().expect("last");
    assert_eq!(last.cumulative.attempted_commands, report.execution.total_attempted);
    assert_eq!(last.cumulative.successful_commands, report.execution.successful);
    assert_eq!(
        last.cumulative.unique_locations_visited_move_based,
        report.world.unique_locations_visited
    );
    assert_eq!(last.world_state.regions_materialized, report.world.regions_materialized);
}

#[test]
fn occupied_location_count_counts_each_place_once() {
    let mut config = RunConfig {
        seed: 77,
        agent_count: 6,
        virtual_days: 3.0,
        snapshots: SnapshotConfig { interval_ms: Some(MS_PER_DAY), max_snapshots: 8 },
        max_actions: Some(60),
        ..RunConfig::default()
    };
    config.population = PopulationMix::monolithic(coffee_sim::Profile::SocialPlayer);
    let report = run_with_config(config).expect("run");
    let snaps = report.longitudinal_snapshots.expect("snaps");
    let snap = snaps.last().unwrap();
    assert!(snap.occupancy.occupied_location_count <= report.config.agent_count as usize);
    assert!(snap.world_state.unique_locations_occupied <= report.config.agent_count as usize);
}

#[test]
fn population_scale_experiment_runs_are_isolated_and_reproducible() {
    let base = RunConfig {
        seed: 99,
        agent_count: 10,
        virtual_days: 5.0,
        max_actions: Some(80),
        population: PopulationMix::exploration_heavy(),
        ..RunConfig::default()
    };
    let req = PopulationScaleRequest { base: base.clone(), player_counts: vec![10, 25] };
    let a = run_population_scale_experiment(req.clone()).expect("scale");
    let b = run_population_scale_experiment(req).expect("scale again");
    assert_eq!(
        a.runs[0].clone().for_reproducibility_comparison(),
        b.runs[0].clone().for_reproducibility_comparison()
    );
    assert_ne!(a.runs[0].config.agent_count, a.runs[1].config.agent_count);
    assert_eq!(a.runs[0].config.seed, a.runs[1].config.seed);
}

#[test]
fn scale_experiment_rejects_too_many_sizes() {
    let base = RunConfig::default();
    let counts = vec![10; MAX_SCALE_EXPERIMENT_RUNS + 1];
    let err =
        run_population_scale_experiment(PopulationScaleRequest { base, player_counts: counts })
            .unwrap_err();
    assert!(matches!(err, coffee_sim::SimulationError::TooManyScaleRuns { .. }));
}

#[test]
fn modest_long_horizon_run_completes_with_snapshots() {
    let config = RunConfig {
        seed: 1,
        agent_count: 8,
        virtual_days: 30.0,
        snapshots: snapshot_config(7.0),
        scheduler: SchedulerConfig { action_interval_ms: MS_PER_DAY, idle_extra_ms: 0 },
        max_actions: Some(200),
        population: PopulationMix::home_oriented(),
        ..RunConfig::default()
    };
    let report = run_with_config(config).expect("long run");
    assert!(report.timing.configured_horizon_ms > report.timing.final_world_ms);
    let snaps = report.longitudinal_snapshots.expect("snapshots");
    assert!(snaps.len() <= 64);
    assert!(!snaps.is_empty());
}
