#![allow(clippy::unwrap_used)]
//! Guest identity, recovery, and permanent upgrade through the hub.

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use coffee_server::{start, ServerConfig};
use terminus_protocol::{ClientMessage, ProtocolClient, RevealedOnce, ServerMessage};

const PASSWORD: &str = "permanent-pass";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn guest_recovers_upgrades_and_survives_restart() {
    let dir = tempfile::tempdir().expect("tempdir");
    let data = dir.path().to_path_buf();
    let running = boot(&data).await;

    let mut client = connect(&running.socket_path).await;
    hello(&mut client).await;
    drop(client);

    let mut client = connect(&running.socket_path).await;
    hello(&mut client).await;
    let created = client
        .call(ClientMessage::Guest { username: "ada".into(), operator_name: "Ada".into() })
        .await
        .expect("guest");
    let (code, view) = expect_auth(created);
    assert_eq!(view.location_name, "The Coffee Shop");
    assert!(view.inventory.iter().any(|item| item.keyword == "chip"));
    let moved = command(&mut client, "go alley").await;
    assert_eq!(moved.location_name, "Neon Alley");
    drop(client);

    let mut returned = connect(&running.socket_path).await;
    hello(&mut returned).await;
    let recovered = returned
        .call(ClientMessage::Recover { username: "ada".into(), recovery_code: code.clone() })
        .await
        .expect("recover");
    let (rotated, view) = expect_auth(recovered);
    assert_ne!(rotated, code);
    assert_eq!(view.location_name, "Neon Alley");
    assert_eq!(view.operator_name, "Ada");

    let mut thief = connect(&running.socket_path).await;
    hello(&mut thief).await;
    let stale = thief
        .call(ClientMessage::Recover { username: "ada".into(), recovery_code: code })
        .await
        .expect("stale recover");
    assert!(
        matches!(stale, ServerMessage::Error { ref message, .. } if message.contains("recovery")),
        "{stale:?}"
    );

    let upgraded = returned
        .call(ClientMessage::Upgrade { password: "short".into(), email: None })
        .await
        .expect("weak upgrade");
    assert!(matches!(upgraded, ServerMessage::Error { .. }), "{upgraded:?}");
    let upgraded = returned
        .call(ClientMessage::Upgrade { password: PASSWORD.into(), email: None })
        .await
        .expect("upgrade");
    match upgraded {
        ServerMessage::AccountStatus { kind, has_email, .. } => {
            assert_eq!(kind, "permanent");
            assert!(!has_email);
        }
        other => panic!("upgrade failed: {other:?}"),
    }
    drop(returned);

    running.shutdown().await;
    let running = boot(&data).await;
    let mut client = connect(&running.socket_path).await;
    hello(&mut client).await;
    let logged = client
        .call(ClientMessage::Login { username: "ada".into(), password: PASSWORD.into() })
        .await
        .expect("login");
    let view = match logged {
        ServerMessage::Authenticated {
            view,
            recovery_code: None,
            account_kind: Some(kind),
            ..
        } => {
            assert_eq!(kind, "permanent");
            view
        }
        other => panic!("login failed: {other:?}"),
    };
    assert_eq!(view.location_name, "Neon Alley");
    assert!(view.inventory.iter().any(|item| item.keyword == "chip"));

    let revoked = client.call(ClientMessage::RevokeRecovery).await.expect("revoke");
    match revoked {
        ServerMessage::AccountStatus { has_recovery: false, .. } => {}
        other => panic!("revoke failed: {other:?}"),
    }
    drop(client);
    let mut client = connect(&running.socket_path).await;
    hello(&mut client).await;
    let denied = client
        .call(ClientMessage::Recover { username: "ada".into(), recovery_code: rotated })
        .await
        .expect("revoked recover");
    assert!(matches!(denied, ServerMessage::Error { .. }), "{denied:?}");

    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recovery_is_rate_limited_and_does_not_reveal_missing_accounts() {
    let dir = tempfile::tempdir().expect("tempdir");
    let running = boot(dir.path()).await;
    let mut client = connect(&running.socket_path).await;
    hello(&mut client).await;
    let mut saw_limit = false;
    for _ in 0..6 {
        let reply = client
            .call(ClientMessage::Recover {
                username: "nobody".into(),
                recovery_code: "ABCD-EFGH-JKLM-NPQR-STUV-WXYZ".into(),
            })
            .await
            .expect("recover");
        match reply {
            ServerMessage::Error { message, .. } if message.contains("too many") => {
                saw_limit = true;
            }
            ServerMessage::Error { message, .. } => {
                assert!(message.contains("recovery") && !message.contains("no such"), "{message}");
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(saw_limit, "recovery attempts should hit the rate limit");
    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn duplicate_guest_handles_are_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let running = boot(dir.path()).await;
    let mut first = connect(&running.socket_path).await;
    let mut second = connect(&running.socket_path).await;
    hello(&mut first).await;
    hello(&mut second).await;
    let (left, right) = tokio::join!(
        first.call(ClientMessage::Guest { username: "ada".into(), operator_name: "Ada".into() }),
        second.call(ClientMessage::Guest { username: "ada".into(), operator_name: "Ada".into() }),
    );
    let left = left.expect("left");
    let right = right.expect("right");
    let oks = matches!(left, ServerMessage::Authenticated { .. }) as u8
        + matches!(right, ServerMessage::Authenticated { .. }) as u8;
    assert_eq!(oks, 1, "left={left:?} right={right:?}");
    running.shutdown().await;
}

fn expect_auth(message: ServerMessage) -> (String, terminus_core::GameView) {
    match message {
        ServerMessage::Authenticated {
            view,
            recovery_code: Some(RevealedOnce(code)),
            account_kind: Some(kind),
            ..
        } => {
            assert_eq!(kind, "guest");
            assert!(!code.is_empty());
            (code, view)
        }
        other => panic!("expected a guest session with a recovery code, got {other:?}"),
    }
}

async fn boot(data: &Path) -> coffee_server::Running {
    let mut config = ServerConfig::new(data);
    config.ssh_listen = SocketAddr::from(([127, 0, 0, 1], 0));
    config.auth_rejection = Duration::from_millis(20);
    start(config).await.expect("start")
}

async fn connect(path: &Path) -> ProtocolClient {
    ProtocolClient::connect(path).await.expect("connect")
}

async fn hello(client: &mut ProtocolClient) {
    let hello =
        client.call(ClientMessage::Hello { client: "accounts".into() }).await.expect("hello");
    assert!(matches!(hello, ServerMessage::Hello { .. }));
}

async fn command(client: &mut ProtocolClient, line: &str) -> terminus_core::GameView {
    match client.call(ClientMessage::Command { line: line.into() }).await.expect("command") {
        ServerMessage::Update { view, .. } => view,
        other => panic!("command failed: {other:?}"),
    }
}
