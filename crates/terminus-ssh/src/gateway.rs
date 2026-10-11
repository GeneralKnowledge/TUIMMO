//! russh server. A successful password auth starts the game TUI on the
//! session channel. Exec, subsystems, and forwarding are refused.

use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::server::{Auth, Config, Handler, Msg, Server, Session};
use russh::{Channel, ChannelId, MethodKind, MethodSet, Pty};
use terminus_core::InputEvent;
use terminus_protocol::ProtocolClient;
use terminus_tui::{run_remote, Input, StartMode, Theme};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::keys::{load_or_create_host_key, public_key_line};
use crate::limits::{GateState, Limits};
use crate::SshError;

/// Token returned by a successful authentication. The TUI resumes it over the game protocol.
#[derive(Clone, Debug)]
pub struct SessionGrant {
    /// `None` is a lobby. The SSH username is not a game account.
    pub token: Option<String>,
    pub display_name: String,
}

/// Why authentication failed. This string is logged. It must not contain a password.
#[derive(Clone, Debug)]
pub struct AuthReject {
    pub reason: String,
}

/// Game-server authentication used by the gateway. Implementations must not log passwords.
pub trait Authenticator: Clone + Send + Sync + 'static {
    fn authenticate(
        &self,
        username: &str,
        password: &str,
    ) -> impl std::future::Future<Output = Result<SessionGrant, AuthReject>> + Send;

    fn disconnect(&self, token: &str) -> impl std::future::Future<Output = ()> + Send;
}

pub struct SshConfig {
    pub listen: SocketAddr,
    pub host_key_path: PathBuf,
    pub game_socket: PathBuf,
    pub theme: Theme,
    pub limits: Limits,
    pub auth_rejection: Duration,
}

/// Bind the SSH port, signal the bound address, and serve until `shutdown` fires.
pub async fn serve<A: Authenticator>(
    ssh: SshConfig,
    auth: A,
    ready: oneshot::Sender<SocketAddr>,
    mut shutdown: broadcast::Receiver<()>,
) -> Result<(), SshError> {
    let key = load_or_create_host_key(&ssh.host_key_path)?;
    match public_key_line(&key) {
        Ok(line) => tracing::info!(public_key = %line, "ssh host key"),
        Err(err) => tracing::warn!(%err, "could not display host public key"),
    }
    let mut methods = MethodSet::empty();
    methods.push(MethodKind::None);
    methods.push(MethodKind::Password);
    let config = Arc::new(Config {
        methods,
        auth_rejection_time: ssh.auth_rejection,
        auth_rejection_time_initial: Some(Duration::from_millis(0)),
        keys: vec![key],
        inactivity_timeout: Some(Duration::from_secs(15 * 60)),
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        nodelay: true,
        max_auth_attempts: 6,
        ..Config::default()
    });
    let shared = Arc::new(Shared {
        auth,
        game_socket: ssh.game_socket,
        theme: ssh.theme,
        limits: Mutex::new(GateState::new(ssh.limits)),
    });
    let mut gateway = Gateway { shared, slot: Arc::new(Slot::inert()) };
    let listener = TcpListener::bind(ssh.listen).await?;
    let addr = listener.local_addr()?;
    let _ = ready.send(addr);
    tracing::info!(%addr, "ssh listening");
    let running = gateway.run_on_socket(config, &listener);
    let handle = running.handle();
    tokio::pin!(running);
    tokio::select! {
        result = &mut running => {
            result.map_err(SshError::Io)?;
        }
        _ = shutdown.recv() => {
            handle.shutdown("server shutting down".into());
            let _ = (&mut running).await;
        }
    }
    Ok(())
}

struct Shared<A> {
    auth: A,
    game_socket: PathBuf,
    theme: Theme,
    limits: Mutex<GateState>,
}

struct Slot {
    addr: Option<SocketAddr>,
    admitted: bool,
    authed: Mutex<Option<SessionGrant>>,
    size: Mutex<(u16, u16)>,
    ui: Mutex<Option<mpsc::UnboundedSender<Input>>>,
    task: Mutex<Option<JoinHandle<()>>>,
    started: Mutex<bool>,
}

impl Slot {
    fn inert() -> Self {
        Self {
            addr: None,
            admitted: false,
            authed: Mutex::new(None),
            size: Mutex::new((80, 24)),
            ui: Mutex::new(None),
            task: Mutex::new(None),
            started: Mutex::new(false),
        }
    }

