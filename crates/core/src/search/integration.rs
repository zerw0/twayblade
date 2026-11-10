use super::*;
use crate::conversation::ConversationSnapshot;
use futures_util::StreamExt;
use matrix_sdk::{
    Client,
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
use std::{path::Path, sync::Mutex, time::Duration};
fn checked<T, E>(value: Result<T, E>, stage: &str) -> T {
    value.unwrap_or_else(|_| panic!("Search fixture stage failed: {stage}"))
}
async fn wait(
    stream: &mut (impl futures_util::Stream<Item = ConversationSnapshot> + Unpin),
    predicate: impl Fn(&ConversationSnapshot) -> bool,
) -> ConversationSnapshot {
    checked(
        tokio::time::timeout(Duration::from_secs(45), async {
            while let Some(snapshot) = stream.next().await {
                if predicate(&snapshot) {
                    return snapshot;
                }
            }
            panic!("Search context stream ended");
        })
        .await,
        "context readiness",
    )
}
#[tokio::test]
#[ignore = "requires isolated TLS Synapse and freshly generated security account"]
async fn search_server_cached_encryption_pagination_and_event_context() {
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
    let path = root.join(format!("search-{}", uuid::Uuid::new_v4()));
    let client = checked(
        Client::builder()
            .homeserver_url("https://127.0.0.1:18448")
            .add_root_certificates(vec![cert])
            .sqlite_store(&path, Some(&uuid::Uuid::new_v4().to_string()))
            .build()
            .await,
        "client",
    );
    checked(
        client
            .matrix_auth()
            .login_username(
                fixture["security_username"].as_str().unwrap(),
                fixture["security_password"].as_str().unwrap(),
            )
            .await,
        "login",
    );
    let account = Account::new(
        client.clone(),
        path,
        matrix_client_platform::SessionStore::new("matrix-client.integration.search-unused"),
        Arc::new(Mutex::new(())),
    );
    let sync_account = account.clone();
    let sync = tokio::spawn(async move {
        let stream = sync_account.room_updates();
        tokio::pin!(stream);
        while stream.next().await.is_some() {}
    });
    let text = format!("searchacceptance{}", uuid::Uuid::new_v4().simple());
    let mut room_ids = Vec::new();
    for encrypted in [false, true] {
        let mut request = create_room::v3::Request::new();
        if encrypted {
            request.initial_state.push(
                checked(
                    Raw::new(&InitialStateEvent::new(
                        Default::default(),
                        RoomEncryptionEventContent::with_recommended_defaults(),
                    )),
                    "encryption state",
                )
                .cast(),
            );
        }
        let room = checked(client.create_room(request).await, "room");
        let mut updates = Box::pin(account.conversation_updates(room.room_id().to_string(), 1));
        wait(&mut updates, |s| s.conversation.is_some()).await;
        let count = if encrypted { 5 } else { 55 };
        let mut first = None;
        for i in 0..count {
            let response = checked(
                room.send(RoomMessageEventContent::text_plain(format!("{text} {i}")))
                    .await,
                "send search event",
            );
            first.get_or_insert(response.response.event_id);
        }
        wait(&mut updates, |s| {
            s.messages
                .iter()
                .any(|m| m.body == format!("{text} {}", count - 1))
        })
        .await;
        let query = SearchQuery {
            text: text.clone(),
            room: Some(room.room_id().to_owned()),
            sender: Some(client.user_id().unwrap().to_owned()),
            has_file: false,
        };
        let mut next = None;
        let mut ids = std::collections::HashSet::new();
        // Synapse's text index can become visible shortly after sync.
        for _ in 0..30 {
            let page = checked(account.search(query.clone(), next).await, "search page");
            assert_eq!(page.limited, encrypted);
            for item in page.results {
                assert!(ids.insert(item.event_id));
                assert!(item.snippet.contains(&text));
                assert!(!item.match_ranges.is_empty());
            }
            next = page.next;
            if next.is_none() {
                if ids.is_empty() {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
                break;
            }
        }
        assert_eq!(ids.len(), count);
        assert!(next.is_none());
        let first = first.unwrap();
        let mut context =
            Box::pin(account.event_updates(room.room_id().to_string(), first.to_string(), 2));
        let focused = wait(&mut context, |s| {
            s.messages
                .iter()
                .any(|m| m.event_id.as_deref() == Some(first.as_str()))
        })
        .await;
        assert!(focused.messages.len() > 1);
        room_ids.push(room.room_id().to_owned());
    }
    let mut next = None;
    let mut seen = std::collections::HashSet::new();
    for _ in 0..10 {
        let page = checked(
            account
                .search(
                    SearchQuery {
                        text: text.clone(),
                        room: None,
                        sender: None,
                        has_file: false,
                    },
                    next,
                )
                .await,
            "global search",
        );
        assert!(page.limited);
        for result in page.results {
            seen.insert(result.room_id);
        }
        next = page.next;
        if next.is_none() {
            break;
        }
    }
    assert_eq!(seen.len(), 2);
    assert!(room_ids.iter().all(|r| seen.contains(r.as_str())));
    checked(client.matrix_auth().logout().await, "logout");
    sync.abort();
}
