//! Coffee.shop server: SSH gateway, protocol listener, and authoritative hub.
//!
//! The gateway and the simulation share a process, but they only meet at [`Hub`].
//! Player terminals talk to the hub through a local unix socket.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used))]

mod hub;
mod proto;
mod theme;

#[cfg(test)]
mod mystery_play;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use coffee_world::{
    canonical_username, hash_password, starter_operator, validate_operator_name, AuthError, Store,
    WorldState,
};
use terminus_core::PlayerId;
use terminus_ssh::{serve, AuthReject, Authenticator, Limits, SessionGrant, SshConfig};
use tokio::sync::{broadcast, oneshot};
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::hub::{is_unique, Hub};
use crate::theme::coffee_theme;

pub use crate::hub::HubAuthConfig;
pub use crate::theme::coffee_theme as theme;

/// How to boot one Coffee.shop process.
#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub data_dir: PathBuf,
    pub ssh_listen: SocketAddr,
    pub allow_registration: bool,
    /// Require a single-use invite code to register.
    pub invite_only: bool,
    /// Soft cap on accounts. Zero means unlimited.
    pub max_accounts: u32,
    /// Friend codes minted for each successful invite redeem.
    pub invites_per_player: u32,
    pub ascii: bool,
    pub max_sessions: usize,
    pub auth_rejection: Duration,
}

impl ServerConfig {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            ssh_listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            allow_registration: true,
            invite_only: false,
            max_accounts: 0,
            invites_per_player: coffee_world::DEFAULT_INVITES_PER_PLAYER,
            ascii: true,
            max_sessions: 64,
            auth_rejection: Duration::from_millis(200),
        }
    }
}

/// A running server. Dropping it does not shut the tasks down; call [`Running::shutdown`].
pub struct Running {
    pub ssh_addr: SocketAddr,
    pub socket_path: PathBuf,
    shutdown: broadcast::Sender<()>,
    hub: Hub,
    tasks: Vec<JoinHandle<()>>,
}

