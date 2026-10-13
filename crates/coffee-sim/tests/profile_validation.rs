#![allow(clippy::unwrap_used)]

//! Phase 2.1 behavioural validation: profile separation, metrics, and comparisons.

use coffee_sim::compare::{compare_populations, ComparisonRequest};
use coffee_sim::population::PopulationMix;
use coffee_sim::profile::Profile;
use coffee_sim::runner::{run_with_config, RunConfig, MS_PER_DAY};
use coffee_sim::scheduler::SchedulerConfig;

/// Controlled scenario: enough agents, horizon, and actions to reach wilderness and
/// exercise profile-specific commands (seed chosen for stable separation).
fn profile_scenario() -> RunConfig {
    RunConfig {
        seed: 77,
        agent_count: 4,
        virtual_days: 7.0,
        scheduler: SchedulerConfig { action_interval_ms: 30 * 60 * 1000, idle_extra_ms: 0 },
        max_actions: Some(200),
        virtual_start_ms: 1_700_000_000_000,
        ..RunConfig::default()
    }
}

fn run_monolithic(profile: Profile, base: &RunConfig) -> coffee_sim::RunReport {
    let mut config = base.clone();
    config.population = PopulationMix::monolithic(profile);
    run_with_config(config).expect("sim run")
}

#[test]
fn wanderer_explores_more_places_than_homebody_under_same_conditions() {
    let base = profile_scenario();
    let wander = run_monolithic(Profile::Wanderer, &base);
    let home = run_monolithic(Profile::Homebody, &base);
    assert!(
        wander.world.unique_locations_visited > home.world.unique_locations_visited,
        "wanderer {} vs homebody {}",
        wander.world.unique_locations_visited,
        home.world.unique_locations_visited,
    );
    assert!(
        wander.derived_rates().unique_locations_per_agent
            > home.derived_rates().unique_locations_per_agent,
        "normalized exploration should favour wanderer"
    );
}

#[test]
fn homebody_establishes_home_claims_while_wanderers_do_not() {
    let base = profile_scenario();
    let home = run_monolithic(Profile::Homebody, &base);
    let wander = run_monolithic(Profile::Wanderer, &base);
    assert!(home.world.home_claims_active >= base.agent_count as usize);
    assert_eq!(wander.world.home_claims_active, 0);
}

#[test]
fn investigator_records_journals_in_journal_context_wanderers_do_not() {
    let base = profile_scenario();
    let invest = run_monolithic(Profile::Investigator, &base);
    let wander = run_monolithic(Profile::Wanderer, &base);
    let i_j = invest.per_profile.get("investigator").map(|p| p.journal_records).unwrap_or(0);
    let w_j = wander.per_profile.get("wanderer").map(|p| p.journal_records).unwrap_or(0);
    assert!(i_j > 0, "investigator mix should produce successful journal records");
    assert!(i_j > w_j, "investigator {i_j} should exceed wanderer {w_j} with same seed/horizon");
}

#[test]
fn social_players_accept_social_commands_when_co_present() {
    let base = profile_scenario();
    let social = run_monolithic(Profile::SocialPlayer, &base);
    assert!(social.world.social_attempts > 0);
    assert_eq!(social.world.social_attempts, social.world.social_successes);
    let wander = run_monolithic(Profile::Wanderer, &base);
    assert_eq!(wander.world.social_attempts, 0);
}

#[test]
fn occasional_visitor_has_offline_inactive_slots_and_lower_activity() {
    let base = profile_scenario();
    let visitor = run_monolithic(Profile::OccasionalVisitor, &base);
    let wander = run_monolithic(Profile::Wanderer, &base);
    let v = visitor.per_profile.get("occasional_visitor").expect("visitor stats");
    assert!(v.inactive_slots > 0);
    assert!(visitor.timing.inactive_slots > wander.timing.inactive_slots);
    assert!(
        visitor.execution.total_attempted < wander.execution.total_attempted,
        "visitor attempts {} vs wanderer {}",
        visitor.execution.total_attempted,
        wander.execution.total_attempted,
    );
    let v_share = visitor.derived_rates().inactive_slot_share.expect("inactive share");
    let w_share = wander.derived_rates().inactive_slot_share.unwrap_or(0.0);
    assert!(v_share > w_share);
}

#[test]
fn mixed_population_enables_social_metrics_without_monolithic_social_mix() {
    let mut config = profile_scenario();
    config.population = PopulationMix {
        wanderer: 25,
        investigator: 25,
        homebody: 25,
        social_player: 25,
        ..Default::default()
    };
    let report = run_with_config(config).expect("mixed run");
    assert!(report.world.social_successes > 0);
    assert!(report.world.home_claims_active > 0);
    assert!(report.world.unique_locations_visited > 0);
}

