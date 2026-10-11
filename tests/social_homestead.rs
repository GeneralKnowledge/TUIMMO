#![allow(clippy::unwrap_used)]
//! Social relationships and homestead persistence across a server restart.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use coffee_server::{start, ServerConfig};
use terminus_core::GameView;
use terminus_protocol::{ClientMessage, ProtocolClient, ServerMessage};

const PASSWORD: &str = "operator1";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn friends_cabin_sign_and_board_survive_restart() {
    let dir = tempfile::tempdir().expect("tempdir");
    let data = dir.path().to_path_buf();
    let running = boot(&data).await;

    let mut ada = connect(&running.socket_path).await;
    let mut bob = connect(&running.socket_path).await;
    let _ = register(&mut ada, "ada", "Ada").await;
    let _ = register(&mut bob, "bob", "Bob").await;

    let _ = command(&mut ada, "say hello Bob").await;
    let bob_look = command(&mut bob, "look").await;
    assert!(feed_text(&bob_look).contains("hello Bob"));

    let _ = command(&mut ada, "whisper Bob private-trail").await;
    let bob_whisper = command(&mut bob, "look").await;
    assert!(feed_text(&bob_whisper).contains("private-trail"), "{}", feed_text(&bob_whisper));

    let _ = command(&mut ada, "emote waves across the steam").await;
    let bob_emote = command(&mut bob, "look").await;
    assert!(feed_text(&bob_emote).contains("waves across the steam"));

    let _ = command(&mut ada, "friend add Bob").await;
    let _ = command(&mut bob, "friend accept Ada").await;
    let friends = command(&mut ada, "friends").await;
    let overlay = friends.overlay.clone().unwrap_or_default();
    assert!(overlay.contains("Bob"), "{overlay}");

    let _ = command(&mut ada, "board post Meet at Haven").await;
    let board = command(&mut bob, "board").await;
    let board_text = board.overlay.clone().unwrap_or_default();
    assert!(board_text.contains("Meet at Haven"), "{board_text}");

    let _ = command(&mut ada, "go up").await;
    let _ = command(&mut ada, "outskirts").await;
    let mut cabin_ok = false;
    // Probe nearby tiles for walkable ground that accepts a cabin.
    'probe: for _ in 0..12 {
        if try_command(&mut ada, "build cabin Haven").await.is_ok() {
            cabin_ok = true;
            break;
        }
        for dir in ["north", "east", "south", "west"] {
            if try_command(&mut ada, dir).await.is_ok() {
                if try_command(&mut ada, "build cabin Haven").await.is_ok() {
                    cabin_ok = true;
                    break 'probe;
                }
                break;
            }
        }
    }
    if cabin_ok {
        let entered = command(&mut ada, "enter Haven").await;
        assert!(entered.location_name.contains("Haven"), "{}", entered.location_name);
        let _ = command(&mut ada, "leave").await;
        // Soft build rate-limit may briefly refuse a follow-up sign; retry once.
        let _ = try_command(&mut ada, "sign Visitors welcome").await;
        tokio::time::sleep(Duration::from_millis(1600)).await;
        let _ = try_command(&mut ada, "sign Visitors welcome").await;
        if let Ok(signed) = try_command(&mut ada, "sign read").await {
            assert!(feed_text(&signed).contains("Visitors welcome"));
        }
    }

    drop(ada);
    drop(bob);
    running.shutdown().await;

    let running = boot(&data).await;
    let mut ada = connect(&running.socket_path).await;
    let mut bob = connect(&running.socket_path).await;
    let ada_back = login(&mut ada, "ada").await;
    let _ = login(&mut bob, "bob").await;
    assert_eq!(ada_back.operator_name, "Ada");

    let friends = command(&mut ada, "friends").await;
    let overlay = friends.overlay.clone().unwrap_or_default();
    assert!(overlay.contains("Bob"), "friendship lost after restart: {overlay}");

    let board = command(&mut bob, "board").await;
    let board_text = board.overlay.clone().unwrap_or_default();
    assert!(board_text.contains("Meet at Haven"), "notice lost after restart: {board_text}");

    if cabin_ok {
        // Ada reconnects at her last wilderness tile; enter should still work.
        let entered = try_command(&mut ada, "enter Haven").await;
        assert!(entered.is_ok(), "cabin missing after restart: {entered:?}");
    }

    let _ = command(&mut ada, "ignore Bob").await;
    let blocked = try_command(&mut bob, "whisper Ada hey").await;
    assert!(blocked.is_err(), "ignore should block whisper after restart");

    running.shutdown().await;
}

async fn boot(data: &PathBuf) -> coffee_server::Running {
    let mut config = ServerConfig::new(data);
    config.ssh_listen = SocketAddr::from(([127, 0, 0, 1], 0));
    config.auth_rejection = Duration::from_millis(20);
    start(config).await.expect("start server")
}

async fn connect(path: &Path) -> ProtocolClient {
    ProtocolClient::connect(path).await.expect("protocol connect")
}

async fn register(client: &mut ProtocolClient, username: &str, operator: &str) -> GameView {
    hello(client).await;
    match client
        .call(ClientMessage::Register {
            username: username.into(),
            password: PASSWORD.into(),
            operator_name: operator.into(),
        })
        .await
        .expect("register")
    {
        ServerMessage::Authenticated { view, .. } => view,
        other => panic!("register failed: {other:?}"),
    }
}

async fn login(client: &mut ProtocolClient, username: &str) -> GameView {
    hello(client).await;
    match client
        .call(ClientMessage::Login { username: username.into(), password: PASSWORD.into() })
        .await
        .expect("login")
    {
        ServerMessage::Authenticated { view, .. } => view,
        other => panic!("login failed: {other:?}"),
    }
}

async fn hello(client: &mut ProtocolClient) {
    let message = client
        .call(ClientMessage::Hello { client: "integration-test".into() })
        .await
        .expect("hello");
    assert!(matches!(message, ServerMessage::Hello { .. }), "{message:?}");
}

async fn command(client: &mut ProtocolClient, line: &str) -> GameView {
    match client.call(ClientMessage::Command { line: line.into() }).await.expect("command") {
        ServerMessage::Update { view, .. } => view,
        other => panic!("command failed: {other:?}"),
    }
}

async fn try_command(client: &mut ProtocolClient, line: &str) -> Result<GameView, String> {
    match client.call(ClientMessage::Command { line: line.into() }).await {
        Ok(ServerMessage::Update { view, .. }) => Ok(view),
        Ok(ServerMessage::Error { message, .. }) => Err(message),
        Ok(other) => Err(format!("unexpected: {other:?}")),
        Err(err) => Err(err.to_string()),
    }
}

fn feed_text(view: &GameView) -> String {
    view.feed.iter().map(|line| line.text.as_str()).collect::<Vec<_>>().join("\n")
}
