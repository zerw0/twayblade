//! One-shot native browser callback. Request targets and credentials are never logged.
use super::AuthError;
use std::{net::Ipv4Addr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::watch,
};
use url::Url;
use zeroize::Zeroizing;

#[derive(Clone, Copy)]
pub(super) enum CallbackKind {
    OAuth,
    Sso,
}

pub(super) struct Loopback {
    listener: TcpListener,
    pub redirect: Url,
    pub kind: CallbackKind,
}

impl Loopback {
    pub async fn bind() -> Result<Self, AuthError> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|_| AuthError::Callback)?;
        let port = listener
            .local_addr()
            .map_err(|_| AuthError::Callback)?
            .port();
        let redirect =
            Url::parse(&format!("http://127.0.0.1:{port}/")).map_err(|_| AuthError::Callback)?;
        Ok(Self {
            listener,
            redirect,
            kind: CallbackKind::OAuth,
        })
    }

    pub async fn receive(
        self,
        state: &str,
        cancel: watch::Receiver<bool>,
    ) -> Result<Url, AuthError> {
        self.receive_with_timeout(state, cancel, Duration::from_secs(300))
            .await
    }

    async fn receive_with_timeout(
        self,
        state: &str,
        mut cancel: watch::Receiver<bool>,
        duration: Duration,
    ) -> Result<Url, AuthError> {
        // Browsers open speculative preconnect sockets and fetch /favicon.ico, so
        // keep listening until one request is a valid callback. Each connection
        // is handled separately so an idle socket cannot block the real one.
        let deadline = tokio::time::sleep(duration);
        tokio::pin!(deadline);
        let mut pending = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                biased;
                _ = cancelled(&mut cancel) => return Err(AuthError::Cancelled),
                _ = &mut deadline => return Err(AuthError::Timeout),
                Some(done) = pending.join_next(), if !pending.is_empty() => {
                    if let Ok(Ok(url)) = done {
                        return Ok(url);
                    }
                }
                accepted = self.listener.accept() => {
                    let Ok((socket, peer)) = accepted else { continue };
                    if !peer.ip().is_loopback() || pending.len() >= MAX_PENDING {
                        continue;
                    }
                    let redirect = self.redirect.clone();
                    let state = state.to_owned();
                    let kind = self.kind;
                    pending.spawn(async move {
                        tokio::time::timeout(CONNECTION_TIMEOUT, handle(socket, &redirect, &state, kind))
                            .await
                            .unwrap_or(Err(AuthError::Callback))
                    });
                }
            }
        }
    }
}

const CONNECTION_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_PENDING: usize = 16;

/// Completes when the UI cancels, or when the UI side has gone away.
async fn cancelled(cancel: &mut watch::Receiver<bool>) {
    while !*cancel.borrow_and_update() {
        if cancel.changed().await.is_err() {
            return;
        }
    }
}

async fn handle(
    mut socket: tokio::net::TcpStream,
    redirect: &Url,
    state: &str,
    kind: CallbackKind,
) -> Result<Url, AuthError> {
    let mut request = Zeroizing::new(Vec::new());
    loop {
        let mut chunk = [0; 1024];
        let count = socket
            .read(&mut chunk)
            .await
            .map_err(|_| AuthError::Callback)?;
        if count == 0 || request.len() + count > 8192 {
            return Err(AuthError::Callback);
        }
        request.extend_from_slice(&chunk[..count]);
        if request.windows(4).any(|part| part == b"\r\n\r\n") {
            break;
        }
    }
    let result = parse_request_kind(&request, redirect, state, kind);
    let response: &[u8] = if result.is_ok() {
        b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\n\r\nReturn to Matrix Client to finish signing in."
    } else {
        b"HTTP/1.1 400 Bad Request\r\nCache-Control: no-store\r\nConnection: close\r\n\r\nSign-in callback rejected. Return to Matrix Client."
    };
    // A browser disconnect after delivery must not discard a valid callback.
    let _ = socket.write_all(response).await;
    result
}

#[cfg(test)]
fn parse_request(request: &[u8], redirect: &Url, state: &str) -> Result<Url, AuthError> {
    parse_request_kind(request, redirect, state, CallbackKind::OAuth)
}

