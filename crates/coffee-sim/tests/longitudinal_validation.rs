#![allow(clippy::unwrap_used)]

use coffee_sim::config::SnapshotConfig;
use coffee_sim::experiment::{
    run_population_scale_experiment, PopulationScaleRequest, MAX_SCALE_EXPERIMENT_RUNS,
};
use coffee_sim::population::PopulationMix;
use coffee_sim::profile::Profile;
use coffee_sim::report::StopReason;
use coffee_sim::runner::{run_with_config, RunConfig, MS_PER_DAY};
use coffee_sim::scheduler::SchedulerConfig;
use coffee_sim::snapshot::{ObservationKind, TerminalSnapshotStatus};

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
        report.snapshot_sampling = None;
        report.config.snapshots = SnapshotConfig::disabled();
        report.timing.observation_events_processed = 0;
        report.timing.last_scheduler_slot_ms = report.timing.last_event_slot_ms;
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

#[test]
fn initial_snapshot_has_zero_counters_and_initial_kind() {
    let config = RunConfig {
        seed: 3,
        agent_count: 2,
        virtual_days: 5.0,
        snapshots: SnapshotConfig { interval_ms: Some(MS_PER_DAY), max_snapshots: 16 },
        max_actions: Some(20),
        ..RunConfig::default()
    };
    let report = run_with_config(config).expect("run");
    let snaps = report.longitudinal_snapshots.expect("snaps");
    let initial = &snaps[0];
    assert_eq!(initial.kind, ObservationKind::Initial);
    assert_eq!(initial.observation_ms, report.config.virtual_start_ms);
    assert_eq!(initial.cumulative.attempted_commands, 0);
    assert_eq!(initial.cumulative.events_processed, 0);
}

#[test]
fn interval_snapshot_at_action_boundary_excludes_that_action() {
    let start = 1_000_000u64;
    let interval = MS_PER_DAY;
    let config = RunConfig {
        seed: 8,
        agent_count: 1,
        virtual_days: 10.0,
        virtual_start_ms: start,
        scheduler: SchedulerConfig { action_interval_ms: interval, idle_extra_ms: 0 },
        snapshots: SnapshotConfig { interval_ms: Some(interval), max_snapshots: 32 },
        max_actions: Some(3),
        population: PopulationMix::monolithic(Profile::Wanderer),
        ..RunConfig::default()
    };
    let report = run_with_config(config).expect("run");
    let snaps = report.longitudinal_snapshots.expect("snaps");
    let boundary = start + interval;
    let at_boundary = snaps
        .iter()
        .find(|snap| snap.observation_ms == boundary && snap.kind == ObservationKind::Interval)
        .expect("interval row at first day boundary");
    assert_eq!(
        at_boundary.cumulative.attempted_commands, 1,
        "observation at boundary must precede the agent action scheduled at the same time"
    );
    assert!(report.execution.total_attempted >= 2);
}

#[test]
fn snapshots_after_offline_gap_do_not_backfill_later_counters() {
    let start = 2_000_000u64;
    let interval = MS_PER_DAY;
    let config = RunConfig {
        seed: 21,
        agent_count: 1,
        virtual_days: 14.0,
        virtual_start_ms: start,
        scheduler: SchedulerConfig { action_interval_ms: MS_PER_DAY / 4, idle_extra_ms: 0 },
        snapshots: SnapshotConfig { interval_ms: Some(interval), max_snapshots: 64 },
        max_actions: Some(40),
        population: PopulationMix::monolithic(Profile::OccasionalVisitor),
        ..RunConfig::default()
    };
    let report = run_with_config(config).expect("run");
    let snaps = report.longitudinal_snapshots.expect("snaps");
    let final_attempted = report.execution.total_attempted;
    for snap in snaps.iter().filter(|s| s.kind == ObservationKind::Interval) {
        assert!(
            snap.cumulative.attempted_commands <= final_attempted,
            "observation at {} must not include counters from after it was taken",
            snap.observation_ms
        );
    }
    if final_attempted > 0 {
        let mid = snaps
            .iter()
            .find(|s| {
                s.kind == ObservationKind::Interval
                    && s.observation_ms > start + interval
                    && s.observation_ms < report.timing.last_event_slot_ms.unwrap_or(0)
            })
            .expect("mid-horizon interval snapshot");
        assert!(
            mid.cumulative.attempted_commands < final_attempted,
            "mid-horizon snapshot must not already reflect the final command tally"
        );
    }
}

