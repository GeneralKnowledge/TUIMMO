#![allow(clippy::unwrap_used)]

use std::collections::BTreeSet;

use coffee_sim::agent::{sim_account_id, sim_player_id, SimAgent};
use coffee_sim::compare::{compare_populations, ComparisonRequest};
use coffee_sim::execute::{execute_line, ActionResult};
use coffee_sim::population::PopulationMix;
use coffee_sim::profile::Profile;
use coffee_sim::report::StopReason;
use coffee_sim::runner::{run_with_config, RunConfig, SimulationError, MS_PER_DAY};
use coffee_sim::scheduler::SchedulerConfig;
use coffee_sim::trace::TraceOutcome;
use coffee_world::geography::{gate_identity, Terrain, WildernessPos};
use coffee_world::home::{Habitation, SETTLING_TIME_MS};
use coffee_world::{WorldState, WORLD_GENERATION_VERSION};

fn base_config() -> RunConfig {
    RunConfig {
        seed: 99,
        agent_count: 5,
        virtual_days: 2.0,
        scheduler: SchedulerConfig { action_interval_ms: 30 * 60 * 1000, idle_extra_ms: 0 },
        max_actions: Some(200),
        virtual_start_ms: 1_000_000,
        record_action_trace: false,
        ..RunConfig::default()
    }
}

#[test]
fn identical_seed_and_config_produce_identical_reports() {
    let config = base_config();
    let a = run_with_config(config.clone()).expect("run a");
    let b = run_with_config(config).expect("run b");
    assert_eq!(a.for_reproducibility_comparison(), b.for_reproducibility_comparison());
}

#[test]
fn per_agent_attempts_reconcile_with_global_total() {
    let report = run_with_config(base_config()).expect("run");
    let sum: u64 = report.per_agent.values().map(|stats| stats.attempted).sum();
    assert_eq!(sum, report.execution.total_attempted);
    let ok: u64 = report.per_agent.values().map(|stats| stats.successful).sum();
    assert_eq!(ok, report.execution.successful);
}

#[test]
fn multi_agent_first_actions_use_configured_start_slot() {
    let mut config = base_config();
    config.agent_count = 4;
    config.max_actions = Some(4);
    config.record_action_trace = true;
    config.virtual_start_ms = 5_000_000;
    let report = run_with_config(config).expect("run");
    let trace = report.action_trace.expect("trace");
    assert_eq!(trace.entries.len(), 4);
    for entry in &trace.entries {
        assert_eq!(entry.event_slot_ms, 5_000_000);
        assert!(entry.world_ms_before_command >= 5_000_000);
        assert_eq!(entry.world_ms_after_command, entry.world_ms_before_command + 1);
    }
}

#[test]
fn equal_timestamp_events_are_deterministic_by_agent_index() {
    let mut config = base_config();
    config.agent_count = 3;
    config.max_actions = Some(3);
    config.record_action_trace = true;
    let report = run_with_config(config.clone()).expect("run");
    let indices: Vec<_> =
        report.action_trace.expect("trace").entries.iter().map(|entry| entry.agent_index).collect();
    assert_eq!(indices, vec![0, 1, 2]);

    let again = run_with_config(config).expect("run again");
    let indices_again: Vec<_> =
        again.action_trace.expect("trace").entries.iter().map(|entry| entry.agent_index).collect();
    assert_eq!(indices, indices_again);
}

#[test]
fn final_world_clock_does_not_jump_to_configured_horizon() {
    let mut config = base_config();
    config.max_actions = Some(10);
    config.virtual_days = 30.0;
    let report = run_with_config(config).expect("run");
    assert!(report.timing.final_world_ms < report.timing.configured_horizon_ms);
    assert_eq!(report.timing.stop_reason, StopReason::ActionLimit);
}

#[test]
fn event_exactly_at_horizon_is_processed() {
    let invalid = RunConfig { virtual_days: 0.0, ..RunConfig::default() };
    assert!(matches!(invalid.validate(), Err(SimulationError::InvalidDuration)));

    // Horizon = 101 (start 100 + 1 ms duration). One action at slot 100 is allowed.
    let config = RunConfig {
        seed: 1,
        agent_count: 1,
        virtual_days: 1.0 / MS_PER_DAY as f64,
        virtual_start_ms: 100,
        scheduler: SchedulerConfig { action_interval_ms: 60_000, idle_extra_ms: 0 },
        max_actions: Some(1),
        record_action_trace: true,
        ..RunConfig::default()
    };
    let report = run_with_config(config).expect("run");
    assert_eq!(report.timing.configured_horizon_ms, 101);
    assert_eq!(report.timing.last_event_slot_ms, Some(100));
    assert_eq!(report.timing.events_processed, 1);
}

