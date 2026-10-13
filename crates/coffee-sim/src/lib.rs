//! Deterministic offline population simulation for Coffee.shop.
//!
//! Synthetic agents drive [`coffee_world::WorldState`] through the same command
//! pathways as live SSH sessions, without network or TUI.

pub mod agent;
pub mod config;
pub mod execute;
pub mod report;
pub mod runner;
pub mod scheduler;
pub mod trace;

pub use report::{RunReport, SimVersion};
pub use runner::{RunConfig, SimulationError, SimulationRunner};
