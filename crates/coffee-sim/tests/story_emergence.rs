#![allow(clippy::unwrap_used)]

use coffee_sim::population::PopulationMix;
use coffee_sim::runner::{run_with_config, RunConfig, MS_PER_DAY};
use coffee_sim::scheduler::SchedulerConfig;

fn short_story_config() -> RunConfig {
    RunConfig {
        seed: 4242,
        agent_count: 6,
        population: PopulationMix::story_emergence(),
        virtual_days: 3.0,
        scheduler: SchedulerConfig { action_interval_ms: 2 * MS_PER_DAY / 24, idle_extra_ms: 0 },
        max_actions: Some(400),
        record_action_trace: true,
        action_trace_limit: 80,
        ..RunConfig::default()
    }
}

#[test]
fn short_story_run_is_deterministic() {
    let config = short_story_config();
    let a = run_with_config(config.clone()).expect("run a");
    let b = run_with_config(config).expect("run b");
    assert_eq!(a.interaction, b.interaction);
    assert_eq!(a.for_reproducibility_comparison(), b.for_reproducibility_comparison());
}

#[test]
fn story_mix_reports_interaction_fields() {
    let report = run_with_config(short_story_config()).expect("run");
    assert!(report.execution.total_attempted > 0);
    assert!(report.world.theory_entries <= report.world.theory_subjects + 100);
}