#[test]
fn rejected_move_does_not_count_as_visit() {
    let mut world = WorldState::for_simulation(0);
    let agent = SimAgent::new(1, 0, Profile::Wanderer);
    world.insert_operator(agent.operator());
    world.connect_player(agent.identity.clone()).unwrap();
    world.align_simulation_clock(0);
    let before = world.operator(agent.player_id).unwrap().location_id.clone();
    let (res, _) = execute_line(&mut world, agent.session_id, "north");
    assert!(matches!(res, ActionResult::Rejected { .. }));
    let after = world.operator(agent.player_id).unwrap().location_id.clone();
    assert_eq!(before, after);
}

#[test]
fn invalid_duration_and_interval_are_rejected() {
    let mut config = base_config();
    config.virtual_days = f64::NAN;
    assert!(matches!(config.validate(), Err(SimulationError::InvalidDuration)));

    config.virtual_days = 2.0;
    config.scheduler.action_interval_ms = 0;
    assert!(matches!(config.validate(), Err(SimulationError::ZeroActionInterval)));
}

#[test]
fn synthetic_identities_are_unique_and_namespaced() {
    let ids: BTreeSet<_> = (0..10).map(|i| sim_player_id(42, i).to_string()).collect();
    assert_eq!(ids.len(), 10);
    for i in 0..10 {
        let account = sim_account_id(42, i);
        assert_ne!(account, sim_player_id(42, i).as_uuid());
    }
}

#[test]
fn rejected_commands_do_not_count_as_successful() {
    let mut world = WorldState::for_simulation(0);
    let agent = SimAgent::new(1, 0, Profile::Wanderer);
    world.insert_operator(agent.operator());
    world.connect_player(agent.identity.clone()).unwrap();
    let (result, _) = execute_line(&mut world, agent.session_id, "friend add nobody");
    assert!(matches!(result, ActionResult::Rejected { .. }));
}

#[test]
fn virtual_clock_advances_home_habitation_without_wall_sleep() {
    let mut world = WorldState::for_simulation(0);
    let agent = SimAgent::new(7, 0, Profile::Homebody);
    world.insert_operator(agent.operator());
    world.connect_player(agent.identity.clone()).unwrap();
    world.align_simulation_clock(0);
    for line in ["go up", "outskirts"] {
        let (res, _) = execute_line(&mut world, agent.session_id, line);
        assert!(res.is_success(), "{res:?}");
    }
    let identity = gate_identity();
    world.apply_terrain_mod(identity, 8, 9, Terrain::Grass, "trail", agent.player_id).unwrap();
    let (res, _) = execute_line(&mut world, agent.session_id, "north");
    assert!(res.is_success(), "{res:?}");
    let loc = world.operator(agent.player_id).unwrap().location_id.clone();
    assert!(WildernessPos::parse(&loc).is_some());
    let (claim, _) = execute_line(&mut world, agent.session_id, "home claim");
    assert!(claim.is_success(), "{claim:?}");
    assert_eq!(world.active_home_base_habitation(agent.player_id), Some(Habitation::Unsettled));
    world.advance_virtual_time(SETTLING_TIME_MS);
    assert_eq!(world.active_home_base_habitation(agent.player_id), Some(Habitation::SettlingIn));
}

#[test]
fn private_journal_not_visible_to_other_agent() {
    let mut world = WorldState::for_simulation(0);
    let ada = SimAgent::new(3, 0, Profile::Investigator);
    let bob = SimAgent::new(3, 1, Profile::Wanderer);
    world.insert_operator(ada.operator());
    world.insert_operator(bob.operator());
    world.connect_player(ada.identity.clone()).unwrap();
    world.connect_player(bob.identity.clone()).unwrap();
    for line in ["go up", "journal record"] {
        let (res, _) = execute_line(&mut world, ada.session_id, line);
        assert!(res.is_success(), "{res:?}");
    }
    let (res, _) = execute_line(&mut world, bob.session_id, "journal read survey-scratch");
    assert!(matches!(res, ActionResult::Rejected { .. }));
}

#[test]
fn agents_reach_wilderness_and_materialize_regions() {
    let config = RunConfig {
        seed: 1,
        agent_count: 6,
        virtual_days: 1.0,
        scheduler: SchedulerConfig { action_interval_ms: MS_PER_DAY / 24, idle_extra_ms: 0 },
        max_actions: Some(80),
        virtual_start_ms: 0,
        ..RunConfig::default()
    };
    let report = run_with_config(config).expect("run");
    assert!(report.execution.successful > 0);
    assert!(report.world.regions_materialized > 0 || report.world.discoveries_recorded > 0);
    assert_eq!(report.version.world_generation_version, WORLD_GENERATION_VERSION);
    assert!(report.world.successful_inspections <= report.world.inspect_attempts);
    assert!(report.world.successful_exploration_moves <= report.world.exploration_move_attempts);
}

