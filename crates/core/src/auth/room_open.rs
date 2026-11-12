//! Opt-in TLS Synapse fault cases. All identities/stores are generated fixture data.
use super::*;
use futures_util::StreamExt;
use matrix_sdk::{reqwest::Certificate, ruma::api::client::room::create_room};
use matrix_sdk_ui::timeline::RoomExt;
use std::{path::Path, process::Stdio, time::Duration};

fn checked<T, E>(value: Result<T, E>, stage: &'static str) -> T {
    value.unwrap_or_else(|_| panic!("Isolated room-open stage failed: {stage}"))
}
struct Proxy {
    _process: tokio::process::Child,
    work: PathBuf,
    url: String,
}
impl Proxy {
    async fn start(fixture: &Path, work: PathBuf) -> Self {
        checked(tokio::fs::create_dir_all(&work).await, "proxy directory");
        let process = checked(
            tokio::process::Command::new("python3")
                .arg(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../tests/integration/room-open-proxy.py"),
                )
                .arg(fixture)
                .arg(&work)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn(),
            "start proxy",
        );
        let port = checked(
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    if let Ok(port) = tokio::fs::read_to_string(work.join("port")).await {
                        break checked(port.parse::<u16>(), "proxy port");
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            })
            .await,
            "proxy ready",
        );
        Self {
            _process: process,
            work,
            url: format!("https://127.0.0.1:{port}"),
        }
    }
    async fn fault(&self, mode: &str, route: &str) {
        let temporary = self.work.join("control.tmp");
        checked(
            tokio::fs::write(
                &temporary,
                serde_json::json!({"mode": mode, "route":route}).to_string(),
            )
            .await,
            "fault plan",
        );
        checked(
            tokio::fs::rename(temporary, self.work.join("control.json")).await,
            "atomic fault plan",
        );
    }
    async fn hits(&self, key: &str) -> u64 {
        let bytes = tokio::fs::read(self.work.join("stats.json"))
            .await
            .unwrap_or_default();
        serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .and_then(|s| s[key].as_u64())
            .unwrap_or(0)
    }
}

async fn fixture() -> (PathBuf, serde_json::Value, Certificate) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/local/integration");
    let data: serde_json::Value = checked(
        serde_json::from_slice(&checked(
            tokio::fs::read(root.join("fixture.json")).await,
            "fixture read",
        )),
        "fixture parse",
    );
    assert!(
        data["homeserver"] == "https://127.0.0.1:18448",
        "Only the isolated loopback fixture is permitted"
    );
    let certificate = checked(
        Certificate::from_pem(&checked(
            tokio::fs::read(root.join("ca.crt")).await,
            "fixture CA",
        )),
        "CA parse",
    );
    (root, data, certificate)
}
async fn client(
    proxy: &Proxy,
    fixture: &serde_json::Value,
    cert: &Certificate,
    path: &Path,
    fixed: bool,
) -> Client {
    let mut builder = Client::builder()
        .respect_login_well_known(false)
        .homeserver_url(&proxy.url)
        .add_root_certificates(vec![cert.clone()])
        .with_threading_support(matrix_sdk::ThreadingSupport::Enabled {
            with_subscriptions: false,
        })
        .sqlite_store(path, Some("isolated-room-open-store-only"));
    if fixed {
        builder = builder.request_config(crate::request_policy::interactive());
    }
    let client = checked(builder.build().await, "client build");
    checked(
        client
            .matrix_auth()
            .login_username(
                checked(
                    fixture["security_username"].as_str().ok_or(()),
                    "fixture username",
                ),
                checked(
                    fixture["security_password"].as_str().ok_or(()),
                    "fixture password",
                ),
            )
            .await,
        "generated account login",
    );
    client
}
fn account(client: Client, path: PathBuf) -> Account {
    Account::new(
        client,
        path,
        secrets::SessionStore::new("twayblade.integration.unused"),
        Arc::new(Mutex::new(())),
    )
}

