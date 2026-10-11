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
    canonical_username, dummy_hash, hash_password, validate_operator_name, validate_password,
    verify_password, AuthError,
};
pub use model::{Item, Operator, PersistOp, STARTER_LOCATION};
pub use state::{starter_operator, Transition, WorldState};
pub use store::{AccountRow, Store, StoreError};
