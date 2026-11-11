//! Actual bundled Element Call runtimes; no synthetic membership or key events.
use super::*;
use futures_util::StreamExt;
use matrix_sdk::{
    Client,
    reqwest::Certificate,
    ruma::{
        api::client::room::create_room,
        events::{InitialStateEvent, room::encryption::RoomEncryptionEventContent},
        serde::Raw,
    },
};
use std::sync::{Arc, Mutex};
use tokio::{
    sync::watch,
    task::JoinHandle,
    time::{Duration, timeout},
};
use twayblade_call_host::Failure;

#[derive(Clone, Default)]
struct Evidence {
    joined: bool,
    keys_received: usize,
    widget_messages: usize,
    sdk_messages: usize,
    failed: Option<&'static str>,
}
struct Tasks(Vec<JoinHandle<()>>);
impl Drop for Tasks {
    fn drop(&mut self) {
        for task in &self.0 {
            task.abort();
        }
    }
}
fn checked<T, E>(value: Result<T, E>, stage: &str) -> T {
    value.unwrap_or_else(|_| panic!("Call fixture failed: {stage}"))
}

#[tokio::test]
#[ignore = "requires isolated TLS Synapse/LiveKit and macOS GUI session; real Element Call windows"]
async fn element_call_runtime_signalling() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/local/integration");
    let fixture: serde_json::Value = checked(
        serde_json::from_slice(&checked(
            tokio::fs::read(root.join("fixture.json")).await,
            "fixture",
        )),
        "fixture JSON",
    );
    assert_eq!(fixture["homeserver"], "https://127.0.0.1:18448");
    let certificate = checked(
        Certificate::from_pem(&checked(tokio::fs::read(root.join("ca.crt")).await, "CA")),
        "certificate",
    );
    let mut accounts = Vec::new();
    for role in ["alice", "bob"] {
        let path = root.join(format!("call-{}", uuid::Uuid::new_v4()));
        let client = checked(
            Client::builder()
                .homeserver_url("https://127.0.0.1:18448")
                .add_root_certificates(vec![certificate.clone()])
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
            "isolated login",
        );
        accounts.push(Account::new(
            client,
            path,
            twayblade_platform::SessionStore::new("twayblade.integration.call-unused"),
            Arc::new(Mutex::new(())),
        ));
    }
    let mut tasks = Tasks(Vec::new());
    for account in &accounts {
        let account = account.clone();
        tasks.0.push(tokio::spawn(async move {
            let stream = account.room_updates();
            tokio::pin!(stream);
            while stream.next().await.is_some() {}
        }));
    }
    let mut request = create_room::v3::Request::new();
    request
        .invite
        .push(accounts[1].client.user_id().unwrap().to_owned());
    request.initial_state.push(
        checked(
            Raw::new(&InitialStateEvent::with_empty_state_key(
                RoomEncryptionEventContent::with_recommended_defaults(),
            )),
            "encryption state",
        )
        .cast(),
    );
    let room = checked(
        accounts[0].client.create_room(request).await,
        "encrypted room",
    );
    checked(
        accounts[1].client.join_room_by_id(room.room_id()).await,
        "join room",
    );
    checked(
        timeout(Duration::from_secs(30), async {
            while !accounts
                .iter()
                .all(|a| a.call_room_joined(room.room_id().as_str()))
            {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await,
        "room sync",
    );
    let mut evidence = Vec::new();
    for (i, account) in accounts.iter().enumerate() {
        let mut widget = checked(
            account
                .call_widget(room.room_id().as_str(), i == 1, false)
                .await,
            "SDK widget/foci discovery",
        );
        assert_eq!(widget.jwt_service, "https://127.0.0.1:22449/jwt");
        // Upstream skipLobby defaults both media inputs to muted. No fake
        // devices, camera/microphone access or synthetic key events are used.
        widget.url = widget.url.replace("skipLobby=false", "skipLobby=true");
        let runtime = checked(spawn_runtime(&widget).await, "runtime spawn");
        let (tx, rx) = watch::channel(Evidence::default());
        evidence.push(rx);
        tasks.0.push(tokio::spawn(async move {
            let result = relay(widget, runtime, tx.clone()).await;
            tx.send_modify(|v| v.failed = Some(result));
        }));
    }
    let signalling = timeout(Duration::from_secs(90), async {
        loop {
            for e in &evidence {
                if let Some(failure) = e.borrow().failed {
                    panic!("Call runtime signalling failed: {failure}");
                }
            }
            let mut participants_ok = true;
            for account in &accounts {
                let calls = account.room_calls().await;
                participants_ok &= calls.get(room.room_id().as_str()).is_some_and(|call| {
                    call.participants.len() == 2
                        && accounts
                            .iter()
                            .all(|a| call.participants.iter().any(|p| p.user_id == a.user_id()))
                });
            }
            if participants_ok
                && evidence.iter().all(|e| {
                    let e = e.borrow();
                    e.joined && e.keys_received > 0
                })
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;
    if signalling.is_err() {
        for (side, value) in evidence.iter().enumerate() {
            let value = value.borrow();
            println!(
                "Side {side}: widget_messages={}, sdk_messages={}, joined={}, received_keys={}",
                value.widget_messages, value.sdk_messages, value.joined, value.keys_received
            );
        }
        panic!(
            "Call fixture failed: two participant lists, memberships and received encryption keys"
        );
    }
    println!("Actual Element Call signalling passed; media and media E2EE remain unverified.");
}
async fn relay(
    widget: CallWidget,
    mut runtime: Supervisor,
    evidence: watch::Sender<Evidence>,
) -> &'static str {
    loop {
        tokio::select! {
            _ = &mut runtime.exited => return "runtime exited",
            frame = runtime.receive.recv() => match frame {
                Some(Frame::Ready) => {},
                Some(Frame::ServiceHealth(reachable)) => {
                    println!("WebKit: trusted_tls_service_reachable={reachable}");
                    if !reachable { return "WebKit cannot reach the fixture service with trusted TLS"; }
                },
                Some(Frame::Environment { secure_context, media_devices, script_transform }) => {
                    println!("WebKit: secure_context={secure_context}, media_devices={media_devices}, script_transform={script_transform}");
                    if !secure_context || !media_devices || !script_transform { return "custom origin lacks encrypted media prerequisites"; }
                }
                Some(Frame::Failed(Failure::PermissionDenied)) => return "permission denied",
                Some(Frame::Widget(value)) => {
                    evidence.send_modify(|v| v.widget_messages += 1);
                    if !widget.accepts_message(&value).await { return "widget room policy rejected"; }
                    let action = value["action"].as_str().unwrap_or("");
                    if ["io.element.join", "io.element.device_mute", "io.element.close", "im.vector.hangup"].contains(&action) {
                        if action == "io.element.join" { evidence.send_modify(|v| v.joined = true); }
                        let mut ack = value; ack["response"] = serde_json::json!({});
                        if runtime.send.try_send(Frame::Widget(ack)).is_err() { return "bridge saturated"; }
                    } else if !widget.handle.send(value.to_string()) { return "SDK driver stopped"; }
                }
                _ => return "runtime protocol rejected",
            },
            value = widget.handle.recv() => {
                evidence.send_modify(|v| v.sdk_messages += 1);
                let Some(value) = value else { return "SDK driver stopped"; };
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&value) else { return "SDK envelope rejected"; };
                if !policy::outgoing_room_scoped(&value, widget.room.room_id().as_str()) {
                    let mut ack = value; ack["response"] = serde_json::json!({});
                    if !widget.handle.send(ack.to_string()) { return "SDK driver stopped"; }
                    continue;
                }
                if value["data"]["type"] == "io.element.call.encryption_keys" && value.get("response").is_none() { evidence.send_modify(|v| v.keys_received += 1); }
                if runtime.send.try_send(Frame::Widget(value)).is_err() { return "bridge saturated"; }
            }
        }
    }
}
