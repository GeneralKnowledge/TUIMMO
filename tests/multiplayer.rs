#![allow(clippy::unwrap_used)]
//! Two protocol clients against one authoritative server.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use coffee_server::{start, ServerConfig};
use terminus_core::GameView;
use terminus_protocol::{ClientMessage, ProtocolClient, ServerMessage};
use tokio::time::sleep;

const PASSWORD: &str = "operator1";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_operators_move_chat_reconnect_and_survive_restart() {
    let dir = tempfile::tempdir().expect("tempdir");
    let data = dir.path().to_path_buf();
    let running = boot(&data).await;

    let mut ada = connect(&running.socket_path).await;
    let mut bob = connect(&running.socket_path).await;
    let ada_view = register(&mut ada, "ada", "Ada").await;
    let bob_view = register(&mut bob, "bob", "Bob").await;
    assert_eq!(ada_view.location_name, "The Coffee Shop");
    assert_eq!(bob_view.location_name, "The Coffee Shop");
    assert!(
        ada_view.inventory.iter().any(|item| item.name.contains("loyalty")),
        "starter item missing: {:?}",
        ada_view.inventory
    );

    let seen = command(&mut ada, "look").await;
    assert!(
        seen.present.iter().any(|person| person.name == "Bob"),
        "ada should see bob: {:?}",
        seen.present
    );

    let bob_left = command(&mut bob, "go alley").await;
    assert_eq!(bob_left.location_name, "Neon Alley");
    let ada_after = command(&mut ada, "look").await;
    assert!(
        !ada_after.present.iter().any(|person| person.name == "Bob"),
        "bob should have left: {:?}",
        ada_after.present
    );
    assert!(
        feed_text(&ada_after).contains("heads toward"),
        "missing movement notice: {}",
        feed_text(&ada_after)
    );

    let _ = command(&mut ada, "say local-secret").await;
    let bob_local = command(&mut bob, "look").await;
    assert!(
        !feed_text(&bob_local).contains("local-secret"),
        "local speech leaked: {}",
        feed_text(&bob_local)
    );

    let _ = command(&mut ada, "chat global-beacon").await;
    let bob_global = command(&mut bob, "look").await;
    assert!(
        feed_text(&bob_global).contains("global-beacon"),
        "global chat missing: {}",
        feed_text(&bob_global)
    );

    let (ada_moved, bob_moved) =
        tokio::join!(command(&mut ada, "go up"), command(&mut bob, "go shop"),);
    assert_eq!(ada_moved.location_name, "Roof Array");
    assert_eq!(bob_moved.location_name, "The Coffee Shop");

    drop(bob);
    let mut saw_leave = false;
    for _ in 0..40 {
        let view = command(&mut ada, "look").await;
        if feed_text(&view).contains("disconnects") {
            saw_leave = true;
            break;
        }
        sleep(Duration::from_millis(50)).await;
    }
    assert!(saw_leave, "ada did not see bob disconnect");
    let still = command(&mut ada, "status").await;
    assert_eq!(still.operator_name, "Ada");

    let mut bob = connect(&running.socket_path).await;
    let bob_back = login(&mut bob, "bob").await;
    assert_eq!(bob_back.location_name, "The Coffee Shop");
    assert_eq!(bob_back.operator_name, "Bob");
    drop(bob);
    drop(ada);

    running.shutdown().await;
    let running = boot(&data).await;
    let mut bob = connect(&running.socket_path).await;
    let restored = login(&mut bob, "bob").await;
    assert_eq!(restored.operator_name, "Bob");
    assert_eq!(restored.location_name, "The Coffee Shop");
    let pong = bob.call(ClientMessage::Ping { nonce: 9 }).await.expect("ping");
    assert!(matches!(pong, ServerMessage::Pong { nonce: 9 }));
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
            email: None,
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

fn feed_text(view: &GameView) -> String {
    view.feed.iter().map(|line| line.text.as_str()).collect::<Vec<_>>().join("\n")
}
