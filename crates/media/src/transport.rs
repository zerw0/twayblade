//! Account-scoped trust on the SDK HTTP client, also for LiveKit WebSockets.
//! No keychain changes, TLS bypasses, global extra roots or credential logging.
use futures_util::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use livekit_net::{Header, TransportError, WsClient, WsConnectResult, WsConnection};
use std::sync::{
    Arc, Mutex, Once, OnceLock, Weak,
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
    initial_token: String,
    room: Weak<livekit::Room>,
    http: reqwest::Client,
}
impl Binding {
    fn matches(&self, origin: &str, token: &str) -> bool {
        self.origin == origin
            && (self.initial_token == token
                || self
                    .room
                    .upgrade()
                    .is_some_and(|room| room.token() == token))
    }
}
fn bindings() -> &'static Mutex<Vec<Binding>> {
    static BINDINGS: OnceLock<Mutex<Vec<Binding>>> = OnceLock::new();
    BINDINGS.get_or_init(Mutex::default)
}
pub(crate) struct TrustBinding(u64);
impl TrustBinding {
    pub fn attach(&self, room: &Arc<livekit::Room>) {
        if let Ok(mut list) = bindings().lock()
            && let Some(binding) = list.iter_mut().find(|b| b.id == self.0)
        {
            binding.room = Arc::downgrade(room);
        }
    }
}
impl Drop for TrustBinding {
    fn drop(&mut self) {
        if let Ok(mut list) = bindings().lock() {
            list.retain(|b| b.id != self.0);
        }
    }
}
pub(crate) fn bind(
    url: &str,
    token: &str,
    http: reqwest::Client,
) -> Result<TrustBinding, TransportError> {
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
    list.push(Binding {
        id,
        origin,
        initial_token: token.to_owned(),
        room: Weak::new(),
        http,
    });
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
        // The SDK registry is process-wide. Origin alone would mix two
        // accounts' custom roots when they call the same SFU. Match the exact
        // SDK bearer token, including its authenticated server refresh, too.
        let token = headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case("authorization"))
            .and_then(|h| h.value.strip_prefix("Bearer "))
            .filter(|token| !token.is_empty())
            .ok_or_else(error)?;
        let http = bindings()
            .lock()
            .map_err(|_| error())?
            .iter()
            .rev()
            .find(|b| b.matches(&origin, token))
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn same_sfu_accounts_cannot_borrow_each_others_trust_binding() {
        let binding = Binding {
            id: 1,
            origin: "wss://sfu.test".into(),
            initial_token: "fixture-account-a".into(),
            room: Weak::new(),
            http: reqwest::Client::new(),
        };
        assert!(binding.matches("wss://sfu.test", "fixture-account-a"));
        assert!(!binding.matches("wss://sfu.test", "fixture-account-b"));
        assert!(!binding.matches("wss://other.test", "fixture-account-a"));
        assert!(!binding.matches("wss://sfu.test", ""));
    }
}
