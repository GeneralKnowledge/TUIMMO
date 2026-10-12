//! Coffee.shop server binary.
//!
//! `serve` is the authoritative process. `play` is a local TUI that talks to
//! that process over the unix socket. `account create` writes a row directly.
//! `invite create` mints single-use registration codes for invite-only hosts.

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use coffee_server::{create_account, create_invite_codes, start, theme, ServerConfig};
use terminus_protocol::ProtocolClient;
use terminus_tui::{run_remote, spawn_crossterm_input, StartMode};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "coffee-server", about = "Coffee.shop authoritative server")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Listen for SSH and protocol connections.
    Serve(ServeArgs),
    /// Open a local TUI against a running server.
    Play(PlayArgs),
    /// Offline account administration.
    Account {
        #[command(subcommand)]
        action: AccountCmd,
    },
    /// Offline invite-code administration.
    Invite {
        #[command(subcommand)]
        action: InviteCmd,
    },
}

#[derive(Parser)]
struct ServeArgs {
    /// Database, host key, and protocol socket directory.
    #[arg(long, default_value = "data", env = "COFFEE_DATA_DIR")]
    data_dir: PathBuf,
    /// SSH listen address.
    #[arg(long, default_value = "0.0.0.0:2222", env = "COFFEE_SSH_LISTEN")]
    ssh_listen: SocketAddr,
    /// Refuse new registrations. Existing accounts can still log in.
    #[arg(long, env = "COFFEE_NO_REGISTRATION", action = clap::ArgAction::SetTrue)]
    no_registration: bool,
    /// Require a single-use invite code to register.
    #[arg(long, env = "COFFEE_INVITE_ONLY", action = clap::ArgAction::SetTrue)]
    invite_only: bool,
    /// Maximum number of accounts. Zero means unlimited.
    #[arg(long, default_value_t = 0, env = "COFFEE_MAX_ACCOUNTS")]
    max_accounts: u32,
    /// Friend codes minted when someone redeems an invite.
    #[arg(long, default_value_t = 2, env = "COFFEE_INVITES_PER_PLAYER")]
    invites_per_player: u32,
    /// Draw plain ASCII borders.
    #[arg(long, env = "COFFEE_ASCII", action = clap::ArgAction::SetTrue)]
    ascii: bool,
    /// Maximum simultaneous SSH sessions.
    #[arg(long, default_value_t = 64)]
    max_sessions: usize,
}

#[derive(Parser)]
struct PlayArgs {
    #[arg(long, default_value = "data", env = "COFFEE_DATA_DIR")]
    data_dir: PathBuf,
    #[arg(long, env = "COFFEE_ASCII", action = clap::ArgAction::SetTrue)]
    ascii: bool,
}

#[derive(Subcommand)]
enum AccountCmd {
    /// Create an account. The password is read from COFFEE_PASSWORD, never from argv.
    Create {
        #[arg(long)]
        username: String,
        /// Display name. Defaults to the username.
        #[arg(long, default_value = "")]
        operator_name: String,
        #[arg(long, default_value = "data", env = "COFFEE_DATA_DIR")]
        data_dir: PathBuf,
    },
}

#[derive(Subcommand)]
enum InviteCmd {
    /// Mint unused registration codes and print them, one per line.
    Create {
        #[arg(long, default_value_t = 1)]
        count: u32,
        /// Short label stored with the batch (for example "reddit").
        #[arg(long, default_value = "")]
        note: String,
        #[arg(long, default_value = "data", env = "COFFEE_DATA_DIR")]
        data_dir: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let cli = Cli::parse();
    match cli.command.unwrap_or_else(default_serve) {
        Command::Serve(args) => serve(args).await,
        Command::Play(args) => play(args).await,
        Command::Account { action: AccountCmd::Create { username, operator_name, data_dir } } => {
            let password = std::env::var("COFFEE_PASSWORD").context(
                "set COFFEE_PASSWORD in the environment; it is not accepted as an argument",
            )?;
            create_account(&data_dir, &username, &password, &operator_name).await
        }
        Command::Invite { action: InviteCmd::Create { count, note, data_dir } } => {
            let codes = create_invite_codes(&data_dir, count, &note).await?;
            for code in codes {
                println!("{code}");
            }
            Ok(())
        }
    }
}

fn default_serve() -> Command {
    let data_dir = std::env::var("COFFEE_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("data"));
    let ssh_listen = std::env::var("COFFEE_SSH_LISTEN")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], 2222)));
    let max_accounts =
        std::env::var("COFFEE_MAX_ACCOUNTS").ok().and_then(|value| value.parse().ok()).unwrap_or(0);
    let invites_per_player = std::env::var("COFFEE_INVITES_PER_PLAYER")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(2);
    Command::Serve(ServeArgs {
        data_dir,
        ssh_listen,
        no_registration: env_flag("COFFEE_NO_REGISTRATION"),
        invite_only: env_flag("COFFEE_INVITE_ONLY"),
        max_accounts,
        invites_per_player,
        ascii: env_flag("COFFEE_ASCII"),
        max_sessions: 64,
    })
}

fn env_flag(name: &str) -> bool {
    matches!(
        std::env::var(name).ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes")
    )
}

async fn serve(args: ServeArgs) -> anyhow::Result<()> {
    let mut config = ServerConfig::new(args.data_dir);
    config.ssh_listen = args.ssh_listen;
    config.allow_registration = !args.no_registration;
    config.invite_only = args.invite_only;
    config.max_accounts = args.max_accounts;
    config.invites_per_player = args.invites_per_player;
    config.ascii = args.ascii;
    config.max_sessions = args.max_sessions;
    let running = start(config).await?;
    tracing::info!(
        ssh = %running.ssh_addr,
        socket = %running.socket_path.display(),
        invite_only = args.invite_only,
        max_accounts = args.max_accounts,
        "coffee.shop ready"
    );
    tokio::signal::ctrl_c().await.context("wait for ctrl-c")?;
    tracing::info!("shutting down");
    running.shutdown().await;
    Ok(())
}

async fn play(args: PlayArgs) -> anyhow::Result<()> {
    let socket = args.data_dir.join("game.sock");
    let mut client = ProtocolClient::connect(&socket).await.with_context(|| {
        format!("connect to {} (is coffee-server serve running?)", socket.display())
    })?;

    let mut stdout = std::io::stdout();
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let mut terminal = ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(stdout))?;
    let _restore = Restore;
    let input = spawn_crossterm_input();
    let result =
        run_remote(&mut terminal, &mut client, theme(args.ascii), StartMode::Login, input).await;
    result.map_err(|err| anyhow::anyhow!(err))
}

struct Restore;

impl Drop for Restore {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
        let mut stdout = std::io::stdout();
        let _ = crossterm::execute!(stdout, crossterm::terminal::LeaveAlternateScreen);
        let _ = std::io::Write::flush(&mut stdout);
    }
}
