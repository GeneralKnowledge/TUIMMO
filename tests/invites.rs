//! Invite-only registration: seed codes, redeem, friend codes, seat cap.

use std::time::Duration;

use coffee_server::{create_invite_codes, start, ServerConfig};
use tempfile::tempdir;
use terminus_core::GameView;
use terminus_protocol::{ClientMessage, ProtocolClient, ServerMessage};

const PASSWORD: &str = "correct horse";

#[tokio::test]
async fn invite_only_register_mints_friend_codes_and_enforces_cap() {
    let dir = tempdir().unwrap();
    let data = dir.path();
    let codes = create_invite_codes(data, 1, "reddit").await.expect("seed");
    assert_eq!(codes.len(), 1);

    let mut config = ServerConfig::new(data);
    config.ssh_listen = "127.0.0.1:0".parse().unwrap();
    config.invite_only = true;
    config.max_accounts = 2;
    config.invites_per_player = 2;
    config.auth_rejection = Duration::from_millis(5);
    let running = start(config).await.expect("start");

    let mut denied = connect(&running.socket_path).await;
    hello(&mut denied).await;
    let missing = denied
        .call(ClientMessage::Register {
            username: "nobody".into(),
            password: PASSWORD.into(),
            operator_name: "Nobody".into(),
            invite_code: String::new(),
        })
        .await
        .expect("register call");
    assert!(
        matches!(
            &missing,
            ServerMessage::Error { code, message }
                if code == "invite" && message.contains("invite code is required")
        ),
        "{missing:?}"
    );

    let mut ada = connect(&running.socket_path).await;
    let ada_view = register(&mut ada, "ada", "Ada", &codes[0]).await;
    assert_eq!(ada_view.location_name, "The Coffee Shop");

    let invites = command(&mut ada, "invite list").await;
    let page = invites.overlay.clone().unwrap_or_default();
    assert!(page.contains("2 unused"), "{page}");
    let friend_codes: Vec<String> = page
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.len() == 9 && trimmed.as_bytes().get(4) == Some(&b'-') {
                Some(trimmed.to_string())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(friend_codes.len(), 2, "{page}");

    let mut bob = connect(&running.socket_path).await;
    let _ = register(&mut bob, "bob", "Bob", &friend_codes[0]).await;

    let mut full = connect(&running.socket_path).await;
    hello(&mut full).await;
    let capped = full
        .call(ClientMessage::Register {
            username: "cara".into(),
            password: PASSWORD.into(),
            operator_name: "Cara".into(),
            invite_code: friend_codes[1].clone(),
        })
        .await
        .expect("register call");
    assert!(matches!(&capped, ServerMessage::Error { code, .. } if code == "full"), "{capped:?}");

    let reused = {
        let mut again = connect(&running.socket_path).await;
        hello(&mut again).await;
        again
            .call(ClientMessage::Register {
                username: "doug".into(),
                password: PASSWORD.into(),
                operator_name: "Doug".into(),
                invite_code: codes[0].clone(),
            })
            .await
            .expect("register call")
    };
    // Cap is checked first when the server is full; either invite-used or full is acceptable
    // once two accounts exist. Prefer invite error if we raise the cap later.
    assert!(
        matches!(&reused, ServerMessage::Error { code, .. } if code == "full" || code == "invite"),
        "{reused:?}"
    );

    running.shutdown().await;
}

async fn connect(path: &std::path::Path) -> ProtocolClient {
    ProtocolClient::connect(path).await.expect("protocol connect")
}

async fn hello(client: &mut ProtocolClient) {
    let message =
        client.call(ClientMessage::Hello { client: "invite-test".into() }).await.expect("hello");
    assert!(matches!(message, ServerMessage::Hello { .. }));
}

async fn register(
    client: &mut ProtocolClient,
    username: &str,
    operator: &str,
    invite_code: &str,
) -> GameView {
    hello(client).await;
    match client
        .call(ClientMessage::Register {
            username: username.into(),
            password: PASSWORD.into(),
            operator_name: operator.into(),
            invite_code: invite_code.into(),
        })
        .await
        .expect("register")
    {
        ServerMessage::Authenticated { view, .. } => view,
        other => panic!("register failed: {other:?}"),
    }
}

async fn command(client: &mut ProtocolClient, line: &str) -> GameView {
    match client.call(ClientMessage::Command { line: line.into() }).await.expect("command") {
        ServerMessage::Update { view, .. } => view,
        other => panic!("command failed: {other:?}"),
    }
}
