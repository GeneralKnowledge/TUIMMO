use serde::{Deserialize, Serialize};
use terminus_core::FeedLine;
use terminus_core::GameView;

/// Protocol version implemented by this crate.
pub const PROTOCOL_VERSION: u16 = 1;

/// Maximum JSON body size. The length prefix is extra.
pub const MAX_FRAME_BYTES: usize = 32 * 1024;

/// Maximum bytes a decoder will buffer for one connection.
pub const MAX_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientFrame {
    pub v: u16,
    pub id: u64,
    pub body: ClientMessage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerFrame {
    pub v: u16,
    pub id: u64,
    pub body: ServerMessage,
}

/// A secret the server shows once. `Debug` never prints it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RevealedOnce(pub String);

impl std::fmt::Debug for RevealedOnce {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted>")
    }
}

/// Messages a client may send. Unknown fields are rejected.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientMessage {
    Hello {
        client: String,
    },
    Register {
        username: String,
        password: String,
        operator_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        email: Option<String>,
    },
    /// Create a guest. No email and no password.
    Guest {
        username: String,
        operator_name: String,
    },
    Login {
        username: String,
        password: String,
    },
    Recover {
        username: String,
        recovery_code: String,
    },
    Resume {
        token: String,
    },
    /// Replace the password on the authenticated permanent account.
    SetPassword {
        password: String,
    },
    /// Turn the authenticated guest into a permanent account. Same ids.
    Upgrade {
        password: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        email: Option<String>,
    },
    RotateRecovery,
    RevokeRecovery,
    AccountInfo,
    Command {
        line: String,
    },
    Ping {
        nonce: u64,
    },
}

/// Messages the server may send.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServerMessage {
    Hello {
        version: u16,
        motd: String,
    },
    /// Login, registration, or resume succeeded.
    Authenticated {
        token: String,
        operator_name: String,
        view: GameView,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        username: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        account_kind: Option<String>,
        /// Shown once. Omitted on ordinary login so older frames stay valid.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        recovery_code: Option<RevealedOnce>,
    },
    AccountStatus {
        username: String,
        kind: String,
        has_recovery: bool,
        has_email: bool,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        recovery_code: Option<RevealedOnce>,
    },
    /// Command result for the requester.
    Update {
        quit: bool,
        view: GameView,
    },
    /// Full snapshot pushed to other sessions.
    Snapshot {
        view: GameView,
    },
    /// Incremental feed lines. Snapshots remain authoritative for the TUI.
    Events {
        lines: Vec<FeedLine>,
    },
    Error {
        code: String,
        message: String,
    },
    Pong {
        nonce: u64,
    },
    Goodbye {
        message: String,
    },
}

impl std::fmt::Debug for ClientMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hello { client } => f.debug_struct("Hello").field("client", client).finish(),
            Self::Register { username, operator_name, .. } => f
                .debug_struct("Register")
                .field("username", username)
                .field("password", &"<redacted>")
                .field("operator_name", operator_name)
                .field("email", &"<redacted>")
                .finish(),
            Self::Guest { username, operator_name } => f
                .debug_struct("Guest")
                .field("username", username)
                .field("operator_name", operator_name)
                .finish(),
            Self::Login { username, .. } => f
                .debug_struct("Login")
                .field("username", username)
                .field("password", &"<redacted>")
                .finish(),
            Self::Recover { username, .. } => f
                .debug_struct("Recover")
                .field("username", username)
                .field("recovery_code", &"<redacted>")
                .finish(),
            Self::SetPassword { .. } => f.debug_struct("SetPassword").finish(),
            Self::Upgrade { .. } => f
                .debug_struct("Upgrade")
                .field("password", &"<redacted>")
                .field("email", &"<redacted>")
                .finish(),
            Self::RotateRecovery => f.debug_struct("RotateRecovery").finish(),
            Self::RevokeRecovery => f.debug_struct("RevokeRecovery").finish(),
            Self::AccountInfo => f.debug_struct("AccountInfo").finish(),
            Self::Resume { .. } => f.debug_struct("Resume").field("token", &"<redacted>").finish(),
            Self::Command { line } => f.debug_struct("Command").field("line", line).finish(),
            Self::Ping { nonce } => f.debug_struct("Ping").field("nonce", nonce).finish(),
        }
    }
}

impl ServerMessage {
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Error { code: code.into(), message: message.into() }
    }
}
