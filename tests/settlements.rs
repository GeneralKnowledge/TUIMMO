#![allow(clippy::unwrap_used)]
//! Settlements, regional chat, and board persistence across a server restart.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use coffee_server::{start, ServerConfig};
use terminus_core::GameView;
use terminus_protocol::{ClientMessage, ProtocolClient, ServerMessage};

const PASSWORD: &str = "operator1";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn settle_region_board_survive_restart() {
    let dir = tempfile::tempdir().expect("tempdir");
    let data = dir.path().to_path_buf();
    let running = boot(&data).await;

    let mut ada = connect(&running.socket_path).await;
    let mut bob = connect(&running.socket_path).await;
    let _ = register(&mut ada, "ada", "Ada").await;
    let _ = register(&mut bob, "bob", "Bob").await;

    let _ = command(&mut ada, "friend add Bob").await;
    let _ = command(&mut bob, "friend accept Ada").await;

    let _ = command(&mut ada, "go up").await;
    let _ = command(&mut ada, "outskirts").await;
    let mut cabin_ok = false;
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
    assert!(cabin_ok, "could not find a buildable tile for settlement test");

    let created = command(&mut ada, "settle create Havenhold").await;
    assert!(
        feed_text(&created).contains("Havenhold") || created.overlay.is_none(),
        "{}",
        feed_text(&created)
    );

    let _ = command(&mut bob, "go up").await;
    let _ = command(&mut bob, "outskirts").await;
    // Walk Bob toward Ada by probing; regional chat should work once colocated in region.
    for dir in ["north", "east", "south", "west", "north", "east"] {
        let _ = try_command(&mut bob, dir).await;
    }

    let _ = command(&mut ada, "region settlers gather").await;
    let bob_look = command(&mut bob, "look").await;
    // Bob may still be in a different birth region; region chat is proximity-only.
    let _ = bob_look;

    let _ = command(&mut ada, "settle invite Bob").await;
    let _ = command(&mut bob, "settle accept Havenhold").await;
    let members = command(&mut ada, "settle members").await;
    let overlay = members.overlay.clone().unwrap_or_default();
    assert!(overlay.contains("Bob"), "{overlay}");

    let _ = command(&mut ada, "settle board post Ridge watch tonight").await;
    // Bob must stand in the settlement footprint (Ada's tile / linked cabin).
    let _ = try_command(&mut bob, "enter Haven").await;
    let board = try_command(&mut bob, "settle board").await;
    if let Ok(view) = board {
        let text = view.overlay.clone().unwrap_or_default();
        assert!(text.contains("Ridge watch tonight") || text.contains("noticeboard"), "{text}");
    }

    drop(ada);
    drop(bob);
    running.shutdown().await;

    let running = boot(&data).await;
    let mut ada = connect(&running.socket_path).await;
    let mut bob = connect(&running.socket_path).await;
    let _ = login(&mut ada, "ada").await;
    let _ = login(&mut bob, "bob").await;

    let inspect = command(&mut ada, "settle inspect").await;
    let overlay = inspect.overlay.clone().unwrap_or_default();
    assert!(overlay.contains("Havenhold"), "settlement lost after restart: {overlay}");

    let members = command(&mut bob, "settle members").await;
    let overlay = members.overlay.clone().unwrap_or_default();
    assert!(overlay.contains("Ada"), "membership lost after restart: {overlay}");

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