#[test]
fn simulation_runs_in_memory_without_sqlite_store() {
    let config = RunConfig {
        seed: 0,
        agent_count: 2,
        virtual_days: 0.01,
        scheduler: SchedulerConfig::default(),
        max_actions: Some(5),
        virtual_start_ms: 0,
        ..RunConfig::default()
    };
    let report = run_with_config(config).expect("in-memory run");
    assert!(report.execution.total_attempted > 0);
}

#[test]
fn gate_region_identity_is_stable_across_runs() {
    let id_a = gate_identity();
    let id_b = gate_identity();
    assert_eq!(id_a, id_b);
}

#[test]
fn zero_population_weights_are_rejected() {
    let mix = PopulationMix::default();
    assert!(matches!(mix.validate(), Err(SimulationError::InvalidPopulationMix(_))));
}

#[test]
fn profile_totals_reconcile_with_global_execution() {
    let mut config = base_config();
    config.population = PopulationMix {
        wanderer: 40,
        investigator: 30,
        homebody: 20,
        social_player: 5,
        occasional_visitor: 5,
    };
    let report = run_with_config(config).expect("run");
    let profile_attempted: u64 = report.per_profile.values().map(|p| p.attempted).sum();
    assert_eq!(profile_attempted, report.execution.total_attempted);
    let profile_ok: u64 = report.per_profile.values().map(|p| p.successful).sum();
    assert_eq!(profile_ok, report.execution.successful);
}

#[test]
fn different_seeds_can_differ_while_each_seed_is_stable() {
    let mut a = base_config();
    a.seed = 100;
    a.max_actions = Some(40);
    let mut b = base_config();
    b.seed = 101;
    b.max_actions = Some(40);
    let ra = run_with_config(a.clone()).expect("a");
    let rb = run_with_config(b).expect("b");
    assert_ne!(ra.clone().for_reproducibility_comparison(), rb.for_reproducibility_comparison());
    let ra2 = run_with_config(a).expect("a again");
    assert_eq!(ra.for_reproducibility_comparison(), ra2.for_reproducibility_comparison());
}

#[test]
fn investigator_mix_records_more_journal_attempts_than_pure_wanderers() {
    let mut wander = base_config();
    wander.seed = 77;
    wander.agent_count = 4;
    wander.virtual_days = 7.0;
    wander.population = PopulationMix::monolithic(Profile::Wanderer);
    wander.max_actions = Some(200);
    wander.scheduler.action_interval_ms = 30 * 60 * 1000;
    let mut invest = wander.clone();
    invest.population = PopulationMix::monolithic(Profile::Investigator);
    let rw = run_with_config(wander).expect("wander");
    let ri = run_with_config(invest).expect("invest");
    let w_j = rw.per_profile.get("wanderer").map(|p| p.journal_records).unwrap_or(0);
    let i_j = ri.per_profile.get("investigator").map(|p| p.journal_records).unwrap_or(0);
    assert!(i_j > 0, "investigator run should record at least one journal entry");
    assert!(i_j > w_j, "investigator journals {i_j} should exceed wanderer {w_j}");
}

#[test]
fn occasional_visitors_have_inactive_scheduler_slots() {
    let mut config = base_config();
    config.agent_count = 3;
    config.population = PopulationMix { occasional_visitor: 100, ..Default::default() };
    config.virtual_days = 14.0;
    config.max_actions = None;
    let report = run_with_config(config).expect("run");
    let visitor = report.per_profile.get("occasional_visitor").expect("visitor profile");
    assert!(visitor.inactive_slots > 0);
    assert!(report.timing.inactive_slots > 0);
}

#[test]
fn population_comparison_produces_isolated_reports() {
    let base = RunConfig {
        seed: 77,
        agent_count: 12,
        virtual_days: 3.0,
        max_actions: Some(150),
        scheduler: SchedulerConfig { action_interval_ms: 20 * 60 * 1000, idle_extra_ms: 0 },
        ..RunConfig::default()
    };
    let comparison = compare_populations(ComparisonRequest {
        base,
        mix_a: PopulationMix::exploration_heavy(),
        mix_b: PopulationMix::home_oriented(),
    })
    .expect("compare");
    assert_ne!(comparison.run_a.config.population, comparison.run_b.config.population);
    assert_eq!(comparison.run_a.config.seed, comparison.run_b.config.seed);
}

#[test]
fn trace_records_outcome_categories() {
    let mut config = base_config();
    config.max_actions = Some(60);
    config.population = PopulationMix { investigator: 100, ..Default::default() };
    config.record_action_trace = true;
    let report = run_with_config(config).expect("run");
    let trace = report.action_trace.expect("trace");
    assert!(trace.entries.iter().any(|e| e.outcome == TraceOutcome::Success));
    // Engine rejections in the trace are exercised when commands fail validation; short profile
    // runs may be all-success. Rejection accounting is covered by direct `execute_line` tests.
    assert!(!trace.entries.is_empty());
}