    fn new(addr: Option<SocketAddr>, admitted: bool) -> Self {
        Self { addr, admitted, ..Self::inert() }
    }
}

struct Gateway<A: Authenticator> {
    shared: Arc<Shared<A>>,
    slot: Arc<Slot>,
}

impl<A: Authenticator> Drop for Gateway<A> {
    fn drop(&mut self) {
        if let Some(task) = self.slot.task.lock().unwrap_or_else(|err| err.into_inner()).take() {
            task.abort();
        }
        if let Some(grant) = self.slot.authed.lock().unwrap_or_else(|err| err.into_inner()).take() {
            if let Some(token) = grant.token {
                let auth = self.shared.auth.clone();
                tokio::spawn(async move {
                    auth.disconnect(&token).await;
                });
            }
        }
        if self.slot.admitted {
            self.shared
                .limits
                .lock()
                .unwrap_or_else(|err| err.into_inner())
                .release(self.slot.addr);
        }
    }
}

impl<A: Authenticator> Server for Gateway<A> {
    type Handler = Self;

    fn new_client(&mut self, peer_addr: Option<SocketAddr>) -> Self::Handler {
        let admitted =
            self.shared.limits.lock().unwrap_or_else(|err| err.into_inner()).try_acquire(peer_addr);
        if !admitted {
            tracing::info!(?peer_addr, "ssh connection refused: limit");
        }
        Self { shared: Arc::clone(&self.shared), slot: Arc::new(Slot::new(peer_addr, admitted)) }
    }

    fn handle_session_error(&mut self, error: <Self::Handler as Handler>::Error) {
        tracing::info!(%error, "ssh session ended");
    }
}

impl<A: Authenticator> Handler for Gateway<A> {
    type Error = SshError;

    async fn auth_none(&mut self, _user: &str) -> Result<Auth, Self::Error> {
        if !self.slot.admitted {
            return Ok(Auth::reject());
        }
        // The operating-system username is not a game identity. The TUI asks who they are.
        tracing::info!("ssh lobby opened");
        *self.slot.authed.lock().unwrap_or_else(|err| err.into_inner()) =
            Some(SessionGrant { token: None, display_name: String::new() });
        Ok(Auth::Accept)
    }

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        if !self.slot.admitted {
            return Ok(Auth::reject());
        }
        let blocked = self
            .shared
            .limits
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .blocked(self.slot.addr);
        if blocked {
            tracing::info!(user, "ssh auth blocked by failure limit");
            return Ok(Auth::reject());
        }
        match self.shared.auth.authenticate(user, password).await {
            Ok(grant) => {
                tracing::info!(user, operator = %grant.display_name, "ssh auth accepted");
                *self.slot.authed.lock().unwrap_or_else(|err| err.into_inner()) = Some(grant);
                Ok(Auth::Accept)
            }
            Err(reject) => {
                self.shared
                    .limits
                    .lock()
                    .unwrap_or_else(|err| err.into_inner())
                    .record_failure(self.slot.addr);
                tracing::info!(user, reason = %reject.reason, "ssh auth rejected");
                Ok(Auth::reject())
            }
        }
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: russh::server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let authed = self.slot.authed.lock().unwrap_or_else(|err| err.into_inner()).is_some();
        if authed {
            reply.accept().await;
        }
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        _command: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_failure(channel)?;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel: ChannelId,
        _name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_failure(channel)?;
        Ok(())
    }

    async fn env_request(
        &mut self,
        channel: ChannelId,
        _variable_name: &str,
        _variable_value: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_failure(channel)?;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        *self.slot.size.lock().unwrap_or_else(|err| err.into_inner()) =
            (col_width as u16, row_height as u16);
        if let Some(tx) = self.slot.ui.lock().unwrap_or_else(|err| err.into_inner()).clone() {
            let _ = tx.send(Input::Event(InputEvent::Resize {
                cols: col_width as u16,
                rows: row_height as u16,
            }));
        }
        session.channel_success(channel)?;
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        channel: ChannelId,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        *self.slot.size.lock().unwrap_or_else(|err| err.into_inner()) =
            (col_width as u16, row_height as u16);
        if let Some(tx) = self.slot.ui.lock().unwrap_or_else(|err| err.into_inner()).clone() {
            let _ = tx.send(Input::Event(InputEvent::Resize {
                cols: col_width as u16,
                rows: row_height as u16,
            }));
        }
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let authed = self.slot.authed.lock().unwrap_or_else(|err| err.into_inner()).is_some();
        if !authed {
            session.channel_failure(channel)?;
            return Ok(());
        }
        session.channel_success(channel)?;
        self.spawn_ui(session.handle(), channel).await;
        Ok(())
    }

    async fn data(
        &mut self,
        _channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        if data.len() > 4096 {
            return Ok(());
        }
        if let Some(tx) = self.slot.ui.lock().unwrap_or_else(|err| err.into_inner()).clone() {
            let _ = tx.send(Input::Event(InputEvent::Bytes(data.to_vec())));
        }
        Ok(())
    }
}

