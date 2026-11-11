//! Isolated native media evidence. Explicit generated tone/pattern, no hardware.
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
use std::{
    borrow::Cow,
    sync::{Arc, Mutex, atomic::Ordering},
};
use tokio::{
    task::JoinHandle,
    time::{Duration, timeout},
};
use twayblade_media::livekit::webrtc::{
    audio_frame::AudioFrame,
    audio_source::{AudioSourceOptions, RtcAudioSource, native::NativeAudioSource},
};
struct Tasks(Vec<JoinHandle<()>>);
impl Drop for Tasks {
    fn drop(&mut self) {
        for task in &self.0 {
            task.abort();
        }
    }
}
fn checked<T, E>(value: Result<T, E>, stage: &str) -> T {
    value.unwrap_or_else(|_| panic!("Native call fixture failed: {stage}"))
}
#[tokio::test]
#[ignore = "requires isolated TLS Synapse and LiveKit; generated audio, no physical devices"]
async fn native_matrixrtc_audio_and_video() {
    let (accounts, mut tasks, room, _fixture) = fixture_pair().await;
    let mut calls = Vec::new();
    for account in &accounts {
        let lease = account.call_slot.clone().try_acquire_owned().unwrap();
        let call = checked(
            timeout(
                Duration::from_secs(30),
                NativeCall::connect(account, room.room_id().as_str(), lease),
            )
            .await,
            "native connection timeout",
        );
        let mut call = call.unwrap_or_else(|e| panic!("native SFU connection: {e:?}"));
        let source = NativeAudioSource::new(AudioSourceOptions::default(), 48000, 1, 100);
        checked(
            call.media
                .publish_audio(RtcAudioSource::Native(source.clone()))
                .await,
            "publish encrypted tone",
        );
        tasks.0.push(tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(10));
            let mut sample = 0u64;
            loop {
                tick.tick().await;
                let data: Vec<i16> = (0..480)
                    .map(|_| {
                        let value = (8000.0
                            * (sample as f64 * 440.0 * std::f64::consts::TAU / 48000.0).sin())
                            as i16;
                        sample += 1;
                        value
                    })
                    .collect();
                if source
                    .capture_frame(&AudioFrame {
                        data: Cow::Owned(data),
                        sample_rate: 48000,
                        num_channels: 1,
                        samples_per_channel: 480,
                    })
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }));
        assert!(call.media.room.e2ee_manager().enabled());
        calls.push(call.drive());
    }
    let mut call_b = calls.pop().unwrap();
    let mut call_a = calls.pop().unwrap();
    checked(
        timeout(Duration::from_secs(90), async {
            loop {
                tokio::select! {
                    value=call_a.updates.changed()=>checked(value,"Alice native update"),
                    value=call_b.updates.changed()=>checked(value,"Bob native update"),
                }
                if [&call_a, &call_b].iter().all(|call| {
                    call.updates.borrow().participants.len() == 2
                        && call.keys_received() > 0
                        && call.evidence().audio_signal_frames.load(Ordering::Relaxed) > 20
                }) {
                    break;
                }
            }
        })
        .await,
        "two memberships, participant lists, bidirectional keys and decrypted audio",
    );
    verify_livekit_pair(
        accounts
            .iter()
            .map(|a| format!("{}:{}", a.user_id(), a.client.device_id().unwrap()))
            .collect(),
    )
    .await;
    println!(
        "native-native audio: memberships=2, lists=2, encrypted key exchange=bidirectional, decrypted signal frames=both"
    );
    for call in [&mut call_a, &mut call_b] {
        publish_pattern(call, &mut tasks).await;
    }
    checked(timeout(Duration::from_secs(45),async{
        loop{
            tokio::select!{r=call_a.updates.changed()=>checked(r,"Alice video update"),r=call_b.updates.changed()=>checked(r,"Bob video update")}
            if [&call_a,&call_b].iter().all(|c|c.evidence().video_pattern_frames.load(Ordering::Relaxed)>10){break;}
        }
    }).await,"decrypted video test pattern in both native clients");
    println!("native-native video: decrypted test pattern frames=both");
    assert!(
        matches!(
            NativeCall::start(&accounts[0], room.room_id().as_str(), false).await,
            Err(CallError::AlreadyActive)
        ),
        "one call per account is enforced before any permission prompt"
    );
    assert!(
        matches!(
            accounts[0]
                .prepare_native_call(room.room_id().as_str(), false)
                .await,
            Err(CallError::MediaE2eeRequired)
        ),
        "encrypted room cannot disable media E2EE"
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(75);
    checked(timeout(Duration::from_secs(90),async{
        loop{
            tokio::select!{r=call_a.updates.changed()=>checked(r,"Alice sustained call"),r=call_b.updates.changed()=>checked(r,"Bob sustained call"),_=tokio::time::sleep_until(deadline)=>break}
            for call in [&call_a,&call_b]{assert!(!matches!(call.updates.borrow().phase,CallPhase::Ended(_)),"membership/media refresh must preserve the call");assert_eq!(call.updates.borrow().participants.len(),2);}
        }
    }).await,"sustained call across expiry refresh and delayed-event renewal");
    verify_livekit_pair(
        accounts
            .iter()
            .map(|a| format!("{}:{}", a.user_id(), a.client.device_id().unwrap()))
            .collect(),
    )
    .await;
    println!("native-native sustained call: membership refresh and delayed leave remained active");

    // Bob leaves the MatrixRTC call but keeps his SFU connection and keeps
    // publishing. Alice must stop receiving him: no subscription, no frames.
    let bob = &accounts[1];
    let bob_room = bob.client.get_room(room.room_id()).expect("Bob's room");
    let bob_key = super::native::state_key(
        bob.client.user_id().unwrap().to_owned(),
        bob.client.device_id().unwrap().as_str(),
        bob_room
            .version()
            .as_ref()
            .map(|v| v.as_str())
            .unwrap_or("10"),
    );
    checked(
        bob_room
            .send_state_event_raw(
                super::native::MEMBER_EVENT,
                bob_key.as_ref(),
                serde_json::json!({}),
            )
            .await,
        "Bob removes his call membership only",
    );
    checked(
        timeout(Duration::from_secs(30), async {
            while call_a.updates.borrow().participants.len() != 1 {
                checked(call_a.updates.changed().await, "Alice sees Bob depart");
            }
        })
        .await,
        "Alice drops the departed member",
    );
    // Allow in-flight frames to drain, then require silence from Bob.
    tokio::time::sleep(Duration::from_secs(2)).await;
    let audio = call_a.evidence().audio_frames.load(Ordering::Relaxed);
    let video = call_a.evidence().video_frames.load(Ordering::Relaxed);
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert_eq!(
        call_a.evidence().audio_frames.load(Ordering::Relaxed),
        audio,
        "no audio from a member who left the call"
    );
    assert_eq!(
        call_a.evidence().video_frames.load(Ordering::Relaxed),
        video,
        "no video from a member who left the call"
    );
    assert!(
        call_a
            .video_tracks()
            .ids()
            .iter()
            .all(|id| id.starts_with("local:")),
        "the departed member's video slots are cleared"
    );
    println!("native-native departure: departed member unsubscribed, no frames received");

    checked(call_a.leave().await, "Alice leave");
    checked(call_b.leave().await, "Bob leave");
}

