use super::*;
use matrix_sdk::{
    Client, config::SyncSettings, reqwest::Certificate, ruma::api::client::room::create_room,
};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};
fn checked<T, E>(value: Result<T, E>, stage: &str) -> T {
    value.unwrap_or_else(|_| panic!("Presence fixture stage failed: {stage}"))
}
#[tokio::test]
#[ignore = "requires isolated Synapse presence-on and presence-off runs"]
async fn presence_enabled_and_disabled_server_evidence() {
    let enabled = match std::env::var("MATRIX_TEST_PRESENCE").as_deref() {
        Ok("enabled") => true,
        Ok("disabled") => false,
        _ => panic!("Explicit isolated presence mode required"),
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/local/integration");
    let fixture: serde_json::Value = checked(
        serde_json::from_slice(&checked(
            tokio::fs::read(root.join("fixture.json")).await,
            "fixture",
        )),
        "fixture JSON",
    );
    assert_eq!(
        fixture["homeserver"].as_str(),
        Some("https://127.0.0.1:18448")
    );
    let cert = checked(
        Certificate::from_pem(&checked(tokio::fs::read(root.join("ca.crt")).await, "CA")),
        "CA format",
    );
    let mut accounts = Vec::new();
    for role in ["alice", "bob"] {
        let path = root.join(format!("presence-{}", uuid::Uuid::new_v4()));
        let client = checked(
            Client::builder()
                .homeserver_url("https://127.0.0.1:18448")
                .add_root_certificates(vec![cert.clone()])
                .sqlite_store(&path, Some(&uuid::Uuid::new_v4().to_string()))
                .build()
                .await,
            "client",
        );
        checked(
            client
                .matrix_auth()
                .login_username(
                    fixture[format!("scenario_{role}_username")]
                        .as_str()
                        .unwrap(),
                    fixture[format!("scenario_{role}_password")]
                        .as_str()
                        .unwrap(),
                )
                .await,
            "login",
        );
        accounts.push(Account::new(
            client,
            path,
            twayblade_platform::SessionStore::new("twayblade.integration.presence-unused"),
            Arc::new(Mutex::new(())),
        ));
    }
    let alice = &accounts[0];
    let bob = &accounts[1];
    let mut request = create_room::v3::Request::new();
    request.invite = vec![bob.client.user_id().unwrap().to_owned()];
    let room = checked(alice.client.create_room(request).await, "room");
    checked(bob.client.join_room_by_id(room.room_id()).await, "join");
    checked(
        alice.client.sync_once(SyncSettings::default()).await,
        "sync Alice",
    );
    checked(
        bob.client.sync_once(SyncSettings::default()).await,
        "sync Bob",
    );
    checked(alice.set_presence(State::Online, None).await, "online");
    checked(
        bob.set_presence(State::Unavailable, Some("fixture availability".into()))
            .await,
        "away and status",
    );
    let visible = vec![bob.client.user_id().unwrap().to_owned()];
    let mut snapshot = checked(alice.presence(visible.clone()).await, "observed presence");
    if enabled {
        for _ in 0..30 {
            if snapshot.enabled
                && snapshot
                    .users
                    .get(&bob.user_id())
                    .is_some_and(|p| p.state == State::Unavailable)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
            snapshot = checked(
                alice.presence(visible.clone()).await,
                "presence propagation",
            );
        }
        assert!(snapshot.enabled);
        assert_eq!(snapshot.own.unwrap().state, State::Online);
        let partner = snapshot
            .users
            .get(&bob.user_id())
            .expect("visible shared-room partner");
        assert_eq!(partner.state, State::Unavailable);
        assert_eq!(partner.status_msg.as_deref(), Some("fixture availability"));
        checked(bob.set_presence(State::Offline, None).await, "offline");
        assert!(
            checked(alice.presence(Vec::new()).await, "empty visible set")
                .users
                .is_empty()
        );
    } else {
        assert!(!snapshot.enabled);
        assert!(snapshot.own.is_none());
        assert!(snapshot.users.is_empty());
    }
    assert!(matches!(
        alice
            .presence(vec![bob.client.user_id().unwrap().to_owned(); 65])
            .await,
        Err(PresenceError::InvalidRequest)
    ));
    for account in accounts {
        checked(account.client.matrix_auth().logout().await, "logout");
    }
}