impl<A: Authenticator> Gateway<A> {
    async fn spawn_ui(&mut self, handle: russh::server::Handle, channel: ChannelId) {
        let mut started = self.slot.started.lock().unwrap_or_else(|err| err.into_inner());
        if *started {
            return;
        }
        *started = true;
        drop(started);

        let grant = self.slot.authed.lock().unwrap_or_else(|err| err.into_inner()).clone();
        let Some(grant) = grant else {
            return;
        };
        let (cols, rows) = *self.slot.size.lock().unwrap_or_else(|err| err.into_inner());
        let (tx, rx) = mpsc::unbounded_channel();
        *self.slot.ui.lock().unwrap_or_else(|err| err.into_inner()) = Some(tx);
        let socket = self.shared.game_socket.clone();
        let theme = self.shared.theme.clone();
        let task = tokio::spawn(async move {
            if let Err(err) =
                drive(handle.clone(), channel, socket, theme, grant, cols, rows, rx).await
            {
                tracing::info!(%err, "tui session ended");
            }
            let _ = handle.close(channel).await;
        });
        *self.slot.task.lock().unwrap_or_else(|err| err.into_inner()) = Some(task);
    }
}

#[allow(clippy::too_many_arguments)]
async fn drive(
    handle: russh::server::Handle,
    channel: ChannelId,
    socket: PathBuf,
    theme: Theme,
    grant: SessionGrant,
    cols: u16,
    rows: u16,
    input: mpsc::UnboundedReceiver<Input>,
) -> Result<(), SshError> {
    let mut client =
        ProtocolClient::connect(&socket).await.map_err(|err| SshError::Message(err.to_string()))?;
    let mut terminal_handle = TerminalHandle::start(handle, channel).await;
    let restore = terminal_handle.sender();
    terminal_handle.write_all(b"\x1b[?1049h\x1b[?25l")?;
    terminal_handle.flush()?;
    let backend = ratatui::backend::CrosstermBackend::new(terminal_handle);
    let area = ratatui::layout::Rect { x: 0, y: 0, width: cols.max(1), height: rows.max(1) };
    let mut terminal = ratatui::Terminal::with_options(
        backend,
        ratatui::TerminalOptions { viewport: ratatui::Viewport::Fixed(area) },
    )?;
    let mode = match grant.token {
        Some(token) => StartMode::Resume(token),
        None => StartMode::Welcome,
    };
    let result = run_remote(&mut terminal, &mut client, theme, mode, input).await;
    let _ = restore.send(b"\x1b[0m\x1b[?25h\x1b[?1049l".to_vec());
    result.map_err(|err| SshError::Message(err.to_string()))
}

struct TerminalHandle {
    sender: mpsc::UnboundedSender<Vec<u8>>,
    sink: Vec<u8>,
}

impl TerminalHandle {
    fn sender(&self) -> mpsc::UnboundedSender<Vec<u8>> {
        self.sender.clone()
    }

    async fn start(handle: russh::server::Handle, channel: ChannelId) -> Self {
        let (sender, mut receiver) = mpsc::unbounded_channel::<Vec<u8>>();
        tokio::spawn(async move {
            while let Some(data) = receiver.recv().await {
                if handle.data(channel, data).await.is_err() {
                    break;
                }
            }
        });
        Self { sender, sink: Vec::new() }
    }
}

impl std::io::Write for TerminalHandle {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.sink.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if self.sink.is_empty() {
            return Ok(());
        }
        self.sender
            .send(std::mem::take(&mut self.sink))
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::BrokenPipe, err))?;
        Ok(())
    }
}