#[tokio::test]
#[ignore = "requires isolated TLS Synapse; run.sh room-open"]
async fn room_open_transient_response_does_not_hide_inside_sdk_retry() {
    let (root, data, cert) = fixture().await;
    let work = root.join(format!("room-open-{}", Uuid::new_v4()));
    let proxy = Proxy::start(&root, work.join("proxy")).await;
    // Control: the exact SDK default keeps build pending on a transient 503.
    let baseline = client(&proxy, &data, &cert, &work.join("baseline"), false).await;
    let room = checked(
        baseline.create_room(create_room::v3::Request::new()).await,
        "baseline room",
    );
    assert!(room.encryption_state().is_unknown());
    proxy.fault("transient", "encryption").await;
    assert!(
        tokio::time::timeout(Duration::from_secs(3), room.timeline_builder().build())
            .await
            .is_err(),
        "SDK baseline unexpectedly completed"
    );
    assert!(proxy.hits("faults").await > 0);
    proxy.fault("normal", "encryption").await;
    checked(baseline.matrix_auth().logout().await, "baseline logout");

    let client = client(&proxy, &data, &cert, &work.join("fixed"), true).await;
    let room = checked(
        client.create_room(create_room::v3::Request::new()).await,
        "fixed room",
    );
    let id = room.room_id().to_string();
    let account = account(client, work.join("fixed"));
    proxy.fault("transient", "encryption").await;
    let mut updates = Box::pin(account.conversation_updates(id, 1));
    let first = checked(
        tokio::time::timeout(Duration::from_secs(3), updates.next()).await,
        "prompt retry status",
    )
    .expect("snapshot");
    assert!(first.status == crate::conversation::TimelineStatus::Retrying);
    // Unknown must never be silently interpreted as unencrypted on an error.
    assert!(first.conversation.is_none() && room.encryption_state().is_unknown());
    proxy.fault("normal", "encryption").await;
    let ready = checked(
        tokio::time::timeout(Duration::from_secs(5), updates.next()).await,
        "automatic recovery",
    )
    .expect("ready snapshot");
    assert!(ready.conversation.is_some());
    assert!(!room.encryption_state().is_unknown());
    drop(updates);
    let limited = checked(
        account
            .client
            .create_room(create_room::v3::Request::new())
            .await,
        "limited room",
    );
    proxy.fault("limited", "encryption").await;
    let mut updates = Box::pin(account.conversation_updates(limited.room_id().to_string(), 2));
    let snapshot = checked(
        tokio::time::timeout(Duration::from_secs(2), updates.next()).await,
        "rate limit status",
    )
    .expect("retry snapshot");
    assert!(snapshot.status == crate::conversation::TimelineStatus::Retrying);
    let hits = proxy.hits("faults").await;
    assert!(
        tokio::time::timeout(Duration::from_millis(300), updates.next())
            .await
            .is_err()
    );
    assert_eq!(
        proxy.hits("faults").await,
        hits,
        "server delay must not be shortened"
    );
    proxy.fault("normal", "encryption").await;
    let snapshot = checked(
        tokio::time::timeout(Duration::from_secs(5), updates.next()).await,
        "rate limit recovery",
    )
    .expect("ready snapshot");
    assert!(snapshot.conversation.is_some());
    drop(updates);
    checked(account.client.matrix_auth().logout().await, "fixed logout");
}

async fn ready(account: &Account, room: String, generation: u64, seconds: u64) {
    let mut stream = Box::pin(account.conversation_updates(room, generation));
    checked(
        tokio::time::timeout(Duration::from_secs(seconds), async {
            loop {
                let snapshot = stream.next().await.expect("room snapshot");
                if snapshot.conversation.is_some() {
                    return;
                }
            }
        })
        .await,
        "room becomes ready",
    );
}

