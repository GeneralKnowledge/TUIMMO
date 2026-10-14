//! Deterministic offline population simulation for Coffee.shop.
//!
//! Synthetic agents drive [`coffee_world::WorldState`] through the same command
//! pathways as live SSH sessions, without network or TUI.

pub mod agent;
pub mod compare;
pub mod config;
pub mod coverage_experiment;
pub mod discovery_metrics;
pub mod execute;
pub mod experiment;
pub mod feed_memory;
pub mod interaction;
pub mod memory;
pub mod multi_seed;
pub mod navigation;
pub mod newcomer_burst;
pub mod population;
pub mod profile;
pub mod report;
pub mod runner;
pub mod scheduler;
pub mod seeds;
pub mod snapshot;
pub mod stats;
pub mod trace;

pub use compare::{compare_populations, ComparisonReport, ComparisonRequest};
pub use config::SnapshotConfig;
pub use coverage_experiment::{
    format_coverage_report, run_density_experiment, DensityExperimentReport,
};
pub use discovery_metrics::{
    format_discovery_benchmark, run_discovery_benchmark, DiscoveryBenchmarkReport,
    DISCOVERY_BENCHMARK_SEEDS,
};
pub use experiment::{
    run_population_scale_experiment, PopulationAllocation, PopulationScaleReport,
    PopulationScaleRequest, MAX_SCALE_EXPERIMENT_RUNS,
};
pub use interaction::{InteractionMetrics, StoryLine, StoryTranscript};
pub use multi_seed::{
    run_multi_seed_experiment, ConditionAggregate, ConditionMeanComparison, ConditionResult,
    ExperimentCondition, MultiSeedExperimentReport, MultiSeedExperimentRequest, ReplicateMetrics,
    ReplicateRecord, SnapshotSeriesPoint, MAX_MULTI_SEED_CONDITIONS, MAX_MULTI_SEED_REPLICATES,
    MAX_MULTI_SEED_TOTAL_RUNS,
};
pub use newcomer_burst::{format_newcomer_burst, run_newcomer_burst, NewcomerBurstReport};
pub use population::PopulationMix;
pub use profile::Profile;
pub use report::{DerivedRates, RunReport, SimVersion};
pub use runner::{RunConfig, SimulationError, SimulationRunner};
pub use seeds::{replicate_seed, ReplicateSeedSchedule, REPLICATE_SEED_STRIDE};
pub use snapshot::{
    LongitudinalSnapshot, ObservationKind, SnapshotSamplingSummary, TerminalSnapshotStatus,
};
pub use stats::{NumericSummary, RateSummary};
