//! Discovery benchmark and static audit must be repeatable.

use coffee_sim::discovery_metrics::{run_discovery_benchmark, DISCOVERY_BENCHMARK_SEEDS};
use coffee_world::measure_thread_coverage_audit;

#[test]
fn thread_coverage_audit_is_stable_across_calls() {
    let a = measure_thread_coverage_audit(42);
    let b = measure_thread_coverage_audit(42);
    assert_eq!(a, b);
}

#[test]
fn discovery_benchmark_report_is_stable_for_fixed_seeds() {
    let seeds = [42_u64, 1000];
    let a = run_discovery_benchmark(&seeds, 4).expect("benchmark");
    let b = run_discovery_benchmark(&seeds, 4).expect("benchmark");
    assert_eq!(a, b);
}

#[test]
fn standard_discovery_seeds_benchmark_runs() {
    let report =
        run_discovery_benchmark(&DISCOVERY_BENCHMARK_SEEDS.to_vec(), 4).expect("benchmark");
    assert!(!report.static_audit.authored_threads.is_empty());
    assert_eq!(report.per_seed_run_summaries.len(), DISCOVERY_BENCHMARK_SEEDS.len());
}
