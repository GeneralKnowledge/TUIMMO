//! Unix-socket client used by the TUI and by integration tests.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::codec::{decode_server, encode_frame, FrameDecoder};
use crate::message::{ClientFrame, ClientMessage, ServerFrame, ServerMessage, PROTOCOL_VERSION};
use crate::ProtocolError;

const CALL_TIMEOUT: Duration = Duration::from_secs(8);

struct Router {
    pending: HashMap<u64, oneshot::Sender<ServerMessage>>,
    pushes: mpsc::Sender<ServerMessage>,
}

/// Connected protocol client.
///
/// Request/response uses frame ids. Server pushes (`id == 0`) are queued
/// separately so a snapshot cannot be mistaken for a command reply.
pub struct ProtocolClient {
    write: mpsc::Sender<Vec<u8>>,
    router: Arc<Mutex<Router>>,
    pushes: Option<mpsc::Receiver<ServerMessage>>,
    next_id: u64,
    reader: tokio::task::JoinHandle<()>,
}

impl ProtocolClient {
    pub async fn connect(path: &Path) -> Result<Self, ProtocolError> {
        let stream = UnixStream::connect(path).await.map_err(|_| ProtocolError::Closed)?;
        let (mut read, mut write) = stream.into_split();
        let (write_tx, mut write_rx) = mpsc::channel::<Vec<u8>>(32);
        let writer = tokio::spawn(async move {
            while let Some(frame) = write_rx.recv().await {
                if write.write_all(&frame).await.is_err() {
                    break;
                }
                if write.flush().await.is_err() {
                    break;
                }
            }
        });
        let (push_tx, push_rx) = mpsc::channel(64);
        let router = Arc::new(Mutex::new(Router { pending: HashMap::new(), pushes: push_tx }));
        let router_reader = router.clone();
        let reader = tokio::spawn(async move {
            let mut dec = FrameDecoder::new();
            let mut buf = [0_u8; 4096];
            loop {
                let n = match read.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                let frames = match dec.push(&buf[..n]) {
                    Ok(frames) => frames,
                    Err(_) => break,
                };
                for body in frames {
                    let Ok(frame) = decode_server(&body) else {
                        continue;
                    };
                    deliver(frame, &router_reader).await;
                }
            }
            writer.abort();
            let mut guard = router_reader.lock().await;
            guard.pending.clear();
        });
        Ok(Self { write: write_tx, router, pushes: Some(push_rx), next_id: 1, reader })
    }

    pub async fn call(&mut self, body: ClientMessage) -> Result<ServerMessage, ProtocolError> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let (tx, rx) = oneshot::channel();
        {
            let mut guard = self.router.lock().await;
            guard.pending.insert(id, tx);
        }
        let frame = ClientFrame { v: PROTOCOL_VERSION, id, body };
        let bytes = encode_frame(&frame)?;
        self.write.send(bytes).await.map_err(|_| ProtocolError::Closed)?;
        match tokio::time::timeout(CALL_TIMEOUT, rx).await {
            Ok(Ok(msg)) => Ok(msg),
            Ok(Err(_)) => Err(ProtocolError::Closed),
            Err(_) => {
                let mut guard = self.router.lock().await;
                guard.pending.remove(&id);
                Err(ProtocolError::Timeout)
            }
        }
    }

    /// Detach the push queue so a UI loop can `select` on it without borrowing the client.
    pub fn take_pushes(&mut self) -> Result<mpsc::Receiver<ServerMessage>, ProtocolError> {
        self.pushes.take().ok_or(ProtocolError::Closed)
    }
}

impl Drop for ProtocolClient {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

async fn deliver(frame: ServerFrame, router: &Arc<Mutex<Router>>) {
    let mut guard = router.lock().await;
    if frame.id == 0 {
        // Keep the newest pushes. A full queue means the consumer is behind;
        // drop this push rather than blocking the reader.
        let _ = guard.pushes.try_send(frame.body);
        return;
    }
    if let Some(tx) = guard.pending.remove(&frame.id) {
        let _ = tx.send(frame.body);
    }
}
