//! Synthetic players with deterministic identities isolated from production accounts.

use std::fmt;

use coffee_world::{starter_operator, Operator};
use terminus_core::{PlayerId, SessionId, SessionIdentity};
use uuid::Uuid;

/// Namespace for UUID v5 simulation identities (never used for real accounts).
const SIM_ID_NAMESPACE: Uuid = Uuid::from_u128(0xa7f3c2e1_4b5d_6e7f_8091_a2b3c4d5e6f7);

#[derive(Clone, Debug)]
pub struct SimAgent {
    pub index: u32,
    pub player_id: PlayerId,
    pub account_id: Uuid,
    pub display_name: String,
    pub session_id: SessionId,
    pub identity: SessionIdentity,
    /// Scripted progression toward the Squalor (shared path, deterministic).
    pub script_step: u32,
    pub attempted: u64,
    pub successful: u64,
    pub rejected: u64,
    pub failed: u64,
    pub inspect_actions: u64,
    pub exploration_moves: u64,
    pub home_claim_attempted: bool,
}

impl SimAgent {
    pub fn new(run_seed: u64, index: u32) -> Self {
        let player_id = sim_player_id(run_seed, index);
        let account_id = sim_account_id(run_seed, index);
        let display_name = format!("sim-{index:04}");
        let identity = SessionIdentity::new(
            player_id,
            display_name.clone(),
            terminus_core::Permissions::player(),
        );
        let session_id = identity.session_id;
        Self {
            index,
            player_id,
            account_id,
            display_name,
            session_id,
            identity,
            script_step: 0,
            attempted: 0,
            successful: 0,
            rejected: 0,
            failed: 0,
            inspect_actions: 0,
            exploration_moves: 0,
            home_claim_attempted: false,
        }
    }

    pub fn operator(&self) -> Operator {
        starter_operator(self.player_id, self.account_id, &self.username(), &self.display_name)
    }

    fn username(&self) -> String {
        format!("sim{:04}", self.index)
    }
}

impl fmt::Display for SimAgent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name)
    }
}

pub fn sim_player_id(run_seed: u64, index: u32) -> PlayerId {
    let name = format!("player:{run_seed}:{index}");
    PlayerId::from_uuid(Uuid::new_v5(&SIM_ID_NAMESPACE, name.as_bytes()))
}

pub fn sim_account_id(run_seed: u64, index: u32) -> Uuid {
    let name = format!("account:{run_seed}:{index}");
    Uuid::new_v5(&SIM_ID_NAMESPACE, name.as_bytes())
}