async fn fixture_pair() -> (Vec<Account>, Tasks, matrix_sdk::Room, serde_json::Value) {
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
        accounts.push(
            Account::new(
                client,
                path,
                twayblade_platform::SessionStore::new("twayblade.integration.call-unused"),
                Arc::new(Mutex::new(())),
            )
            .with_certificate(Some(certificate.clone())),
        );
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
    // Reused from feature/call-signalling: ordinary members may publish only
    // RTC membership; retain all other default power levels.
    request.power_level_content_override = Some(
        Raw::from_json_string(
            serde_json::json!({"events":{"org.matrix.msc3401.call.member":0}}).to_string(),
        )
        .unwrap(),
    );
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

    (accounts, tasks, room, fixture)
}

async fn publish_pattern(call: &mut CallHandle, tasks: &mut Tasks) {
    use twayblade_media::livekit::webrtc::{
        video_frame::{I420Buffer, VideoFrame, VideoRotation},
        video_source::{RtcVideoSource, VideoResolution, native::NativeVideoSource},
    };
    let source = NativeVideoSource::new(
        VideoResolution {
            width: 640,
            height: 360,
        },
        false,
    );
    checked(
        call.publish_video(RtcVideoSource::Native(source.clone()))
            .await,
        "publish encrypted test pattern",
    );
    tasks.0.push(tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(100));
        let clock = std::time::Instant::now();
        loop {
            tick.tick().await;
            let mut buffer = I420Buffer::new(640, 360);
            let (stride, _, _) = buffer.strides();
            let (y, u, v) = buffer.data_mut();
            for row in 0..360usize {
                for column in 0..640usize {
                    y[row * stride as usize + column] = [30, 128, 220][(column * 3 / 640).min(2)];
                }
            }
            u.fill(128);
            v.fill(128);
            let mut frame = VideoFrame::new(VideoRotation::VideoRotation0, buffer);
            frame.timestamp_us = clock.elapsed().as_micros() as i64;
            source.capture_frame(&frame);
        }
    }));
}

