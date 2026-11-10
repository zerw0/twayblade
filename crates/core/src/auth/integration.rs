//! Opt-in real-server checks. No access to the application's OS credential entry.
use super::*;
use futures_util::StreamExt;
use matrix_sdk::{
    reqwest::Certificate,
    ruma::{
        api::client::{message::get_message_events, room::create_room},
        events::{InitialStateEvent, room::encryption::RoomEncryptionEventContent},
        serde::Raw,
    },
};
use std::{path::Path, time::Duration};

#[derive(Deserialize)]
struct Fixture {
    homeserver: String,
    alice: String,
    bob: String,
}

// Discard SDK error payloads: tests must not print credentials or message bodies.
fn checked<T, E>(result: Result<T, E>, stage: &'static str) -> T {
    result.unwrap_or_else(|_| panic!("Integration stage failed: {stage}"))
}

async fn test_client(root: &Path, store: &str, passphrase: &str, cert: &Certificate) -> Client {
    checked(
        Client::builder()
            .with_threading_support(matrix_sdk::ThreadingSupport::Enabled {
                with_subscriptions: false,
            })
            .homeserver_url("https://127.0.0.1:18448")
            .add_root_certificates(vec![cert.clone()])
            .sqlite_store(root.join(store), Some(passphrase))
            .build()
            .await,
        "open isolated encrypted store",
    )
}

fn sync(account: &Account) -> tokio::task::JoinHandle<()> {
    let account = account.clone();
    tokio::spawn(async move {
        let stream = account.room_updates();
        tokio::pin!(stream);
        while stream.next().await.is_some() {}
    })
}