impl Running {
    pub async fn shutdown(self) {
        let _ = self.shutdown.send(());
        for task in self.tasks {
            let _ = tokio::time::timeout(Duration::from_secs(5), task).await;
        }
        self.hub.shutdown().await;
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

/// Open the database, seed the Undernet, and serve SSH plus the local protocol socket.
pub async fn start(config: ServerConfig) -> anyhow::Result<Running> {
    std::fs::create_dir_all(&config.data_dir)
        .with_context(|| format!("create {}", config.data_dir.display()))?;
    let db_path = config.data_dir.join("coffee.sqlite");
    let store =
        Store::open(&db_path).await.with_context(|| format!("open {}", db_path.display()))?;
    store
        .seed(&coffee_world::seed_locations(), &coffee_world::seed_exits())
        .await
        .context("seed world")?;
    let loaded = store.load().await.context("load world")?;
    let mut world = WorldState::new();
    world.install_loaded(loaded.operators, loaded.log);
    world.install_geography(loaded.geography);
    world.install_social(loaded.social);
    world.install_settlements(loaded.settlements);
    world.install_waypoints(loaded.waypoints);
    world.install_homes(loaded.homes);
    world.install_mail(loaded.mailbox);
    world.install_theories(loaded.theories);
    world.install_invites(loaded.invites);
    let hub = Hub::spawn(
        world,
        store,
        HubAuthConfig {
            allow_registration: config.allow_registration,
            invite_only: config.invite_only,
            max_accounts: config.max_accounts,
            invites_per_player: config.invites_per_player,
        },
    );

    let socket_path = config.data_dir.join("game.sock");
    let listener =
        proto::bind(&socket_path).with_context(|| format!("bind {}", socket_path.display()))?;
    tracing::info!(socket = %socket_path.display(), "protocol listening");

    let (shutdown, _) = broadcast::channel(1);
    let proto_shutdown = shutdown.subscribe();
    let ssh_shutdown = shutdown.subscribe();
    let proto_hub = hub.clone();
    let proto_task = tokio::spawn(async move {
        proto::serve(listener, proto_hub, proto_shutdown).await;
    });

    let (ready_tx, ready_rx) = oneshot::channel();
    let ssh = SshConfig {
        listen: config.ssh_listen,
        host_key_path: config.data_dir.join("keys").join("ssh_host_ed25519"),
        game_socket: socket_path.clone(),
        theme: coffee_theme(config.ascii),
        limits: Limits { max_connections: config.max_sessions, ..Limits::default() },
        auth_rejection: config.auth_rejection,
    };
    let auth = HubAuth { hub: hub.clone() };
    let ssh_task = tokio::spawn(async move {
        if let Err(err) = serve(ssh, auth, ready_tx, ssh_shutdown).await {
            tracing::error!(%err, "ssh gateway stopped");
        }
    });
    let ssh_addr = ready_rx.await.context("ssh gateway exited before it was listening")?;

    Ok(Running { ssh_addr, socket_path, shutdown, hub, tasks: vec![proto_task, ssh_task] })
}

/// Create an account without starting SSH. Safe while a server has the same database open.
pub async fn create_account(
    data_dir: &Path,
    username: &str,
    password: &str,
    operator_name: &str,
) -> anyhow::Result<()> {
    let username = canonical_username(username).map_err(|err| anyhow::anyhow!(err))?;
    let operator_name = if operator_name.trim().is_empty() {
        username.clone()
    } else {
        validate_operator_name(operator_name).map_err(|err| anyhow::anyhow!(err))?
    };
    let password = password.to_string();
    let hash = tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .context("password hashing task")?
        .map_err(|err| anyhow::anyhow!(err))?;
    let db_path = data_dir.join("coffee.sqlite");
    let store = Store::open(&db_path).await?;
    store.seed(&coffee_world::seed_locations(), &coffee_world::seed_exits()).await?;
    let account_id = Uuid::new_v4();
    let operator_id = PlayerId::new();
    let operator = starter_operator(operator_id, account_id, &username, &operator_name);
    let created =
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    if let Err(err) = store.create_account(account_id, &username, &hash, &operator, created).await {
        if matches!(err, coffee_world::StoreError::NameTaken) || is_unique(&err) {
            anyhow::bail!("that username is already registered");
        }
        return Err(err).context("create account");
    }
    tracing::info!(%username, operator = %operator_name, "account created");
    Ok(())
}

/// Mint unused invite codes for Reddit/admin seeding. Prints one code per line.
pub async fn create_invite_codes(
    data_dir: &Path,
    count: u32,
    note: &str,
) -> anyhow::Result<Vec<String>> {
    if count == 0 {
        anyhow::bail!("count must be at least 1");
    }
    if count > 1_000 {
        anyhow::bail!("count must be at most 1000");
    }
    let db_path = data_dir.join("coffee.sqlite");
    let store = Store::open(&db_path).await?;
    store.seed(&coffee_world::seed_locations(), &coffee_world::seed_exits()).await?;
    let created =
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    let rows = store.create_invite_codes(count, None, created, note).await?;
    Ok(rows.into_iter().map(|row| row.display_code).collect())
}

#[derive(Clone)]
struct HubAuth {
    hub: Hub,
}

impl Authenticator for HubAuth {
    async fn authenticate(
        &self,
        username: &str,
        password: &str,
    ) -> Result<SessionGrant, AuthReject> {
        match self.hub.login(username.to_string(), password.to_string(), None).await {
            Ok(grant) => Ok(SessionGrant { token: grant.token, display_name: grant.operator_name }),
            Err(err) => Err(AuthReject { reason: auth_reason(&err) }),
        }
    }

    async fn disconnect(&self, token: &str) {
        self.hub.disconnect(token).await;
    }
}

fn auth_reason(err: &AuthError) -> String {
    err.to_string()
}
