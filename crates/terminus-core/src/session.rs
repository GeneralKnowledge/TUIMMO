//! Who is connected, and what they are allowed to do.

use serde::{Deserialize, Serialize};

use crate::id::{PlayerId, SessionId};

/// Coarse permissions checked by the simulation before a command runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permissions {
    pub play: bool,
    pub speak: bool,
}

impl Permissions {
    pub const fn player() -> Self {
        Self { play: true, speak: true }
    }

    pub const fn spectator() -> Self {
        Self { play: false, speak: false }
    }

    pub const fn can_play(self) -> bool {
        self.play
    }

    pub const fn can_speak(self) -> bool {
        self.speak
    }
}

/// Authenticated identity handed to the simulation.
///
/// The engine does not authenticate passwords. The server resolves an account
/// and then builds one of these.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionIdentity {
    pub session_id: SessionId,
    pub player_id: PlayerId,
    pub name: String,
    pub permissions: Permissions,
}

impl SessionIdentity {
    pub fn new(player_id: PlayerId, name: impl Into<String>, permissions: Permissions) -> Self {
        Self { session_id: SessionId::new(), player_id, name: name.into(), permissions }
    }
}
