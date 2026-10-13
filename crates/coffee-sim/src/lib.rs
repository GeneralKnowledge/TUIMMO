//! Deterministic offline population simulation for Coffee.shop.
//!
//! Synthetic agents drive [`coffee_world::WorldState`] through the same command
//! pathways as live SSH sessions, without network or TUI.

pub mod agent;
pub mod compare;
pub mod config;
pub mod execute;
pub mod experiment;
pub mod memory;
pub mod navigation;
pub mod population;
pub mod profile;
pub mod report;
pub mod runner;
pub mod scheduler;
pub mod snapshot;
pub mod trace;

pub use compare::{compare_populations, ComparisonReport, ComparisonRequest};
pub use config::SnapshotConfig;
pub use experiment::{
    run_population_scale_experiment, PopulationAllocation, PopulationScaleReport,
    PopulationScaleRequest, MAX_SCALE_EXPERIMENT_RUNS,
};
pub use population::PopulationMix;
pub use profile::Profile;
pub use report::{DerivedRates, RunReport, SimVersion};
pub use runner::{RunConfig, SimulationError, SimulationRunner};
pub use snapshot::LongitudinalSnapshot;
