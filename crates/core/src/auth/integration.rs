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
    let alice = Account::new(alice_client);
    let bob = Account::new(bob_client);
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
    let restored = Account::new(restored_client);
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