fn parse_request_kind(
    request: &[u8],
    redirect: &Url,
    state: &str,
    kind: CallbackKind,
) -> Result<Url, AuthError> {
    let request = std::str::from_utf8(request).map_err(|_| AuthError::Callback)?;
    let mut lines = request.split("\r\n");
    let mut first = lines.next().ok_or(AuthError::Callback)?.split_whitespace();
    if first.next() != Some("GET") {
        return Err(AuthError::Callback);
    }
    let target = first.next().ok_or(AuthError::Callback)?;
    if !target.starts_with("/?") || first.next() != Some("HTTP/1.1") || first.next().is_some() {
        return Err(AuthError::Callback);
    }
    let expected_host = format!("127.0.0.1:{}", redirect.port().ok_or(AuthError::Callback)?);
    let hosts: Vec<_> = lines
        .filter_map(|line| line.split_once(':'))
        .filter(|(key, _)| key.eq_ignore_ascii_case("host"))
        .collect();
    if hosts.len() != 1 || hosts[0].1.trim() != expected_host {
        return Err(AuthError::Callback);
    }
    let url = redirect.join(target).map_err(|_| AuthError::Callback)?;
    let states: Vec<_> = url
        .query_pairs()
        .filter(|(key, _)| key == "state")
        .collect();
    if states.len() != 1 || states[0].1 != state || url.fragment().is_some() {
        return Err(AuthError::Callback);
    }
    let credential = match kind {
        CallbackKind::OAuth => "code",
        CallbackKind::Sso => "loginToken",
    };
    let codes = url
        .query_pairs()
        .filter(|(key, value)| key == credential && !value.is_empty())
        .count();
    if url
        .query_pairs()
        .filter(|(key, _)| key == credential)
        .count()
        != codes
    {
        return Err(AuthError::Callback);
    }
    let errors = url.query_pairs().filter(|(key, _)| key == "error").count();
    if !matches!((codes, errors), (1, 0) | (0, 1)) {
        return Err(AuthError::Callback);
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_wrong_state_duplicates_foreign_host_and_malformed_requests() {
        let base = Url::parse("http://127.0.0.1:54321/").unwrap();
        for target in [
            "/?code=one&state=wrong",
            "/?code=one&state=expected&state=expected",
            "/?state=expected",
            "/?state=expected&code=a&code=b",
            "/?state=expected&code=a&error=denied",
            "/other?state=expected&code=a",
            "/?state=expected&code=a#fragment",
        ] {
            let request = format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:54321\r\n\r\n");
            assert!(parse_request(request.as_bytes(), &base, "expected").is_err());
        }
        for host in ["evil.invalid", "localhost:54321", "127.0.0.1:1234"] {
            let request = format!("GET /?code=a&state=expected HTTP/1.1\r\nHost: {host}\r\n\r\n");
            assert!(parse_request(request.as_bytes(), &base, "expected").is_err());
        }
        assert!(
            parse_request(
                b"GET /?code=a&state=expected HTTP/1.1\r\nHost: 127.0.0.1:54321\r\n\r\n",
                &base,
                "expected"
            )
            .is_ok()
        );
    }

    #[test]
    fn legacy_sso_callback_requires_matching_state_and_exactly_one_token() {
        let base = Url::parse("http://127.0.0.1:54321/").unwrap();
        for query in [
            "loginToken=sample&state=wrong",
            "loginToken=&state=expected",
            "loginToken=a&loginToken=b&state=expected",
            "loginToken=a",
            "code=a&state=expected",
        ] {
            let request = format!("GET /?{query} HTTP/1.1\r\nHost: 127.0.0.1:54321\r\n\r\n");
            assert!(
                parse_request_kind(request.as_bytes(), &base, "expected", CallbackKind::Sso)
                    .is_err()
            );
        }
        assert!(
            parse_request_kind(
                b"GET /?loginToken=sample&state=expected HTTP/1.1\r\nHost: 127.0.0.1:54321\r\n\r\n",
                &base,
                "expected",
                CallbackKind::Sso
            )
            .is_ok()
        );
    }

    #[tokio::test]
    async fn cancellation_closes_the_listener() {
        let listener = Loopback::bind().await.unwrap();
        let address = listener.listener.local_addr().unwrap();
        let (cancel, receiver) = watch::channel(false);
        let waiting = tokio::spawn(listener.receive("expected", receiver));
        cancel.send_replace(true);
        assert!(matches!(waiting.await.unwrap(), Err(AuthError::Cancelled)));
        assert!(tokio::net::TcpStream::connect(address).await.is_err());
    }

    #[tokio::test]
    async fn timeout_closes_the_listener_even_with_an_incomplete_request() {
        let listener = Loopback::bind().await.unwrap();
        let address = listener.listener.local_addr().unwrap();
        let (_cancel, receiver) = watch::channel(false);
        let waiting = tokio::spawn(listener.receive_with_timeout(
            "expected",
            receiver,
            Duration::from_millis(20),
        ));
        let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
        socket.write_all(b"GET /?code=sample").await.unwrap();
        assert!(matches!(waiting.await.unwrap(), Err(AuthError::Timeout)));
        assert!(tokio::net::TcpStream::connect(address).await.is_err());
    }

    #[tokio::test]
    async fn preconnect_and_stray_requests_do_not_abort_sign_in() {
        let listener = Loopback::bind().await.unwrap();
        let address = listener.listener.local_addr().unwrap();
        let (_cancel, receiver) = watch::channel(false);
        let waiting = tokio::spawn(listener.receive("expected", receiver));
        // An idle preconnect socket and a favicon request arrive first.
        let _idle = tokio::net::TcpStream::connect(address).await.unwrap();
        let mut favicon = tokio::net::TcpStream::connect(address).await.unwrap();
        favicon
            .write_all(format!("GET /favicon.ico HTTP/1.1\r\nHost: {address}\r\n\r\n").as_bytes())
            .await
            .unwrap();
        let mut forged = tokio::net::TcpStream::connect(address).await.unwrap();
        forged
            .write_all(
                format!("GET /?code=x&state=wrong HTTP/1.1\r\nHost: {address}\r\n\r\n").as_bytes(),
            )
            .await
            .unwrap();
        let mut callback = tokio::net::TcpStream::connect(address).await.unwrap();
        callback
            .write_all(
                format!("GET /?code=sample&state=expected HTTP/1.1\r\nHost: {address}\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .unwrap();
        let url = waiting
            .await
            .unwrap()
            .expect("valid callback after stray requests");
        assert!(
            url.query_pairs()
                .any(|(key, value)| key == "code" && value == "sample")
        );
    }

    #[tokio::test]
    async fn callback_is_consumed_exactly_once() {
        let listener = Loopback::bind().await.unwrap();
        let address = listener.listener.local_addr().unwrap();
        let (_cancel, receiver) = watch::channel(false);
        let waiting = tokio::spawn(listener.receive("expected", receiver));
        let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
        socket
            .write_all(
                format!("GET /?code=sample&state=expected HTTP/1.1\r\nHost: {address}\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .unwrap();
        assert!(waiting.await.unwrap().is_ok());
        assert!(tokio::net::TcpStream::connect(address).await.is_err());
    }
}