#[test]
fn additive_world_metrics_reconcile_with_per_profile_sums() {
    let mut config = profile_scenario();
    config.population = PopulationMix {
        wanderer: 20,
        investigator: 20,
        homebody: 20,
        follower: 0,
        social_player: 20,
        occasional_visitor: 20,
    };
    let report = run_with_config(config).expect("mixed run");

    let journal: u64 = report.per_profile.values().map(|p| p.journal_records).sum();
    assert_eq!(journal, report.world.journal_records);

    let inspections: u64 = report.per_profile.values().map(|p| p.successful_inspections).sum();
    assert_eq!(inspections, report.world.successful_inspections);

    let social_attempts: u64 = report.per_profile.values().map(|p| p.social_attempts).sum();
    assert_eq!(social_attempts, report.world.social_attempts);

    let social_ok: u64 = report.per_profile.values().map(|p| p.social_successes).sum();
    assert_eq!(social_ok, report.world.social_successes);

    let repeats: u64 = report.per_profile.values().map(|p| p.repeat_visits).sum();
    assert_eq!(repeats, report.world.repeat_visits);

    let home_returns: u64 = report.per_profile.values().map(|p| p.home_returns).sum();
    assert_eq!(home_returns, report.world.home_returns);

    let inactive: u64 = report.per_profile.values().map(|p| p.inactive_slots).sum();
    assert_eq!(inactive, report.timing.inactive_slots);
}

#[test]
fn per_agent_metrics_use_assigned_profile_only() {
    let mut config = profile_scenario();
    config.agent_count = 8;
    config.population = PopulationMix { wanderer: 50, investigator: 50, ..Default::default() };
    let report = run_with_config(config).expect("run");
    for stats in report.per_agent.values() {
        assert!(
            stats.profile == "wanderer" || stats.profile == "investigator",
            "unexpected profile {}",
            stats.profile
        );
    }
    let wander_agents: u64 = report
        .per_agent
        .values()
        .filter(|a| a.profile == "wanderer")
        .map(|a| a.journal_records)
        .sum();
    let invest_agents: u64 = report
        .per_agent
        .values()
        .filter(|a| a.profile == "investigator")
        .map(|a| a.journal_records)
        .sum();
    assert!(invest_agents > wander_agents);
}

#[test]
fn derived_rates_use_none_for_zero_denominators() {
    let wander = run_monolithic(Profile::Wanderer, &profile_scenario());
    let rates = wander.derived_rates();
    assert!(rates.social_success_rate.is_none());
    assert!(rates.exploration_move_success_rate.is_some());
}

#[test]
fn population_comparison_is_reproducible_and_configuration_matched() {
    let base = profile_scenario();
    let request = ComparisonRequest {
        base: base.clone(),
        mix_a: PopulationMix::exploration_heavy(),
        mix_b: PopulationMix::home_oriented(),
    };
    let first = compare_populations(request.clone()).expect("compare");
    let second = compare_populations(request).expect("compare again");
    assert_eq!(
        first.run_a.clone().for_reproducibility_comparison(),
        second.run_a.for_reproducibility_comparison()
    );
    assert_eq!(
        first.run_b.clone().for_reproducibility_comparison(),
        second.run_b.for_reproducibility_comparison()
    );
    assert_eq!(first.run_a.config.seed, first.run_b.config.seed);
    assert_eq!(first.run_a.config.virtual_duration_ms, first.run_b.config.virtual_duration_ms);
    assert_eq!(first.run_a.config.agent_count, first.run_b.config.agent_count);
    assert_eq!(first.run_a.config.scheduler, first.run_b.config.scheduler);
    assert_ne!(first.run_a.config.population, first.run_b.config.population);
    assert!(first.derived_a.exploration_move_success_rate.is_some());
}

#[test]
fn comparison_derived_rates_distinguish_activity_from_behaviour() {
    let comparison = compare_populations(ComparisonRequest {
        base: profile_scenario(),
        mix_a: PopulationMix::exploration_heavy(),
        mix_b: PopulationMix::home_oriented(),
    })
    .expect("compare");
    // Home-oriented mix includes visitors → more inactive slots, not necessarily more attempts.
    let inactive_a = comparison.derived_a.inactive_slot_share.unwrap_or(0.0);
    let inactive_b = comparison.derived_b.inactive_slot_share.unwrap_or(0.0);
    assert!(inactive_b >= inactive_a);
}

#[test]
fn reproducibility_preserved_when_profile_mix_changes() {
    let base = profile_scenario();
    let a = run_monolithic(Profile::Wanderer, &base);
    let mut other = base.clone();
    other.population = PopulationMix::monolithic(Profile::Homebody);
    let b = run_with_config(other).expect("home run");
    assert_ne!(a.clone().for_reproducibility_comparison(), b.for_reproducibility_comparison());
    let a2 = run_monolithic(Profile::Wanderer, &base);
    assert_eq!(a.for_reproducibility_comparison(), a2.for_reproducibility_comparison());
}

#[test]
fn long_horizon_visitor_scenario_uses_multi_day_offline_gaps() {
    let config = RunConfig {
        seed: 42,
        agent_count: 2,
        population: PopulationMix::monolithic(Profile::OccasionalVisitor),
        virtual_days: 14.0,
        scheduler: SchedulerConfig { action_interval_ms: MS_PER_DAY / 4, idle_extra_ms: 0 },
        max_actions: None,
        virtual_start_ms: 0,
        ..RunConfig::default()
    };
    let report = run_with_config(config.clone()).expect("visitor run");
    assert!(report.timing.inactive_slots > 0);
    assert!(report.execution.total_attempted > 0);
    let again = run_with_config(config).expect("repeat");
    assert_eq!(report.for_reproducibility_comparison(), again.for_reproducibility_comparison());
}
