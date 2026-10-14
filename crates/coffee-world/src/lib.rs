//! Coffee.shop's Undernet: locations, operators, chat, and SQLite persistence.
//!
//! The simulation in [`WorldState`] does not know about SSH, sockets, or widgets.
//! [`Store`] loads and commits rows. Passwords are hashed in [`auth`] and are not
//! part of the world model.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod auth;
pub mod catalog;
pub mod geography;
pub mod history;
pub mod home;
pub mod invite;
pub mod mail;
pub mod model;
pub mod settlement;
pub mod sim;
pub mod social;
pub mod state;
pub mod store;
pub mod subway;
pub mod theories;
pub mod threads;
pub mod undernet;
pub mod waypoint;

/// Locations and exits seeded on every startup: Hub rooms plus subway stations.
pub fn seed_locations() -> Vec<undernet::LocationDef> {
    undernet::locations().iter().copied().chain(subway::locations().iter().copied()).collect()
}

/// Static exits reseeded on startup, including ordinary subway hops.
pub fn seed_exits() -> Vec<undernet::ExitDef> {
    undernet::all_exits().into_iter().chain(subway::station_exits().iter().copied()).collect()
}

/// Resolve a hand-authored static place (Hub room or subway station).
pub fn static_location(id: &str) -> Option<&'static undernet::LocationDef> {
    undernet::location(id).or_else(|| subway::location(id))
}

pub use auth::{
    canonical_username, dummy_hash, hash_password, validate_operator_name, validate_password,
    verify_password, AuthError,
};
pub use invite::{
    format_invite_code, normalize_invite_code, AccountInvite, InviteBook,
    DEFAULT_INVITES_PER_PLAYER,
};
pub use model::{Item, Operator, PersistOp, STARTER_LOCATION};
pub use settlement::SettlementState;
pub use sim::{
    SimulationOccupancySummary, UgcStructureAudit, WorldOutcomeSnapshot, WORLD_GENERATION_VERSION,
};
pub use social::SocialState;
pub use state::{starter_operator, Transition, WorldState};
pub use store::{AccountInvitePolicy, AccountRow, LoadedWorld, NewAccount, Store, StoreError};
pub use threads::{
    format_thread_coverage_audit, is_thread_clue, measure_coverage_report,
    measure_placement_inventory, measure_thread_coverage_audit, ActionDiscoveryReport,
    ClueDiscoveryReport, CoverageParams, DiscoveryRunSummary, LongThreadProgressReport,
    PlacementInventoryReport, ThreadCoverageAuditReport, ThreadCoverageReport, ThreadSimReport,
};