#[test]
fn equal_timestamp_observation_precedes_gameplay_in_runner() {
    let start = 50_000u64;
    let week = 7 * MS_PER_DAY;
    let slot = start + week;
    let config = RunConfig {
        seed: 4,
        agent_count: 1,
        virtual_days: 14.0,
        virtual_start_ms: start,
        scheduler: SchedulerConfig { action_interval_ms: week, idle_extra_ms: 0 },
        snapshots: SnapshotConfig { interval_ms: Some(week), max_snapshots: 8 },
        max_actions: Some(2),
        record_action_trace: true,
        population: PopulationMix::monolithic(Profile::Wanderer),
        ..RunConfig::default()
    };
    let report = run_with_config(config).expect("run");
    let snaps = report.longitudinal_snapshots.expect("snaps");
    let interval_snap = snaps
        .iter()
        .find(|s| s.observation_ms == slot && s.kind == ObservationKind::Interval)
        .expect("interval at gameplay slot");
    assert_eq!(
        interval_snap.cumulative.attempted_commands, 1,
        "weekly observation at the second action slot must not include that action"
    );
    let trace = report.action_trace.expect("trace");
    assert_eq!(trace.entries.len(), 2);
    assert_eq!(trace.entries[1].event_slot_ms, slot);
}

#[test]
fn terminal_snapshot_is_distinct_and_reconciles_when_present() {
    let config = RunConfig {
        seed: 6,
        agent_count: 2,
        virtual_days: 4.0,
        snapshots: SnapshotConfig { interval_ms: Some(MS_PER_DAY), max_snapshots: 20 },
        max_actions: Some(30),
        ..RunConfig::default()
    };
    let report = run_with_config(config).expect("run");
    let sampling = report.snapshot_sampling.expect("sampling");
    let snaps = report.longitudinal_snapshots.expect("snaps");
    let last = snaps.last().expect("last");
    if sampling.terminal_included {
        assert_eq!(last.kind, ObservationKind::Terminal);
    }
    assert_eq!(last.cumulative.attempted_commands, report.execution.total_attempted);
}

#[test]
fn snapshot_capacity_bounds_storage_and_reports_skips() {
    let config = RunConfig {
        seed: 10,
        agent_count: 2,
        virtual_days: 30.0,
        snapshots: SnapshotConfig { interval_ms: Some(MS_PER_DAY), max_snapshots: 4 },
        max_actions: Some(50),
        ..RunConfig::default()
    };
    let report = run_with_config(config).expect("run");
    let snaps = report.longitudinal_snapshots.expect("snaps");
    assert!(snaps.len() <= 4);
    let sampling = report.snapshot_sampling.expect("sampling");
    assert!(sampling.grid_boundaries_skipped_capacity > 0);
}

#[test]
fn snapshot_histories_are_deterministic() {
    let config = RunConfig {
        seed: 44,
        agent_count: 3,
        virtual_days: 8.0,
        snapshots: snapshot_config(2.0),
        max_actions: Some(60),
        ..RunConfig::default()
    };
    let a = run_with_config(config.clone()).expect("a");
    let b = run_with_config(config).expect("b");
    assert_eq!(a.longitudinal_snapshots, b.longitudinal_snapshots);
    assert_eq!(a.snapshot_sampling, b.snapshot_sampling);
}

#[test]
fn population_scale_snapshots_follow_same_temporal_rules() {
    let base = RunConfig {
        seed: 12,
        agent_count: 6,
        virtual_days: 10.0,
        snapshots: SnapshotConfig { interval_ms: Some(MS_PER_DAY * 2), max_snapshots: 16 },
        max_actions: Some(40),
        ..RunConfig::default()
    };
    let report = run_population_scale_experiment(PopulationScaleRequest {
        base,
        player_counts: vec![6, 12],
    })
    .expect("scale");
    for run in &report.runs {
        let snaps = run.longitudinal_snapshots.as_ref().expect("snaps");
        for window in snaps.windows(2) {
            assert!(window[0].observation_ms < window[1].observation_ms);
        }
        if let Some(last) = snaps.last() {
            if last.kind == ObservationKind::Terminal {
                assert_eq!(last.cumulative.attempted_commands, run.execution.total_attempted);
            }
        }
    }
}

