//! Account-scoped trust on the SDK HTTP client, also for LiveKit WebSockets.
//! No keychain changes, TLS bypasses, global extra roots or credential logging.
use futures_util::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use livekit_net::{Header, TransportError, WsClient, WsConnectResult, WsConnection};
use std::sync::{
    Arc, Mutex, Once, OnceLock,
    atomic::{AtomicU64, Ordering},
};
use tokio::sync::Mutex as AsyncMutex;
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{
        Message,
        handshake::{client::generate_key, derive_accept_key},
        protocol::Role,
    },
};

struct Binding {
    id: u64,
    origin: String,
    http: reqwest::Client,
}
fn bindings() -> &'static Mutex<Vec<Binding>> {
    static BINDINGS: OnceLock<Mutex<Vec<Binding>>> = OnceLock::new();
    BINDINGS.get_or_init(Mutex::default)
}
pub(crate) struct TrustBinding(u64);
impl Drop for TrustBinding {
    fn drop(&mut self) {
        if let Ok(mut list) = bindings().lock() {
            list.retain(|b| b.id != self.0);
        }
    }
}
pub(crate) fn bind(url: &str, http: reqwest::Client) -> Result<TrustBinding, TransportError> {
    static INIT: Once = Once::new();
    static NEXT: AtomicU64 = AtomicU64::new(1);
    INIT.call_once(|| {
        livekit_net::set_ws_client(Arc::new(AccountTransport));
    });
    let origin = url::Url::parse(url)
        .map_err(|_| error())?
        .origin()
        .ascii_serialization();
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let mut list = bindings().lock().map_err(|_| error())?;
    if list.len() >= 8 {
        return Err(error());
    }
    list.push(Binding { id, origin, http });
    Ok(TrustBinding(id))
}
fn error() -> TransportError {
    TransportError::Connection("call transport failed".into())
}
struct AccountTransport;
#[async_trait::async_trait]
impl WsClient for AccountTransport {
    async fn connect(
        &self,
        url: String,
        headers: Vec<Header>,
        timeout_ms: u64,
    ) -> Result<WsConnectResult, TransportError> {
        let mut parsed = url::Url::parse(&url).map_err(|_| error())?;
        let origin = parsed.origin().ascii_serialization();
        let http = bindings()
            .lock()
            .map_err(|_| error())?
            .iter()
            .rev()
            .find(|b| b.origin == origin)
            .map(|b| b.http.clone())
            .ok_or_else(error)?;
        if parsed.scheme() != "wss" {
            return Err(error());
        }
        parsed.set_scheme("https").map_err(|_| error())?;
        let key = generate_key();
        let mut request = http
            .get(parsed.as_str())
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header("Sec-WebSocket-Key", &key);
        for header in headers {
            request = request.header(header.name, header.value);
        }
        let response = request.send().await.map_err(|_| error())?;
        if response.status() != 101
            || response
                .headers()
                .get("Sec-WebSocket-Accept")
                .and_then(|h| h.to_str().ok())
                != Some(derive_accept_key(key.as_bytes()).as_str())
        {
            return Err(error());
        }
        let upgraded = response.upgrade().await.map_err(|_| error())?;
        let stream = WebSocketStream::from_raw_socket(upgraded, Role::Client, None).await;
        let (send, receive) = stream.split();
        Ok(WsConnectResult {
            connection: Arc::new(Connection {
                send: AsyncMutex::new(send),
                receive: AsyncMutex::new(receive),
            }),
        })
    }
}
struct Connection {
    send: AsyncMutex<SplitSink<WebSocketStream<reqwest::Upgraded>, Message>>,
    receive: AsyncMutex<SplitStream<WebSocketStream<reqwest::Upgraded>>>,
}
#[async_trait::async_trait]
impl WsConnection for Connection {
    async fn send(&self, frame: Vec<u8>) -> Result<(), TransportError> {
        self.send
            .lock()
            .await
            .send(Message::Binary(frame.into()))
            .await
            .map_err(|_| error())
    }
    async fn recv(&self) -> Result<Option<Vec<u8>>, TransportError> {
        loop {
            let message = self.receive.lock().await.next().await;
            match message {
                Some(Ok(Message::Binary(data))) => return Ok(Some(data.to_vec())),
                Some(Ok(Message::Ping(data))) => {
                    self.send
                        .lock()
                        .await
                        .send(Message::Pong(data))
                        .await
                        .map_err(|_| error())?;
                }
                Some(Ok(Message::Close(_))) | None => return Ok(None),
                Some(Err(_)) => return Err(error()),
                _ => {}
            }
        }
    }
    async fn close(&self) {
        let _ = self.send.lock().await.close().await;
    }
}
