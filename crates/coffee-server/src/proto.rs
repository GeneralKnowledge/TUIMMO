//! Unix-socket protocol server.
//!
//! The socket is mode `0600` and is the only path from a TUI into the hub.
//! SSH clients authenticate in-process, then resume over this socket.
//! Frames are untrusted until they pass the codec.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use terminus_protocol::{
    decode_client, encode_frame, ClientMessage, FrameDecoder, ServerFrame, ServerMessage,
    PROTOCOL_VERSION,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

use crate::hub::Hub;
use coffee_world::AuthError;
use terminus_core::GameError;

const MAX_PROTOCOL_CONNECTIONS: usize = 128;

pub fn bind(path: &std::path::Path) -> std::io::Result<UnixListener> {
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let listener = UnixListener::bind(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(listener)
}

pub async fn serve(listener: UnixListener, hub: Hub, mut shutdown: broadcast::Receiver<()>) {
    let live = Arc::new(AtomicUsize::new(0));
    let mut tasks: Vec<JoinHandle<()>> = Vec::new();
    loop {
        tokio::select! {
            _ = shutdown.recv() => break,
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else {
                    break;
                };
                if live.load(Ordering::Relaxed) >= MAX_PROTOCOL_CONNECTIONS {
                    tracing::info!("protocol connection refused: limit");
                    drop(stream);
                    continue;
                }
                live.fetch_add(1, Ordering::Relaxed);
                let hub = hub.clone();
                let counter = Arc::clone(&live);
                tasks.retain(|task| !task.is_finished());
                tasks.push(tokio::spawn(async move {
                    handle_conn(stream, hub).await;
                    counter.fetch_sub(1, Ordering::Relaxed);
                }));
            }
        }
    }
    for task in tasks {
        task.abort();
    }
}

async fn handle_conn(stream: UnixStream, hub: Hub) {
    let (mut read, mut write) = stream.into_split();
    let (out_tx, mut out_rx) = mpsc::channel::<Vec<u8>>(32);
    let writer = tokio::spawn(async move {
        while let Some(frame) = out_rx.recv().await {
            if write.write_all(&frame).await.is_err() || write.flush().await.is_err() {
                break;
            }
        }
    });

    let mut decoder = FrameDecoder::new();
    let mut buf = [0_u8; 4096];
    let mut greeted = false;
    let mut token: Option<String> = None;
    let mut forward: Option<JoinHandle<()>> = None;

    'conn: loop {
        let n = match read.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let frames = match decoder.push(&buf[..n]) {
            Ok(frames) => frames,
            Err(err) => {
                let _ = send(&out_tx, 0, ServerMessage::error("protocol", err.to_string())).await;
                break;
            }
        };
        for body in frames {
            let frame = match decode_client(&body) {
                Ok(frame) => frame,
                Err(err) => {
                    let _ =
                        send(&out_tx, 0, ServerMessage::error("protocol", err.to_string())).await;
                    break 'conn;
                }
            };
            if !greeted {
                match frame.body {
                    ClientMessage::Hello { client } => {
                        let client = client.chars().take(64).collect::<String>();
                        tracing::debug!(%client, "protocol hello");
                        greeted = true;
                        if send(
                            &out_tx,
                            frame.id,
                            ServerMessage::Hello {
                                version: PROTOCOL_VERSION,
                                motd: "Coffee.shop — Undernet. This session runs the game, not a shell.".into(),
                            },
                        )
                        .await
                        .is_err()
                        {
                            break 'conn;
                        }
                    }
                    _ => {
                        let _ = send(
                            &out_tx,
                            frame.id,
                            ServerMessage::error("protocol", "send hello first"),
                        )
                        .await;
                        break 'conn;
                    }
                }
                continue;
            }

            let close =
                dispatch(&hub, &out_tx, &mut token, &mut forward, frame.id, frame.body).await;
            if close {
                break 'conn;
            }
        }
    }

    if let Some(token) = token.take() {
        hub.disconnect(&token).await;
    }
    if let Some(task) = forward.take() {
        task.abort();
    }
    writer.abort();
}

