//! Deterministic offline population simulation for Coffee.shop.
//!
//! Synthetic agents drive [`coffee_world::WorldState`] through the same command
//! pathways as live SSH sessions, without network or TUI.

pub mod agent;
pub mod compare;
pub mod config;
pub mod execute;
pub mod memory;
pub mod navigation;
pub mod population;
pub mod profile;
pub mod report;
pub mod runner;
pub mod scheduler;
pub mod trace;

pub use compare::{compare_populations, ComparisonReport, ComparisonRequest};
pub use population::PopulationMix;
pub use profile::Profile;
pub use report::{DerivedRates, RunReport, SimVersion};
pub use runner::{RunConfig, SimulationError, SimulationRunner};
