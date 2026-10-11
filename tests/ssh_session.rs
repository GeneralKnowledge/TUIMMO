#![allow(clippy::unwrap_used)]
//! An SSH client must land in the game TUI, not a shell.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use coffee_server::{start, ServerConfig};
use russh::client::Handler;
use russh::keys::PublicKeyOrCertificate;
use russh::ChannelMsg;
use terminus_protocol::{ClientMessage, ProtocolClient, ServerMessage};
use tokio::time::timeout;

const PASSWORD: &str = "operator1";

struct AcceptKey;

impl Handler for AcceptKey {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ssh_opens_the_game_and_refuses_exec() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut config = ServerConfig::new(dir.path());
    config.ssh_listen = SocketAddr::from(([127, 0, 0, 1], 0));
    config.auth_rejection = Duration::from_millis(20);
    let running = start(config).await.expect("start");

    let mut setup = ProtocolClient::connect(&running.socket_path).await.expect("protocol");
    let hello =
        setup.call(ClientMessage::Hello { client: "ssh-setup".into() }).await.expect("hello");
    assert!(matches!(hello, ServerMessage::Hello { .. }));
    let created = setup
        .call(ClientMessage::Register {
            username: "ada".into(),
            password: PASSWORD.into(),
            operator_name: "Ada".into(),
        })
        .await
        .expect("register");
    assert!(matches!(created, ServerMessage::Authenticated { .. }));
    drop(setup);

    let mut session = ssh_connect(running.ssh_addr).await;
    let auth = session.authenticate_password("ada", PASSWORD).await.expect("auth");
    assert!(auth.success(), "password auth should succeed");

    let mut channel = session.channel_open_session().await.expect("session");
    channel.request_pty(false, "xterm", 80, 24, 0, 0, &[]).await.expect("pty");
    channel.request_shell(false).await.expect("shell");
    let text = read_until(&mut channel, "Coffee", Duration::from_secs(20)).await;
    assert!(text.contains("Coffee"), "expected the location in the TUI stream, got {text:?}");
    let _ = channel.close().await;
    let _ = session.disconnect(russh::Disconnect::ByApplication, "", "").await;

    let mut rejected = ssh_connect(running.ssh_addr).await;
    let bad =
        rejected.authenticate_password("ada", "not-the-password").await.expect("bad auth call");
    assert!(!bad.success(), "wrong password must be rejected");

    let mut session = ssh_connect(running.ssh_addr).await;
    let auth = session.authenticate_password("ada", PASSWORD).await.expect("auth");
    assert!(auth.success());
    let mut channel = session.channel_open_session().await.expect("session");
    channel.exec(true, b"echo SHOULD_NOT_RUN").await.expect("exec request");
    let message = timeout(Duration::from_secs(5), channel.wait())
        .await
        .expect("exec reply")
        .expect("channel open");
    match message {
        ChannelMsg::Failure => {}
        ChannelMsg::Data { data } => {
            let text = String::from_utf8_lossy(&data);
            panic!("exec produced output: {text}");
        }
        other => panic!("expected channel failure, got {other:?}"),
    }

    running.shutdown().await;
}

async fn ssh_connect(addr: SocketAddr) -> russh::client::Handle<AcceptKey> {
    let config = Arc::new(russh::client::Config {
        inactivity_timeout: Some(Duration::from_secs(30)),
        ..Default::default()
    });
    russh::client::connect(config, addr, AcceptKey).await.expect("ssh connect")
}

async fn read_until(
    channel: &mut russh::Channel<russh::client::Msg>,
    needle: &str,
    limit: Duration,
) -> String {
    let deadline = tokio::time::Instant::now() + limit;
    let mut collected = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match timeout(remaining, channel.wait()).await {
            Ok(Some(ChannelMsg::Data { data })) => {
                collected.extend_from_slice(&data);
                let text = String::from_utf8_lossy(&collected);
                if text.contains(needle) {
                    return text.into_owned();
                }
            }
            Ok(Some(ChannelMsg::Close)) | Ok(None) | Err(_) => break,
            Ok(Some(_)) => {}
        }
    }
    String::from_utf8_lossy(&collected).into_owned()
}