async fn dispatch(
    hub: &Hub,
    out_tx: &mpsc::Sender<Vec<u8>>,
    token: &mut Option<String>,
    forward: &mut Option<JoinHandle<()>>,
    id: u64,
    body: ClientMessage,
) -> bool {
    match body {
        ClientMessage::Hello { .. } => {
            let _ = send(out_tx, id, ServerMessage::error("protocol", "already greeted")).await;
            false
        }
        ClientMessage::Ping { nonce } => {
            let _ = send(out_tx, id, ServerMessage::Pong { nonce }).await;
            false
        }
        ClientMessage::Login { .. }
        | ClientMessage::Register { .. }
        | ClientMessage::Resume { .. }
            if token.is_some() =>
        {
            let _ = send(
                out_tx,
                id,
                ServerMessage::error("auth", "this link is already authenticated"),
            )
            .await;
            false
        }
        ClientMessage::Login { username, password } => {
            let events = spawn_forward(out_tx.clone(), forward);
            match hub.login(username, password, Some(events)).await {
                Ok(grant) => finish_auth(out_tx, token, id, grant).await,
                Err(err) => {
                    stop_forward(forward);
                    let _ = send(out_tx, id, auth_error(err)).await;
                    false
                }
            }
        }
        ClientMessage::Register { username, password, operator_name, invite_code } => {
            let events = spawn_forward(out_tx.clone(), forward);
            match hub.register(username, password, operator_name, invite_code, Some(events)).await {
                Ok(grant) => finish_auth(out_tx, token, id, grant).await,
                Err(err) => {
                    stop_forward(forward);
                    let _ = send(out_tx, id, auth_error(err)).await;
                    false
                }
            }
        }
        ClientMessage::Resume { token: resume_token } => {
            let events = spawn_forward(out_tx.clone(), forward);
            match hub.resume(resume_token, events).await {
                Ok(grant) => finish_auth(out_tx, token, id, grant).await,
                Err(err) => {
                    stop_forward(forward);
                    let _ = send(out_tx, id, auth_error(err)).await;
                    false
                }
            }
        }
        ClientMessage::Command { line } => {
            let Some(current) = token.clone() else {
                let _ = send(out_tx, id, ServerMessage::error("auth", "not authenticated")).await;
                return false;
            };
            match hub.command(current, line).await {
                Ok((quit, view)) => {
                    let _ = send(out_tx, id, ServerMessage::Update { quit, view }).await;
                    quit
                }
                Err(err) => {
                    let _ = send(out_tx, id, game_error(err)).await;
                    false
                }
            }
        }
    }
}

fn spawn_forward(
    out_tx: mpsc::Sender<Vec<u8>>,
    forward: &mut Option<JoinHandle<()>>,
) -> mpsc::Sender<ServerMessage> {
    stop_forward(forward);
    let (events_tx, mut events_rx) = mpsc::channel(64);
    *forward = Some(tokio::spawn(async move {
        while let Some(message) = events_rx.recv().await {
            if send(&out_tx, 0, message).await.is_err() {
                break;
            }
        }
    }));
    events_tx
}

fn stop_forward(forward: &mut Option<JoinHandle<()>>) {
    if let Some(task) = forward.take() {
        task.abort();
    }
}

async fn finish_auth(
    out_tx: &mpsc::Sender<Vec<u8>>,
    token: &mut Option<String>,
    id: u64,
    grant: crate::hub::Grant,
) -> bool {
    *token = Some(grant.token.clone());
    send(
        out_tx,
        id,
        ServerMessage::Authenticated {
            token: grant.token,
            operator_name: grant.operator_name,
            view: grant.view,
        },
    )
    .await
    .is_err()
}

fn auth_error(err: AuthError) -> ServerMessage {
    let code = match err {
        AuthError::RateLimited => "rate",
        AuthError::RegistrationClosed => "registration",
        AuthError::InviteRequired | AuthError::InvalidInvite | AuthError::InviteUsed => "invite",
        AuthError::AccountCapReached => "full",
        AuthError::NameTaken => "taken",
        AuthError::InvalidUsername | AuthError::InvalidOperatorName | AuthError::WeakPassword => {
            "invalid"
        }
        AuthError::InvalidCredentials => "auth",
        AuthError::Internal => "internal",
    };
    ServerMessage::error(code, err.to_string())
}

fn game_error(err: GameError) -> ServerMessage {
    match err {
        GameError::UnknownSession => ServerMessage::error("session", "unknown session"),
        GameError::Rejected(message) => ServerMessage::error("rejected", message),
    }
}

async fn send(tx: &mpsc::Sender<Vec<u8>>, id: u64, body: ServerMessage) -> Result<(), ()> {
    let frame = ServerFrame { v: PROTOCOL_VERSION, id, body };
    let bytes = encode_frame(&frame).map_err(|_| ())?;
    tx.send(bytes).await.map_err(|_| ())
}
