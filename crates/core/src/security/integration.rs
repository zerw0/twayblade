//! Real local Synapse crypto checks; sensitive SDK errors and keys are suppressed.
use super::*;
use matrix_sdk::{
    reqwest::Certificate,
    ruma::{
        api::client::room::create_room,
        events::{
            InitialStateEvent,
            room::{encryption::RoomEncryptionEventContent, message::RoomMessageEventContent},
        },
        serde::Raw,
    },
};
use serde::Deserialize;
use std::{path::Path, time::Duration};
use uuid::Uuid;

fn checked<T, E>(result: Result<T, E>, stage: &'static str) -> T {
    result.unwrap_or_else(|_| panic!("Security integration stage failed: {stage}"))
}
#[derive(Deserialize)]
struct Fixture {
    homeserver: String,
    security_username: String,
    security_password: String,
}
async fn login(root: &Path, fixture: &Fixture, certificate: &Certificate, label: &str) -> Account {
    let path = root.join(format!("security-{label}-{}", Uuid::new_v4()));
    let client = checked(
        Client::builder()
            .homeserver_url(&fixture.homeserver)
            .add_root_certificates(vec![certificate.clone()])
            .sqlite_store(&path, Some(&Uuid::new_v4().to_string()))
            .build()
            .await,
        "client store",
    );
    checked(
        client
            .matrix_auth()
            .login_username(&fixture.security_username, &fixture.security_password)
            .await,
        "login",
    );
    Account::new(
        client,
        path,
        matrix_client_platform::SessionStore::new("matrix-client.integration.crypto-unused"),
        Arc::new(Mutex::new(())),
    )
}
fn sync(account: &Account) -> JoinHandle<()> {
    let account = account.clone();
    tokio::spawn(async move {
        let stream = account.room_updates();
        tokio::pin!(stream);
        while stream.next().await.is_some() {}
    })
}
async fn wait_security(
    stage: &'static str,
    security: &Security,
    predicate: impl Fn(&SecuritySnapshot) -> bool,
) -> SecuritySnapshot {
    let mut updates = security.subscribe();
    checked(
        tokio::time::timeout(Duration::from_secs(45), async {
            loop {
                let snapshot = updates.borrow_and_update().clone();
                if predicate(&snapshot) {
                    return snapshot;
                }
                checked(updates.changed().await, "security stream");
            }
        })
        .await,
        stage,
    )
}
async fn wait_room(account: &Account, id: &matrix_sdk::ruma::RoomId) {
    checked(
        tokio::time::timeout(Duration::from_secs(45), async {
            loop {
                if account
                    .client
                    .get_room(id)
                    .is_some_and(|room| room.encryption_state().is_encrypted())
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await,
        "room sync",
    );
}

#[tokio::test]
#[ignore = "requires isolated Synapse fixture with a generated security user"]
async fn secure_backup_fresh_store_recovery_and_sas() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/local/integration");
    let fixture: Fixture = checked(
        serde_json::from_slice(&checked(
            tokio::fs::read(root.join("fixture.json")).await,
            "fixture file",
        )),
        "fixture JSON",
    );
    assert!(
        fixture.homeserver == "https://127.0.0.1:18448",
        "loopback fixture required"
    );
    let certificate = checked(
        Certificate::from_pem(&checked(
            tokio::fs::read(root.join("ca.crt")).await,
            "test CA",
        )),
        "CA format",
    );
    let first = login(&root, &fixture, &certificate, "first").await;
    let first_sync = sync(&first);
    let first_security = Security::new(&first);
    wait_security("initial security", &first_security, |snapshot| {
        snapshot.ready
    })
    .await;
    let key = checked(
        first_security.setup_backup().await,
        "cross-signing and backup setup",
    );
    let mut request = create_room::v3::Request::new();
    let encryption = InitialStateEvent::new(
        Default::default(),
        RoomEncryptionEventContent::with_recommended_defaults(),
    );
    request.initial_state = vec![checked(Raw::new(&encryption), "encryption state").cast()];
    let room = checked(first.client.create_room(request).await, "encrypted room");
    let room_id = room.room_id().to_owned();
    wait_room(&first, &room_id).await;
    checked(
        room.send(RoomMessageEventContent::text_plain(
            "isolated recovery sample",
        ))
        .await,
        "encrypted historical send",
    );
    checked(
        first
            .client
            .encryption()
            .backups()
            .wait_for_steady_state()
            .await,
        "upload old room keys",
    );

    first_sync.abort();
    let _ = first_sync.await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let second = login(&root, &fixture, &certificate, "fresh").await;
    let second_sync = sync(&second);
    let second_security = Security::new(&second);
    wait_room(&second, &room_id).await;
    wait_security("fresh-store security", &second_security, |snapshot| {
        snapshot.ready
    })
    .await;
    second.select_room(Some(room_id.as_str()));
    let timeline = second.conversation_updates(room_id.to_string(), 1);
    tokio::pin!(timeline);
    checked(
        tokio::time::timeout(Duration::from_secs(45), async {
            while let Some(snapshot) = timeline.next().await {
                eprintln!(
                    "Recovery fixture timeline counts: total={}, undecryptable={}",
                    snapshot.messages.len(),
                    snapshot
                        .messages
                        .iter()
                        .filter(|message| message.kind
                            == crate::conversation::MessageKind::Undecryptable)
                        .count()
                );
                if snapshot
                    .messages
                    .iter()
                    .any(|message| message.kind == crate::conversation::MessageKind::Undecryptable)
                {
                    return;
                }
            }
            panic!("fresh timeline did not expose missing keys");
        })
        .await,
        "undecryptable historical timeline",
    );
    assert!(
        second_security
            .recover(Zeroizing::new("wrong recovery key".into()))
            .await
            .is_err()
    );
    checked(
        second_security
            .recover(Zeroizing::new(key.as_str().to_owned()))
            .await,
        "fresh-store recovery",
    );
    checked(
        tokio::time::timeout(Duration::from_secs(45), async {
            while let Some(snapshot) = timeline.next().await {
                if snapshot.messages.iter().any(|message| {
                    message.kind == crate::conversation::MessageKind::Text
                        && message.body == "isolated recovery sample"
                }) {
                    return;
                }
            }
            panic!("timeline ended before live redecryption");
        })
        .await,
        "live timeline re-decryption from backup",
    );
    assert!(
        second
            .client
            .encryption()
            .cross_signing_status()
            .await
            .is_some_and(|status| status.is_complete())
    );

    eprintln!("PASS: wrong-key rejection and fresh-store live backup re-decryption");
    let third = login(&root, &fixture, &certificate, "sas").await;
    let third_sync = sync(&third);
    let third_security = Security::new(&third);
    wait_room(&third, &room_id).await;
    checked(third_security.refresh().await, "device query");
    checked(
        third_security
            .request_verification(second.client.device_id().unwrap().to_string())
            .await,
        "SAS request",
    );
    wait_security("incoming SAS request", &second_security, |snapshot| {
        snapshot
            .verification
            .as_ref()
            .is_some_and(|flow| flow.phase == VerificationPhase::Requested)
    })
    .await;
    checked(second_security.accept_verification().await, "SAS accept");
    let a = wait_security("recipient SAS emojis", &second_security, |snapshot| {
        snapshot
            .verification
            .as_ref()
            .is_some_and(|flow| matches!(flow.phase, VerificationPhase::Emojis(_)))
    })
    .await;
    let b = wait_security("requester SAS emojis", &third_security, |snapshot| {
        snapshot
            .verification
            .as_ref()
            .is_some_and(|flow| matches!(flow.phase, VerificationPhase::Emojis(_)))
    })
    .await;
    assert!(
        a.verification.unwrap().phase == b.verification.unwrap().phase,
        "SAS emojis differ"
    );
    checked(
        second_security.confirm_verification().await,
        "first SAS confirm",
    );
    checked(
        third_security.confirm_verification().await,
        "third SAS confirm",
    );
    wait_security("recipient SAS done", &second_security, |snapshot| {
        snapshot
            .verification
            .as_ref()
            .is_some_and(|flow| flow.phase == VerificationPhase::Done)
    })
    .await;
    wait_security("requester SAS done", &third_security, |snapshot| {
        snapshot
            .verification
            .as_ref()
            .is_some_and(|flow| flow.phase == VerificationPhase::Done)
    })
    .await;
    checked(
        third_security
            .request_verification(second.client.device_id().unwrap().to_string())
            .await,
        "cancellable verification request",
    );
    wait_security(
        "incoming cancellable request",
        &second_security,
        |snapshot| {
            snapshot
                .verification
                .as_ref()
                .is_some_and(|flow| flow.phase == VerificationPhase::Requested)
        },
    )
    .await;
    checked(
        second_security.cancel_verification(false).await,
        "cancel verification",
    );
    wait_security("cancelled verification", &third_security, |snapshot| {
        snapshot
            .verification
            .as_ref()
            .is_some_and(|flow| flow.phase == VerificationPhase::Cancelled)
    })
    .await;
    checked(
        second_security
            .sign_out_device(
                third.client.device_id().unwrap().to_string(),
                Zeroizing::new(fixture.security_password.clone()),
            )
            .await,
        "sign out another device",
    );
    assert!(
        third.client.whoami().await.is_err(),
        "other device token was not revoked"
    );
    let reset_key = checked(second_security.reset_recovery().await, "reset recovery key");
    let fourth = login(&root, &fixture, &certificate, "reset").await;
    let fourth_security = Security::new(&fourth);
    assert!(
        fourth_security
            .recover(Zeroizing::new(key.as_str().to_owned()))
            .await
            .is_err(),
        "old recovery key survived reset"
    );
    checked(
        fourth_security
            .recover(Zeroizing::new(reset_key.as_str().to_owned()))
            .await,
        "new recovery key restore",
    );
    drop(reset_key);
    checked(fourth.client.logout().await, "fourth logout");
    drop(key);
    second_sync.abort();
    third_sync.abort();
    checked(first.client.logout().await, "first logout");
    checked(second.client.logout().await, "second logout");
}
