//! Single-owner simulation task. Every state change is applied here, then committed.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use coffee_world::{
    canonical_username, hash_password, starter_operator, validate_operator_name, verify_password,
    AuthError, Store, StoreError, WorldState,
};
use terminus_core::{
    Game, GameError, GameEvent, GameView, Permissions, PlayerId, SessionId, SessionIdentity,
};
use terminus_protocol::ServerMessage;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

const COMMAND_RATE: f64 = 8.0;
const COMMAND_BURST: f64 = 20.0;

#[derive(Clone)]
pub struct Hub {
    tx: mpsc::Sender<Msg>,
}

pub struct Grant {
    pub token: String,
    pub operator_name: String,
    pub view: GameView,
}

struct TokenRec {
    session_id: SessionId,
    player_id: PlayerId,
    name: String,
}

struct Bucket {
    tokens: f64,
    updated: Instant,
}

impl Bucket {
    fn new() -> Self {
        Self { tokens: COMMAND_BURST, updated: Instant::now() }
    }

    fn allow(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.updated).as_secs_f64();
        self.updated = now;
        self.tokens = (self.tokens + elapsed * COMMAND_RATE).min(COMMAND_BURST);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

enum Msg {
    Login {
        username: String,
        password: String,
        events: Option<mpsc::Sender<ServerMessage>>,
        reply: oneshot::Sender<Result<Grant, AuthError>>,
    },
    Register {
        username: String,
        password: String,
        operator_name: String,
        events: Option<mpsc::Sender<ServerMessage>>,
        reply: oneshot::Sender<Result<Grant, AuthError>>,
    },
    Resume {
        token: String,
        events: mpsc::Sender<ServerMessage>,
        reply: oneshot::Sender<Result<Grant, AuthError>>,
    },
    Command {
        token: String,
        line: String,
        reply: oneshot::Sender<Result<(bool, GameView), GameError>>,
    },
    Disconnect {
        token: String,
    },
    Shutdown {
        reply: oneshot::Sender<()>,
    },
}

struct Actor {
    world: WorldState,
    store: Store,
    allow_registration: bool,
    tokens: HashMap<String, TokenRec>,
    subs: HashMap<SessionId, mpsc::Sender<ServerMessage>>,
    buckets: HashMap<SessionId, Bucket>,
    auth_failures: HashMap<String, Vec<Instant>>,
    dummy_hash: String,
}

impl Hub {
    pub fn spawn(world: WorldState, store: Store, allow_registration: bool) -> Self {
        let (tx, mut rx) = mpsc::channel(128);
        let dummy_hash = coffee_world::dummy_hash();
        tokio::spawn(async move {
            let mut actor = Actor {
                world,
                store,
                allow_registration,
                tokens: HashMap::new(),
                subs: HashMap::new(),
                buckets: HashMap::new(),
                auth_failures: HashMap::new(),
                dummy_hash,
            };
            while let Some(msg) = rx.recv().await {
                match msg {
                    Msg::Shutdown { reply } => {
                        let _ = reply.send(());
                        break;
                    }
                    other => actor.handle(other).await,
                }
            }
        });
        Self { tx }
    }

    pub async fn login(
        &self,
        username: String,
        password: String,
        events: Option<mpsc::Sender<ServerMessage>>,
    ) -> Result<Grant, AuthError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(Msg::Login { username, password, events, reply })
            .await
            .map_err(|_| AuthError::Internal)?;
        rx.await.map_err(|_| AuthError::Internal)?
    }

    pub async fn register(
        &self,
        username: String,
        password: String,
        operator_name: String,
        events: Option<mpsc::Sender<ServerMessage>>,
    ) -> Result<Grant, AuthError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(Msg::Register { username, password, operator_name, events, reply })
            .await
            .map_err(|_| AuthError::Internal)?;
        rx.await.map_err(|_| AuthError::Internal)?
    }

    pub async fn resume(
        &self,
        token: String,
        events: mpsc::Sender<ServerMessage>,
    ) -> Result<Grant, AuthError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(Msg::Resume { token, events, reply })
            .await
            .map_err(|_| AuthError::Internal)?;
        rx.await.map_err(|_| AuthError::Internal)?
    }

    pub async fn command(
        &self,
        token: String,
        line: String,
    ) -> Result<(bool, GameView), GameError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(Msg::Command { token, line, reply })
            .await
            .map_err(|_| GameError::Rejected("server is shutting down".into()))?;
        rx.await.map_err(|_| GameError::Rejected("server is shutting down".into()))?
    }

    pub async fn disconnect(&self, token: &str) {
        let _ = self.tx.send(Msg::Disconnect { token: token.to_string() }).await;
    }

    pub async fn shutdown(&self) {
        let (reply, rx) = oneshot::channel();
        if self.tx.send(Msg::Shutdown { reply }).await.is_ok() {
            let _ = rx.await;
        }
    }
}