async fn wait_for_room(account: &Account, id: &matrix_sdk::ruma::RoomId) {
    checked(
        tokio::time::timeout(Duration::from_secs(45), async {
            loop {
                if account
                    .client
                    .get_room(id)
                    .is_some_and(|room| room.state() == matrix_sdk::RoomState::Joined)
                {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await,
        "joined room sync",
    );
}

async fn wait_for_message(account: &Account, room: String, body: &str) {
    account.select_room(Some(&room));
    let updates = account.conversation_updates(room, 1);
    tokio::pin!(updates);
    checked(
        tokio::time::timeout(Duration::from_secs(60), async {
            while let Some(snapshot) = updates.next().await {
                if snapshot.messages.iter().any(|message| message.body == body) {
                    return;
                }
            }
            panic!("Conversation updates ended before delivery");
        })
        .await,
        "decrypted application timeline",
    );
}

#[tokio::test]
#[ignore = "requires the isolated TLS Synapse fixture; see tests/integration/README.md"]
async fn encrypted_delivery_profile_and_same_device_store_restore() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/local/integration");
    let fixture: Fixture = checked(
        serde_json::from_slice(&checked(
            tokio::fs::read(root.join("fixture.json")).await,
            "read fixture",
        )),
        "parse fixture",
    );
    // This guard is deliberate: an opt-in integration test can never target a real account/server.
    assert!(
        fixture.homeserver == "https://127.0.0.1:18448",
        "Only the fixed loopback fixture is permitted"
    );
    let cert = checked(
        Certificate::from_pem(&checked(
            tokio::fs::read(root.join("ca.crt")).await,
            "read test certificate",
        )),
        "parse test certificate",
    );
    let run = root.join(format!("run-{}", Uuid::new_v4()));
    checked(
        tokio::fs::create_dir_all(&run).await,
        "create isolated stores",
    );
    let alice_passphrase = Zeroizing::new(Uuid::new_v4().to_string());
    let bob_passphrase = Zeroizing::new(Uuid::new_v4().to_string());
    let alice_client = test_client(&run, "alice", &alice_passphrase, &cert).await;
    let bob_client = test_client(&run, "bob", &bob_passphrase, &cert).await;
    checked(
        alice_client
            .matrix_auth()
            .login_username("alice", &fixture.alice)
            .await,
        "Alice test login",
    );
    checked(
        bob_client
            .matrix_auth()
            .login_username("bob", &fixture.bob)
            .await,
        "Bob test login",
    );
    checked(
        alice_client
            .account()
            .set_display_name(Some("Integration Alice"))
            .await,
        "set test profile",
    );
    let alice = Account::new(
        alice_client,
        run.join("alice"),
        secrets::SessionStore::default(),
        Arc::new(Mutex::new(())),
    );
    let bob = Account::new(
        bob_client,
        run.join("bob"),
        secrets::SessionStore::default(),
        Arc::new(Mutex::new(())),
    );
    checked(
        alice
            .update_profile(crate::profile::ProfileUpdate::DisplayName(
                "Integration Alice".into(),
            ))
            .await,
        "edit account display name through core",
    );
    let avatar_path = run.join("avatar.png");
    checked(
        tokio::fs::write(
            &avatar_path,
            include_bytes!("../../../gpui/assets/attachment-sample.png"),
        )
        .await,
        "prepare test avatar",
    );
    checked(
        alice
            .update_profile(crate::profile::ProfileUpdate::Avatar(avatar_path))
            .await,
        "edit account avatar through core",
    );
    assert!(checked(alice.client.account().get_avatar_url().await, "read avatar").is_some());
    checked(
        alice
            .update_profile(crate::profile::ProfileUpdate::RemoveAvatar)
            .await,
        "remove test avatar",
    );
    assert!(
        checked(
            alice.client.account().get_avatar_url().await,
            "read removed avatar"
        )
        .is_none()
    );
    let alice_sync = sync(&alice);
    let bob_sync = sync(&bob);
    let device = bob.client.device_id().expect("test device").to_owned();
    let session = bob.client.matrix_auth().session().expect("test session");
    let encryption = InitialStateEvent::new(
        Default::default(),
        RoomEncryptionEventContent::with_recommended_defaults(),
    );
    let mut request = create_room::v3::Request::new();
    request.name = Some("Isolated encrypted acceptance".into());
    request.invite = vec![bob.client.user_id().expect("test user").to_owned()];
    request.initial_state =
        vec![checked(Raw::new(&encryption), "serialize encryption state").cast()];
    let room = checked(
        alice.client.create_room(request).await,
        "create encrypted test room",
    );
    let room_id = room.room_id().to_owned();
    checked(
        bob.client.join_room_by_id(&room_id).await,
        "join encrypted test room",
    );
    wait_for_room(&alice, &room_id).await;
    wait_for_room(&bob, &room_id).await;
    let profile = checked(
        bob.profile(alice.user_id(), Some(room_id.to_string()))
            .await,
        "fetch real profile",
    );
    assert!(
        profile.display_name == "Integration Alice" && profile.server_profile_available,
        "Profile response mismatch"
    );
    eprintln!("PASS: isolated login, unified sync and real profile fetch");

    alice.select_room(Some(room_id.as_str()));
    let updates = alice.conversation_updates(room_id.to_string(), 1);
    tokio::pin!(updates);
    let conversation = checked(
        tokio::time::timeout(Duration::from_secs(45), async {
            while let Some(snapshot) = updates.next().await {
                if let Some(conversation) = snapshot.conversation {
                    return conversation;
                }
            }
            panic!("Timeline ended before becoming ready");
        })
        .await,
        "sender application timeline",
    );
    let body = format!("isolated-e2ee-{}", Uuid::new_v4());
    checked(
        conversation.send(body.clone()).await,
        "application send queue",
    );
    wait_for_message(&bob, room_id.to_string(), &body).await;
    let response = checked(
        alice
            .client
            .send(get_message_events::v3::Request::new(
                room_id.clone(),
                matrix_sdk::ruma::api::Direction::Backward,
            ))
            .await,
        "inspect server event types",
    );
    assert!(
        response.chunk.iter().any(|event| event
            .get_field::<String>("type")
            .ok()
            .flatten()
            .as_deref()
            == Some("m.room.encrypted")),
        "Encrypted wire event missing"
    );
    assert!(
        response
            .chunk
            .iter()
            .all(|event| !event.json().get().contains(&body)),
        "Plaintext found in server event payload"
    );
    eprintln!(
        "PASS: application send queue, encrypted server event and decrypted recipient timeline"
    );

    bob_sync.abort();
    let _ = bob_sync.await;
    drop(bob);
    // Allow the SDK sync-service drop guard to complete stop and release store handles.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let restored_client = test_client(&run, "bob", &bob_passphrase, &cert).await;
    checked(
        restored_client
            .matrix_auth()
            .restore_session(session, Default::default())
            .await,
        "same-device SDK session restore",
    );
    assert!(
        restored_client.device_id() == Some(device.as_ref()),
        "Device identity changed after restore"
    );
    let restored = Account::new(
        restored_client,
        run.join("bob"),
        secrets::SessionStore::default(),
        Arc::new(Mutex::new(())),
    );
    let restored_sync = sync(&restored);
    wait_for_room(&restored, &room_id).await;
    wait_for_message(&restored, room_id.to_string(), &body).await;
    eprintln!("PASS: same-device encrypted store reopen and history decryption");

    // Revoke only these generated test sessions; never call Account::logout,
    // whose production implementation owns the normal OS credential entry.
    checked(
        restored.client.matrix_auth().logout().await,
        "revoke restored test session",
    );
    checked(
        alice.client.matrix_auth().logout().await,
        "revoke sender test session",
    );
    alice_sync.abort();
    restored_sync.abort();
    let _ = alice_sync.await;
    let _ = restored_sync.await;
    eprintln!("PASS: generated test sessions revoked; production Keychain untouched");
}

/// Real OS credential storage, separate processes, and normal auth/logout code.
/// Uses a unique credential slot; never touches the user's application entry.
#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires isolated Synapse and an unlocked macOS Keychain"]
fn keychain_session_lifecycle_across_processes() {
    let run_id = Uuid::new_v4().to_string();
    for phase in ["login", "restore-and-logout"] {
        let status = checked(
            std::process::Command::new(checked(std::env::current_exe(), "test executable"))
                .args([
                    "--exact",
                    "auth::integration::keychain_session_lifecycle_phase",
                    "--ignored",
                    "--nocapture",
                ])
                .env("MATRIX_LIFECYCLE_RUN", &run_id)
                .env("MATRIX_LIFECYCLE_PHASE", phase)
                .status(),
            "run lifecycle subprocess",
        );
        assert!(status.success(), "Lifecycle subprocess failed: {phase}");
    }
    eprintln!("PASS: OS Keychain persistence across processes, same device, logout/store deletion");
}

#[cfg(target_os = "macos")]
#[derive(Serialize, Deserialize)]
struct LifecycleEvidence {
    device: String,
    store: PathBuf,
    room: String,
}

#[cfg(target_os = "macos")]
#[tokio::test]
#[ignore = "internal subprocess for the isolated Keychain lifecycle test"]
async fn keychain_session_lifecycle_phase() {
    let Ok(run_id) = std::env::var("MATRIX_LIFECYCLE_RUN") else {
        return;
    };
    let run_id = checked(Uuid::parse_str(&run_id), "validate lifecycle scope");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/local/integration");
    let fixture: Fixture = checked(
        serde_json::from_slice(&checked(
            tokio::fs::read(root.join("fixture.json")).await,
            "read fixture",
        )),
        "parse fixture",
    );
    assert!(
        fixture.homeserver == "https://127.0.0.1:18448",
        "Only loopback fixture allowed"
    );
    let certificate = checked(
        Certificate::from_pem(&checked(
            tokio::fs::read(root.join("ca.crt")).await,
            "test CA",
        )),
        "parse test CA",
    );
    let context = AuthContext {
        session_store: secrets::SessionStore::new(&format!("matrix-client.integration.{run_id}")),
        data_dir: root.join(format!("lifecycle-{run_id}")),
        root_certificate: Some(certificate),
    };
    checked(
        tokio::fs::create_dir_all(&context.data_dir).await,
        "create lifecycle directory",
    );
    let evidence_path = context.data_dir.join("evidence.json");
    match std::env::var("MATRIX_LIFECYCLE_PHASE").as_deref() {
        Ok("login") => {
            assert!(checked(restore_in(&context).await, "empty restore").is_none());
            assert!(
                login_password_in(
                    &context,
                    fixture.homeserver.clone(),
                    "alice".into(),
                    Zeroizing::new("invalid-fixture-password".into())
                )
                .await
                .is_err(),
                "Bad login accepted"
            );
            assert!(
                checked(
                    context.session_store.load().await,
                    "failed login Keychain cleanup"
                )
                .is_none()
            );
            let account = checked(
                login_password_in(
                    &context,
                    fixture.homeserver,
                    "alice".into(),
                    Zeroizing::new(fixture.alice),
                )
                .await,
                "scoped normal login",
            );
            let sync_task = sync(&account);
            let mut request = create_room::v3::Request::new();
            request.name = Some("Isolated lifecycle acceptance".into());
            let room = checked(account.client.create_room(request).await, "lifecycle room");
            wait_for_room(&account, room.room_id()).await;
            // Exercise the app's timeline and SDK send queue before process exit.
            account.select_room(Some(room.room_id().as_str()));
            let updates = account.conversation_updates(room.room_id().to_string(), 1);
            tokio::pin!(updates);
            let conversation = checked(
                tokio::time::timeout(Duration::from_secs(45), async {
                    while let Some(snapshot) = updates.next().await {
                        if let Some(conversation) = snapshot.conversation {
                            return conversation;
                        }
                    }
                    panic!("Lifecycle timeline ended");
                })
                .await,
                "lifecycle timeline",
            );
            let body = "isolated lifecycle sample";
            checked(conversation.send(body.into()).await, "lifecycle send");
            wait_for_message(&account, room.room_id().to_string(), body).await;
            let evidence = LifecycleEvidence {
                device: account.client.device_id().expect("test device").to_string(),
                store: account.store_path.clone(),
                room: room.room_id().to_string(),
            };
            // Device ID, room ID and path only; credentials remain solely in Keychain.
            checked(
                tokio::fs::write(
                    evidence_path,
                    checked(serde_json::to_vec(&evidence), "serialize public evidence"),
                )
                .await,
                "write public evidence",
            );
            sync_task.abort();
            let _ = sync_task.await;
            eprintln!("PASS: isolated normal login, room sync and application send");
        }
        Ok("restore-and-logout") => {
            let evidence: LifecycleEvidence = checked(
                serde_json::from_slice(&checked(
                    tokio::fs::read(evidence_path).await,
                    "read public evidence",
                )),
                "parse public evidence",
            );
            let account = checked(
                restore_in(&context).await,
                "restore from actual OS Keychain",
            )
            .expect("saved test account");
            assert!(
                account
                    .client
                    .device_id()
                    .is_some_and(|id| id.as_str() == evidence.device),
                "Device changed after process restart"
            );
            assert!(
                account.store_path == evidence.store,
                "Store changed after process restart"
            );
            let sync_task = sync(&account);
            wait_for_message(&account, evidence.room, "isolated lifecycle sample").await;
            sync_task.abort();
            let _ = sync_task.await;
            checked(account.logout().await, "normal scoped logout");
            assert!(
                !account.store_path.exists(),
                "Logout retained the store directory"
            );
            assert!(
                checked(context.session_store.load().await, "Keychain after logout").is_none(),
                "Logout retained credentials"
            );
            assert!(
                checked(restore_in(&context).await, "restore after logout").is_none(),
                "Account restored after logout"
            );
            eprintln!("PASS: same-device Keychain restore, synced history and logout cleanup");
        }
        _ => panic!("Invalid lifecycle phase"),
    }
}

async fn wait_snapshot<S>(
    updates: &mut S,
    predicate: impl Fn(&crate::conversation::ConversationSnapshot) -> bool,
    stage: &'static str,
) -> crate::conversation::ConversationSnapshot
where
    S: futures_util::Stream<Item = crate::conversation::ConversationSnapshot> + Unpin,
{
    checked(
        tokio::time::timeout(Duration::from_secs(45), async {
            while let Some(snapshot) = updates.next().await {
                if predicate(&snapshot) {
                    return snapshot;
                }
            }
            panic!("Timeline ended during {stage}");
        })
        .await,
        stage,
    )
}

#[tokio::test]
#[ignore = "requires isolated Synapse fixture"]
async fn timeline_metadata_and_send_queue_actions() {
    use crate::conversation::{DeliveryState, MessageKind, ReplyTarget};
    use matrix_sdk::ruma::{
        events::relation::{Reply, Thread},
        events::room::message::{Relation, RoomMessageEventContent},
        int,
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/local/integration");
    let fixture: Fixture = checked(
        serde_json::from_slice(&checked(
            tokio::fs::read(root.join("fixture.json")).await,
            "fixture",
        )),
        "parse fixture",
    );
    assert!(
        fixture.homeserver == "https://127.0.0.1:18448",
        "Loopback fixture only"
    );
    let cert = checked(
        Certificate::from_pem(&checked(
            tokio::fs::read(root.join("ca.crt")).await,
            "test CA",
        )),
        "parse CA",
    );
    let run = root.join(format!("metadata-{}", Uuid::new_v4()));
    let alice_client = test_client(&run, "alice", &Uuid::new_v4().to_string(), &cert).await;
    let bob_client = test_client(&run, "bob", &Uuid::new_v4().to_string(), &cert).await;
    checked(
        alice_client
            .matrix_auth()
            .login_username("alice", &fixture.alice)
            .await,
        "Alice login",
    );
    checked(
        bob_client
            .matrix_auth()
            .login_username("bob", &fixture.bob)
            .await,
        "Bob login",
    );
    checked(
        alice_client
            .account()
            .set_display_name(Some("Sample Alice"))
            .await,
        "test display name",
    );
    let alice = Account::new(
        alice_client,
        run.join("alice"),
        secrets::SessionStore::new("matrix-client.integration.metadata-unused"),
        Arc::new(Mutex::new(())),
    );
    let bob = Account::new(
        bob_client,
        run.join("bob"),
        secrets::SessionStore::new("matrix-client.integration.metadata-unused"),
        Arc::new(Mutex::new(())),
    );
    let alice_sync = sync(&alice);
    let bob_sync = sync(&bob);
    let mut request = create_room::v3::Request::new();
    request.name = Some("Isolated metadata acceptance".into());
    request.invite = vec![bob.client.user_id().expect("Bob").to_owned()];
    let room = checked(alice.client.create_room(request).await, "metadata room");
    let room_id = room.room_id().to_owned();
    let bob_room = checked(
        bob.client.join_room_by_id(&room_id).await,
        "join metadata room",
    );
    wait_for_room(&alice, &room_id).await;
    wait_for_room(&bob, &room_id).await;
    bob.select_room(Some(room_id.as_str()));
    let mut updates = Box::pin(bob.conversation_updates(room_id.to_string(), 1));
    let ready = wait_snapshot(
        &mut updates,
        |s| s.conversation.is_some(),
        "metadata timeline",
    )
    .await;
    let mut conversation = ready.conversation.expect("conversation");
    let root_event = checked(
        room.send(RoomMessageEventContent::text_plain("sample root excerpt"))
            .await,
        "root send",
    )
    .response
    .event_id;
    let root_snapshot = wait_snapshot(
        &mut updates,
        |s| s.messages.iter().any(|m| m.body == "sample root excerpt"),
        "root projection",
    )
    .await;
    let root_id = root_snapshot
        .messages
        .iter()
        .find(|m| m.body == "sample root excerpt")
        .expect("root row")
        .id
        .clone();
    let mut reply = RoomMessageEventContent::text_plain("sample reply");
    reply.relates_to = Some(Relation::Reply(Reply::with_event_id(root_event.clone())));
    checked(bob_room.send(reply).await, "reply send");
    wait_snapshot(&mut updates, |s| s.messages.iter().any(|m| matches!(&m.reply, Some(ReplyTarget::Ready { sender, excerpt }) if sender == "Sample Alice" && excerpt == "sample root excerpt")), "reply profile/excerpt").await;
    checked(
        conversation.toggle_reaction(&root_id, "👍").await,
        "add reaction",
    );
    wait_snapshot(
        &mut updates,
        |s| {
            s.messages.iter().any(|m| {
                m.id == root_id
                    && m.reactions
                        .iter()
                        .any(|r| r.key == "👍" && r.count == 1 && r.mine)
            })
        },
        "own reaction projection",
    )
    .await;
    checked(
        conversation.toggle_reaction(&root_id, "👍").await,
        "remove reaction",
    );
    wait_snapshot(
        &mut updates,
        |s| {
            s.messages
                .iter()
                .any(|m| m.id == root_id && m.reactions.is_empty())
        },
        "reaction removal and stable identity",
    )
    .await;
    for (msgtype, kind) in [
        ("m.emote", MessageKind::Emote),
        ("m.notice", MessageKind::Notice),
        ("m.image", MessageKind::Image),
        ("m.file", MessageKind::File),
    ] {
        let content: RoomMessageEventContent = checked(
            serde_json::from_value(
                serde_json::json!({"msgtype":msgtype,"body":"sample item","url":"mxc://matrixclient.test/sample", "info":{"size":2048}}),
            ),
            "message fixture",
        );
        checked(room.send(content).await, "kind send");
        wait_snapshot(
            &mut updates,
            |s| {
                s.messages.iter().any(|m| {
                    m.kind == kind
                        && (m.attachment.is_none()
                            || m.attachment.as_ref().is_some_and(|a| a.size == Some(2048)))
                })
            },
            "message kind projection",
        )
        .await;
    }
    let mut thread_reply = RoomMessageEventContent::text_plain("sample thread reply");
    thread_reply.relates_to = Some(Relation::Thread(Thread::plain(
        root_event.clone(),
        root_event,
    )));
    checked(room.send(thread_reply).await, "thread reply");
    // A fresh timeline fetches the SDK's server-bundled thread summary too.
    drop(updates);
    updates = Box::pin(bob.conversation_updates(room_id.to_string(), 2));
    let threaded = wait_snapshot(
        &mut updates,
        |s| {
            s.messages
                .iter()
                .any(|m| m.body == "sample root excerpt" && m.thread_replies == 1)
        },
        "thread summary",
    )
    .await;
    conversation = threaded.conversation.expect("threaded timeline");
    checked(
        room.update_power_levels(vec![(bob.client.user_id().expect("Bob"), int!(-1))])
            .await,
        "restrict test sender",
    );
    checked(
        conversation.send("sample failed retry".into()).await,
        "queue failing message",
    );
    let failed = wait_snapshot(
        &mut updates,
        |s| {
            s.messages.iter().any(|m| {
                m.body == "sample failed retry"
                    && m.delivery == DeliveryState::Failed
                    && m.can_retry
            })
        },
        "failed local echo handle",
    )
    .await;
    let failed_id = failed
        .messages
        .iter()
        .find(|m| m.body == "sample failed retry")
        .expect("failed echo")
        .id
        .clone();
    checked(
        room.update_power_levels(vec![(bob.client.user_id().expect("Bob"), int!(0))])
            .await,
        "restore test sender power",
    );
    checked(conversation.retry(&failed_id).await, "SDK retry handle");
    wait_snapshot(
        &mut updates,
        |s| {
            s.messages.iter().any(|m| {
                m.id == failed_id
                    && m.body == "sample failed retry"
                    && matches!(m.delivery, DeliveryState::Sent | DeliveryState::Remote)
            })
        },
        "retry stable identity/delivery",
    )
    .await;
    checked(
        room.update_power_levels(vec![(bob.client.user_id().expect("Bob"), int!(-1))])
            .await,
        "restrict test sender again",
    );
    checked(
        conversation.send("sample failed cancel".into()).await,
        "queue cancellation sample",
    );
    let failed = wait_snapshot(
        &mut updates,
        |s| {
            s.messages
                .iter()
                .any(|m| m.body == "sample failed cancel" && m.delivery == DeliveryState::Failed)
        },
        "second failed echo",
    )
    .await;
    let cancel_id = failed
        .messages
        .iter()
        .find(|m| m.body == "sample failed cancel")
        .expect("failed echo")
        .id
        .clone();
    checked(conversation.cancel(&cancel_id).await, "SDK cancel handle");
    wait_snapshot(
        &mut updates,
        |s| !s.messages.iter().any(|m| m.id == cancel_id),
        "cancel removes local echo",
    )
    .await;
    checked(alice.client.matrix_auth().logout().await, "Alice logout");
    checked(bob.client.matrix_auth().logout().await, "Bob logout");
    alice_sync.abort();
    bob_sync.abort();
    let _ = alice_sync.await;
    let _ = bob_sync.await;
    eprintln!(
        "PASS: SDK reply details, reaction toggle, kinds, thread summary, failed echo retry/cancel"
    );
}

#[tokio::test]
#[ignore = "requires isolated Synapse fixture"]
async fn members_typing_and_read_receipts() {
    use crate::room_state::{MemberRole, MembersState};
    use matrix_sdk::ruma::{
        events::receipt::{ReceiptThread, ReceiptType},
        events::room::message::RoomMessageEventContent,
        int,
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/local/integration");
    let fixture: Fixture = checked(
        serde_json::from_slice(&checked(
            tokio::fs::read(root.join("fixture.json")).await,
            "fixture",
        )),
        "parse fixture",
    );
    assert!(
        fixture.homeserver == "https://127.0.0.1:18448",
        "Loopback fixture only"
    );
    let cert = checked(
        Certificate::from_pem(&checked(
            tokio::fs::read(root.join("ca.crt")).await,
            "test CA",
        )),
        "parse CA",
    );
    let run = root.join(format!("room-state-{}", Uuid::new_v4()));
    let alice_client = test_client(&run, "alice", &Uuid::new_v4().to_string(), &cert).await;
    let bob_client = test_client(&run, "bob", &Uuid::new_v4().to_string(), &cert).await;
    checked(
        alice_client
            .matrix_auth()
            .login_username("alice", &fixture.alice)
            .await,
        "Alice login",
    );
    checked(
        bob_client
            .matrix_auth()
            .login_username("bob", &fixture.bob)
            .await,
        "Bob login",
    );
    checked(
        alice_client
            .account()
            .set_display_name(Some("Sample Alice"))
            .await,
        "Alice name",
    );
    checked(
        bob_client
            .account()
            .set_display_name(Some("Sample Bob"))
            .await,
        "Bob name",
    );
    let alice = Account::new(
        alice_client,
        run.join("alice"),
        secrets::SessionStore::new("matrix-client.integration.unused"),
        Arc::new(Mutex::new(())),
    );
    let bob = Account::new(
        bob_client,
        run.join("bob"),
        secrets::SessionStore::new("matrix-client.integration.unused"),
        Arc::new(Mutex::new(())),
    );
    let alice_sync = sync(&alice);
    let bob_sync = sync(&bob);
    let mut request = create_room::v3::Request::new();
    request.name = Some("Isolated room-state acceptance".into());
    request.invite = vec![bob.client.user_id().expect("Bob").to_owned()];
    let room = checked(alice.client.create_room(request).await, "room");
    let id = room.room_id().to_owned();
    checked(bob.client.join_room_by_id(&id).await, "join");
    wait_for_room(&alice, &id).await;
    wait_for_room(&bob, &id).await;
    let _power_state = checked(
        alice.sync_rooms(vec![id.clone()]).await,
        "power-level state ready",
    );
    checked(
        room.update_power_levels(vec![(bob.client.user_id().expect("Bob"), int!(50))])
            .await,
        "moderator power",
    );
    bob.select_room(Some(id.as_str()));
    let mut updates = Box::pin(bob.conversation_updates(id.to_string(), 1));
    let state = wait_snapshot(
        &mut updates,
        |s| {
            s.members_state == MembersState::Ready
                && s.members
                    .iter()
                    .any(|m| m.user_id == bob.user_id() && m.role == MemberRole::Moderator)
        },
        "member projection",
    )
    .await;
    assert!(
        state.members.iter().any(|m| m.user_id == alice.user_id()
            && m.display_name == "Sample Alice"
            && m.role == MemberRole::Admin),
        "Admin/display name projection failed"
    );
    let conversation = state.conversation.expect("timeline");
    checked(
        alice.typing_notice(id.as_str(), true).await,
        "own typing intent",
    );
    wait_snapshot(
        &mut updates,
        |s| s.typing.contains(&alice.user_id()) && !s.typing.contains(&bob.user_id()),
        "remote typing notification",
    )
    .await;
    checked(alice.typing_notice(id.as_str(), false).await, "stop typing");
    wait_snapshot(&mut updates, |s| s.typing.is_empty(), "typing stopped").await;
    let first = checked(
        room.send(RoomMessageEventContent::text_plain("sample read boundary"))
            .await,
        "first message",
    )
    .response
    .event_id;
    let latest = checked(
        room.send(RoomMessageEventContent::text_plain("sample unread message"))
            .await,
        "second message",
    )
    .response
    .event_id;
    wait_snapshot(
        &mut updates,
        |s| {
            s.messages
                .iter()
                .any(|m| m.event_id.as_deref() == Some(latest.as_str()))
        },
        "messages synced",
    )
    .await;
    checked(
        conversation.mark_read(first.as_str()).await,
        "initial fully-read marker",
    );
    wait_snapshot(
        &mut updates,
        |s| {
            s.messages
                .iter()
                .any(|m| m.event_id.as_deref() == Some(latest.as_str()) && m.new_messages)
        },
        "SDK fully-read separator",
    )
    .await;
    checked(
        conversation.mark_read(latest.as_str()).await,
        "latest visible read receipt",
    );
    wait_snapshot(
        &mut updates,
        |s| !s.messages.iter().any(|m| m.new_messages),
        "fully-read marker advanced",
    )
    .await;
    checked(
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if room
                    .load_user_receipt(
                        ReceiptType::Read,
                        &ReceiptThread::Main,
                        bob.client.user_id().expect("Bob"),
                    )
                    .await
                    .ok()
                    .flatten()
                    .is_some_and(|(event, _)| event == latest)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await,
        "receipt visible to other SDK client",
    );
    checked(alice.client.matrix_auth().logout().await, "Alice logout");
    checked(bob.client.matrix_auth().logout().await, "Bob logout");
    alice_sync.abort();
    bob_sync.abort();
    let _ = alice_sync.await;
    let _ = bob_sync.await;
    eprintln!("PASS: SDK members/roles, typing start/stop, fully-read boundary and public receipt");
}

#[tokio::test]
#[ignore = "requires isolated Synapse fixture"]
async fn pagination_preserves_message_identity() {
    use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/local/integration");
    let fixture: Fixture = checked(
        serde_json::from_slice(&checked(
            tokio::fs::read(root.join("fixture.json")).await,
            "fixture",
        )),
        "parse fixture",
    );
    assert!(
        fixture.homeserver == "https://127.0.0.1:18448",
        "Loopback fixture only"
    );
    let cert = checked(
        Certificate::from_pem(&checked(
            tokio::fs::read(root.join("ca.crt")).await,
            "test CA",
        )),
        "parse CA",
    );
    let run = root.join(format!("pagination-{}", Uuid::new_v4()));
    let alice_client = test_client(&run, "alice", &Uuid::new_v4().to_string(), &cert).await;

    checked(
        alice_client
            .matrix_auth()
            .login_username("alice", &fixture.alice)
            .await,
        "pagination login",
    );
    let room = checked(
        alice_client
            .create_room(create_room::v3::Request::new())
            .await,
        "pagination room",
    );
    let id = room.room_id().to_owned();
    for index in 0..90 {
        checked(
            room.send(RoomMessageEventContent::text_plain(format!(
                "sample history {index}"
            )))
            .await,
            "seed history",
        );
    }
    let account = Account::new(
        alice_client,
        run.join("alice"),
        secrets::SessionStore::new("matrix-client.integration.unused"),
        Arc::new(Mutex::new(())),
    );
    let syncing = sync(&account);
    wait_for_room(&account, &id).await;
    account.select_room(Some(id.as_str()));
    let mut updates = Box::pin(account.conversation_updates(id.to_string(), 1));
    let initial = wait_snapshot(
        &mut updates,
        |s| s.messages.iter().any(|m| m.body == "sample history 89"),
        "initial history",
    )
    .await;
    assert!(
        initial.messages.len() < 90,
        "Fixture did not require pagination"
    );
    let latest = initial.messages.last().expect("latest").id.clone();
    let conversation = initial.conversation.expect("timeline");
    let mut count = initial.messages.len();
    let mut reached_start = false;
    for _ in 0..10 {
        reached_start = checked(conversation.older().await, "SDK load earlier");
        let page = wait_snapshot(
            &mut updates,
            |s| s.messages.len() > count || reached_start,
            "history page",
        )
        .await;
        assert!(
            page.messages.iter().any(|m| m.id == latest),
            "Existing message identity changed"
        );
        let unique: std::collections::HashSet<_> = page.messages.iter().map(|m| &m.id).collect();
        assert_eq!(unique.len(), page.messages.len());
        count = page.messages.len();
        if reached_start {
            break;
        }
    }
    assert!(
        reached_start && count == 90,
        "Incomplete history pagination"
    );
    checked(
        account.client.matrix_auth().logout().await,
        "pagination logout",
    );
    syncing.abort();
    let _ = syncing.await;
    eprintln!("PASS: SDK history pagination reaches start without duplicates or changed IDs");
}

async fn wait_rooms<S>(
    updates: &mut S,
    predicate: impl Fn(&crate::rooms::RoomSnapshot) -> bool,
) -> crate::rooms::RoomSnapshot
where
    S: futures_util::Stream<Item = crate::rooms::RoomSnapshot> + Unpin,
{
    checked(
        tokio::time::timeout(Duration::from_secs(45), async {
            while let Some(snapshot) = updates.next().await {
                if predicate(&snapshot) {
                    return snapshot;
                }
            }
            panic!("Room list ended");
        })
        .await,
        "room list snapshot",
    )
}

#[tokio::test]
#[ignore = "requires isolated Synapse fixture"]
async fn room_list_service_tracks_recency_and_details() {
    use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/local/integration");
    let fixture: Fixture = checked(
        serde_json::from_slice(&checked(
            tokio::fs::read(root.join("fixture.json")).await,
            "fixture",
        )),
        "parse fixture",
    );
    assert!(
        fixture.homeserver == "https://127.0.0.1:18448",
        "Loopback fixture only"
    );
    let cert = checked(
        Certificate::from_pem(&checked(
            tokio::fs::read(root.join("ca.crt")).await,
            "test CA",
        )),
        "parse CA",
    );
    let run = root.join(format!("room-list-{}", Uuid::new_v4()));
    let alice_client = test_client(&run, "alice", &Uuid::new_v4().to_string(), &cert).await;

    checked(
        alice_client
            .matrix_auth()
            .login_username("alice", &fixture.alice)
            .await,
        "room-list login",
    );
    let first = checked(
        alice_client
            .create_room(create_room::v3::Request::new())
            .await,
        "first room",
    );
    let second = checked(
        alice_client
            .create_room(create_room::v3::Request::new())
            .await,
        "second room",
    );
    let first_id = first.room_id().to_string();
    let second_id = second.room_id().to_string();
    let account = Account::new(
        alice_client,
        run.join("alice"),
        secrets::SessionStore::new("matrix-client.integration.unused"),
        Arc::new(Mutex::new(())),
    );
    let mut updates = Box::pin(account.room_updates());
    wait_rooms(&mut updates, |s| {
        s.rooms.iter().any(|r| r.id == first_id) && s.rooms.iter().any(|r| r.id == second_id)
    })
    .await;
    checked(
        first
            .send(RoomMessageEventContent::text_plain("sample newest first"))
            .await,
        "first bump",
    );
    wait_rooms(&mut updates, |s| {
        s.rooms.first().is_some_and(|r| r.id == first_id)
    })
    .await;
    checked(
        second
            .send(RoomMessageEventContent::text_plain("sample newest second"))
            .await,
        "second bump",
    );
    wait_rooms(&mut updates, |s| {
        s.rooms.first().is_some_and(|r| r.id == second_id)
    })
    .await;
    checked(
        second.set_room_topic("Sample updated topic").await,
        "topic change",
    );
    checked(
        second.set_name("Sample renamed room".into()).await,
        "name change",
    );
    let updated = wait_rooms(&mut updates, |s| {
        s.rooms.iter().any(|r| {
            r.id == second_id
                && r.name == "Sample renamed room"
                && r.topic.as_deref() == Some("Sample updated topic")
        })
    })
    .await;
    assert_eq!(
        updated
            .rooms
            .iter()
            .map(|r| &r.id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        updated.rooms.len()
    );
    checked(second.leave().await, "leave room");
    wait_rooms(&mut updates, |s| !s.rooms.iter().any(|r| r.id == second_id)).await;
    checked(
        account.client.matrix_auth().logout().await,
        "room-list logout",
    );
    eprintln!("PASS: RoomListService recency, metadata refresh, membership removal and unique IDs");
}

#[cfg(target_os = "macos")]
#[tokio::test]
#[ignore = "requires isolated normal + short-lived Synapse and unlocked Keychain"]
async fn revoked_device_and_soft_logout_recovery() {
    use matrix_sdk::ruma::api::client::uiaa::{AuthData, Password, UserIdentifier};
    for soft in [false, true] {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(if soft {
            "../../test-data/local/recovery"
        } else {
            "../../test-data/local/integration"
        });
        let fixture: Fixture = checked(
            serde_json::from_slice(&checked(
                tokio::fs::read(root.join("fixture.json")).await,
                "fixture",
            )),
            "fixture JSON",
        );
        let certificate = checked(
            Certificate::from_pem(&checked(tokio::fs::read(root.join("ca.crt")).await, "CA")),
            "CA format",
        );
        let scope = Uuid::new_v4();
        let context = AuthContext {
            session_store: secrets::SessionStore::new(&format!(
                "matrix-client.integration.recovery.{scope}"
            )),
            data_dir: root.join(format!("lifecycle-{scope}")),
            root_certificate: Some(certificate.clone()),
        };
        let account = checked(
            login_password_in(
                &context,
                fixture.homeserver.clone(),
                "alice".into(),
                Zeroizing::new(fixture.alice.clone()),
            )
            .await,
            "initial login",
        );
        assert!(
            account.discard_revoked_session().await.is_err(),
            "active session cannot be discarded as revoked"
        );
        let path = account.store_path.clone();
        let device = checked(account.client.device_id().ok_or(()), "device").to_owned();
        let before = checked(context.session_store.load().await, "saved session").unwrap();
        let mut changes = account.session_state();
        let stream = account.room_updates();
        let sync = tokio::spawn(async move {
            tokio::pin!(stream);
            while let Some(snapshot) = stream.next().await {
                if snapshot.status == crate::rooms::SyncStatus::Stopped {
                    return;
                }
            }
            panic!("revoked sync did not report stop");
        });
        let second = checked(
            Client::builder()
                .with_threading_support(matrix_sdk::ThreadingSupport::Enabled {
                    with_subscriptions: false,
                })
                .homeserver_url(&fixture.homeserver)
                .add_root_certificates(vec![certificate])
                .build()
                .await,
            "second client",
        );
        if soft {
            tokio::time::sleep(Duration::from_secs(6)).await;
        } else {
            checked(
                second
                    .matrix_auth()
                    .login_username("alice", &fixture.alice)
                    .await,
                "second client login",
            );
            let mut auth = Password::new(
                UserIdentifier::from(checked(second.user_id().ok_or(()), "second user").to_owned()),
                fixture.alice.clone(),
            );
            if let Err(error) = second
                .delete_devices(
                    std::slice::from_ref(&device),
                    Some(AuthData::Password(auth.clone())),
                )
                .await
            {
                auth.session = checked(
                    error
                        .as_uiaa_response()
                        .and_then(|info| info.session.clone())
                        .ok_or(()),
                    "device revocation challenge",
                )
                .into();
                checked(
                    second
                        .delete_devices(
                            std::slice::from_ref(&device),
                            Some(AuthData::Password(auth)),
                        )
                        .await,
                    "revoke device from second client",
                );
            }
        }
        assert!(
            account.client.whoami().await.is_err(),
            "revoked/expired token accepted"
        );
        checked(
            tokio::time::timeout(Duration::from_secs(10), async {
                while *changes.borrow_and_update() == SessionState::Active {
                    checked(changes.changed().await, "revocation event");
                }
            })
            .await,
            "revocation notification",
        );
        assert_eq!(
            *changes.borrow(),
            if soft {
                SessionState::SoftLoggedOut
            } else {
                SessionState::HardLoggedOut
            }
        );
        checked(
            checked(
                tokio::time::timeout(Duration::from_secs(15), sync).await,
                "sync shutdown",
            ),
            "sync task",
        );
        assert!(
            path.exists(),
            "store must be retained before recovery/confirmation"
        );
        assert!(checked(context.session_store.load().await, "retained credential").is_some());
        if soft {
            assert!(
                account
                    .reauthenticate_password(Zeroizing::new("invalid-password".into()))
                    .await
                    .is_err()
            );
            assert!(
                checked(context.session_store.load().await, "failed reauth record")
                    .unwrap()
                    .as_str()
                    == before.as_str(),
                "failed reauthentication changed saved credentials"
            );
            let restored = checked(
                account
                    .reauthenticate_password(Zeroizing::new(fixture.alice))
                    .await,
                "soft reauthentication",
            );
            assert_eq!(restored.client.device_id(), Some(device.as_ref()));
            assert_eq!(restored.store_path, path);
            checked(restored.client.whoami().await, "reauthenticated token");
            checked(restored.logout().await, "reauthenticated logout");
        } else {
            checked(
                account.discard_revoked_session().await,
                "confirmed local discard",
            );
            checked(second.logout().await, "second client logout");
        }
        assert!(!path.exists());
        assert!(checked(context.session_store.load().await, "credential cleanup").is_none());
    }
}

#[tokio::test]
#[ignore = "requires isolated Synapse fixture"]
async fn threads_send_paginate_receipts_and_closed_unread() {
    use matrix_sdk::ruma::events::{
        receipt::{ReceiptThread, ReceiptType},
        room::message::RoomMessageEventContent,
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/local/integration");
    let fixture: Fixture = checked(
        serde_json::from_slice(&checked(
            tokio::fs::read(root.join("fixture.json")).await,
            "fixture",
        )),
        "parse fixture",
    );
    assert!(
        fixture.homeserver == "https://127.0.0.1:18448",
        "Loopback fixture only"
    );
    let cert = checked(
        Certificate::from_pem(&checked(
            tokio::fs::read(root.join("ca.crt")).await,
            "test CA",
        )),
        "parse CA",
    );
    let run = root.join(format!("threads-{}", Uuid::new_v4()));
    let alice_client = test_client(&run, "alice", &Uuid::new_v4().to_string(), &cert).await;
    let bob_client = test_client(&run, "bob", &Uuid::new_v4().to_string(), &cert).await;
    checked(
        alice_client
            .matrix_auth()
            .login_username("alice", &fixture.alice)
            .await,
        "Alice login",
    );
    checked(
        bob_client
            .matrix_auth()
            .login_username("bob", &fixture.bob)
            .await,
        "Bob login",
    );
    checked(
        alice_client
            .account()
            .set_display_name(Some("Sample Alice"))
            .await,
        "Alice name",
    );
    checked(
        bob_client
            .account()
            .set_display_name(Some("Sample Bob"))
            .await,
        "Bob name",
    );
    let alice = Account::new(
        alice_client,
        run.join("alice"),
        secrets::SessionStore::new("matrix-client.integration.unused"),
        Arc::new(Mutex::new(())),
    );
    let bob = Account::new(
        bob_client,
        run.join("bob"),
        secrets::SessionStore::new("matrix-client.integration.unused"),
        Arc::new(Mutex::new(())),
    );
    let alice_sync = sync(&alice);
    let bob_sync = sync(&bob);
    let mut request = create_room::v3::Request::new();
    request.name = Some("Isolated thread acceptance".into());
    request.invite = vec![bob.client.user_id().expect("Bob").to_owned()];
    let room = checked(alice.client.create_room(request).await, "room");
    let id = room.room_id().to_owned();
    checked(bob.client.join_room_by_id(&id).await, "join");
    wait_for_room(&alice, &id).await;
    wait_for_room(&bob, &id).await;

    alice.select_room(Some(id.as_str()));
    bob.select_room(Some(id.as_str()));
    let root_event = checked(
        room.send(RoomMessageEventContent::text_plain("thread root sample"))
            .await,
        "send root",
    )
    .response
    .event_id;
    let mut main = Box::pin(bob.conversation_updates(id.to_string(), 1));
    let main_state = wait_snapshot(
        &mut main,
        |s| {
            s.messages
                .iter()
                .any(|m| m.event_id.as_deref() == Some(root_event.as_str()))
        },
        "main root",
    )
    .await;
    let main_conversation = main_state.conversation.expect("main timeline");
    let mut alice_thread =
        Box::pin(alice.thread_updates(id.to_string(), root_event.to_string(), 2));
    let alice_state = wait_snapshot(
        &mut alice_thread,
        |s| s.conversation.is_some(),
        "Alice thread",
    )
    .await;
    let alice_conversation = alice_state.conversation.expect("Alice timeline");
    checked(
        alice_conversation.send("thread seed sample".into()).await,
        "start new thread through SDK",
    );
    checked(
        alice_conversation.send("thread send sample".into()).await,
        "send through SDK thread",
    );
    let mut bob_thread = Box::pin(bob.thread_updates(id.to_string(), root_event.to_string(), 3));
    let bob_state = wait_snapshot(
        &mut bob_thread,
        |s| s.messages.iter().any(|m| m.body == "thread send sample"),
        "Bob receives threaded send",
    )
    .await;
    let bob_conversation = bob_state.conversation.expect("Bob timeline");
    let latest = bob_state
        .messages
        .iter()
        .find(|m| m.body == "thread send sample")
        .and_then(|m| m.event_id.clone())
        .expect("remote thread reply");
    checked(bob_conversation.mark_read(&latest).await, "thread receipt");
    checked(
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if room
                    .load_user_receipt(
                        ReceiptType::Read,
                        &ReceiptThread::Thread(root_event.clone()),
                        bob.client.user_id().expect("Bob"),
                    )
                    .await
                    .ok()
                    .flatten()
                    .is_some_and(|(event, _)| event.as_str() == latest)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await,
        "thread-scoped receipt received by Alice",
    );
    wait_snapshot(
        &mut alice_thread,
        |s| {
            s.messages.iter().any(|m| {
                m.event_id.as_deref() == Some(latest.as_str()) && m.read_by.contains(&bob.user_id())
            })
        },
        "typed thread receipt projection",
    )
    .await;
    checked(
        bob_conversation.send("thread response sample".into()).await,
        "Bob sends in thread",
    );
    wait_snapshot(
        &mut alice_thread,
        |s| {
            s.messages
                .iter()
                .any(|m| m.body == "thread response sample")
        },
        "Alice receives Bob reply",
    )
    .await;
    drop(bob_thread);
    for ix in 0..65 {
        checked(
            alice_conversation
                .send(format!("pagination sample {ix}"))
                .await,
            "thread send queue",
        );
    }
    let mut reopened = Box::pin(bob.thread_updates(id.to_string(), root_event.to_string(), 4));
    let latest_state = wait_snapshot(
        &mut reopened,
        |s| s.unread > 0 && s.messages.iter().any(|m| m.body == "pagination sample 64"),
        "closed thread receive",
    )
    .await;
    let reopened_conversation = latest_state.conversation.clone().expect("reopened thread");
    assert!(
        latest_state.unread > 0,
        "closed thread unread must survive reopening"
    );
    let main_state = wait_snapshot(
        &mut main,
        |s| {
            s.messages
                .iter()
                .any(|m| m.thread_replies >= 68 && m.thread_unread > 0)
        },
        "live root reply summary",
    )
    .await;
    assert!(
        !main_state
            .messages
            .iter()
            .any(|m| m.body.starts_with("pagination sample") || m.body == "thread send sample"),
        "main timeline must hide threaded replies"
    );
    checked(
        main_conversation.mark_read(root_event.as_str()).await,
        "main-scoped receipt",
    );
    assert!(
        checked(
            reopened_conversation.unread(root_event.as_str()).await,
            "closed unread"
        ) > 0,
        "reading main must not clear thread unread"
    );
    let mut state = latest_state;
    for _ in 0..8 {
        if state
            .messages
            .iter()
            .any(|m| m.body == "thread seed sample")
        {
            break;
        }
        checked(reopened_conversation.older().await, "thread pagination");
        state = wait_snapshot(
            &mut reopened,
            |s| s.messages.len() > state.messages.len(),
            "earlier thread page",
        )
        .await;
    }
    assert!(
        state
            .messages
            .iter()
            .any(|m| m.body == "thread seed sample"),
        "thread pagination must reach first reply"
    );
    let ids = state
        .messages
        .iter()
        .map(|m| &m.id)
        .collect::<std::collections::HashSet<_>>();
    assert!(
        ids.len() == state.messages.len(),
        "thread IDs must remain unique"
    );
    checked(alice.client.matrix_auth().logout().await, "Alice logout");
    checked(bob.client.matrix_auth().logout().await, "Bob logout");
    alice_sync.abort();
    bob_sync.abort();
    println!(
        "PASS: SDK thread send/receive, pagination, scoped receipts, main filtering and closed-thread unread"
    );
}

#[tokio::test]
#[ignore = "requires isolated Synapse fixture"]
async fn spaces_hierarchy_invites_creation_restricted_join_and_selective_leave() {
    use crate::space_actions::{CreateRoom, JoinAccess};
    use matrix_sdk::ruma::{
        events::{
            room::join_rules::RoomJoinRulesEventContent, space::child::SpaceChildEventContent,
        },
        room::RoomType,
    };
    use matrix_sdk_ui::spaces::SpaceService;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/local/integration");
    #[derive(Deserialize)]
    struct SpaceFixture {
        homeserver: String,
        scenario_alice_username: String,
        scenario_alice_password: String,
        scenario_bob_username: String,
        scenario_bob_password: String,
    }
    let fixture: SpaceFixture = checked(
        serde_json::from_slice(&checked(
            tokio::fs::read(root.join("fixture.json")).await,
            "fixture",
        )),
        "parse fixture",
    );
    assert!(
        fixture.homeserver == "https://127.0.0.1:18448",
        "Loopback fixture only"
    );
    let cert = checked(
        Certificate::from_pem(&checked(
            tokio::fs::read(root.join("ca.crt")).await,
            "test CA",
        )),
        "parse CA",
    );
    let run = root.join(format!("spaces-{}", Uuid::new_v4()));
    let alice_client = test_client(&run, "alice", &Uuid::new_v4().to_string(), &cert).await;
    let bob_client = test_client(&run, "bob", &Uuid::new_v4().to_string(), &cert).await;
    checked(
        alice_client
            .matrix_auth()
            .login_username(
                &fixture.scenario_alice_username,
                &fixture.scenario_alice_password,
            )
            .await,
        "Alice login",
    );
    checked(
        bob_client
            .matrix_auth()
            .login_username(
                &fixture.scenario_bob_username,
                &fixture.scenario_bob_password,
            )
            .await,
        "Bob login",
    );
    checked(
        alice_client
            .account()
            .set_display_name(Some("Sample Alice"))
            .await,
        "Alice name",
    );
    checked(
        bob_client
            .account()
            .set_display_name(Some("Sample Bob"))
            .await,
        "Bob name",
    );
    let alice = Account::new(
        alice_client,
        run.join("alice"),
        secrets::SessionStore::new("matrix-client.integration.unused"),
        Arc::new(Mutex::new(())),
    );
    let bob = Account::new(
        bob_client,
        run.join("bob"),
        secrets::SessionStore::new("matrix-client.integration.unused"),
        Arc::new(Mutex::new(())),
    );
    let alice_sync = sync(&alice);
    let bob_sync = sync(&bob);

    let mut request = create_room::v3::Request::new();
    request.name = Some("Isolated public space".into());
    request.preset = Some(create_room::v3::RoomPreset::PublicChat);
    let mut creation = create_room::v3::CreationContent::new();
    creation.room_type = Some(RoomType::Space);
    request.creation_content = Some(checked(Raw::new(&creation), "space creation content"));
    let parent = checked(alice.client.create_room(request).await, "public space");
    let parent_id = parent.room_id().to_owned();
    let via = alice
        .client
        .user_id()
        .expect("Alice")
        .server_name()
        .to_owned();
    let mut request = create_room::v3::Request::new();
    request.name = Some("Isolated private nested space".into());
    request.preset = Some(create_room::v3::RoomPreset::PrivateChat);
    request.creation_content = Some(checked(Raw::new(&creation), "nested creation content"));
    let nested = checked(
        alice.client.create_room(request).await,
        "private nested space",
    );
    let nested_id = nested.room_id().to_owned();
    checked(
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if parent.is_space() && nested.is_space() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await,
        "space state synced",
    );
    let _space_sync = checked(
        alice
            .sync_rooms(vec![parent_id.clone(), nested_id.clone()])
            .await,
        "spaces fully synced",
    );
    let service = SpaceService::new(alice.client.clone()).await;
    if let Err(error) = service
        .add_child_to_space(nested_id.clone(), parent_id.clone())
        .await
    {
        use matrix_sdk_ui::spaces::Error;
        match error {
            Error::UpdateRelationship(e) => panic!(
                "Nested forward relationship failed, HTTP status: {:?}",
                e.as_client_api_error().map(|e| e.status_code.as_u16())
            ),
            Error::UpdateInverseRelationship(e) => panic!(
                "Nested inverse relationship failed, HTTP status: {:?}",
                e.as_client_api_error().map(|e| e.status_code.as_u16())
            ),
            _ => panic!("Nested relationship failed before update"),
        }
    }
    checked(
        alice.join_space(parent_id.as_str()).await,
        "idempotent space join",
    );
    checked(
        bob.join_space(parent_id.as_str()).await,
        "Bob joins public space",
    );
    wait_for_room(&bob, &parent_id).await;
    let encrypted = (alice
        .create_room(CreateRoom {
            name: "Encrypted space child".into(),
            topic: "Sample topic".into(),
            encrypted: true,
            parent: Some(parent_id.to_string()),
        })
        .await)
        .unwrap_or_else(|error| panic!("Encrypted child creation failed: {error}"));
    let child_id = checked(matrix_sdk::ruma::RoomId::parse(&encrypted), "child ID");
    let child = alice.client.get_room(&child_id).expect("created child");
    // Rooms created in a space must be joinable by its members, not invite-only.
    checked(
        tokio::time::timeout(Duration::from_secs(30), async {
            while !matches!(
                child.join_rule(),
                Some(matrix_sdk::ruma::events::room::join_rules::JoinRule::Restricted(_))
            ) {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await,
        "space child uses a restricted join rule",
    );
    checked(
        child
            .invite_user_by_id(bob.client.user_id().expect("Bob"))
            .await,
        "room invitation",
    );
    checked(
        nested
            .invite_user_by_id(bob.client.user_id().expect("Bob"))
            .await,
        "private space invitation",
    );
    let dm = checked(alice.start_dm(&bob.user_id()).await, "create encrypted DM");
    checked(
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let invites = bob.invites().await;
                if invites.iter().any(|i| i.id == encrypted)
                    && invites
                        .iter()
                        .any(|i| i.id == nested_id.as_str() && i.space)
                    && invites.iter().any(|i| i.id == dm && i.direct)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await,
        "typed room/space/DM invites",
    );
    checked(
        bob.respond_to_invite(&encrypted, true).await,
        "accept child invite",
    );
    checked(
        bob.respond_to_invite(nested_id.as_str(), true).await,
        "accept private space invite",
    );
    checked(bob.respond_to_invite(&dm, false).await, "decline DM invite");
    wait_for_room(&bob, &child_id).await;
    wait_for_room(&bob, &nested_id).await;
    checked(
        service
            .add_child_to_space(child_id.clone(), nested_id.clone())
            .await,
        "multi-parent child link",
    );
    let restricted = checked(
        alice
            .client
            .create_room(create_room::v3::Request::new())
            .await,
        "restricted child",
    );
    let rules: RoomJoinRulesEventContent = checked(
        serde_json::from_value(
            serde_json::json!({"join_rule":"restricted","allow":[{"type":"m.room_membership","room_id":parent_id}]}),
        ),
        "restricted join rule",
    );
    checked(restricted.send_state_event(rules).await, "restricted state");
    let mut suggested = SpaceChildEventContent::new(vec![via.clone()]);
    suggested.suggested = true;
    checked(
        parent
            .send_state_event_for_key(restricted.room_id(), suggested)
            .await,
        "suggested restricted child",
    );
    // Synapse caps hierarchy pages at 50 rooms. Force a real continuation.
    for ix in 0..52 {
        let mut request = create_room::v3::Request::new();
        request.name = Some(format!("Directory pagination sample {ix}"));
        request.preset = Some(create_room::v3::RoomPreset::PublicChat);
        let extra = checked(alice.client.create_room(request).await, "pagination child");
        checked(
            parent
                .send_state_event_for_key(
                    extra.room_id(),
                    SpaceChildEventContent::new(vec![via.clone()]),
                )
                .await,
            "pagination child link",
        );
    }
    let hidden = checked(
        alice
            .client
            .create_room(create_room::v3::Request::new())
            .await,
        "inaccessible private child",
    );
    checked(
        parent
            .send_state_event_for_key(
                hidden.room_id(),
                SpaceChildEventContent::new(vec![via.clone()]),
            )
            .await,
        "inaccessible child link",
    );
    let directory = checked(bob.directory(parent_id.as_str()).await, "open hierarchy");
    let mut page = checked(directory.next_page().await, "hierarchy first page");
    assert!(
        !page.end_reached,
        "fixture must exercise a real hierarchy continuation"
    );
    for _ in 0..10 {
        if page.end_reached {
            break;
        }
        page = checked(directory.next_page().await, "hierarchy next page");
    }
    assert!(
        page.rooms.len() >= 55,
        "all accessible directory children must paginate"
    );
    assert_eq!(
        page.rooms
            .iter()
            .map(|r| &r.id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        page.rooms.len()
    );
    assert!(
        !page.rooms.iter().any(|r| r.id == hidden.room_id().as_str()),
        "inaccessible private child must not expose a Join control"
    );
    assert!(
        page.end_reached
            && page
                .rooms
                .iter()
                .any(|r| r.id == restricted.room_id().as_str()
                    && r.access == JoinAccess::Restricted
                    && r.suggested),
        "restricted suggested child missing"
    );
    assert!(
        page.rooms.iter().any(|r| r.id == encrypted && r.joined),
        "joined child missing"
    );
    checked(
        bob.join_room_or_space(restricted.room_id().as_str(), vec![via.to_string()])
            .await,
        "restricted join through allowed space",
    );
    assert!(
        bob.create_room(CreateRoom {
            name: "Must not be created".into(),
            parent: Some(parent_id.to_string()),
            encrypted: true,
            ..Default::default()
        })
        .await
        .is_err(),
        "unprivileged user must not create in parent"
    );
    checked(
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if child.encryption_state().is_encrypted() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await,
        "created child encryption",
    );
    let mut spaces = Box::pin(bob.space_updates());
    checked(
        tokio::time::timeout(Duration::from_secs(30), async {
            while let Some(snapshot) = spaces.next().await {
                if snapshot
                    .spaces
                    .iter()
                    .any(|s| s.id == parent_id.as_str() && s.room_ids.contains(&encrypted))
                    && snapshot
                        .spaces
                        .iter()
                        .any(|s| s.id == nested_id.as_str() && s.room_ids.contains(&encrypted))
                {
                    break;
                }
            }
        })
        .await,
        "multi-parent navigation",
    );
    let plan = checked(
        bob.plan_leave_space(parent_id.as_str()).await,
        "SDK leave plan",
    );
    assert!(
        plan.rooms.iter().any(|r| r.id == encrypted && r.shared),
        "shared child must be offered explicitly"
    );
    checked(
        plan.leave(std::collections::HashSet::from([parent_id.to_string()]))
            .await,
        "leave only selected parent",
    );
    assert!(
        bob.client
            .get_room(&child_id)
            .is_some_and(|r| r.state() == matrix_sdk::RoomState::Joined),
        "space leave must preserve unselected child"
    );
    checked(alice.client.matrix_auth().logout().await, "Alice logout");
    checked(bob.client.matrix_auth().logout().await, "Bob logout");
    alice_sync.abort();
    bob_sync.abort();
    println!(
        "PASS: SDK nested/private hierarchy, restricted join, invites, encrypted creation, multi-parent navigation and selective leave"
    );
}

#[tokio::test]
#[ignore = "requires isolated Synapse fixture"]
async fn notification_rules_round_trip_and_second_client_changes() {
    use crate::notifications::{DefaultLevel, RoomClass, RoomMode, RuleChange};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/local/integration");
    let fixture: Fixture = checked(
        serde_json::from_slice(&checked(
            tokio::fs::read(root.join("fixture.json")).await,
            "fixture",
        )),
        "parse fixture",
    );
    assert!(fixture.homeserver == "https://127.0.0.1:18448");
    let cert = checked(
        Certificate::from_pem(&checked(
            tokio::fs::read(root.join("ca.crt")).await,
            "test CA",
        )),
        "parse CA",
    );
    let run = root.join(format!("notifications-{}", Uuid::new_v4()));
    let mut accounts = Vec::new();
    for store in ["first", "second"] {
        let client = test_client(&run, store, &Uuid::new_v4().to_string(), &cert).await;
        checked(
            client
                .matrix_auth()
                .login_username("alice", &fixture.alice)
                .await,
            "isolated login",
        );
        accounts.push(Account::new(
            client,
            run.join(store),
            secrets::SessionStore::new("matrix-client.integration.unused"),
            Arc::new(Mutex::new(())),
        ));
    }
    let first = &accounts[0];
    let second = &accounts[1];
    let (room_updates_tx, mut room_updates_rx) = tokio::sync::watch::channel(None);
    let stream_account = first.clone();
    let first_sync = tokio::spawn(async move {
        let stream = stream_account.room_updates();
        tokio::pin!(stream);
        while let Some(snapshot) = stream.next().await {
            room_updates_tx.send_replace(Some(snapshot));
        }
    });
    let second_sync = sync(second);
    let room = checked(
        first
            .client
            .create_room(create_room::v3::Request::new())
            .await,
        "room",
    );
    wait_for_room(second, room.room_id()).await;
    let rules = checked(first.notification_rules().await, "first settings");
    let original = rules.snapshot().await;
    let second_rules = checked(second.notification_rules().await, "second settings");
    let mut second_changes = second_rules.subscribe();
    for class in RoomClass::ALL {
        for level in [DefaultLevel::All, DefaultLevel::Mentions] {
            checked(
                rules.change(RuleChange::Default(class, level)).await,
                "set default",
            );
            assert_eq!(rules.snapshot().await.defaults[class.index()], level);
            checked(
                tokio::time::timeout(Duration::from_secs(30), async {
                    loop {
                        if second_rules.snapshot().await.defaults[class.index()] == level {
                            break;
                        }
                        checked(second_changes.recv().await, "round trip settings event");
                    }
                })
                .await,
                "default round trip through second client",
            );
        }
    }
    let word = format!("fixture-{}", Uuid::new_v4());
    checked(
        rules.change(RuleChange::AddKeyword(word.clone())).await,
        "add keyword",
    );
    assert!(rules.snapshot().await.keywords.contains(&word));
    checked(
        rules.change(RuleChange::RemoveKeyword(word.clone())).await,
        "remove keyword",
    );
    assert!(!rules.snapshot().await.keywords.contains(&word));
    for mode in [
        RoomMode::All,
        RoomMode::Mentions,
        RoomMode::Mute,
        RoomMode::Default,
    ] {
        checked(
            rules
                .change(RuleChange::Room(room.room_id().to_string(), mode))
                .await,
            "set room override",
        );
        assert_eq!(
            rules.snapshot().await.room_mode(room.room_id().as_str()),
            mode
        );
    }
    let mut changes = rules.subscribe();
    checked(
        second_rules
            .change(RuleChange::AddKeyword(word.clone()))
            .await,
        "second client keyword",
    );
    checked(
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if rules.snapshot().await.keywords.contains(&word) {
                    break;
                }
                checked(changes.recv().await, "settings change subscription");
            }
        })
        .await,
        "observe second client change",
    );
    let bob_client = test_client(&run, "bob", &Uuid::new_v4().to_string(), &cert).await;
    checked(
        bob_client
            .matrix_auth()
            .login_username("bob", &fixture.bob)
            .await,
        "Bob login",
    );
    let bob = Account::new(
        bob_client,
        run.join("bob"),
        secrets::SessionStore::new("matrix-client.integration.unused"),
        Arc::new(Mutex::new(())),
    );
    let bob_sync = sync(&bob);
    checked(
        room.invite_user_by_id(bob.client.user_id().expect("Bob"))
            .await,
        "invite unread sender",
    );
    checked(
        bob.client.join_room_by_id(room.room_id()).await,
        "join unread room",
    );
    wait_for_room(&bob, room.room_id()).await;
    let _recipient_state = checked(
        first.sync_rooms(vec![room.room_id().to_owned()]).await,
        "unread recipient state",
    );
    let _sender_state = checked(
        bob.sync_rooms(vec![room.room_id().to_owned()]).await,
        "unread sender state",
    );
    checked(
        rules
            .change(RuleChange::Room(room.room_id().to_string(), RoomMode::All))
            .await,
        "enable unread room",
    );
    let bob_room = bob.client.get_room(room.room_id()).expect("Bob room");
    checked(
        bob_room
            .send(
                matrix_sdk::ruma::events::room::message::RoomMessageEventContent::text_plain(
                    "isolated unread notification sample",
                ),
            )
            .await,
        "send unread event",
    );
    let unread = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if room_updates_rx
                .borrow_and_update()
                .as_ref()
                .is_some_and(|snapshot| {
                    snapshot
                        .rooms
                        .iter()
                        .any(|r| r.id == room.room_id().as_str() && !r.muted && r.unread > 0)
                })
            {
                break;
            }
            checked(room_updates_rx.changed().await, "unread projection update");
        }
    })
    .await;
    if unread.is_err() {
        let projected = room_updates_rx.borrow().as_ref().and_then(|snapshot| {
            snapshot
                .rooms
                .iter()
                .find(|r| r.id == room.room_id().as_str())
                .map(|r| (r.unread, r.muted))
        });
        eprintln!(
            "Unread fixture counts: SDK notifications={}, SDK messages={}, SDK local notifications={}, projected={projected:?}",
            room.unread_notification_counts().notification_count,
            room.num_unread_messages(),
            room.num_unread_notifications()
        );
    }
    checked(unread, "real unread before mute");
    checked(
        rules
            .change(RuleChange::Room(room.room_id().to_string(), RoomMode::Mute))
            .await,
        "mute room",
    );
    checked(
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if room_updates_rx
                    .borrow_and_update()
                    .as_ref()
                    .is_some_and(|snapshot| {
                        snapshot
                            .rooms
                            .iter()
                            .any(|r| r.id == room.room_id().as_str() && r.muted && r.unread == 0)
                    })
                {
                    break;
                }
                checked(room_updates_rx.changed().await, "room projection update");
            }
        })
        .await,
        "muted room projection",
    );
    checked(
        rules
            .change(RuleChange::Room(
                room.room_id().to_string(),
                RoomMode::Default,
            ))
            .await,
        "reset room override",
    );
    checked(
        rules.change(RuleChange::RemoveKeyword(word)).await,
        "cleanup keyword",
    );
    for class in RoomClass::ALL {
        checked(
            rules
                .change(RuleChange::Default(class, original.defaults[class.index()]))
                .await,
            "restore fixture default",
        );
    }
    first_sync.abort();
    second_sync.abort();
    bob_sync.abort();
    eprintln!(
        "PASS: notification defaults, keywords, room overrides, muted badges and other-client observation"
    );
}

#[tokio::test]
#[ignore = "requires isolated Synapse fixture"]
async fn encrypted_notification_delivery_and_private_preview() {
    use crate::notifications::{NotificationContext, RoomMode, RuleChange};
    use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/local/integration");
    let fixture: Fixture = checked(
        serde_json::from_slice(&checked(
            tokio::fs::read(root.join("fixture.json")).await,
            "fixture",
        )),
        "parse fixture",
    );
    assert!(fixture.homeserver == "https://127.0.0.1:18448");
    let cert = checked(
        Certificate::from_pem(&checked(
            tokio::fs::read(root.join("ca.crt")).await,
            "test CA",
        )),
        "parse CA",
    );
    let run = root.join(format!("notification-delivery-{}", Uuid::new_v4()));
    let mut accounts = Vec::new();
    for (name, password) in [("alice", &fixture.alice), ("bob", &fixture.bob)] {
        let client = test_client(&run, name, &Uuid::new_v4().to_string(), &cert).await;
        checked(
            client.matrix_auth().login_username(name, password).await,
            "isolated login",
        );
        accounts.push(Account::new(
            client,
            run.join(name),
            secrets::SessionStore::new("matrix-client.integration.unused"),
            Arc::new(Mutex::new(())),
        ));
    }
    let alice = &accounts[0];
    let bob = &accounts[1];
    checked(
        alice
            .client
            .account()
            .set_display_name(Some("Sample sender"))
            .await,
        "sender profile",
    );
    let alice_sync = sync(alice);
    let bob_sync = sync(bob);
    let (context, receiver) = tokio::sync::watch::channel(NotificationContext {
        enabled: true,
        ..Default::default()
    });
    let (items, mut received) = tokio::sync::mpsc::channel(8);
    let bob_notifications = bob.clone();
    let notifications = tokio::spawn(async move {
        let stream = bob_notifications.notification_updates(receiver);
        tokio::pin!(stream);
        while let Some(item) = stream.next().await {
            if items.send(item).await.is_err() {
                break;
            }
        }
    });
    let encryption = InitialStateEvent::new(
        Default::default(),
        RoomEncryptionEventContent::with_recommended_defaults(),
    );
    let mut request = create_room::v3::Request::new();
    request.name = Some("Isolated notification room".into());
    request.invite = vec![bob.client.user_id().expect("test user").to_owned()];
    request.initial_state = vec![checked(Raw::new(&encryption), "encryption state").cast()];
    let room = checked(alice.client.create_room(request).await, "encrypted room");
    checked(
        bob.client.join_room_by_id(room.room_id()).await,
        "join encrypted room",
    );
    wait_for_room(alice, room.room_id()).await;
    wait_for_room(bob, room.room_id()).await;
    alice.select_room(Some(room.room_id().as_str()));
    bob.select_room(Some(room.room_id().as_str()));
    let rules = checked(bob.notification_rules().await, "notification rules");
    checked(
        rules
            .change(RuleChange::Room(room.room_id().to_string(), RoomMode::All))
            .await,
        "enable room notifications",
    );
    let body = format!("isolated encrypted notification {}", Uuid::new_v4());
    let event = checked(
        room.send(RoomMessageEventContent::text_plain(&body)).await,
        "encrypted send",
    )
    .response
    .event_id;
    let item = checked(
        tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                let item = received.recv().await.expect("notification stream");
                if item.event == event.as_str() {
                    break item;
                }
            }
        })
        .await,
        "decrypted typed notification",
    );
    assert!(
        item.body == body && item.sender == alice.user_id() && item.room == room.room_id().as_str()
    );
    assert!(item.sender_name == "Sample sender");
    let private = item.preview(false);
    assert!(private.sender.is_none() && private.body.is_none() && private.avatar.is_none());
    // The server stored ciphertext; the item above came through the SDK resolver.
    let stored = checked(
        alice
            .client
            .send(
                matrix_sdk::ruma::api::client::room::get_room_event::v3::Request::new(
                    room.room_id().to_owned(),
                    event.clone(),
                ),
            )
            .await,
        "server encrypted event",
    );
    assert!(
        checked(stored.event.get_field::<String>("type"), "event type").as_deref()
            == Some("m.room.encrypted")
    );
    context.send_replace(NotificationContext {
        enabled: false,
        ..Default::default()
    });
    checked(
        rules
            .change(RuleChange::Room(
                room.room_id().to_string(),
                RoomMode::Default,
            ))
            .await,
        "reset room override",
    );
    notifications.abort();
    alice_sync.abort();
    bob_sync.abort();
    eprintln!("PASS: encrypted SDK notification resolution, typed sender/body and private preview");
}