#[tokio::test]
#[ignore = "requires isolated Element Call/LiveKit and pinned headless Chromium; generated media"]
async fn native_matrixrtc_element_audio_and_video() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let (accounts, mut tasks, room, fixture) = fixture_pair().await;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut peer = checked(
        tokio::process::Command::new("node")
            .arg(root.join("tests/integration/browser/peer.mjs"))
            .env("PLAYWRIGHT_BROWSERS_PATH", root.join("target/playwright"))
            .current_dir(&root)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn(),
        "headless Element Call peer",
    );
    let mut input = peer.stdin.take().unwrap();
    let config = serde_json::json!({"room_id":room.room_id(),"username":fixture["scenario_bob_username"],"password":fixture["scenario_bob_password"]});
    checked(
        input.write_all(format!("{config}\n").as_bytes()).await,
        "peer private configuration",
    );
    drop(config);
    let mut lines = BufReader::new(peer.stdout.take().unwrap()).lines();
    let ready = checked(
        timeout(Duration::from_secs(90), lines.next_line()).await,
        "Element Call readiness deadline",
    );
    let ready: serde_json::Value = checked(
        serde_json::from_str(&checked(ready, "peer readiness").expect("peer exited before ready")),
        "peer counts protocol",
    );
    assert_eq!(ready["ready"], true, "Element Call failed before joining");
    let lease = accounts[0].call_slot.clone().try_acquire_owned().unwrap();
    let mut call = checked(
        NativeCall::connect(&accounts[0], room.room_id().as_str(), lease).await,
        "native Element interop connection",
    );
    let source = NativeAudioSource::new(AudioSourceOptions::default(), 48000, 1, 100);
    checked(
        call.media
            .publish_audio(RtcAudioSource::Native(source.clone()))
            .await,
        "native interop tone",
    );
    tasks.0.push(tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        let mut sample = 0u64;
        loop {
            tick.tick().await;
            let data = (0..480)
                .map(|_| {
                    let v = (8000.0
                        * (sample as f64 * 440.0 * std::f64::consts::TAU / 48000.0).sin())
                        as i16;
                    sample += 1;
                    v
                })
                .collect::<Vec<_>>();
            if source
                .capture_frame(&AudioFrame {
                    data: Cow::Owned(data),
                    sample_rate: 48000,
                    num_channels: 1,
                    samples_per_channel: 480,
                })
                .await
                .is_err()
            {
                break;
            }
        }
    }));
    let mut call = call.drive();
    let mut browser = serde_json::Value::Null;
    let mut report = std::time::Instant::now();
    let audio_result=timeout(Duration::from_secs(90),async{
        loop{
            tokio::select!{
                result=call.updates.changed()=>checked(result,"native Element audio update"),
                line=lines.next_line()=>{browser=checked(serde_json::from_str(&checked(line,"browser output").expect("peer exited")),"peer evidence");assert_ne!(browser["failed"],true,"Element Call peer failed");}
            }
            if report.elapsed()>Duration::from_secs(5){
                eprintln!("Interop progress: participants={}, keys={}, signal={}, browser={}",call.updates.borrow().participants.len(),call.keys_received(),call.evidence().audio_signal_frames.load(Ordering::Relaxed),browser);
                let rtc=accounts[0].client.get_room(room.room_id()).unwrap();
                let raw=rtc.get_state_events_static::<matrix_sdk::ruma::events::call::member::CallMemberEventContent>().await.unwrap();
                let nonempty=raw.iter().filter_map(|r|r.deserialize().ok()).filter(|e|e.original_content().is_some_and(|c|!c.memberships().is_empty())).count();
                eprintln!("Membership diagnostic: state_events={}, nonempty={}, sdk_participants={}",raw.len(),nonempty,rtc.active_room_call_participants().len());
                report=std::time::Instant::now();
            }
            if browser["audio"].as_u64().unwrap_or(0)>10 && browser["keys"].as_u64().unwrap_or(0)>0 && browser["participants"]==2 && call.keys_received()>0 && call.updates.borrow().participants.len()==2 && call.evidence().audio_signal_frames.load(Ordering::Relaxed)>20 {break;}
        }
    }).await;
    if audio_result.is_err() {
        eprintln!(
            "Interop counts: native_participants={}, native_keys={}, native_signal={}, browser_participants={}, browser_keys={}, browser_audio={}",
            call.updates.borrow().participants.len(),
            call.keys_received(),
            call.evidence().audio_signal_frames.load(Ordering::Relaxed),
            browser["participants"],
            browser["keys"],
            browser["audio"]
        );
    }
    checked(
        audio_result,
        "Element/native bidirectional decrypted audio, key exchange and participants",
    );
    verify_livekit_pair(vec![
        format!(
            "{}:{}",
            accounts[0].user_id(),
            accounts[0].client.device_id().unwrap()
        ),
        format!(
            "{}:{}",
            ready["user_id"].as_str().unwrap(),
            ready["device_id"].as_str().unwrap()
        ),
    ])
    .await;
    println!(
        "native-Element Call audio: two participant lists, bidirectional encrypted keys and decrypted signal frames passed"
    );
    checked(
        input.write_all(b"{\"video\":true}\n").await,
        "enable browser video",
    );
    publish_pattern(&mut call, &mut tasks).await;
    checked(timeout(Duration::from_secs(60),async{
        loop{
            tokio::select!{result=call.updates.changed()=>checked(result,"native Element video update"),line=lines.next_line()=>{browser=checked(serde_json::from_str(&checked(line,"browser output").expect("peer exited")),"peer video evidence");assert_ne!(browser["failed"],true);}}
            if browser["video"].as_u64().unwrap_or(0)>10 && call.evidence().video_pattern_frames.load(Ordering::Relaxed)>10 {break;}
        }
    }).await,"Element/native bidirectional decrypted video");
    println!("native-Element Call video: remote decrypted video frames passed on both sides");
    checked(input.write_all(b"{\"stop\":true}\n").await, "peer stop");
    checked(call.leave().await, "native leave");
    checked(
        timeout(Duration::from_secs(10), peer.wait()).await,
        "browser exit deadline",
    )
    .expect("browser wait");
}

async fn verify_livekit_pair(identities: Vec<String>) {
    use tokio::io::AsyncWriteExt;
    let mut child = checked(
        tokio::process::Command::new("python3")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/integration/livekit-evidence.py"),
            )
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .spawn(),
        "LiveKit server evidence process",
    );
    let mut input = child.stdin.take().unwrap();
    checked(
        input
            .write_all(serde_json::to_string(&identities).unwrap().as_bytes())
            .await,
        "LiveKit identities private input",
    );
    drop(input);
    let output = checked(child.wait_with_output().await, "LiveKit server evidence");
    assert!(
        output.status.success(),
        "LiveKit server must contain exactly both expected participants"
    );
    let counts: serde_json::Value = checked(
        serde_json::from_slice(&output.stdout),
        "LiveKit evidence counts",
    );
    assert_eq!(counts["livekit_participants"], 2);
}