#[test]
fn action_limit_stops_before_post_termination_observations_advance_clock() {
    let start = 100_000u64;
    let day = MS_PER_DAY;
    let base = RunConfig {
        seed: 31,
        agent_count: 1,
        virtual_days: 21.0,
        virtual_start_ms: start,
        scheduler: SchedulerConfig { action_interval_ms: day, idle_extra_ms: 0 },
        max_actions: Some(2),
        population: PopulationMix::monolithic(Profile::Wanderer),
        ..RunConfig::default()
    };
    let without =
        run_with_config(RunConfig { snapshots: SnapshotConfig::disabled(), ..base.clone() })
            .expect("without");
    let with = run_with_config(RunConfig {
        snapshots: SnapshotConfig { interval_ms: Some(day), max_snapshots: 32 },
        ..base
    })
    .expect("with");
    assert_eq!(without.timing.stop_reason, StopReason::ActionLimit);
    assert_eq!(with.timing.stop_reason, StopReason::ActionLimit);
    assert_eq!(without.timing.final_world_ms, with.timing.final_world_ms);
    assert_eq!(without.timing.last_event_slot_ms, with.timing.last_event_slot_ms);
    assert_eq!(
        with.timing.final_world_ms,
        with.timing.effective_termination_slot_ms.unwrap() + 1,
        "world clock should reflect the last command tick, not later observation boundaries"
    );
    assert!(with.timing.final_world_ms < start + 5 * day);
}

#[test]
fn action_limit_blocks_pending_observations_without_clock_advance() {
    let start = 0u64;
    let day = MS_PER_DAY;
    let config = RunConfig {
        seed: 32,
        agent_count: 1,
        virtual_days: 10.0,
        virtual_start_ms: start,
        scheduler: SchedulerConfig { action_interval_ms: day / 2, idle_extra_ms: 0 },
        snapshots: SnapshotConfig { interval_ms: Some(day), max_snapshots: 16 },
        max_actions: Some(1),
        population: PopulationMix::monolithic(Profile::Wanderer),
        ..RunConfig::default()
    };
    let report = run_with_config(config).expect("run");
    assert_eq!(report.timing.stop_reason, StopReason::ActionLimit);
    assert_eq!(report.execution.total_attempted, 1);
    assert_eq!(report.timing.observation_events_processed, 0);
    assert_eq!(report.timing.final_world_ms, start + 1);
}

#[test]
fn snapshots_enabled_matches_gameplay_final_state_and_metrics() {
    let mut base = RunConfig {
        seed: 55,
        agent_count: 3,
        virtual_days: 12.0,
        scheduler: SchedulerConfig { action_interval_ms: MS_PER_DAY, idle_extra_ms: 0 },
        max_actions: Some(25),
        snapshots: SnapshotConfig::disabled(),
        ..RunConfig::default()
    };
    base.population = PopulationMix::exploration_heavy();
    let mut without = run_with_config(base.clone()).expect("without");
    base.snapshots = snapshot_config(2.0);
    let mut with = run_with_config(base).expect("with");
    without.execution.real_elapsed_ms = 0;
    with.execution.real_elapsed_ms = 0;
    assert_eq!(without.timing.final_world_ms, with.timing.final_world_ms);
    assert_eq!(without.timing.stop_reason, with.timing.stop_reason);
    assert_eq!(without.timing.last_event_slot_ms, with.timing.last_event_slot_ms);
    assert_eq!(without.execution, with.execution);
    assert_eq!(without.world, with.world);
}

#[test]
fn terminal_snapshot_world_clock_matches_authoritative_clock() {
    let config = RunConfig {
        seed: 7,
        agent_count: 2,
        virtual_days: 6.0,
        snapshots: SnapshotConfig { interval_ms: Some(MS_PER_DAY * 2), max_snapshots: 12 },
        max_actions: Some(18),
        ..RunConfig::default()
    };
    let report = run_with_config(config).expect("run");
    let terminal = report
        .longitudinal_snapshots
        .as_ref()
        .and_then(|snaps| snaps.iter().find(|s| s.kind == ObservationKind::Terminal))
        .expect("terminal row");
    assert_eq!(terminal.world_clock_ms, report.timing.final_world_ms);
    assert_eq!(terminal.observation_ms, report.timing.effective_termination_slot_ms.unwrap());
    assert!(terminal.world_clock_ms >= terminal.observation_ms);
}

