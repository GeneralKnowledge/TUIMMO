//! Drive the TUI from the versioned protocol.

use ratatui::backend::Backend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use terminus_core::InputEvent;
use terminus_protocol::{ClientMessage, ProtocolClient, ServerMessage, PROTOCOL_VERSION};
use tokio::sync::mpsc;

use crate::app::{App, Effect};
use crate::decode::InputDecoder;
use crate::theme::Theme;
use crate::TuiError;

/// Bytes or already-decoded keys from a transport.
#[derive(Clone, Debug)]
pub enum Input {
    Event(InputEvent),
}

#[derive(Clone, Debug)]
pub enum StartMode {
    /// SSH lobby or local play. The player picks guest, login, register, or recovery.
    Welcome,
    /// The gateway already authenticated this operator with a game password.
    Resume(String),
}

pub async fn run_remote<B: Backend>(
    terminal: &mut Terminal<B>,
    client: &mut ProtocolClient,
    theme: Theme,
    mode: StartMode,
    mut input: mpsc::UnboundedReceiver<Input>,
) -> Result<(), TuiError> {
    let hello = client
        .call(ClientMessage::Hello { client: format!("terminus-tui/{PROTOCOL_VERSION}") })
        .await?;
    if let ServerMessage::Error { message, .. } = hello {
        return Err(TuiError::Protocol(terminus_protocol::ProtocolError::Rejected(message)));
    }

    let mut app = match mode {
        StartMode::Welcome => App::welcome(theme),
        StartMode::Resume(token) => {
            let message = client.call(ClientMessage::Resume { token }).await?;
            match message {
                ServerMessage::Authenticated { view, .. } => App::play(theme, view),
                ServerMessage::Error { message, .. } => {
                    return Err(TuiError::Protocol(terminus_protocol::ProtocolError::Rejected(
                        message,
                    )));
                }
                other => {
                    return Err(TuiError::Protocol(terminus_protocol::ProtocolError::Rejected(
                        format!("unexpected greeting: {other:?}"),
                    )));
                }
            }
        }
    };

    let mut pushes = client.take_pushes()?;
    let mut decoder = InputDecoder::new();
    loop {
        terminal.draw(|frame| app.render(frame)).map_err(term_err)?;
        tokio::select! {
            incoming = input.recv() => {
                let Some(Input::Event(event)) = incoming else {
                    break;
                };
                if !handle_input(terminal, client, &mut app, &mut decoder, event).await? {
                    break;
                }
            }
            push = pushes.recv() => {
                let Some(message) = push else {
                    app.set_notice("The server closed the link.");
                    terminal.draw(|frame| app.render(frame)).map_err(term_err)?;
                    break;
                };
                if !apply_server(&mut app, message) {
                    terminal.draw(|frame| app.render(frame)).map_err(term_err)?;
                    break;
                }
            }
        }
    }
    Ok(())
}

async fn handle_input<B: Backend>(
    terminal: &mut Terminal<B>,
    client: &mut ProtocolClient,
    app: &mut App,
    decoder: &mut InputDecoder,
    event: InputEvent,
) -> Result<bool, TuiError> {
    match event {
        InputEvent::Resize { cols, rows } => {
            terminal.resize(Rect { x: 0, y: 0, width: cols, height: rows }).map_err(term_err)?;
            Ok(true)
        }
        InputEvent::Bytes(bytes) => {
            let mut stay = true;
            for key in decoder.push(&bytes) {
                if !handle_key(client, app, key).await? {
                    stay = false;
                    break;
                }
            }
            Ok(stay)
        }
        InputEvent::Key(key) => handle_key(client, app, key).await,
    }
}

async fn handle_key(
    client: &mut ProtocolClient,
    app: &mut App,
    key: terminus_core::Key,
) -> Result<bool, TuiError> {
    match app.handle_key(key) {
        Effect::None => Ok(true),
        Effect::Quit => Ok(false),
        Effect::SubmitCommand(line) => {
            let message = client.call(ClientMessage::Command { line }).await?;
            Ok(apply_server(app, message))
        }
        Effect::SubmitGuest { username, operator_name } => {
            let message = client.call(ClientMessage::Guest { username, operator_name }).await?;
            Ok(apply_server(app, message))
        }
        Effect::SubmitLogin { username, password } => {
            let message = client.call(ClientMessage::Login { username, password }).await?;
            Ok(apply_server(app, message))
        }
        Effect::SubmitRegister { username, password, operator_name, email } => {
            let message = client
                .call(ClientMessage::Register { username, password, operator_name, email })
                .await?;
            Ok(apply_server(app, message))
        }
        Effect::SubmitRecover { username, recovery_code } => {
            let message = client.call(ClientMessage::Recover { username, recovery_code }).await?;
            Ok(apply_server(app, message))
        }
        Effect::SubmitSecret { password, upgrade, email } => {
            let body = if upgrade {
                ClientMessage::Upgrade { password, email }
            } else {
                ClientMessage::SetPassword { password }
            };
            let message = client.call(body).await?;
            Ok(apply_server(app, message))
        }
        Effect::RotateRecovery => {
            let message = client.call(ClientMessage::RotateRecovery).await?;
            Ok(apply_server(app, message))
        }
        Effect::RevokeRecovery => {
            let message = client.call(ClientMessage::RevokeRecovery).await?;
            Ok(apply_server(app, message))
        }
        Effect::OpenAccount => {
            let message = client.call(ClientMessage::AccountInfo).await?;
            Ok(apply_server(app, message))
        }
    }
}

fn term_err<E: std::fmt::Display>(err: E) -> TuiError {
    TuiError::Io(std::io::Error::other(err.to_string()))
}

fn apply_server(app: &mut App, message: ServerMessage) -> bool {
    match message {
        ServerMessage::Authenticated { view, recovery_code, account_kind, .. } => {
            if let Some(code) = recovery_code {
                app.show_recovery_code(code.0, account_kind.unwrap_or_default(), view);
            } else {
                app.enter_play(view);
            }
            true
        }
        ServerMessage::AccountStatus { kind, message, recovery_code, .. } => {
            if let Some(code) = recovery_code {
                let view = app.view().clone();
                app.show_recovery_code(code.0, kind, view);
            } else {
                app.show_account(kind, message);
            }
            true
        }
        ServerMessage::Update { quit, view } => {
            app.set_view(view);
            !quit
        }
        ServerMessage::Snapshot { view } => {
            app.set_view(view);
            true
        }
        ServerMessage::Error { message, .. } => {
            app.set_notice(message);
            true
        }
        ServerMessage::Goodbye { message } => {
            app.set_notice(message);
            false
        }
        ServerMessage::Hello { .. } | ServerMessage::Pong { .. } | ServerMessage::Events { .. } => {
            true
        }
    }
}
