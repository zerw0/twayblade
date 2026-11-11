use super::*;
use matrix_sdk::{
    Client,
    reqwest::Certificate,
    ruma::{
        api::client::room::create_room,
        events::{
            InitialStateEvent,
            room::{
                history_visibility::{HistoryVisibility, RoomHistoryVisibilityEventContent},
                join_rules::{JoinRule, RoomJoinRulesEventContent},
                message::RoomMessageEventContent,
            },
        },
        serde::Raw,
    },
};
use std::sync::{Arc, Mutex};
fn checked<T, E>(r: Result<T, E>, stage: &str) -> T {
    r.unwrap_or_else(|_| panic!("Directory fixture failed: {stage}"))
}
#[tokio::test]
#[ignore = "requires isolated TLS Synapse"]
async fn public_directory_pagination_filter_links_preview_and_knock() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/local/integration");
    let f: serde_json::Value = checked(
        serde_json::from_slice(&checked(
            tokio::fs::read(root.join("fixture.json")).await,
            "fixture",
        )),
        "JSON",
    );
    assert_eq!(f["homeserver"], "https://127.0.0.1:18448");
    let cert = checked(
        Certificate::from_pem(&checked(tokio::fs::read(root.join("ca.crt")).await, "CA")),
        "certificate",
    );
    let mut accounts = Vec::new();
    for user in ["alice", "bob"] {
        let path = root.join(format!("directory-{}", uuid::Uuid::new_v4()));
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
                .login_username(user, f[user].as_str().unwrap())
                .await,
            "login",
        );
        accounts.push(Account::new(
            client,
            path,
            twayblade_platform::SessionStore::new("twayblade.integration.directory-unused"),
            Arc::new(Mutex::new(())),
        ));
    }
    let creator = &accounts[0];
    let visitor = &accounts[1];
    let mut tasks = Vec::new();
    for a in &accounts {
        let a = a.clone();
        tasks.push(tokio::spawn(async move {
            use futures_util::StreamExt;
            let s = a.room_updates();
            tokio::pin!(s);
            while s.next().await.is_some() {}
        }));
    }
    let label = format!("directory{}", uuid::Uuid::new_v4().simple());
    let mut ids = Vec::new();
    for i in 0..3 {
        let mut r = create_room::v3::Request::new();
        r.name = Some(format!("{label}-{i}"));
        r.room_alias_name = Some(format!("{label}-{i}"));
        r.visibility = ruma::api::client::room::Visibility::Public;
        r.preset = Some(create_room::v3::RoomPreset::PublicChat);
        r.initial_state.push(
            checked(
                Raw::new(&InitialStateEvent::with_empty_state_key(
                    RoomHistoryVisibilityEventContent::new(HistoryVisibility::WorldReadable),
                )),
                "history state",
            )
            .cast(),
        );
        let room = checked(creator.client.create_room(r).await, "create public room");
        checked(
            creator
                .client
                .send(
                    ruma::api::client::directory::set_room_visibility::v3::Request::new(
                        room.room_id().to_owned(),
                        ruma::api::client::room::Visibility::Public,
                    ),
                )
                .await,
            "publish directory room",
        );
        checked(
            room.send(RoomMessageEventContent::text_plain("preview fixture"))
                .await,
            "message",
        );
        ids.push(room.room_id().to_owned());
    }
    let query = DirectoryQuery {
        server: None,
        filter: Some(label.clone()),
        network: None,
        since: None,
        limit: 1,
    };
    let page = checked(
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                let page = checked(visitor.public_rooms(query.clone()).await, "first page");
                if !page.rooms.is_empty() {
                    break page;
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        })
        .await,
        "directory indexing",
    );
    assert_eq!(page.rooms.len(), 1);
    assert!(page.next.is_some());
    assert!(matches!(
        creator
            .public_rooms(DirectoryQuery {
                since: page.next.clone(),
                ..query.clone()
            })
            .await,
        Err(DirectoryError::InvalidQuery)
    ));
    assert!(matches!(
        visitor
            .public_rooms(DirectoryQuery {
                limit: 101,
                ..query.clone()
            })
            .await,
        Err(DirectoryError::InvalidQuery)
    ));
    eprintln!("Directory fixture: first page indexed");
    let page2 = checked(
        visitor
            .public_rooms(DirectoryQuery {
                since: page.next,
                ..query.clone()
            })
            .await,
        "second page",
    );
    assert_eq!(page2.rooms.len(), 1);
    assert_ne!(page.rooms[0].room_id, page2.rooms[0].room_id);
    assert!(
        visitor
            .public_rooms(DirectoryQuery {
                filter: Some(uuid::Uuid::new_v4().to_string()),
                since: None,
                ..query.clone()
            })
            .await
            .unwrap()
            .rooms
            .is_empty()
    );
    assert!(
        visitor
            .public_rooms(DirectoryQuery {
                server: Some(ruma::ServerName::parse("unreachable.invalid").unwrap()),
                ..query
            })
            .await
            .is_err()
    );
    eprintln!("Directory fixture: pagination, filter and remote error passed");
    let target = RoomTarget::parse(ids[0].as_str(), vec![]).unwrap();
    let preview = checked(
        visitor.preview_room(target.clone()).await,
        "world readable preview",
    );
    assert!(preview.room.world_readable);
    assert!(!preview.room.joined);
    assert!(preview.messages.iter().any(|m| m.body == "preview fixture"));
    assert!(preview.messages.len() <= 20);
    assert_eq!(
        checked(
            visitor
                .join_room(
                    RoomTarget::parse(
                        &format!(
                            "#{label}-0:{}",
                            creator.client.user_id().unwrap().server_name()
                        ),
                        vec![]
                    )
                    .unwrap()
                )
                .await,
            "alias join"
        ),
        ids[0].as_str()
    );
    assert_eq!(
        checked(
            visitor
                .join_room(
                    RoomTarget::parse(&format!("https://matrix.to/#/{}", ids[1]), vec![]).unwrap()
                )
                .await,
            "matrix.to join"
        ),
        ids[1].as_str()
    );
    eprintln!("Directory fixture: preview and joins passed");
    let mut r = create_room::v3::Request::new();
    r.initial_state.push(
        checked(
            Raw::new(&InitialStateEvent::with_empty_state_key(
                RoomJoinRulesEventContent::new(JoinRule::Knock),
            )),
            "knock state",
        )
        .cast(),
    );
    let knock = checked(creator.client.create_room(r).await, "create knock room");
    let id = checked(
        visitor
            .knock_room(
                RoomTarget::parse(knock.room_id().as_str(), vec![]).unwrap(),
                Some("fixture".into()),
            )
            .await,
        "knock",
    );
    assert_eq!(id, knock.room_id().as_str());
    assert_eq!(
        visitor.client.get_room(knock.room_id()).unwrap().state(),
        RoomState::Knocked
    );
    for task in tasks {
        task.abort();
    }
    // Requests are explicitly bounded; no external federation acceptance is claimed.
}