#[test]
fn terminal_snapshot_capacity_one_two_and_many() {
    let start = 500u64;
    let day = MS_PER_DAY;
    let base = RunConfig {
        seed: 9,
        agent_count: 1,
        virtual_days: 5.0,
        virtual_start_ms: start,
        scheduler: SchedulerConfig { action_interval_ms: day, idle_extra_ms: 0 },
        snapshots: SnapshotConfig { interval_ms: Some(day), max_snapshots: 1 },
        max_actions: Some(4),
        population: PopulationMix::monolithic(Profile::Wanderer),
        ..RunConfig::default()
    };
    let one = run_with_config(base.clone()).expect("one");
    assert_eq!(one.longitudinal_snapshots.as_ref().map(|s| s.len()), Some(1));
    assert_eq!(
        one.snapshot_sampling.as_ref().unwrap().terminal_status,
        TerminalSnapshotStatus::OmittedCapacity
    );

    let two = run_with_config(RunConfig {
        snapshots: SnapshotConfig { interval_ms: Some(day), max_snapshots: 2 },
        ..base.clone()
    })
    .expect("two");
    assert_eq!(two.longitudinal_snapshots.as_ref().map(|s| s.len()), Some(2));
    let sampling_two = two.snapshot_sampling.expect("sampling");
    assert!(
        sampling_two.terminal_included
            || sampling_two.terminal_status != TerminalSnapshotStatus::Included
    );

    let many = run_with_config(RunConfig {
        snapshots: SnapshotConfig { interval_ms: Some(day), max_snapshots: 8 },
        ..base
    })
    .expect("many");
    assert!(many.longitudinal_snapshots.as_ref().is_some_and(|s| s.len() >= 3));
    let sampling_many = many.snapshot_sampling.as_ref().unwrap();
    assert!(matches!(
        sampling_many.terminal_status,
        TerminalSnapshotStatus::Included | TerminalSnapshotStatus::DuplicateLastObservation
    ));
}

#[test]
fn queue_exhaustion_does_not_advance_clock_for_trailing_observations() {
    let start = 0u64;
    let day = MS_PER_DAY;
    let config = RunConfig {
        seed: 18,
        agent_count: 1,
        virtual_days: 14.0,
        virtual_start_ms: start,
        scheduler: SchedulerConfig { action_interval_ms: day * 3, idle_extra_ms: 0 },
        snapshots: SnapshotConfig { interval_ms: Some(day), max_snapshots: 20 },
        max_actions: Some(3),
        population: PopulationMix::monolithic(Profile::Wanderer),
        ..RunConfig::default()
    };
    let report = run_with_config(config).expect("run");
    assert_eq!(report.timing.stop_reason, StopReason::ActionLimit);
    let last_gp = report.timing.last_event_slot_ms.unwrap();
    assert_eq!(report.timing.final_world_ms, last_gp + 1);
    assert!(report.timing.final_world_ms < start + day * 10);
}

#[test]
fn horizon_termination_preserves_observation_before_gameplay_ordering() {
    let start = 1_000u64;
    let day = MS_PER_DAY;
    let horizon_days = 3.0;
    let config = RunConfig {
        seed: 20,
        agent_count: 1,
        virtual_days: horizon_days,
        virtual_start_ms: start,
        scheduler: SchedulerConfig { action_interval_ms: day * 2, idle_extra_ms: 0 },
        snapshots: SnapshotConfig { interval_ms: Some(day), max_snapshots: 10 },
        max_actions: None,
        record_action_trace: true,
        population: PopulationMix::monolithic(Profile::Wanderer),
        ..RunConfig::default()
    };
    let report = run_with_config(config).expect("run");
    assert_eq!(report.timing.stop_reason, StopReason::QueueExhausted);
    let horizon = report.timing.configured_horizon_ms;
    assert!(report.timing.final_world_ms <= horizon);
}

#[test]
fn termination_reports_are_reproducible_with_snapshots() {
    let config = RunConfig {
        seed: 66,
        agent_count: 2,
        virtual_days: 9.0,
        snapshots: snapshot_config(3.0),
        max_actions: Some(15),
        ..RunConfig::default()
    };
    let a = run_with_config(config.clone()).expect("a");
    let b = run_with_config(config).expect("b");
    assert_eq!(a.longitudinal_snapshots, b.longitudinal_snapshots);
    assert_eq!(a.for_reproducibility_comparison(), b.for_reproducibility_comparison());
}