impl Actor {
    async fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Login { username, password, events, reply } => {
                let result = self.login(username, password, events).await;
                let _ = reply.send(result);
            }
            Msg::Register { username, password, operator_name, events, reply } => {
                let result = self.register(username, password, operator_name, events).await;
                let _ = reply.send(result);
            }
            Msg::Resume { token, events, reply } => {
                let result = self.resume(token, events);
                let _ = reply.send(result);
            }
            Msg::Command { token, line, reply } => {
                let result = self.command(token, line).await;
                let _ = reply.send(result);
            }
            Msg::Disconnect { token } => self.disconnect(&token).await,
            Msg::Shutdown { .. } => {}
        }
    }

    async fn login(
        &mut self,
        username: String,
        password: String,
        events: Option<mpsc::Sender<ServerMessage>>,
    ) -> Result<Grant, AuthError> {
        let username = canonical_username(&username)?;
        if self.auth_blocked(&username) {
            return Err(AuthError::RateLimited);
        }
        match self.store.find_account(&username).await {
            Ok(Some(row)) => {
                let hash = row.password_hash.clone();
                let password_check = password.clone();
                let ok =
                    tokio::task::spawn_blocking(move || verify_password(&password_check, &hash))
                        .await
                        .unwrap_or(false);
                if !ok {
                    self.note_auth_failure(&username);
                    return Err(AuthError::InvalidCredentials);
                }
                self.auth_failures.remove(&username);
                self.ensure_operator(row.operator_id).await?;
                let name = self
                    .world
                    .operator(row.operator_id)
                    .map(|op| op.name.clone())
                    .unwrap_or(row.operator_name);
                self.attach(row.operator_id, name, events).await
            }
            Ok(None) if self.allow_registration => {
                self.register(username, password, String::new(), events).await
            }
            Ok(None) => {
                let dummy = self.dummy_hash.clone();
                let _ =
                    tokio::task::spawn_blocking(move || verify_password(&password, &dummy)).await;
                self.note_auth_failure(&username);
                Err(AuthError::InvalidCredentials)
            }
            Err(err) => {
                tracing::error!(error = %err, "account lookup failed");
                Err(AuthError::Internal)
            }
        }
    }

    async fn register(
        &mut self,
        username: String,
        password: String,
        operator_name: String,
        events: Option<mpsc::Sender<ServerMessage>>,
    ) -> Result<Grant, AuthError> {
        if !self.allow_registration {
            return Err(AuthError::RegistrationClosed);
        }
        let username = canonical_username(&username)?;
        if self.auth_blocked(&username) {
            return Err(AuthError::RateLimited);
        }
        let operator_name = if operator_name.trim().is_empty() {
            username.clone()
        } else {
            validate_operator_name(&operator_name)?
        };
        if let Ok(Some(_)) = self.store.find_account(&username).await {
            return Err(AuthError::NameTaken);
        }
        let password_for_hash = password;
        let encoded = tokio::task::spawn_blocking(move || hash_password(&password_for_hash))
            .await
            .map_err(|_| AuthError::Internal)??;
        let account_id = Uuid::new_v4();
        let operator_id = PlayerId::new();
        let operator = starter_operator(operator_id, account_id, &username, &operator_name);
        let created = system_now_ms();
        if let Err(err) =
            self.store.create_account(account_id, &username, &encoded, &operator, created).await
        {
            if is_unique(&err) {
                return Err(AuthError::NameTaken);
            }
            tracing::error!(error = %err, "failed to create account");
            return Err(AuthError::Internal);
        }
        self.world.insert_operator(operator);
        self.attach(operator_id, operator_name, events).await
    }

    async fn attach(
        &mut self,
        player_id: PlayerId,
        name: String,
        events: Option<mpsc::Sender<ServerMessage>>,
    ) -> Result<Grant, AuthError> {
        let identity = SessionIdentity::new(player_id, name.clone(), Permissions::player());
        let session_id = identity.session_id;
        let (transition, replaced) =
            self.world.connect_player(identity).map_err(|_| AuthError::Internal)?;
        if let Err(err) = self.store.commit(&transition.ops).await {
            tracing::error!(error = %err, "failed to persist session start");
            let _ = self.world.detach(session_id);
            return Err(AuthError::Internal);
        }
        if let Some(old) = replaced {
            self.drop_session(old, "Your link was replaced.");
        }
        if let Some(events) = events {
            self.subs.insert(session_id, events);
        }
        let token = new_token();
        self.tokens.insert(token.clone(), TokenRec { session_id, player_id, name: name.clone() });
        self.publish(session_id, &transition.outcome.events);
        let view = self.world.view(session_id).map_err(|_| AuthError::Internal)?;
        Ok(Grant { token, operator_name: name, view })
    }

    fn resume(
        &mut self,
        token: String,
        events: mpsc::Sender<ServerMessage>,
    ) -> Result<Grant, AuthError> {
        let rec = self.tokens.get(&token).ok_or(AuthError::InvalidCredentials)?;
        let session_id = rec.session_id;
        let name = rec.name.clone();
        if let Some(old) = self.subs.insert(session_id, events) {
            let _ = old.try_send(ServerMessage::Goodbye {
                message: "This session resumed on another link.".into(),
            });
        }
        let view = self.world.view(session_id).map_err(|_| AuthError::Internal)?;
        Ok(Grant { token, operator_name: name, view })
    }

    async fn command(
        &mut self,
        token: String,
        line: String,
    ) -> Result<(bool, GameView), GameError> {
        let Some(rec) = self.tokens.get(&token).map(|rec| TokenRec {
            session_id: rec.session_id,
            player_id: rec.player_id,
            name: rec.name.clone(),
        }) else {
            return Err(GameError::Rejected("session expired".into()));
        };
        let bucket = self.buckets.entry(rec.session_id).or_insert_with(Bucket::new);
        if !bucket.allow() {
            return Err(GameError::Rejected("You are sending commands too quickly.".into()));
        }
        let snapshot = self.world.log_snapshot();
        let operator = self.world.operator(rec.player_id).cloned();
        let geography = self.world.geography_snapshot();
        let social = self.world.social_snapshot();
        let transition = match self.world.apply(rec.session_id, &line) {
            Ok(transition) => transition,
            Err(err) => return Err(err),
        };
        if let Err(err) = self.store.commit(&transition.ops).await {
            tracing::error!(error = %err, "failed to persist command");
            self.world.replace_log(snapshot);
            self.world.restore_geography(geography);
            self.world.restore_social(social);
            if let Some(operator) = operator {
                self.world.insert_operator(operator);
            }
            return Err(GameError::Rejected(
                "The world could not save that. Nothing was kept.".into(),
            ));
        }
        let view = self.world.view(rec.session_id)?;
        self.publish(rec.session_id, &transition.outcome.events);
        if transition.outcome.quit {
            self.disconnect(&token).await;
        }
        Ok((transition.outcome.quit, view))
    }

    async fn disconnect(&mut self, token: &str) {
        let Some(rec) = self.tokens.remove(token) else {
            return;
        };
        self.subs.remove(&rec.session_id);
        self.buckets.remove(&rec.session_id);
        match self.world.detach(rec.session_id) {
            Ok(transition) => {
                if let Err(err) = self.store.commit(&transition.ops).await {
                    tracing::error!(error = %err, "failed to persist disconnect");
                }
                self.publish(rec.session_id, &transition.outcome.events);
            }
            Err(err) => tracing::info!(%err, "disconnect"),
        }
    }

    fn drop_session(&mut self, session_id: SessionId, message: &str) {
        self.tokens.retain(|_, rec| rec.session_id != session_id);
        self.buckets.remove(&session_id);
        if let Some(tx) = self.subs.remove(&session_id) {
            let _ = tx.try_send(ServerMessage::Goodbye { message: message.into() });
        }
    }

    fn publish(&mut self, origin: SessionId, events: &[GameEvent]) {
        if events.is_empty() {
            return;
        }
        let targets: Vec<SessionId> =
            self.subs.keys().copied().filter(|id| *id != origin).collect();
        for session_id in targets {
            let Ok(view) = self.world.view(session_id) else {
                continue;
            };
            let player = self.world.player_of(session_id);
            let location = self.world.location_key(session_id).unwrap_or_default();
            let lines = events
                .iter()
                .filter(|event| match player {
                    Some(player_id) => event.entry.visible_to(&location, player_id),
                    None => false,
                })
                .map(|event| event.entry.to_feed())
                .collect::<Vec<_>>();
            let Some(tx) = self.subs.get(&session_id) else {
                continue;
            };
            if tx.try_send(ServerMessage::Snapshot { view }).is_err() {
                tracing::debug!(%session_id, "dropped snapshot for a slow client");
            }
            if !lines.is_empty() && tx.try_send(ServerMessage::Events { lines }).is_err() {
                tracing::debug!(%session_id, "dropped events for a slow client");
            }
        }
    }

    async fn ensure_operator(&mut self, id: PlayerId) -> Result<(), AuthError> {
        if self.world.operator(id).is_some() {
            return Ok(());
        }
        match self.store.load().await {
            Ok(loaded) => {
                if let Some(op) = loaded.operators.into_iter().find(|op| op.id == id) {
                    self.world.insert_operator(op);
                    Ok(())
                } else {
                    Err(AuthError::Internal)
                }
            }
            Err(err) => {
                tracing::error!(error = %err, "failed to load operator");
                Err(AuthError::Internal)
            }
        }
    }

    fn auth_blocked(&mut self, username: &str) -> bool {
        let now = Instant::now();
        let window = self.auth_failures.entry(username.to_string()).or_default();
        window.retain(|at| now.saturating_duration_since(*at) < Duration::from_secs(60));
        window.len() >= 8
    }

    fn note_auth_failure(&mut self, username: &str) {
        let window = self.auth_failures.entry(username.to_string()).or_default();
        window.push(Instant::now());
    }
}

fn system_now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn new_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

pub(crate) fn is_unique(err: &StoreError) -> bool {
    let mut current: Option<&dyn std::error::Error> = Some(err);
    while let Some(err) = current {
        let text = err.to_string();
        if text.contains("UNIQUE") || text.contains("unique") {
            return true;
        }
        current = err.source();
    }
    false
}