#[tokio::test]
#[ignore = "requires isolated TLS Synapse; run.sh room-open"]
async fn room_open_stalled_request_drop_and_selection_cancellation_recover() {
    let (root, data, cert) = fixture().await;
    let work = root.join(format!("room-open-network-{}", Uuid::new_v4()));
    let proxy = Proxy::start(&root, work.join("proxy")).await;
    let client = client(&proxy, &data, &cert, &work.join("store"), true).await;
    let room = checked(
        client.create_room(create_room::v3::Request::new()).await,
        "stalled room",
    );
    let id = room.room_id().to_string();
    let account = account(client, work.join("store"));
    proxy.fault("stall", "encryption").await;
    let mut updates = Box::pin(account.conversation_updates(id.clone(), 1));
    let snapshot = checked(
        tokio::time::timeout(Duration::from_secs(12), updates.next()).await,
        "stalled response deadline",
    )
    .expect("retry snapshot");
    assert!(
        snapshot.status == crate::conversation::TimelineStatus::Retrying
            && snapshot.conversation.is_none()
    );
    drop(updates);
    // Reopening after cancellation must not leave the SDK's deduplication or
    // cache locks stuck. No fake SDK state and no cache deletion/reset.
    proxy.fault("normal", "encryption").await;
    ready(&account, id.clone(), 2, 5).await;

    let first = checked(
        account
            .client
            .create_room(create_room::v3::Request::new())
            .await,
        "switch source",
    );
    let second = checked(
        account
            .client
            .create_room(create_room::v3::Request::new())
            .await,
        "switch target",
    );
    proxy.fault("stall", "encryption").await;
    let hits = proxy.hits("faults").await;
    let source = first.room_id().to_string();
    let mut stream = Box::pin(account.conversation_updates(source.clone(), 3));
    let task = tokio::spawn(async move { stream.next().await });
    checked(
        tokio::time::timeout(Duration::from_secs(3), async {
            while proxy.hits("faults").await <= hits {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await,
        "selection in-flight",
    );
    task.abort();
    let _ = task.await;
    proxy.fault("normal", "encryption").await;
    ready(&account, second.room_id().to_string(), 4, 5).await;
    ready(&account, source, 5, 5).await;

    let dropped = checked(
        account
            .client
            .create_room(create_room::v3::Request::new())
            .await,
        "dropped room",
    );
    proxy.fault("drop", "encryption").await;
    let mut updates = Box::pin(account.conversation_updates(dropped.room_id().to_string(), 6));
    let snapshot = checked(
        tokio::time::timeout(Duration::from_secs(3), updates.next()).await,
        "dropped request response",
    )
    .expect("retry snapshot");
    assert!(snapshot.status == crate::conversation::TimelineStatus::Retrying);
    proxy.fault("normal", "encryption").await;
    let snapshot = checked(
        tokio::time::timeout(Duration::from_secs(5), updates.next()).await,
        "dropped request recovery",
    )
    .expect("ready snapshot");
    assert!(snapshot.conversation.is_some());
    drop(updates);
    checked(
        account.client.matrix_auth().logout().await,
        "network test logout",
    );
}

#[tokio::test]
#[ignore = "requires isolated TLS Synapse; run.sh room-open"]
async fn room_open_contended_event_cache_and_crypto_lease() {
    use matrix_sdk::cross_process_lock::CrossProcessLockConfig;
    let (root, data, cert) = fixture().await;
    let work = root.join(format!("room-open-lock-{}", Uuid::new_v4()));
    let proxy = Proxy::start(&root, work.join("proxy")).await;
    let path = work.join("store");
    let client = client(&proxy, &data, &cert, &path, true).await;
    let room = checked(
        client.create_room(create_room::v3::Request::new()).await,
        "lock room",
    );
    let id = room.room_id().to_string();
    let contender = checked(
        Client::builder()
            .respect_login_well_known(false)
            .homeserver_url(&proxy.url)
            .add_root_certificates(vec![cert])
            .request_config(crate::request_policy::interactive())
            .cross_process_store_config(CrossProcessLockConfig::multi_process(
                "isolated-other-holder",
            ))
            .sqlite_store(&path, Some("isolated-room-open-store-only"))
            .build()
            .await,
        "second store holder",
    );
    let guard = checked(
        contender.event_cache_store().lock().await,
        "take event-cache lease",
    );
    let account = account(client, path);
    let mut updates = Box::pin(account.conversation_updates(id.clone(), 1));
    let snapshot = checked(
        tokio::time::timeout(Duration::from_secs(4), updates.next()).await,
        "bounded store contention",
    )
    .expect("contention snapshot");
    assert!(
        snapshot.status == crate::conversation::TimelineStatus::Retrying
            && snapshot.conversation.is_none()
    );
    drop(guard);
    let snapshot = checked(
        tokio::time::timeout(Duration::from_secs(5), updates.next()).await,
        "event-cache lease release",
    )
    .expect("ready snapshot");
    assert!(snapshot.conversation.is_some());
    drop(updates);
    // A live crypto heartbeat is not itself evidence that a timeline is waiting
    // on that lease. Exercise a second holder over the same isolated SQLite DB.
    checked(
        contender
            .matrix_auth()
            .restore_session(
                account
                    .client
                    .matrix_auth()
                    .session()
                    .expect("test session"),
                Default::default(),
            )
            .await,
        "restore contender session",
    );
    checked(
        contender
            .encryption()
            .enable_cross_process_store_lock("isolated-crypto-holder".into())
            .await,
        "enable other crypto lease",
    );
    let crypto_guard = checked(
        contender.encryption().spin_lock_store(None).await,
        "hold crypto lease",
    );
    assert!(crypto_guard.is_some());
    ready(&account, id, 2, 5).await;
    drop(crypto_guard);
    // Expired lease can be reacquired without touching database contents.
    tokio::time::sleep(Duration::from_millis(650)).await;
    checked(
        account
            .client
            .encryption()
            .enable_cross_process_store_lock("isolated-our-holder".into())
            .await,
        "enable our crypto lease",
    );
    let lease = checked(
        account.client.encryption().spin_lock_store(None).await,
        "reacquire expired crypto lease",
    );
    assert!(lease.is_some());
    drop(lease);
    checked(
        account.client.matrix_auth().logout().await,
        "lock test logout",
    );
}

#[tokio::test]
#[ignore = "requires isolated TLS Synapse; run.sh room-open"]
async fn room_open_before_first_sync_and_during_sync_restart() {
    let (root, data, cert) = fixture().await;
    let work = root.join(format!("room-open-sync-{}", Uuid::new_v4()));
    let proxy = Proxy::start(&root, work.join("proxy")).await;
    let client = client(&proxy, &data, &cert, &work.join("store"), true).await;
    let room = checked(
        client.create_room(create_room::v3::Request::new()).await,
        "cold room",
    );
    let id = room.room_id().to_string();
    let account = account(client, work.join("store"));
    // There has been no sync response at all for this newly created SDK room.
    ready(&account, id.clone(), 1, 5).await;
    proxy.fault("stall", "sync").await;
    let drain = account.clone();
    let sync = tokio::spawn(async move {
        let stream = drain.room_updates();
        tokio::pin!(stream);
        while stream.next().await.is_some() {}
    });
    checked(
        tokio::time::timeout(Duration::from_secs(10), async {
            while proxy.hits("sync").await == 0 {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await,
        "first sync stalled",
    );
    ready(&account, id.clone(), 2, 5).await;
    let service = account
        .notification_service
        .borrow()
        .clone()
        .expect("sync service");
    checked(
        tokio::time::timeout(Duration::from_secs(5), service.stop()).await,
        "stop sync for restart",
    );
    proxy.fault("normal", "sync").await;
    // Core's state observer restarts a terminated SyncService with backoff.
    ready(&account, id, 3, 5).await;
    checked(
        tokio::time::timeout(Duration::from_secs(15), async {
            let mut states = Box::pin(service.state());
            while let Some(state) = states.next().await {
                if matches!(state, matrix_sdk_ui::sync_service::State::Running) {
                    return;
                }
            }
            panic!("sync state stream ended");
        })
        .await,
        "sync restarts",
    );
    sync.abort();
    let _ = sync.await;
    service.stop().await;
    checked(
        account.client.matrix_auth().logout().await,
        "sync test logout",
    );
}
