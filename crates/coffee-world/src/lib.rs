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
pub mod model;
pub mod state;
pub mod store;
pub mod undernet;

pub use auth::{
    canonical_recovery_code, canonical_username, dummy_hash, generate_recovery_code, hash_password,
    hash_recovery_code, hash_unusable_password, validate_email, validate_operator_name,
    validate_password, verify_password, verify_recovery_code, AccountKind, AuthError,
};
pub use model::{Item, Operator, PersistOp, STARTER_LOCATION};
pub use state::{starter_operator, Transition, WorldState};
pub use store::{AccountRow, NewAccount, Store, StoreError};
