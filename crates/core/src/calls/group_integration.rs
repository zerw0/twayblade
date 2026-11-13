//! Three distinct fixture identities; generated media, never physical acceptance.
use super::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

async fn tone(account: &Account, room: &str, tasks: &mut Tasks) -> CallHandle {
    let lease = account.call_slot.clone().try_acquire_owned().unwrap();
    let mut call = checked(
        NativeCall::connect(account, room, lease).await,
        "group native connect",
    );
    let source = NativeAudioSource::new(AudioSourceOptions::default(), 48000, 1, 100);
    checked(
        call.media
            .publish_audio(RtcAudioSource::Native(source.clone()))
            .await,
        "group tone",
    );
    tasks.0.push(tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        let mut sample = 0u64;
        loop {
            tick.tick().await;
            let data = (0..480)
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
    call.drive()
}
fn count(value: &serde_json::Value, key: &str) -> u64 {
    value[key].as_u64().unwrap_or(0)
}
fn frames(call: &CallHandle) -> u64 {
    call.evidence().audio_signal_frames.load(Ordering::Relaxed)
}
fn members_count(call: &CallHandle) -> usize {
    call.updates.borrow().participants.len()
}

#[tokio::test]
#[ignore = "requires isolated Synapse/LiveKit/Element Call 0.28.2 and generated media"]
async fn native_matrixrtc_three_party_join_leave_rotation_and_screen() {
    checked(
        timeout(Duration::from_secs(240), scenario()).await,
        "three-party overall deadline",
    );
}
async fn scenario() {
    let (accounts, mut tasks, room, fixture) = fixture_accounts(&["alice", "bob", "carol"]).await;
    let mut a = tone(&accounts[0], room.room_id().as_str(), &mut tasks).await;
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
        "group Element peer",
    );
    let mut input = peer.stdin.take().unwrap();
    let config = serde_json::json!({"room_id":room.room_id(), "username":fixture["scenario_carol_username"], "password":fixture["scenario_carol_password"]});
    checked(
        input.write_all(format!("{config}\n").as_bytes()).await,
        "group private configuration",
    );
    drop(config);
    let mut lines = BufReader::new(peer.stdout.take().unwrap()).lines();
    let ready = checked(
        timeout(Duration::from_secs(60), lines.next_line()).await,
        "group peer readiness",
    );
    let ready: serde_json::Value = checked(
        serde_json::from_str(&checked(ready, "group readiness line").expect("peer ended")),
        "group readiness JSON",
    );
    assert_eq!(
        ready["ready"], true,
        "Element failed at stage {}",
        ready["stage"]
    );
    assert_ne!(ready["user_id"], accounts[0].user_id().as_str());
    assert_ne!(ready["user_id"], accounts[1].user_id().as_str());
    let mut browser = serde_json::Value::Null;
    checked(timeout(Duration::from_secs(45), async {
        loop {
            tokio::select! {
                result = a.updates.changed() => checked(result, "initial native membership"),
                line = lines.next_line() => {
                    browser = checked(serde_json::from_str(&checked(line, "initial Element evidence").expect("peer ended")), "group evidence JSON");
                    assert_ne!(browser["failed"], true, "Element stage {}", browser["stage"]);
                }
            }
            if members_count(&a) == 2 && frames(&a) > 20 && count(&browser,"audio") > 20 && browser["participants"] == 2 { break; }
        }
    }).await, "native and Element establish an encrypted call before the third joins");
    let mut b = tone(&accounts[1], room.room_id().as_str(), &mut tasks).await;
    let audio_before_join = frames(&a);
    let browser_keys_before_join = count(&browser, "keys");
    checked(timeout(Duration::from_secs(45), async {
        loop {
            tokio::select! {
                r=a.updates.changed()=>checked(r,"Alice group join"),
                r=b.updates.changed()=>checked(r,"Bob group join"),
                line=lines.next_line()=>{
                    browser=checked(serde_json::from_str(&checked(line,"Element group join").expect("peer ended")),"group join JSON");
                    assert_ne!(browser["failed"],true);
                }
            }
            if members_count(&a)==3 && members_count(&b)==3 && browser["participants"]==3
                && frames(&a)>audio_before_join+20 && frames(&b)>20 && a.keys_received()>=2 && b.keys_received()>=2
                && count(&browser,"keys")>browser_keys_before_join && count(&browser,"audioTracks")==2 {break;}
        }
    }).await,"three participant lists, encrypted key exchange and third-party media");
    let expected: std::collections::HashSet<_> = [
        accounts[0].user_id().to_string(),
        accounts[1].user_id().to_string(),
        ready["user_id"].as_str().unwrap().to_owned(),
    ]
    .into_iter()
    .collect();
    for call in [&a, &b] {
        assert_eq!(
            call.updates
                .borrow()
                .participants
                .iter()
                .map(|p| p.user_id.clone())
                .collect::<std::collections::HashSet<_>>(),
            expected
        );
        assert!(
            call.updates
                .borrow()
                .participants
                .iter()
                .all(|p| !p.media_blocked)
        );
    }
    println!(
        "three-party: distinct native/native/Element identities, mid-call third join, memberships and encrypted media passed"
    );
    verify_livekit_participants(vec![
        format!(
            "{}:{}",
            accounts[0].user_id(),
            accounts[0].client.device_id().unwrap()
        ),
        format!(
            "{}:{}",
            accounts[1].user_id(),
            accounts[1].client.device_id().unwrap()
        ),
        format!(
            "{}:{}",
            ready["user_id"].as_str().unwrap(),
            ready["device_id"].as_str().unwrap()
        ),
    ])
    .await;
    publish_generated(&mut a, &mut tasks, true).await;
    checked(timeout(Duration::from_secs(45),async{
        loop {
            tokio::select! {
                r=a.updates.changed()=>checked(r,"local group screen"),
                r=b.updates.changed()=>checked(r,"remote group screen"),
                line=lines.next_line()=>{browser=checked(serde_json::from_str(&checked(line,"Element group screen").expect("peer ended")),"group screen JSON");assert_ne!(browser["failed"],true);}
            }
            let screen=b.updates.borrow().video_tracks.iter().find(|t|t.source==VideoSource::Screen&&!t.local).map(|t|t.id.clone());
            if members_count(&a)==3 && members_count(&b)==3 && browser["participants"]==3
                && screen.and_then(|id|b.video_tracks().get(&id)).is_some_and(|h|h.latest().is_some())
                && b.evidence().video_pattern_frames.load(Ordering::Relaxed)>10 && count(&browser,"video")>10 {break;}
        }
    }).await,"three-member screen share decrypted by native and Element (no cameras published)");
    println!(
        "three-party screen: native source, decoded screen pattern on both remote clients with all three memberships"
    );
    let indices = [a.sender_key_index().await, b.sender_key_index().await];
    let received = [a.keys_received(), b.keys_received()];
    checked(
        input.write_all(b"{\"leave\":true}\n").await,
        "Element graceful leave",
    );
    checked(
        timeout(Duration::from_secs(30), async {
            loop {
                let value: serde_json::Value = checked(
                    serde_json::from_str(
                        &checked(lines.next_line().await, "Element leave result")
                            .expect("peer ended without leaving"),
                    ),
                    "leave JSON",
                );
                assert_ne!(
                    value["failed"], true,
                    "Element leave stage {}",
                    value["stage"]
                );
                if value["left"] == true {
                    break;
                }
            }
        })
        .await,
        "Element publishes departure before closing",
    );
    let status = checked(
        timeout(Duration::from_secs(15), peer.wait()).await,
        "Element group exit",
    )
    .unwrap();
    assert!(
        status.success(),
        "Element peer exits successfully after leaving"
    );
    checked(
        timeout(Duration::from_secs(45), async {
            loop {
                if members_count(&a) == 2
                    && members_count(&b) == 2
                    && a.sender_key_index().await != indices[0]
                    && b.sender_key_index().await != indices[1]
                    && a.keys_received() > received[0]
                    && b.keys_received() > received[1]
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await,
        "both remaining native senders rotate and exchange keys after Element departure",
    );
    // Count only frames after the five-second sender key-switch grace period.
    tokio::time::sleep(Duration::from_secs(7)).await;
    let audio = [frames(&a), frames(&b)];
    let video = b.evidence().video_pattern_frames.load(Ordering::Relaxed);
    checked(
        timeout(Duration::from_secs(20), async {
            loop {
                assert_eq!(members_count(&a), 2);
                assert_eq!(members_count(&b), 2);
                if frames(&a) > audio[0] + 30
                    && frames(&b) > audio[1] + 30
                    && b.evidence().video_pattern_frames.load(Ordering::Relaxed) > video + 10
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await,
        "decrypted audio and screen continue using post-departure keys",
    );
    let before = a.sender_key_index().await;
    checked(b.leave().await, "native mid-call leave");
    checked(
        timeout(Duration::from_secs(30), async {
            loop {
                if members_count(&a) == 1 && a.sender_key_index().await != before {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await,
        "native departure removes membership and rotates the last sender again",
    );
    checked(a.leave().await, "last group member leaves");
    println!(
        "three-party departure: Element and native mid-call leaves, native sender rotations and post-switch encrypted audio/screen passed"
    );
}
