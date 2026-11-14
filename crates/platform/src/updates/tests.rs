use super::*;
use std::sync::Arc;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

fn release(tag: &str, prerelease: bool, draft: bool) -> Release {
    Release {
        tag_name: tag.into(),
        prerelease,
        draft,
        body: Some("Changes".into()),
    }
}
fn chosen(releases: Vec<Release>, channel: Channel, current: &str) -> Option<Available> {
    select(
        releases,
        channel,
        &Version::parse(current).unwrap(),
        &Installation::default(),
    )
}
#[test]
fn stable_never_offers_drafts_or_preview_tags() {
    let result = chosen(
        vec![
            release("v3.0.0", true, false),
            release("v4.0.0", false, true),
            release("v5.0.0-beta.1", false, false),
            release("v1.2.0", false, false),
        ],
        Channel::Stable,
        "1.0.0",
    )
    .unwrap();
    assert_eq!(result.version, "1.2.0");
    assert!(!result.prerelease);
    assert_eq!(
        result.release_url,
        "https://github.com/zerw0/twayblade/releases/tag/v1.2.0"
    );
}
#[test]
fn opt_in_preview_selects_by_semver_and_can_advance_to_stable() {
    assert_eq!(
        chosen(
            vec![
                release("v1.9.0", false, false),
                release("v1.10.0-beta.2", true, false),
                release("v1.10.0-beta.10", true, false)
            ],
            Channel::Prerelease,
            "1.9.0"
        )
        .unwrap()
        .version,
        "1.10.0-beta.10"
    );
    assert_eq!(
        chosen(
            vec![
                release("v2.0.0-beta.1", true, false),
                release("v2.0.0", false, false)
            ],
            Channel::Prerelease,
            "2.0.0-beta.1"
        )
        .unwrap()
        .version,
        "2.0.0"
    );
}
#[test]
fn rejects_downgrades_build_only_changes_and_invalid_tags() {
    for tag in [
        "0.9.0",
        "1.0.0",
        "1.0.0+new",
        "../../arbitrary",
        "999999999999999999999999.0.0",
    ] {
        assert!(
            chosen(
                vec![release(tag, false, false)],
                Channel::Prerelease,
                "1.0.0+old"
            )
            .is_none()
        );
    }
}
#[test]
fn bounds_changelog_on_a_unicode_boundary_and_supplies_managed_command() {
    let mut data = release("v2.0.0", false, false);
    data.body = Some("я".repeat(MAX_CHANGELOG));
    let installation = Installation {
        manager: PackageManager::Homebrew,
        ..Default::default()
    };
    let result = select(
        vec![data],
        Channel::Stable,
        &Version::new(1, 0, 0),
        &installation,
    )
    .unwrap();
    assert_eq!(result.changelog.len(), MAX_CHANGELOG);
    assert!(result.changelog.ends_with('я'));
    assert_eq!(
        result.upgrade_command.as_deref(),
        Some("brew upgrade --cask twayblade")
    );
}
#[test]
fn old_preferences_default_to_enabled_stable_but_allow_an_off_switch() {
    assert_eq!(
        serde_json::from_str::<Preferences>("{}").unwrap(),
        Preferences::default()
    );
    assert_eq!(
        serde_json::from_str::<Preferences>(r#"{"enabled":false,"channel":"prerelease"}"#).unwrap(),
        Preferences {
            enabled: false,
            channel: Channel::Prerelease
        }
    );
}
async fn listener() -> (Arc<TcpListener>, String) {
    let listener = Arc::new(TcpListener::bind("127.0.0.1:0").await.unwrap());
    let base = format!("http://{}/releases", listener.local_addr().unwrap());
    (listener, base)
}
async fn request(socket: &mut tokio::net::TcpStream) -> String {
    let mut data = Vec::new();
    while !data.ends_with(b"\r\n\r\n") {
        assert!(data.len() < 8192);
        data.push(socket.read_u8().await.unwrap());
    }
    String::from_utf8(data).unwrap()
}
async fn reply(listener: Arc<TcpListener>, status: &str, extra: &str, body: &str) -> String {
    let (mut socket, _) = listener.accept().await.unwrap();
    let request = request(&mut socket).await;
    socket
        .write_all(
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    request
}
const BODY: &str = r#"{"tag_name":"v2.0.0","prerelease":false,"draft":false,"body":"What's new"}"#;
#[tokio::test]
async fn github_request_has_no_auth_cookies_account_or_platform_data() {
    let (listener, base) = listener().await;
    let server = tokio::spawn(reply(listener, "200 OK", "", BODY));
    let checker = Checker::new(&base, "1.0.0").unwrap();
    assert_eq!(
        checker
            .check(Channel::Stable, &Installation::default())
            .await
            .unwrap()
            .unwrap()
            .version,
        "2.0.0"
    );
    let request = server.await.unwrap().to_lowercase();
    assert!(request.starts_with("get /releases/latest http/1.1"));
    assert!(request.contains("x-github-api-version: 2026-03-10"));
    assert!(request.contains("user-agent: twayblade update check"));
    for forbidden in [
        "authorization:",
        "cookie:",
        "referer:",
        "account",
        "device_id",
        "homeserver",
    ] {
        assert!(!request.contains(forbidden));
    }
}
#[tokio::test]
async fn preview_endpoint_and_empty_release_repository_are_supported() {
    let (listener, base) = listener().await;
    let server = tokio::spawn(reply(listener, "200 OK", "", "[]"));
    let checker = Checker::new(&base, "1.0.0").unwrap();
    assert!(
        checker
            .check(Channel::Prerelease, &Installation::default())
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        server
            .await
            .unwrap()
            .starts_with("GET /releases?per_page=100 ")
    );
    let (listener, base) = self::listener().await;
    let server = tokio::spawn(reply(listener, "404 Not Found", "", ""));
    assert!(
        Checker::new(&base, "1.0.0")
            .unwrap()
            .check(Channel::Stable, &Installation::default())
            .await
            .unwrap()
            .is_none()
    );
    server.await.unwrap();
}
#[tokio::test]
async fn refuses_redirects_http_errors_bad_json_and_oversized_responses() {
    for (status, extra, body, error) in [
        (
            "302 Found",
            "Location: https://example.invalid/\r\n",
            "",
            UpdateError::Http(302),
        ),
        ("403 Forbidden", "", "", UpdateError::Http(403)),
        ("200 OK", "", "not json", UpdateError::InvalidResponse),
    ] {
        let (listener, base) = listener().await;
        let server = tokio::spawn(reply(listener, status, extra, body));
        assert_eq!(
            Checker::new(&base, "1.0.0")
                .unwrap()
                .check(Channel::Stable, &Installation::default())
                .await,
            Err(error)
        );
        server.await.unwrap();
    }
    let (listener, base) = listener().await;
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        request(&mut socket).await;
        socket
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                    MAX_RESPONSE + 1
                )
                .as_bytes(),
            )
            .await
            .unwrap();
    });
    assert_eq!(
        Checker::new(&base, "1.0.0")
            .unwrap()
            .check(Channel::Stable, &Installation::default())
            .await,
        Err(UpdateError::ResponseTooLarge)
    );
    server.await.unwrap();
}
#[tokio::test]
async fn disabled_checks_make_no_request_and_cancel_pending_results() {
    let (listener, base) = listener().await;
    let (config, receive) = watch::channel(Preferences {
        enabled: false,
        ..Default::default()
    });
    let (send, mut status) = watch::channel(Snapshot::default());
    let task = tokio::spawn(run_with(
        receive,
        move |snapshot| {
            send.send_replace(snapshot);
        },
        Checker::new(&base, "1.0.0"),
        DAILY,
    ));
    status.changed().await.unwrap();
    assert_eq!(status.borrow_and_update().phase, Phase::Disabled);
    assert!(
        tokio::time::timeout(Duration::from_millis(30), listener.accept())
            .await
            .is_err()
    );
    config.send_replace(Preferences::default());
    let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
        .await
        .unwrap()
        .unwrap();
    request(&mut socket).await;
    config.send_replace(Preferences {
        enabled: false,
        ..Default::default()
    });
    loop {
        status.changed().await.unwrap();
        if status.borrow_and_update().phase == Phase::Disabled {
            break;
        }
    }
    let _ = socket
        .write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{BODY}",
                BODY.len()
            )
            .as_bytes(),
        )
        .await;
    tokio::task::yield_now().await;
    assert!(status.borrow().available.is_none());
    drop(config);
    task.await.unwrap();
}
#[tokio::test]
async fn checks_at_launch_then_daily_and_same_config_does_not_repeat() {
    let (listener, base) = listener().await;
    let server = tokio::spawn(reply(listener.clone(), "200 OK", "", BODY));
    let (config, receive) = watch::channel(Preferences::default());
    let (send, mut status) = watch::channel(Snapshot::default());
    let task = tokio::spawn(run_with(
        receive,
        move |snapshot| {
            send.send_replace(snapshot);
        },
        Checker::new(&base, "1.0.0"),
        DAILY,
    ));
    loop {
        status.changed().await.unwrap();
        if status.borrow_and_update().phase == Phase::Available {
            break;
        }
    }
    server.await.unwrap();
    tokio::time::pause();
    config.send_replace(Preferences::default());
    tokio::task::yield_now().await;
    tokio::time::advance(DAILY - Duration::from_secs(1)).await;
    assert_eq!(status.borrow().phase, Phase::Available);
    tokio::time::advance(Duration::from_secs(1)).await;
    status.changed().await.unwrap();
    assert_eq!(status.borrow_and_update().phase, Phase::Checking);
    drop(config);
    task.await.unwrap();
}
