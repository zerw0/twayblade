//! Native call views: the header call buttons, the call stage above the
//! timeline, the in-another-room strip, the incoming-call card and device
//! notices. Call state comes from the backend snapshot; video from bounded
//! latest-frame handles, each drawn by its own small view so only it repaints.
use super::call_layout::{self as layout, CONTROLS_IDLE, CallFocus};
use super::*;
use crate::shortcuts::{ShortcutAction as Action, hint};
use std::sync::Arc;
use twayblade_core::calls::{
    CallError, CallEvent, CallKind, CallPhase, CallState, CallVideoTrack, DeviceKind, EndReason,
    IncomingCall, LatestFrame, Participant, ScreenShare, VideoSource,
    ringing::{CallIntent, NotifyType},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CallShortcut {
    Mute,
    Camera,
    Screen,
    Leave,
    Answer,
    Decline,
}

/// Height kept free under the tiles in full screen for the floating controls.
const CONTROLS_ROOM: f32 = 84.;
/// More than this many people share one "+N" tile.
const MAX_TILES: usize = 8;

/// One video track's latest frame.
/// Cameras fill their tile; shared screens fit inside it uncropped.
pub(crate) struct VideoTile {
    frame: LatestFrame,
    image: Option<(i64, Arc<RenderImage>)>,
    /// Fit inside the tile (screens) instead of filling it (cameras).
    contain: bool,
}
impl VideoTile {
    pub(crate) fn new(frame: LatestFrame, contain: bool, cx: &mut Context<Self>) -> Self {
        cx.on_release(|this: &mut Self, cx| {
            if let Some((_, image)) = this.image.take() {
                cx.drop_image(image, None);
            }
        })
        .detach();
        Self {
            frame,
            image: None,
            contain,
        }
    }
}
impl Render for VideoTile {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        if let Some(frame) = self.frame.latest()
            && self
                .image
                .as_ref()
                .is_none_or(|(at, _)| *at != frame.timestamp_us)
            // Frames are BGRA, which is what GPUI uploads.
            && let Some(buffer) =
                image::RgbaImage::from_raw(frame.width, frame.height, frame.bgra.clone())
        {
            let next = Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]));
            if let Some((_, old)) = self.image.replace((frame.timestamp_us, next)) {
                let _ = window.drop_image(old);
            }
        }
        // Look for the next frame on every display refresh while shown.
        window.request_animation_frame();
        div()
            .size_full()
            .children(self.image.as_ref().map(|(_, image)| {
                img(image.clone()).size_full().object_fit(if self.contain {
                    ObjectFit::Contain
                } else {
                    ObjectFit::Cover
                })
            }))
    }
}

/// A chooser still uploaded for GPUI, keyed by its source and frame.
pub(crate) struct ScreenStill {
    selection: twayblade_core::calls::ScreenSelection,
    frame: Arc<[u8]>,
    image: Arc<RenderImage>,
}

fn live(call: &CallState) -> bool {
    !matches!(call.phase, CallPhase::Ended(_))
}

/// User wording for call failures; core returns only typed errors.
pub(crate) fn call_error_text(error: CallError) -> &'static str {
    match error {
        // Where the OS has no picker, Share opens our own chooser first, so this is
        // only a fallback if a share starts without a choice.
        CallError::ScreenCapture(twayblade_core::calls::ScreenError::SelectionRequired) => {
            "Choose a screen or window before sharing."
        }
        CallError::ScreenCapture(_) => "Screen sharing couldn't start or was stopped.",
        CallError::NoFoci | CallError::UnsupportedServer => "Your server doesn't support calls.",
        CallError::MediaE2eeRequired => "The call couldn't be encrypted end to end.",
        CallError::KeyRejected => "Encryption keys couldn't be exchanged.",
        CallError::TransportFailed => "The connection was lost.",
        CallError::MembershipForbidden => "You don't have permission to call in this room.",
        // Windows enforces its privacy switches when the device opens, so a
        // blocked microphone or camera looks unavailable rather than denied.
        CallError::DeviceUnavailable if cfg!(target_os = "windows") => {
            "Your microphone or camera isn't available. Check that it's connected and that desktop apps may use it in Settings › Privacy & security."
        }
        CallError::DeviceUnavailable => "Your microphone or camera isn't available.",
        CallError::InvalidVolume => "The playback volume is invalid.",
        CallError::ScreenUnavailable => "Screen sharing couldn't start.",
        // Only macOS asks for a per-app grant; elsewhere this is a fallback.
        CallError::PermissionDenied if cfg!(target_os = "macos") => {
            "Twayblade can't use your microphone or camera. Allow it in System Settings › Privacy & Security."
        }
        CallError::PermissionDenied if cfg!(target_os = "windows") => {
            "Twayblade can't use your microphone or camera. Allow desktop apps to use them in Settings › Privacy & security."
        }
        CallError::PermissionDenied => {
            "Twayblade can't use your microphone or camera. Check your system's privacy settings."
        }
        CallError::PermissionRestricted => {
            "Microphone and camera access is restricted on this computer."
        }
        CallError::AlreadyActive => "You're already in a call.",
        CallError::InvalidRoom => "You can't call in this room.",
        CallError::UnencryptedRoom => "Calls need an encrypted room.",
        CallError::ReconnectTimeout => "Couldn't reconnect, so the call ended.",
        CallError::SessionClosed | CallError::StaleRequest | CallError::ControlUnavailable => {
            "The call has ended."
        }
    }
}

fn device_word(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Input => "microphone",
        DeviceKind::Output => "speaker",
        DeviceKind::Camera => "camera",
    }
}

/// Whether you muted this person for yourself.
fn playback_muted(participant: &Participant) -> bool {
    participant.playback.muted
}

/// Your playback volume for this person, as a whole percent.
fn volume_percent(participant: &Participant) -> i32 {
    (participant.playback.volume() * 100.).round() as i32
}

fn duration_text(started_ms: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default();
    let secs = now.saturating_sub(started_ms) / 1000;
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
    } else {
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}

/// What the call stage shows large.
enum Main<'a> {
    Grid,
    /// A person; true when the follow-the-speaker mode picked them.
    Person(&'a Participant, bool),
    /// Someone's shared screen; true when it was focused (nobody else shown).
    Screen(&'a CallVideoTrack, bool),
}

impl MatrixApp {
    fn person_name(&self, participant: &Participant) -> String {
        if self.is_local(participant) {
            "you".into()
        } else {
            participant
                .display_name
                .clone()
                .unwrap_or_else(|| participant.user_id.clone())
        }
    }

    pub(crate) fn live_call(&self) -> Option<&CallState> {
        self.snapshot.call.as_ref().filter(|call| live(call))
    }

    fn room_name(&self, id: &str) -> String {
        self.snapshot
            .rooms
            .iter()
            .find(|room| room.id == id)
            .map(|room| room.name.clone())
            .unwrap_or_else(|| "this room".into())
    }

    fn call_result(&mut self, result: Result<(), CallError>, cx: &mut Context<Self>) {
        if let Err(error) = result {
            self.status = call_error_text(error).into();
        }
        cx.notify();
    }

    /// Full screen for the call: the call fills the whole window, in the macOS
    /// full-screen space (or the platform's equivalent). Only while a call is live.
    pub(crate) fn toggle_call_full_screen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.live_call().is_none() {
            return;
        }
        if self.call_full_screen || window.is_fullscreen() {
            self.call_full_screen = false;
            self.call_full_screen_entered = false;
            if window.is_fullscreen() {
                window.toggle_fullscreen();
            }
        } else {
            self.call_full_screen = true;
            self.call_full_screen_entered = false;
            self.call_controls.activity(std::time::Instant::now(), true);
            window.toggle_fullscreen();
        }
        cx.notify();
    }

    /// Whether the call is filling the window right now.
    pub(crate) fn call_fills_window(&self) -> bool {
        self.call_full_screen && self.live_call().is_some()
    }

    fn fresh_call(&mut self) {
        self.call_end_dismissed = false;
        self.call_focus = None;
        self.call_follow.reset();
        self.call_follow_timer = None;
        self.screen_picker = None;
        self.call_volume.clear();
        self.call_menu = None;
        self.call_event_seen = 0;
        self.status.clear();
    }

    pub(crate) fn begin_call(&mut self, room: String, video: bool, cx: &mut Context<Self>) {
        self.fresh_call();
        let result = self.start_call(room, video);
        self.call_result(result, cx);
    }

    fn join_room_call(&mut self, room: String, cx: &mut Context<Self>) {
        self.fresh_call();
        let result = self.join_call(room);
        self.call_result(result, cx);
    }

    fn answer(&mut self, room: String, video: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.fresh_call();
        if self.navigation.selected_room.as_deref() != Some(&room) {
            self.select_room(room.clone(), window, cx);
        }
        let result = self.accept_call(room, video);
        self.call_result(result, cx);
    }

    /// Keeps one video view per shown track, the speaker-follow pick, the
    /// volume sliders and a 1 s clock while in a call.
    pub(crate) fn sync_call_views(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.call_stage_size.set(window.viewport_size());
        let tracks: Vec<(String, VideoSource)> = self
            .live_call()
            .map(|call| {
                call.video_tracks
                    .iter()
                    .map(|t| (t.id.clone(), t.source))
                    .collect()
            })
            .unwrap_or_default();
        self.call_tiles
            .retain(|id, _| tracks.iter().any(|(track, _)| track == id));
        for (id, source) in tracks {
            if !self.call_tiles.contains_key(&id)
                && let Some(frame) = self.call_video_handle(&id)
            {
                let contain = source == VideoSource::Screen;
                let tile = cx.new(|cx| VideoTile::new(frame, contain, cx));
                self.call_tiles.insert(id, tile);
            }
        }
        if self
            .call_screen_focus
            .as_ref()
            .is_some_and(|id| !self.call_tiles.contains_key(id))
        {
            self.call_screen_focus = None;
        }
        // A focus on someone who left, or a screen that stopped, is released.
        let gone = match &self.call_focus {
            Some(CallFocus::Screen(id)) => !self.call_tiles.contains_key(id),
            Some(CallFocus::Person(user, device)) => !self.live_call().is_some_and(|call| {
                call.participants
                    .iter()
                    .any(|p| &p.user_id == user && &p.device_id == device)
            }),
            None => false,
        };
        if gone {
            self.call_focus = None;
        }
        if self.screen_picker.is_some()
            && !self
                .live_call()
                .is_some_and(|call| call.local.screen == ScreenShare::Off)
        {
            self.screen_picker = None;
        }
        self.sync_full_screen(window);
        self.sync_speaker_follow(cx);
        self.sync_volume_sliders(cx);
        if self.live_call().is_none() {
            self.call_clock = None;
        } else if self.call_clock.is_none() {
            self.call_clock = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(1))
                        .await;
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        break;
                    }
                }
            }));
        }
    }

    /// The call fills the window only while the window really is full screen
    /// and the call is live; it follows the window out of full screen.
    fn sync_full_screen(&mut self, window: &mut Window) {
        if !self.call_full_screen {
            return;
        }
        if self.live_call().is_none() {
            self.call_full_screen = false;
            self.call_full_screen_entered = false;
            if window.is_fullscreen() {
                window.toggle_fullscreen();
            }
        } else if window.is_fullscreen() {
            self.call_full_screen_entered = true;
        } else if self.call_full_screen_entered {
            self.call_full_screen = false;
            self.call_full_screen_entered = false;
        }
    }

    /// Feeds voice activity to the debouncer and wakes the view when the pick
    /// may change.
    fn sync_speaker_follow(&mut self, cx: &mut Context<Self>) {
        if !self.preferences.calls.follow_speaker || self.live_call().is_none() {
            self.call_follow.reset();
            self.call_follow_timer = None;
            return;
        }
        let now = std::time::Instant::now();
        let people: Vec<(String, bool)> = self
            .live_call()
            .map(|call| {
                call.participants
                    .iter()
                    .filter(|p| !self.is_local(p))
                    .map(|p| {
                        (
                            p.id.clone(),
                            p.speaking == Some(true) && p.muted != Some(true),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let flags: Vec<(&str, bool)> = people.iter().map(|(id, on)| (id.as_str(), *on)).collect();
        self.call_follow.update(now, &flags);
        if self.call_follow_timer.is_none()
            && let Some(at) = self.call_follow.next_deadline(now)
        {
            self.call_follow_timer = Some(cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(at.saturating_duration_since(std::time::Instant::now()))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    this.call_follow_timer = None;
                    cx.notify();
                });
            }));
        }
    }

    /// One 0–200% slider per other participant, kept for the call.
    fn sync_volume_sliders(&mut self, cx: &mut Context<Self>) {
        use gpui_kit::component::slider::{SliderEvent, SliderState};
        let remote: Vec<(String, f32)> = self
            .live_call()
            .map(|call| {
                call.participants
                    .iter()
                    .filter(|p| !self.is_local(p))
                    .map(|p| (p.id.clone(), p.playback.volume()))
                    .collect()
            })
            .unwrap_or_default();
        self.call_volume
            .retain(|id, _| remote.iter().any(|(person, _)| person == id));
        for (id, volume) in remote {
            if self.call_volume.contains_key(&id) {
                continue;
            }
            let state = cx.new(|_| {
                SliderState::new()
                    .min(0.)
                    .max(200.)
                    .step(5.)
                    .default_value(volume * 100.)
            });
            let target = id.clone();
            let subscription = cx.subscribe(
                &state,
                move |this: &mut Self, _, event: &SliderEvent, _| match event {
                    SliderEvent::Change(value) => this.send_volume(&target, value.end(), false),
                    SliderEvent::Release(value) => this.send_volume(&target, value.end(), true),
                },
            );
            self.call_volume.insert(id, (state, subscription));
        }
    }

    /// Sends a playback volume (percent). While dragging, sends are spaced out so
    /// the bounded control queue never fills; the release always sends.
    pub(crate) fn send_volume(&mut self, id: &str, percent: f32, force: bool) {
        let now = std::time::Instant::now();
        if !force
            && self
                .call_volume_sent
                .is_some_and(|at| now.duration_since(at) < std::time::Duration::from_millis(60))
        {
            return;
        }
        self.call_volume_sent = Some(now);
        let _ = self.set_participant_volume(id.to_owned(), percent / 100.);
    }

    /// Start/join buttons for the conversation header.
    pub(crate) fn call_buttons(
        &self,
        room_id: &str,
        encrypted: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let current = self.live_call();
        let summary = self
            .snapshot
            .call
            .as_ref()
            .is_some_and(|call| call.room_id == room_id && !self.call_end_dismissed);
        if summary || current.is_some_and(|call| call.room_id == room_id) {
            return None;
        }
        let busy = current.is_some();
        let voice_channel = self.is_voice_channel(room_id);
        let joined = self
            .snapshot
            .room_calls
            .get(room_id)
            .map_or(0, |call| call.participants.len());
        if (joined > 0 || voice_channel) && !busy {
            let room = room_id.to_owned();
            let label = if voice_channel {
                format!("Join voice · {joined}")
            } else {
                format!("Join call · {joined}")
            };
            return Some(
                Button::new("call-join")
                    .disabled(!encrypted || self.preview)
                    .primary()
                    .h(px(24.))
                    .px(px(8.))
                    .accessibility_label(label.clone())
                    .tooltip(if joined == 1 {
                        "1 person is in the call".to_owned()
                    } else {
                        format!("{joined} people are in the call")
                    })
                    .child(
                        h_flex()
                            .gap(px(5.))
                            .text_size(px(12.))
                            .child(Icon::new(IconName::Phone).size(px(13.)))
                            .child(label),
                    )
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.join_room_call(room.clone(), cx)),
                    )
                    .into_any_element(),
            );
        }
        let why = if !encrypted {
            Some("Calls need an encrypted room")
        } else if busy {
            Some("Leave your current call first")
        } else {
            None
        };
        let button = |id: &'static str, icon: IconName, label: &'static str, video: bool| {
            let room = room_id.to_owned();
            Button::new(id)
                .ghost()
                .size(px(28.))
                .icon(Icon::new(icon).size(px(15.)))
                .accessibility_label(label)
                .tooltip(why.unwrap_or(label))
                .disabled(why.is_some() || self.preview)
                .on_click(
                    cx.listener(move |this, _, _, cx| this.begin_call(room.clone(), video, cx)),
                )
        };
        Some(
            h_flex()
                .gap(px(2.))
                .child(button(
                    "call-voice",
                    IconName::Phone,
                    "Start voice call",
                    false,
                ))
                .child(button(
                    "call-video",
                    IconName::Video,
                    "Start video call",
                    true,
                ))
                .into_any_element(),
        )
    }

    /// The call in the shown room, or a strip pointing at a call elsewhere.
    pub(crate) fn call_banner(&self, room_id: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let call = self.snapshot.call.as_ref()?;
        if call.room_id == room_id {
            return (live(call) || !self.call_end_dismissed).then(|| self.call_stage(call, cx));
        }
        live(call).then(|| self.call_elsewhere(call, cx))
    }

    fn call_elsewhere(&self, call: &CallState, cx: &mut Context<Self>) -> AnyElement {
        let palette = theme::palette(cx);
        let room = call.room_id.clone();
        let label = match call.phase {
            CallPhase::Reconnecting => "Reconnecting to the call in",
            _ => "In a call in",
        };
        h_flex()
            .id("call-elsewhere")
            .role(Role::Status)
            .aria_label(format!("{label} {}", self.room_name(&call.room_id)))
            .h(px(32.))
            .flex_none()
            .px(px(16.))
            .gap(px(8.))
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().secondary)
            .text_size(px(12.))
            .child(
                Icon::new(IconName::Phone)
                    .size(px(13.))
                    .text_color(palette.online),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(4.))
                    .child(div().text_color(cx.theme().muted_foreground).child(label))
                    .child(
                        div()
                            .truncate()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.room_name(&call.room_id)),
                    ),
            )
            .child(
                Button::new("call-return")
                    .ghost()
                    .h(px(22.))
                    .px(px(8.))
                    .label("Return")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_room(room.clone(), window, cx)
                    })),
            )
            .child(
                Button::new("call-leave-elsewhere")
                    .danger()
                    .h(px(22.))
                    .px(px(8.))
                    .label("Leave")
                    .on_click(cx.listener(|this, _, _, cx| {
                        let result = this.leave_call();
                        this.call_result(result, cx)
                    })),
            )
            .into_any_element()
    }

    /// Title and detail line for the stage's status.
    fn call_status(&self, call: &CallState) -> (String, Option<String>) {
        let others = call
            .participants
            .iter()
            .filter(|p| !self.is_local(p))
            .count();
        match call.phase {
            CallPhase::Starting | CallPhase::Joining => ("Connecting…".into(), None),
            CallPhase::Ringing { .. } => (
                if call.kind == CallKind::Direct {
                    format!("Calling {}…", self.room_name(&call.room_id))
                } else {
                    "Waiting for others to join…".into()
                },
                None,
            ),
            CallPhase::Reconnecting => (
                "Reconnecting…".into(),
                Some("Your connection dropped. The call continues when it's back.".into()),
            ),
            CallPhase::Connected => {
                let started = self
                    .snapshot
                    .room_calls
                    .get(&call.room_id)
                    .and_then(|active| active.started_ms);
                let people = match others {
                    0 => "Only you".to_owned(),
                    1 if call.kind == CallKind::Direct => self.room_name(&call.room_id),
                    n => format!("{} people", n + 1),
                };
                (
                    started.map_or_else(|| "In call".into(), duration_text),
                    Some(people),
                )
            }
            CallPhase::Ended(reason) => (
                match reason {
                    EndReason::Left | EndReason::Logout => "Call ended",
                    EndReason::Declined => "Call declined",
                    EndReason::RingExpired => "No answer",
                    EndReason::RoomLeft => "You left the room",
                    EndReason::Failed => "Call failed",
                }
                .into(),
                call.error.map(|error| call_error_text(error).to_owned()),
            ),
        }
    }

    /// Our own participant: our user, with no remote media peer.
    fn is_local(&self, participant: &Participant) -> bool {
        participant.user_id == self.snapshot.user_id && participant.speaking.is_none()
    }

    fn call_tile(
        &self,
        participant: &Participant,
        call: &CallState,
        (width, height): (f32, f32),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let local = self.is_local(participant);
        let name = if local {
            "You".to_owned()
        } else {
            participant
                .display_name
                .clone()
                .unwrap_or_else(|| participant.user_id.clone())
        };
        let video = call
            .video_tracks
            .iter()
            .find(|track| {
                track.local == local
                    && track.source == VideoSource::Camera
                    && track.user_id == participant.user_id
                    && track.device_id == participant.device_id
            })
            .and_then(|track| self.call_tiles.get(&track.id))
            .filter(|_| participant.video != Some(false))
            .cloned();
        let muted = participant.muted == Some(true);
        let speaking = participant.speaking == Some(true) && !muted;
        let avatar = self
            .snapshot
            .members
            .iter()
            .find(|m| m.user_id == participant.user_id)
            .and_then(|m| m.avatar.clone());
        let label = h_flex()
            .absolute()
            .bottom(px(6.))
            .left(px(6.))
            .max_w(px(width - 12.))
            .gap(px(4.))
            .px(px(6.))
            .py(px(2.))
            .rounded(px(5.))
            .bg(gpui_kit::black().opacity(0.55))
            .text_color(gpui_kit::white())
            .text_size(px(11.))
            .when(muted, |label| {
                label.child(Icon::new(IconName::MicOff).size(px(11.)))
            })
            .when(participant.screen, |label| {
                label.child(Icon::new(IconName::ScreenShare).size(px(11.)))
            })
            .child(div().truncate().child(name.clone()))
            .when(!local && playback_muted(participant), |label| {
                label.child(Icon::new(IconName::VolumeOff).size(px(11.)))
            })
            .when(
                !local && !playback_muted(participant) && volume_percent(participant) != 100,
                |label| label.child(format!("{}%", volume_percent(participant))),
            );
        let key = CallFocus::Person(participant.user_id.clone(), participant.device_id.clone());
        let focused = self.call_focus.as_ref() == Some(&key);
        let ended_call = !live(call);
        let focus_label = if focused {
            format!("Show everyone, leave focus on {name}")
        } else {
            format!("Focus on {name}")
        };
        let focus_button = Button::new(SharedString::from(format!(
            "call-focus-{}-{}",
            participant.user_id, participant.device_id
        )))
        .ghost()
        .size(px(22.))
        .rounded(px(5.))
        .bg(gpui_kit::black().opacity(0.55))
        .text_color(gpui_kit::white())
        .icon(
            Icon::new(if focused {
                IconName::Minimize
            } else {
                IconName::Maximize
            })
            .size(px(12.)),
        )
        .accessibility_label(focus_label)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.call_focus = if this.call_focus.as_ref() == Some(&key) {
                None
            } else {
                Some(key.clone())
            };
            cx.notify();
        }));
        let tile_buttons = h_flex()
            .absolute()
            .top(px(6.))
            .right(px(6.))
            .gap(px(4.))
            .when(!local, |row| {
                row.child(self.tile_menu(participant, &name, cx))
            })
            .child(focus_button);
        let playback = participant.playback;
        let heard_less = !local && (playback.muted || (playback.volume() - 1.0).abs() > 0.004);
        let mut spoken = name.clone();
        for (on, word) in [
            (speaking, "speaking"),
            (muted, "muted"),
            (video.is_some(), "camera on"),
            (participant.screen, "sharing screen"),
            (participant.media_blocked, "not encrypted, so not shown"),
        ] {
            if on {
                spoken.push_str(", ");
                spoken.push_str(word);
            }
        }
        if heard_less {
            if playback.muted {
                spoken.push_str(", muted for you");
            } else {
                spoken.push_str(&format!(
                    ", volume {}% for you",
                    volume_percent(participant)
                ));
            }
        }
        div()
            .id(SharedString::from(format!(
                "call-tile-{}-{}",
                participant.user_id, participant.device_id
            )))
            .role(Role::Group)
            .aria_label(spoken)
            .relative()
            .flex_none()
            .w(px(width))
            .h(px(height))
            .rounded(px(10.))
            .overflow_hidden()
            .bg(if video.is_some() {
                gpui_kit::black()
            } else {
                cx.theme().secondary
            })
            .border_2()
            .border_color(if speaking {
                cx.theme().primary
            } else {
                gpui_kit::transparent_black()
            })
            .map(|tile| match video {
                Some(view) => tile.child(view),
                None => tile.child(v_flex().size_full().items_center().justify_center().child(
                    self.avatar_image(
                        avatar.as_deref(),
                        &participant.user_id,
                        &name,
                        (height * 0.42).min(56.),
                        (height * 0.21).min(28.),
                        cx,
                    ),
                )),
            })
            .when(participant.media_blocked, |tile| {
                tile.child(
                    v_flex()
                        .absolute()
                        .inset_0()
                        .items_center()
                        .justify_center()
                        .bg(gpui_kit::black().opacity(0.6))
                        .text_color(gpui_kit::white())
                        .text_size(px(11.))
                        .child("Not encrypted, so not shown"),
                )
            })
            .child(label)
            .when(!ended_call, |tile| tile.child(tile_buttons))
            .into_any_element()
    }

    /// The tile's "more" menu: mute this person for yourself and set how loud
    /// they are. Local playback only; nobody else hears a change.
    fn tile_menu(
        &self,
        participant: &Participant,
        name: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use gpui_kit::component::{popover::Popover, slider::Slider};
        let id = participant.id.clone();
        let slider = self.call_volume.get(&id).map(|(state, _)| state.clone());
        let weak = cx.entity().downgrade();
        let open = self.call_menu.as_deref() == Some(id.as_str());
        let muted = playback_muted(participant);
        let name = name.to_owned();
        let trigger = Button::new(SharedString::from(format!("call-more-{id}")))
            .ghost()
            .size(px(22.))
            .rounded(px(5.))
            .bg(gpui_kit::black().opacity(0.55))
            .text_color(gpui_kit::white())
            .icon(Icon::new(IconName::Ellipsis).size(px(12.)))
            .accessibility_label(format!("Volume and mute for {name}"));
        let (change_weak, change_id) = (weak.clone(), id.clone());
        Popover::new(SharedString::from(format!("call-menu-{id}")))
            .anchor(Anchor::TopRight)
            .open(open)
            .on_open_change(move |open, _, cx| {
                let (id, open) = (change_id.clone(), *open);
                let _ = change_weak.update(cx, |this, cx| {
                    this.call_menu = open.then_some(id);
                    this.poke_controls(true, cx);
                    cx.notify();
                });
            })
            .trigger(trigger)
            .content(move |_, _, cx| {
                let percent = slider
                    .as_ref()
                    .map_or(100, |state| state.read(cx).value().end().round() as i32);
                let (mute_weak, mute_id) = (weak.clone(), id.clone());
                let (reset_weak, reset_id, reset_slider) =
                    (weak.clone(), id.clone(), slider.clone());
                v_flex()
                    .w(px(232.))
                    .p(px(10.))
                    .gap(px(10.))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(name.clone()),
                    )
                    .child(
                        h_flex()
                            .justify_between()
                            .text_size(px(12.))
                            .child("Mute for me")
                            .child(
                                switch::Switch::new(SharedString::from(format!(
                                    "call-mute-for-me-{id}"
                                )))
                                .accessibility_label(format!("Mute {name} for me"))
                                .checked(muted)
                                .on_click(
                                    move |checked: &bool, _, cx| {
                                        let (id, checked) = (mute_id.clone(), *checked);
                                        let _ = mute_weak.update(cx, |this, cx| {
                                            let result =
                                                this.set_participant_muted_locally(id, checked);
                                            this.call_result(result, cx)
                                        });
                                    },
                                ),
                            ),
                    )
                    .children(slider.as_ref().map(|state| {
                        h_flex()
                            .gap(px(8.))
                            .child(Icon::new(IconName::Volume2).size(px(14.)))
                            .child(
                                Slider::new(state)
                                    .horizontal()
                                    .accessibility_label(format!("Volume for {name}"))
                                    .flex_1(),
                            )
                            .child(
                                div()
                                    .w(px(38.))
                                    .text_right()
                                    .text_size(px(12.))
                                    .child(format!("{percent}%")),
                            )
                    }))
                    .child(
                        Button::new(SharedString::from(format!("call-volume-reset-{id}")))
                            .outline()
                            .h(px(24.))
                            .label("Reset to 100%")
                            .on_click(move |_, window, cx| {
                                if let Some(state) = &reset_slider {
                                    state.update(cx, |state, cx| state.set_value(100., window, cx));
                                }
                                let id = reset_id.clone();
                                let _ = reset_weak.update(cx, |this, cx| {
                                    this.send_volume(&id, 100., true);
                                    cx.notify();
                                });
                            }),
                    )
            })
            .into_any_element()
    }

    fn track_owner(&self, call: &CallState, track: &CallVideoTrack) -> String {
        call.participants
            .iter()
            .find(|p| p.user_id == track.user_id && p.device_id == track.device_id)
            .and_then(|p| p.display_name.clone())
            .unwrap_or_else(|| track.user_id.clone())
    }

    /// The remote screen to show large: the focused one, the one picked from the
    /// switcher, or the first. None when nobody else shares.
    fn shown_screen<'a>(&self, call: &'a CallState) -> Option<&'a CallVideoTrack> {
        let screens = || {
            call.video_tracks.iter().filter(|t| {
                t.source == VideoSource::Screen && !t.local && self.call_tiles.contains_key(&t.id)
            })
        };
        let wanted = match &self.call_focus {
            Some(CallFocus::Screen(id)) => Some(id),
            _ => self.call_screen_focus.as_ref(),
        };
        wanted
            .and_then(|id| screens().find(|t| &t.id == id))
            .or_else(|| screens().next())
    }

    /// Someone else's shared screen, large. With several, the shown one can be
    /// switched; the focus button hides everyone else.
    fn screen_presentation(
        &self,
        call: &CallState,
        shown: &CallVideoTrack,
        height: f32,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let screens: Vec<&CallVideoTrack> = call
            .video_tracks
            .iter()
            .filter(|t| {
                t.source == VideoSource::Screen && !t.local && self.call_tiles.contains_key(&t.id)
            })
            .collect();
        let view = self.call_tiles.get(&shown.id)?.clone();
        let owner = self.track_owner(call, shown);
        let switcher = (screens.len() > 1).then(|| {
            h_flex()
                .absolute()
                .top(px(8.))
                .right(px(40.))
                .gap(px(4.))
                .children(screens.iter().map(|track| {
                    let id = track.id.clone();
                    Button::new(SharedString::from(format!("call-screen-{}", track.id)))
                        .outline()
                        .h(px(22.))
                        .px(px(8.))
                        .text_size(px(11.))
                        .label(self.track_owner(call, track))
                        .selected(track.id == shown.id)
                        .accessibility_label(format!(
                            "Show {}'s screen",
                            self.track_owner(call, track)
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.call_screen_focus = Some(id.clone());
                            if matches!(this.call_focus, Some(CallFocus::Screen(_))) {
                                this.call_focus = Some(CallFocus::Screen(id.clone()));
                            }
                            cx.notify();
                        }))
                }))
        });
        let id = shown.id.clone();
        let focus_button = Button::new("call-screen-focus")
            .ghost()
            .size(px(22.))
            .rounded(px(5.))
            .bg(gpui_kit::black().opacity(0.55))
            .text_color(gpui_kit::white())
            .icon(
                Icon::new(if focused {
                    IconName::Minimize
                } else {
                    IconName::Maximize
                })
                .size(px(12.)),
            )
            .accessibility_label(if focused {
                format!("Show everyone, leave focus on {owner}'s screen")
            } else {
                format!("Focus on {owner}'s screen")
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                let key = CallFocus::Screen(id.clone());
                this.call_focus = if this.call_focus.as_ref() == Some(&key) {
                    None
                } else {
                    Some(key)
                };
                cx.notify();
            }));
        Some(
            div()
                .id("call-screen")
                .role(Role::Group)
                .aria_label(format!("{owner}'s screen"))
                .relative()
                .w_full()
                .min_w_0()
                .h(px(height))
                .rounded(px(10.))
                .overflow_hidden()
                .bg(gpui_kit::black())
                .child(view)
                .child(
                    h_flex()
                        .absolute()
                        .bottom(px(8.))
                        .left(px(8.))
                        .gap(px(5.))
                        .px(px(7.))
                        .py(px(3.))
                        .rounded(px(5.))
                        .bg(gpui_kit::black().opacity(0.55))
                        .text_color(gpui_kit::white())
                        .text_size(px(11.))
                        .child(Icon::new(IconName::ScreenShare).size(px(12.)))
                        .child(div().truncate().child(format!("{owner}'s screen"))),
                )
                .children(switcher)
                .child(
                    div()
                        .absolute()
                        .top(px(8.))
                        .right(px(8.))
                        .child(focus_button),
                )
                .into_any_element(),
        )
    }

    /// Your own shared screen as a tile, so you can see what others see.
    fn own_screen_tile(
        &self,
        call: &CallState,
        (width, height): (f32, f32),
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let track = call
            .video_tracks
            .iter()
            .find(|t| t.local && t.source == VideoSource::Screen)?;
        let view = self.call_tiles.get(&track.id)?.clone();
        Some(
            div()
                .id("call-tile-own-screen")
                .role(Role::Group)
                .aria_label("Your screen")
                .relative()
                .flex_none()
                .w(px(width))
                .h(px(height))
                .rounded(px(10.))
                .overflow_hidden()
                .bg(gpui_kit::black())
                .border_2()
                .border_color(cx.theme().border)
                .child(view)
                .child(
                    h_flex()
                        .absolute()
                        .bottom(px(6.))
                        .left(px(6.))
                        .gap(px(4.))
                        .px(px(6.))
                        .py(px(2.))
                        .rounded(px(5.))
                        .bg(gpui_kit::black().opacity(0.55))
                        .text_color(gpui_kit::white())
                        .text_size(px(11.))
                        .child(Icon::new(IconName::ScreenShare).size(px(11.)))
                        .child("Your screen"),
                )
                .into_any_element(),
        )
    }

    /// The person being rung in a direct call, before they answer.
    fn callee_tile(
        &self,
        call: &CallState,
        (width, height): (f32, f32),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let room = self
            .snapshot
            .rooms
            .iter()
            .find(|room| room.id == call.room_id);
        let name = self.room_name(&call.room_id);
        let seed = room
            .and_then(|room| room.partner.clone())
            .unwrap_or_else(|| call.room_id.clone());
        let avatar = room.and_then(|room| room.avatar.clone());
        v_flex()
            .id("call-tile-callee")
            .role(Role::Group)
            .aria_label(format!("{name}, ringing"))
            .flex_none()
            .w(px(width))
            .h(px(height))
            .rounded(px(10.))
            .border_2()
            .border_dashed()
            .border_color(cx.theme().border)
            .items_center()
            .justify_center()
            .gap(px(6.))
            .child(div().opacity(0.6).child(self.avatar_image(
                avatar.as_deref(),
                &seed,
                &name,
                (height * 0.42).min(56.),
                (height * 0.21).min(28.),
                cx,
            )))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(cx.theme().muted_foreground)
                    .child("Ringing…"),
            )
            .into_any_element()
    }

    fn call_stage(&self, call: &CallState, cx: &mut Context<Self>) -> AnyElement {
        let voice_channel = self.is_voice_channel(&call.room_id);
        let palette = theme::palette(cx);
        let muted_fg = cx.theme().muted_foreground;
        let (title, mut detail) = self.call_status(call);
        let ended = !live(call);
        if !ended {
            match call.local.screen {
                ScreenShare::Choosing => {
                    detail = Some("Choose a window or screen to share".into());
                }
                ScreenShare::Live => {
                    detail = Some(match detail {
                        Some(people) => format!("Sharing your screen · {people}"),
                        None => "Sharing your screen".into(),
                    });
                }
                ScreenShare::Off => {}
            }
        }
        let full = !ended && self.call_fills_window();
        // Layout is in design points, so divide the window by the UI scale.
        let scale = f32::from(self.preferences.appearance.scale.clamp(85, 150)) / 100.;
        let view = self.call_stage_size.get();
        let (view_w, view_h) = (
            f32::from(view.width) / scale,
            f32::from(view.height) / scale,
        );
        let avail_w = if full {
            (view_w - 32.).max(320.)
        } else {
            // Rail, room list and gutters sit beside the conversation.
            (view_w - 360.).max(360.)
        };
        let stage_h = if full {
            // Leave room under the tiles for the floating controls.
            (view_h - 32. - CONTROLS_ROOM).max(300.)
        } else {
            (view_h * 0.56).max(320.)
        };
        // You first, then everyone else in membership order; with follow on,
        // the speaker leads the others.
        let following = self.preferences.calls.follow_speaker;
        let mut people: Vec<&Participant> = call.participants.iter().collect();
        people.sort_by_key(|p| {
            (
                !self.is_local(p),
                !(following && self.call_follow.picked() == Some(p.id.as_str())),
            )
        });
        let cameras = call
            .video_tracks
            .iter()
            .filter(|t| t.source == VideoSource::Camera && self.call_tiles.contains_key(&t.id))
            .count();
        let screen_track = if ended { None } else { self.shown_screen(call) };
        let find = |user: &str, device: &str| {
            people
                .iter()
                .copied()
                .find(|p| p.user_id == user && p.device_id == device)
        };
        // What is large: a person you focused, the screen someone shares, the
        // person being followed, or the grid. Your own choice is never overridden.
        let main = if ended {
            Main::Grid
        } else if let Some(CallFocus::Person(user, device)) = &self.call_focus
            && let Some(person) = find(user, device)
        {
            Main::Person(person, false)
        } else if let Some(track) = screen_track {
            Main::Screen(
                track,
                matches!(&self.call_focus, Some(CallFocus::Screen(id)) if id == &track.id),
            )
        } else if following
            && let Some(person) = self
                .call_follow
                .picked()
                .and_then(|id| people.iter().copied().find(|p| p.id == id))
        {
            Main::Person(person, true)
        } else {
            Main::Grid
        };
        let expanded = !ended && (full || cameras >= 2 || screen_track.is_some());
        let ringing_direct =
            matches!(call.phase, CallPhase::Ringing { .. }) && call.kind == CallKind::Direct;
        let reconnecting_dim = call.phase == CallPhase::Reconnecting;
        let strip_size = if expanded { (148., 92.) } else { (112., 70.) };
        let strip = |cx: &mut Context<Self>, skip: Option<&Participant>| {
            h_flex()
                .flex_wrap()
                .justify_center()
                .gap(px(8.))
                .children(
                    people
                        .iter()
                        .filter(|p| {
                            skip.is_none_or(|s| {
                                (&p.user_id, &p.device_id) != (&s.user_id, &s.device_id)
                            })
                        })
                        .take(MAX_TILES)
                        .map(|p| self.call_tile(p, call, strip_size, cx))
                        .collect::<Vec<_>>(),
                )
                .children(self.own_screen_tile(call, strip_size, cx))
                .into_any_element()
        };
        // Announced to screen readers through the status line.
        let mode = match &main {
            Main::Person(person, true) => Some(format!(
                "Following the speaker, showing {}",
                self.person_name(person)
            )),
            Main::Person(person, false) => Some(format!("Focused on {}", self.person_name(person))),
            Main::Screen(track, _) => Some(format!(
                "Showing {}'s screen",
                self.track_owner(call, track)
            )),
            Main::Grid if following && !ended => Some("Following the speaker".to_owned()),
            Main::Grid => None,
        };
        let tiles: Option<AnyElement> = match &main {
            _ if ended || people.is_empty() => None,
            Main::Person(focus, _) => {
                let strip_h = if people.len() > 1 {
                    strip_size.1 + 8.
                } else {
                    0.
                };
                let mut height = if expanded {
                    (stage_h - strip_h - 20.).max(160.)
                } else {
                    300.
                };
                let mut width = height * 1.6;
                if width > avail_w {
                    width = avail_w;
                    height = width / 1.6;
                }
                Some(
                    v_flex()
                        .items_center()
                        .gap(px(8.))
                        .px(px(16.))
                        .pt(px(12.))
                        .when(reconnecting_dim, |col| col.opacity(0.5))
                        .child(self.call_tile(focus, call, (width, height), cx))
                        .child(strip(cx, Some(focus)))
                        .into_any_element(),
                )
            }
            Main::Screen(track, focused) => {
                let strip_h = if *focused { 0. } else { strip_size.1 + 8. };
                let height = (stage_h - strip_h - 20.).max(200.);
                self.screen_presentation(call, track, height, *focused, cx)
                    .map(|screen| {
                        v_flex()
                            .gap(px(8.))
                            .px(px(16.))
                            .pt(px(12.))
                            .when(reconnecting_dim, |col| col.opacity(0.5))
                            .child(screen)
                            .when(!*focused, |col| col.child(strip(cx, None)))
                            .into_any_element()
                    })
            }
            Main::Grid => {
                let overflow = people.len().saturating_sub(MAX_TILES);
                let shown = if overflow > 0 {
                    MAX_TILES - 1
                } else {
                    people.len()
                };
                let own_screen = call
                    .video_tracks
                    .iter()
                    .any(|t| t.local && t.source == VideoSource::Screen)
                    as usize;
                let count = shown + own_screen + ringing_direct as usize + (overflow > 0) as usize;
                let size = if expanded {
                    layout::fit_grid(count, avail_w - 32., stage_h - 24.)
                } else {
                    match (people.len(), cameras > 0) {
                        (_, false) => (132., 96.),
                        (0..=2, true) => (260., 162.),
                        (3..=4, true) => (200., 125.),
                        _ => (156., 98.),
                    }
                };
                let row = h_flex()
                    .flex_wrap()
                    .justify_center()
                    .gap(px(8.))
                    .px(px(16.))
                    .pt(px(12.))
                    .when(reconnecting_dim, |row| row.opacity(0.5))
                    .children(
                        people
                            .iter()
                            .take(shown)
                            .map(|p| self.call_tile(p, call, size, cx))
                            .collect::<Vec<_>>(),
                    )
                    .children(self.own_screen_tile(call, size, cx))
                    .when(ringing_direct, |row| {
                        row.child(self.callee_tile(call, size, cx))
                    })
                    .when(overflow > 0, |row| {
                        row.child(
                            v_flex()
                                .id("call-tile-more")
                                .role(Role::Group)
                                .aria_label(format!("{} more people", overflow + 1))
                                .flex_none()
                                .w(px(size.0))
                                .h(px(size.1))
                                .rounded(px(10.))
                                .bg(cx.theme().secondary)
                                .items_center()
                                .justify_center()
                                .text_size(px(15.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(muted_fg)
                                .child(format!("+{}", overflow + 1)),
                        )
                    });
                Some(if expanded {
                    v_flex()
                        .h(px(stage_h))
                        .justify_center()
                        .child(row)
                        .into_any_element()
                } else {
                    row.into_any_element()
                })
            }
        };
        let notice = (!ended)
            .then(|| {
                call.events
                    .iter()
                    .rev()
                    .find(|(serial, _)| *serial > self.call_event_seen)
            })
            .flatten()
            .map(|(serial, event)| {
                let name = |id: &str| {
                    call.devices
                        .inputs
                        .iter()
                        .chain(&call.devices.outputs)
                        .chain(&call.devices.cameras)
                        .find(|d| d.id == id)
                        .map(|d| d.name.clone())
                        .unwrap_or_else(|| "the system default".into())
                };
                let text = match event {
                    CallEvent::DeviceFallback {
                        kind, to: Some(to), ..
                    } => format!(
                        "Your {} was disconnected. Now using {}.",
                        device_word(*kind),
                        name(to)
                    ),
                    CallEvent::DeviceFallback { kind, to: None, .. } => match kind {
                        DeviceKind::Input => {
                            "Your microphone was disconnected, so you're muted.".into()
                        }
                        DeviceKind::Output => "Your speaker was disconnected.".into(),
                        DeviceKind::Camera => {
                            "Your camera was disconnected, so it's turned off.".into()
                        }
                    },
                    CallEvent::PermissionRevoked { kind, .. } => match kind {
                        DeviceKind::Input => {
                            "Microphone access was turned off, so you're muted.".into()
                        }
                        DeviceKind::Camera => {
                            "Camera access was turned off, so your camera is off.".into()
                        }
                        DeviceKind::Output => "Speaker access was turned off.".into(),
                    },
                };
                let serial = *serial;
                h_flex()
                    .id("call-notice")
                    .role(Role::Alert)
                    .aria_label(text.clone())
                    .mx(px(16.))
                    .mt(px(10.))
                    .px(px(10.))
                    .py(px(6.))
                    .gap(px(8.))
                    .rounded(px(7.))
                    .bg(palette.warning_background)
                    .text_size(px(12.))
                    .child(
                        Icon::new(IconName::TriangleAlert)
                            .size(px(13.))
                            .text_color(palette.warning),
                    )
                    .child(div().flex_1().min_w_0().child(text))
                    .child(
                        Button::new("call-notice-close")
                            .ghost()
                            .size(px(20.))
                            .icon(Icon::new(IconName::Close).size(px(12.)))
                            .accessibility_label("Dismiss")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.call_event_seen = serial;
                                cx.notify();
                            })),
                    )
            });
        let reconnecting = call.phase == CallPhase::Reconnecting;
        let picker = (!ended)
            .then(|| self.screen_picker_view(call, cx))
            .flatten();
        let status = v_flex()
            .id("call-status")
            .role(Role::Status)
            .aria_label({
                let mut spoken = match &detail {
                    Some(detail) => format!("{title}, {detail}"),
                    None => title.clone(),
                };
                if let Some(mode) = &mode {
                    spoken.push_str(". ");
                    spoken.push_str(mode);
                }
                if full {
                    spoken.push_str(". Full screen");
                }
                spoken
            })
            .flex_1()
            .min_w_0()
            .child(
                h_flex()
                    .gap(px(6.))
                    .child(
                        Icon::new(if ended {
                            IconName::PhoneOff
                        } else if reconnecting {
                            IconName::WifiOff
                        } else {
                            IconName::Phone
                        })
                        .size(px(14.))
                        .text_color(if ended {
                            muted_fg
                        } else if reconnecting {
                            palette.warning
                        } else {
                            palette.online
                        }),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    ),
            )
            .children(detail.map(|detail| {
                div()
                    .truncate()
                    .text_size(px(12.))
                    .text_color(muted_fg)
                    .child(detail)
            }));
        let controls = if ended {
            let room = call.room_id.clone();
            let again = !matches!(
                call.phase,
                CallPhase::Ended(EndReason::RoomLeft | EndReason::Logout)
            );
            let video = call.local.video;
            // Others still in the call: rejoin it rather than ring again.
            let rejoin = self
                .snapshot
                .room_calls
                .get(&call.room_id)
                .is_some_and(|active| !active.participants.is_empty());
            h_flex()
                .gap(px(6.))
                .when(again, |row| {
                    row.child(
                        Button::new("call-again")
                            .outline()
                            .h(px(28.))
                            .px(px(10.))
                            .label(if rejoin { "Rejoin" } else { "Call Again" })
                            .disabled(self.live_call().is_some())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if rejoin {
                                    this.join_room_call(room.clone(), cx)
                                } else {
                                    this.begin_call(room.clone(), video, cx)
                                }
                            })),
                    )
                })
                .child(
                    Button::new("call-close")
                        .ghost()
                        .size(px(28.))
                        .icon(Icon::new(IconName::Close).size(px(14.)))
                        .accessibility_label("Close")
                        .tooltip("Close")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.call_end_dismissed = true;
                            cx.notify();
                        })),
                )
        } else {
            let muted = call.local.muted;
            let camera = call.local.video;
            let screen = call.local.screen;
            let connecting = matches!(call.phase, CallPhase::Starting);
            h_flex()
                .gap(px(6.))
                .child(
                    Button::new("call-mute")
                        .outline()
                        .size(px(32.))
                        .rounded(px(16.))
                        .icon(
                            Icon::new(if muted {
                                IconName::MicOff
                            } else {
                                IconName::Mic
                            })
                            .size(px(15.)),
                        )
                        .selected(muted)
                        .disabled(connecting)
                        .accessibility_label(if muted { "Unmute" } else { "Mute" })
                        .tooltip(hint(
                            if muted { "Unmute" } else { "Mute" },
                            Action::ToggleMute,
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let result = this.set_muted(!muted);
                            this.call_result(result, cx)
                        })),
                )
                .when(!voice_channel, |view| {
                    view.child(
                        Button::new("call-camera")
                            .outline()
                            .size(px(32.))
                            .rounded(px(16.))
                            .icon(
                                Icon::new(if camera {
                                    IconName::Video
                                } else {
                                    IconName::VideoOff
                                })
                                .size(px(15.)),
                            )
                            .selected(!camera)
                            .disabled(connecting)
                            .accessibility_label(if camera {
                                "Turn camera off"
                            } else {
                                "Turn camera on"
                            })
                            .tooltip(hint(
                                if camera {
                                    "Turn Camera Off"
                                } else {
                                    "Turn Camera On"
                                },
                                Action::ToggleCamera,
                            ))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let result = this.set_video(!camera);
                                this.call_result(result, cx)
                            })),
                    )
                })
                .when(!voice_channel, |view| {
                    view.child(
                        Button::new("call-screen-share")
                            .outline()
                            .size(px(32.))
                            .rounded(px(16.))
                            .icon(
                                Icon::new(if screen == ScreenShare::Off {
                                    IconName::ScreenShare
                                } else {
                                    IconName::ScreenShareOff
                                })
                                .size(px(15.)),
                            )
                            .selected(screen != ScreenShare::Off || self.screen_picker.is_some())
                            .disabled(connecting)
                            .accessibility_label(match screen {
                                ScreenShare::Off => "Share screen",
                                ScreenShare::Choosing => "Cancel screen sharing",
                                ScreenShare::Live => "Stop sharing screen",
                            })
                            .tooltip(hint(
                                match screen {
                                    ScreenShare::Off => "Share Screen",
                                    ScreenShare::Choosing => "Cancel Sharing",
                                    ScreenShare::Live => "Stop Sharing",
                                },
                                Action::ToggleScreenShare,
                            ))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let result = this.toggle_screen(screen == ScreenShare::Off, cx);
                                this.call_result(result, cx)
                            })),
                    )
                })
                .when(
                    call.participants
                        .iter()
                        .filter(|p| !self.is_local(p))
                        .count()
                        >= 2,
                    |row| {
                        row.child(
                            Button::new("call-follow")
                                .outline()
                                .size(px(32.))
                                .rounded(px(16.))
                                .icon(Icon::new(IconName::AudioLines).size(px(15.)))
                                .selected(following)
                                .accessibility_label(if following {
                                    "Stop following the speaker"
                                } else {
                                    "Follow the speaker"
                                })
                                .tooltip(if following {
                                    "Stop Following the Speaker"
                                } else {
                                    "Follow the Speaker"
                                })
                                .on_click(cx.listener(|this, _, window, cx| {
                                    let on = !this.preferences.calls.follow_speaker;
                                    this.preferences.calls.follow_speaker = on;
                                    if !on {
                                        this.call_follow.reset();
                                    }
                                    this.save_preferences(window, cx);
                                    cx.notify();
                                })),
                        )
                    },
                )
                .child(self.call_devices_menu(call, cx))
                .child(
                    Button::new("call-full-screen")
                        .outline()
                        .size(px(32.))
                        .rounded(px(16.))
                        .icon(
                            Icon::new(if full {
                                IconName::Minimize
                            } else {
                                IconName::Maximize
                            })
                            .size(px(15.)),
                        )
                        .accessibility_label(if full {
                            "Exit full screen"
                        } else {
                            "Full screen"
                        })
                        .tooltip(hint(
                            if full {
                                "Exit Full Screen"
                            } else {
                                "Full Screen"
                            },
                            Action::ToggleCallFullScreen,
                        ))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.toggle_call_full_screen(window, cx)
                        })),
                )
                .child(
                    Button::new("call-leave")
                        .danger()
                        .h(px(32.))
                        .px(px(12.))
                        .rounded(px(16.))
                        .accessibility_label(if ringing_direct {
                            "Cancel call"
                        } else {
                            "Leave call"
                        })
                        .tooltip(hint(
                            if ringing_direct {
                                "Cancel Call"
                            } else {
                                "Leave Call"
                            },
                            Action::LeaveCall,
                        ))
                        .child(
                            h_flex()
                                .gap(px(5.))
                                .child(Icon::new(IconName::PhoneOff).size(px(14.)))
                                .child(if ringing_direct { "Cancel" } else { "Leave" }),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            let result = this.leave_call();
                            this.call_result(result, cx)
                        })),
                )
        };
        let bar = h_flex()
            .h(px(52.))
            .px(px(16.))
            .gap(px(12.))
            .child(status)
            .child(controls);
        if full {
            // The call fills the window; the bar floats and fades after a pause.
            let pinned = self.call_controls_pinned || self.call_menu.is_some();
            let visible = self
                .call_controls
                .visible(std::time::Instant::now(), pinned);
            let floating = div()
                .absolute()
                .bottom(px(20.))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    bar.id("call-controls")
                        .rounded(px(26.))
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().popover)
                        .shadow_lg()
                        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                            this.call_controls_pinned = *hovered;
                            this.poke_controls(true, cx);
                        })),
                );
            return div()
                .id("call-stage")
                .role(Role::Region)
                .aria_label("Call, full screen")
                .relative()
                .size_full()
                .bg(cx.theme().background)
                .overflow_hidden()
                .on_mouse_move(
                    cx.listener(|this, _: &MouseMoveEvent, _, cx| this.poke_controls(false, cx)),
                )
                .on_key_down(
                    cx.listener(|this, _: &KeyDownEvent, _, cx| this.poke_controls(false, cx)),
                )
                .child(
                    v_flex()
                        .size_full()
                        .justify_center()
                        .pb(px(CONTROLS_ROOM))
                        .children(tiles),
                )
                .children(picker)
                .children(notice)
                .child(motion::fade_toggle(
                    floating,
                    SharedString::from(format!("call-controls-{visible}")),
                    visible,
                    self.snapshot.reduce_motion,
                ))
                .into_any_element();
        }
        motion::rise(
            v_flex()
                .id("call-stage")
                .role(Role::Region)
                .aria_label("Call")
                .flex_none()
                .border_b_1()
                .border_color(cx.theme().border)
                .children(tiles)
                .children(picker)
                .children(notice)
                .child(bar),
            "call-stage-motion",
            self.snapshot.reduce_motion,
        )
    }

    /// Whether sharing needs our own chooser: the OS has no picker of its own
    /// (macOS and Wayland do; X11 and Windows don't).
    fn needs_source_picker(&self) -> bool {
        self.live_call()
            .and_then(|call| call.screen_backend)
            .is_some_and(|backend| !backend.uses_system_picker())
    }

    /// Starts or stops sharing. Where the OS can't ask which screen or window,
    /// starting opens the chooser instead; clicking again closes it.
    pub(crate) fn toggle_screen(
        &mut self,
        start: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), CallError> {
        use twayblade_core::calls::SourceKind;
        if start && self.needs_source_picker() {
            if self.screen_picker.is_some() {
                self.screen_picker = None;
            } else {
                self.screen_picker = Some(SourceKind::Screen);
                if !self.preview {
                    self.request_screen_sources(SourceKind::Screen)?;
                }
            }
            cx.notify();
            return Ok(());
        }
        self.screen_picker = None;
        self.set_screen_share(start)
    }

    /// Releases stills whose source is gone or changed, or whose tab is not shown.
    fn prune_screen_stills(&self, call: &CallState, cx: &mut Context<Self>) {
        let shown = self.screen_picker;
        let mut stills = self.screen_stills.borrow_mut();
        let mut kept = Vec::with_capacity(stills.len());
        for still in stills.drain(..) {
            let listed = shown == Some(still.selection.kind)
                && call.screen_sources.iter().any(|source| {
                    source.selection == still.selection
                        && source
                            .thumbnail
                            .as_ref()
                            .is_some_and(|t| Arc::ptr_eq(&t.bgra, &still.frame))
                });
            if listed {
                kept.push(still);
            } else {
                cx.drop_image(still.image, None);
            }
        }
        *stills = kept;
    }

    /// The GPU image for a source's still, uploaded once per frame.
    fn screen_still(
        &self,
        source: &twayblade_core::calls::ScreenSource,
    ) -> Option<Arc<RenderImage>> {
        let thumbnail = source.thumbnail.as_ref()?;
        let mut stills = self.screen_stills.borrow_mut();
        if let Some(still) = stills.iter().find(|still| {
            still.selection == source.selection && Arc::ptr_eq(&still.frame, &thumbnail.bgra)
        }) {
            return Some(still.image.clone());
        }
        // Stills are BGRA, which is what GPUI uploads.
        let buffer =
            image::RgbaImage::from_raw(thumbnail.width, thumbnail.height, thumbnail.bgra.to_vec())?;
        let image = Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]));
        stills.push(ScreenStill {
            selection: source.selection,
            frame: thumbnail.bgra.clone(),
            image: image.clone(),
        });
        Some(image)
    }

    /// The screen and window chooser, above the call controls.
    fn screen_picker_view(&self, call: &CallState, cx: &mut Context<Self>) -> Option<AnyElement> {
        use twayblade_core::calls::SourceKind;
        self.prune_screen_stills(call, cx);
        let kind = self.screen_picker?;
        let muted_fg = cx.theme().muted_foreground;
        let sources: Vec<_> = call
            .screen_sources
            .iter()
            .filter(|source| source.selection.kind == kind)
            .collect();
        let tab = |tab: SourceKind, label: &'static str, id: &'static str| {
            Button::new(id)
                .ghost()
                .xsmall()
                .flex_1()
                .h(px(22.))
                .rounded(px(5.))
                .label(label)
                .selected(kind == tab)
                .when(kind == tab, |button| {
                    button.bg(theme::palette(cx).pill).shadow_xs()
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.screen_picker = Some(tab);
                    if !this.preview {
                        let result = this.request_screen_sources(tab);
                        this.call_result(result, cx);
                    }
                    cx.notify();
                }))
        };
        let noun = match kind {
            SourceKind::Screen => "screens",
            SourceKind::Window => "windows",
        };
        let rows = sources.iter().enumerate().map(|(index, source)| {
            let selection = source.selection;
            let title = source.title.clone().filter(|t| !t.trim().is_empty());
            let name = title.unwrap_or_else(|| match kind {
                SourceKind::Screen => format!("Screen {}", index + 1),
                SourceKind::Window => format!("Window {}", index + 1),
            });
            // A 16:10 frame for the still (or the icon when there is none),
            // so rows line up whatever the source's shape.
            let still = self.screen_still(source);
            let preview = div()
                .flex_none()
                .w(px(64.))
                .h(px(40.))
                .rounded(px(4.))
                .overflow_hidden()
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().secondary)
                .flex()
                .items_center()
                .justify_center()
                .map(|frame| match still {
                    Some(image) => {
                        frame.child(img(image).size_full().object_fit(ObjectFit::Contain))
                    }
                    None => frame.child(
                        Icon::new(match kind {
                            SourceKind::Screen => IconName::Monitor,
                            SourceKind::Window => IconName::AppWindow,
                        })
                        .size(px(14.))
                        .text_color(muted_fg),
                    ),
                });
            Button::new(SharedString::from(format!("share-source-{index}")))
                .ghost()
                .h(px(48.))
                .w_full()
                .justify_start()
                .px(px(6.))
                .accessibility_label(format!("Share {name}"))
                .child(
                    h_flex()
                        .gap(px(10.))
                        .w_full()
                        .child(preview)
                        .child(div().flex_1().min_w_0().truncate().text_left().child(name)),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.screen_picker = None;
                    let result = this.share_screen_source(selection);
                    this.call_result(result, cx);
                }))
        });
        Some(motion::rise(
            v_flex()
                .id("screen-picker")
                .role(Role::Group)
                .aria_label("Choose what to share")
                .mx(px(16.))
                .mt(px(10.))
                .p(px(10.))
                .gap(px(8.))
                .rounded(px(9.))
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().popover)
                .child(
                    h_flex()
                        .gap(px(8.))
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Choose what to share"),
                        )
                        .child(
                            Button::new("screen-picker-refresh")
                                .ghost()
                                .size(px(22.))
                                .icon(Icon::new(IconName::RefreshCw).size(px(12.)))
                                .accessibility_label(format!("Refresh {noun}"))
                                .tooltip(format!("Refresh {noun}"))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if !this.preview
                                        && let Some(kind) = this.screen_picker
                                    {
                                        let result = this.request_screen_sources(kind);
                                        this.call_result(result, cx);
                                    }
                                })),
                        )
                        .child(
                            Button::new("screen-picker-cancel")
                                .ghost()
                                .size(px(22.))
                                .icon(Icon::new(IconName::Close).size(px(12.)))
                                .accessibility_label("Cancel sharing")
                                .tooltip("Cancel")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.screen_picker = None;
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    h_flex()
                        .p(px(2.))
                        .gap(px(2.))
                        .rounded(px(7.))
                        .bg(cx.theme().secondary)
                        .child(tab(SourceKind::Screen, "Screens", "screen-picker-screens"))
                        .child(tab(SourceKind::Window, "Windows", "screen-picker-windows")),
                )
                .child(
                    v_flex()
                        .id("screen-picker-list")
                        .max_h(px(220.))
                        .overflow_y_scroll()
                        .children(rows)
                        .when(sources.is_empty(), |list| {
                            list.child(
                                div()
                                    .py(px(8.))
                                    .px(px(8.))
                                    .text_size(px(12.))
                                    .text_color(muted_fg)
                                    .child(format!("No {noun} found.")),
                            )
                        }),
                ),
            "screen-picker-motion",
            self.snapshot.reduce_motion,
        ))
    }

    /// Shows the full-screen controls and restarts their hide timer. `force`
    /// restarts it even for a tiny move (used when the pointer leaves the bar
    /// or a menu closes).
    pub(crate) fn poke_controls(&mut self, force: bool, cx: &mut Context<Self>) {
        if !self.call_fills_window() {
            return;
        }
        let now = std::time::Instant::now();
        let pinned = self.call_controls_pinned || self.call_menu.is_some();
        let was_visible = self.call_controls.visible(now, pinned);
        if self.call_controls.activity(now, force) {
            self.call_controls_timer = Some(cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(CONTROLS_IDLE + std::time::Duration::from_millis(50))
                    .await;
                let _ = this.update(cx, |_, cx| cx.notify());
            }));
        }
        if !was_visible || force {
            cx.notify();
        }
    }

    /// The call alone, filling the window. Used while the call is full screen.
    pub(crate) fn call_full_screen_view(&self, cx: &mut Context<Self>) -> AnyElement {
        match self.live_call() {
            Some(call) => self.call_stage(call, cx),
            None => div().size_full().into_any_element(),
        }
    }

    fn call_devices_menu(&self, call: &CallState, cx: &mut Context<Self>) -> AnyElement {
        use gpui_kit::component::menu::DropdownMenu;
        let devices = call.devices.clone();
        let local = call.local.clone();
        let weak = cx.entity().downgrade();
        Button::new("call-devices")
            .outline()
            .size(px(32.))
            .rounded(px(16.))
            .icon(Icon::new(IconName::Settings2).size(px(15.)))
            .accessibility_label("Microphone, speaker and camera")
            .tooltip("Devices")
            .dropdown_menu_with_anchor(Anchor::BottomRight, move |mut menu, _, _| {
                let sections: [(
                    &str,
                    &Vec<twayblade_core::calls::CallDevice>,
                    &Option<String>,
                    DeviceKind,
                ); 3] = [
                    (
                        "Microphone",
                        &devices.inputs,
                        &local.input_device,
                        DeviceKind::Input,
                    ),
                    (
                        "Speaker",
                        &devices.outputs,
                        &local.output_device,
                        DeviceKind::Output,
                    ),
                    (
                        "Camera",
                        &devices.cameras,
                        &local.camera,
                        DeviceKind::Camera,
                    ),
                ];
                let mut first = true;
                for (title, list, selected, kind) in sections {
                    if list.is_empty() {
                        continue;
                    }
                    if !first {
                        menu = menu.separator();
                    }
                    first = false;
                    menu = menu.label(title);
                    for device in list {
                        let weak = weak.clone();
                        let id = device.id.clone();
                        menu = menu.item(
                            PopupMenuItem::new(device.name.clone())
                                .checked(selected.as_deref() == Some(&device.id))
                                .on_click(move |_, _, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        let result = match kind {
                                            DeviceKind::Input => this.select_input(id.clone()),
                                            DeviceKind::Output => this.select_output(id.clone()),
                                            DeviceKind::Camera => this.select_camera(id.clone()),
                                        };
                                        this.call_result(result, cx)
                                    });
                                }),
                        );
                    }
                }
                if first {
                    menu = menu.label("No devices found");
                }
                menu
            })
            .into_any_element()
    }

    /// Incoming calls the card can show: not for the room you're already in a call in.
    fn shown_incoming(&self) -> impl Iterator<Item = &IncomingCall> {
        let busy_room = self.live_call().map(|call| call.room_id.as_str());
        self.snapshot
            .incoming_calls
            .iter()
            .filter(move |call| busy_room != Some(call.room_id.as_str()))
    }

    /// Rings while the card shows a ringing call, unless you're already in a
    /// call, the room is muted or notification sound is off.
    pub(crate) fn sync_ringtone(&mut self) {
        let ring = !self.preview
            && self.preferences.notifications.sound
            && self.live_call().is_none()
            && self.shown_incoming().any(|call| {
                call.ringing
                    && call.notify_type == NotifyType::Ring
                    && self.snapshot.notifications.room_mode(&call.room_id)
                        != twayblade_core::notifications::RoomMode::Mute
            });
        if !ring {
            self.ringtone = None;
        } else if self.ringtone.is_none() {
            self.ringtone = twayblade_platform::ringtone::start();
        }
    }

    /// Keyboard shortcuts act on the current call or the call on the card.
    pub(crate) fn call_shortcut(
        &mut self,
        shortcut: CallShortcut,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        if self.preview {
            return;
        }
        let incoming = self
            .shown_incoming()
            .next()
            .map(|call| (call.room_id.clone(), call.notify_type == NotifyType::Ring));
        let current = self.live_call().map(|call| {
            (
                call.local.muted,
                call.local.video,
                call.local.screen,
                matches!(call.phase, CallPhase::Starting),
            )
        });
        let result = match (shortcut, current, incoming) {
            (CallShortcut::Answer, None, Some((room, _))) => {
                if let Some(window) = window {
                    self.answer(room, false, window, cx);
                }
                return;
            }
            (CallShortcut::Decline, _, Some((room, true))) => self.decline_call(room),
            (CallShortcut::Decline, _, Some((room, false))) => self.dismiss_incoming(room),
            (_, Some((_, _, _, true)), _) => return,
            (CallShortcut::Mute, Some((muted, ..)), _) => self.set_muted(!muted),
            (CallShortcut::Camera, Some((_, video, ..)), _) => self.set_video(!video),
            (CallShortcut::Screen, Some((_, _, screen, _)), _) => {
                self.toggle_screen(screen == ScreenShare::Off, cx)
            }
            (CallShortcut::Leave, Some(_), _) => self.leave_call(),
            _ => return,
        };
        self.call_result(result, cx);
    }

    /// A card for the newest incoming call, over the window's top-right corner.
    pub(crate) fn incoming_call_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let mut calls = self.shown_incoming();
        let call: &IncomingCall = calls.next()?;
        let more = calls.count();
        let direct = self
            .snapshot
            .rooms
            .iter()
            .find(|room| room.id == call.room_id)
            .is_some_and(|room| room.direct);
        let room_avatar = self
            .snapshot
            .rooms
            .iter()
            .find(|room| room.id == call.room_id)
            .and_then(|room| room.avatar.clone());
        let video = call.kind == CallIntent::Video;
        let ring = call.notify_type == NotifyType::Ring;
        let (title, detail) = if direct {
            (
                call.caller_name.clone(),
                if video {
                    "Incoming video call"
                } else {
                    "Incoming call"
                }
                .to_owned(),
            )
        } else {
            (
                call.room_name.clone(),
                format!(
                    "{} started {}",
                    call.caller_name,
                    if video { "a video call" } else { "a call" }
                ),
            )
        };
        let room = call.room_id.clone();
        let actions = if ring {
            let (decline, audio, with_video) = (room.clone(), room.clone(), room.clone());
            h_flex()
                .gap(px(6.))
                .child(
                    Button::new("incoming-decline")
                        .danger()
                        .flex_1()
                        .h(px(28.))
                        .tooltip(hint("Decline", Action::DeclineCall))
                        .child(
                            h_flex()
                                .gap(px(5.))
                                .child(Icon::new(IconName::PhoneOff).size(px(13.)))
                                .child("Decline"),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let result = this.decline_call(decline.clone());
                            this.call_result(result, cx)
                        })),
                )
                .child(
                    Button::new("incoming-accept")
                        .success()
                        .flex_1()
                        .h(px(28.))
                        .tooltip(hint("Answer with audio", Action::AnswerCall))
                        .child(
                            h_flex()
                                .gap(px(5.))
                                .child(Icon::new(IconName::Phone).size(px(13.)))
                                .child(if video { "Audio" } else { "Accept" }),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.answer(audio.clone(), false, window, cx)
                        })),
                )
                .when(video, |row| {
                    row.child(
                        Button::new("incoming-video")
                            .success()
                            .flex_1()
                            .h(px(28.))
                            .child(
                                h_flex()
                                    .gap(px(5.))
                                    .child(Icon::new(IconName::Video).size(px(13.)))
                                    .child("Video"),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.answer(with_video.clone(), true, window, cx)
                            })),
                    )
                })
        } else {
            let (dismiss, join) = (room.clone(), room.clone());
            h_flex()
                .gap(px(6.))
                .child(
                    Button::new("incoming-dismiss")
                        .outline()
                        .flex_1()
                        .h(px(28.))
                        .label("Dismiss")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let result = this.dismiss_incoming(dismiss.clone());
                            this.call_result(result, cx)
                        })),
                )
                .child(
                    Button::new("incoming-join")
                        .primary()
                        .flex_1()
                        .h(px(28.))
                        .label("Join")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.answer(join.clone(), false, window, cx)
                        })),
                )
        };
        Some(
            div()
                .absolute()
                .top(px(12.))
                .right(px(12.))
                .child(motion::rise(
                    v_flex()
                        .id("incoming-call")
                        .role(Role::Alert)
                        .aria_label(format!("{title}, {detail}"))
                        .occlude()
                        .w(px(300.))
                        .p(px(12.))
                        .gap(px(10.))
                        .rounded(px(12.))
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().popover)
                        .shadow_lg()
                        .child(
                            h_flex()
                                .gap(px(10.))
                                .child(self.avatar_image(
                                    if direct { room_avatar.as_deref() } else { None },
                                    if direct {
                                        &call.caller_user
                                    } else {
                                        &call.room_id
                                    },
                                    &title,
                                    36.,
                                    if direct { 18. } else { 8. },
                                    cx,
                                ))
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_w_0()
                                        .child(
                                            div()
                                                .truncate()
                                                .text_size(px(13.))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child(title),
                                        )
                                        .child(
                                            div()
                                                .truncate()
                                                .text_size(px(12.))
                                                .text_color(cx.theme().muted_foreground)
                                                .child(detail),
                                        ),
                                ),
                        )
                        .child(actions)
                        .when(more > 0, |card| {
                            card.child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(if more == 1 {
                                        "1 more incoming call".to_owned()
                                    } else {
                                        format!("{more} more incoming calls")
                                    }),
                            )
                        }),
                    SharedString::from(format!("incoming-{}", call.event_id)),
                    self.snapshot.reduce_motion,
                ))
                .into_any_element(),
        )
    }
}

impl MatrixApp {
    /// Sample call states for
    /// `--preview-call[-group|-ringing|-ended|-reconnecting|-screen|-sharing]` and
    /// `--preview-incoming`. Never shown with a live account.
    pub(crate) fn preview_call(&mut self, flag: &str) {
        use twayblade_core::calls::{ActiveCall, CallDevice, CallDevices, LocalCallState};
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or_default();
        let person =
            |user: &str, name: Option<&str>, speaking: Option<bool>, muted: bool| Participant {
                user_id: user.into(),
                device_id: "PREVIEW".into(),
                display_name: name.map(Into::into),
                // Preview participants use default playback state.
                id: format!("{user}:PREVIEW"),
                playback: Default::default(),
                speaking,
                muted: Some(muted),
                video: Some(false),
                connection_quality: None,
                media_blocked: false,
                screen: false,
            };
        if flag == "--preview-incoming" {
            let call = |room: &str, room_name: &str, user: &str, name: &str, video, ring, at| {
                IncomingCall {
                    room_id: room.into(),
                    room_name: room_name.into(),
                    caller_user: user.into(),
                    caller_device: Some("PREVIEW".into()),
                    caller_name: name.into(),
                    kind: if video {
                        CallIntent::Video
                    } else {
                        CallIntent::Audio
                    },
                    notify_type: if ring {
                        NotifyType::Ring
                    } else {
                        NotifyType::Notification
                    },
                    event_id: format!("$preview-{room}"),
                    since: now - at,
                    expires: now + 60_000,
                    verified: true,
                    ringing: ring,
                }
            };
            self.snapshot.incoming_calls = vec![
                call(
                    "alex",
                    "Alex",
                    "@alex:preview.invalid",
                    "Alex",
                    true,
                    true,
                    0,
                ),
                call(
                    "critique",
                    "Critique",
                    "@sam:preview.invalid",
                    "Sam",
                    false,
                    false,
                    5_000,
                ),
            ];
            return;
        }
        let group = matches!(
            flag,
            "--preview-call-group" | "--preview-call-video" | "--preview-call-fullscreen"
        );
        let direct = flag == "--preview-call-ringing";
        let room = if direct { "alex" } else { "general" };
        let sharing = flag == "--preview-call-sharing";
        let mut you = person(&self.snapshot.user_id.clone(), Some("You"), None, false);
        you.screen = sharing;
        let mut participants = vec![you];
        if !direct {
            let mut alex = person("@alex:preview.invalid", Some("Alex"), Some(true), false);
            alex.screen = flag == "--preview-call-screen";
            participants.push(alex);
            participants.push(person(
                "@sam:preview.invalid",
                Some("Sam"),
                Some(false),
                true,
            ));
            if group {
                for (user, name) in [
                    ("@kai:preview.invalid", "Kai"),
                    ("@mira:preview.invalid", "Mira"),
                ] {
                    participants.push(person(user, Some(name), Some(false), false));
                }
            }
        }
        // Cameras on for the video and full-screen previews.
        let cameras_on = matches!(flag, "--preview-call-video" | "--preview-call-fullscreen");
        if cameras_on {
            for p in &mut participants {
                p.video = Some(true);
            }
        }
        if flag == "--preview-call-volume" {
            for p in &mut participants {
                match p.user_id.as_str() {
                    "@alex:preview.invalid" => {
                        let _ = p.playback.set_volume(0.6);
                    }
                    "@sam:preview.invalid" => p.playback.muted = true,
                    _ => {}
                }
            }
            // Show the menu open, as a capture of the volume controls.
            self.call_menu = Some("@alex:preview.invalid:PREVIEW".into());
        }
        if flag == "--preview-call-fullscreen" {
            self.call_full_screen = true;
        }
        if flag == "--preview-call-picker" {
            self.screen_picker = Some(twayblade_core::calls::SourceKind::Screen);
        }
        let device = |id: &str, name: &str| CallDevice {
            id: id.into(),
            name: name.into(),
        };
        let phase = match flag {
            "--preview-call-ringing" => CallPhase::Ringing {
                since: now,
                expires: now + 90_000,
            },
            "--preview-call-ended" => CallPhase::Ended(EndReason::Failed),
            "--preview-call-reconnecting" => CallPhase::Reconnecting,
            _ => CallPhase::Connected,
        };
        let events = if flag == "--preview-call" {
            vec![(
                1,
                CallEvent::DeviceFallback {
                    kind: DeviceKind::Input,
                    from: "usb".into(),
                    to: Some("builtin".into()),
                },
            )]
        } else {
            Vec::new()
        };
        let mut video_tracks = Vec::new();
        let shared = match flag {
            "--preview-call-screen" => {
                Some(("preview-screen-alex", "@alex:preview.invalid", false))
            }
            "--preview-call-sharing" => {
                Some(("local:screen:preview", self.snapshot.user_id.as_str(), true))
            }
            _ => None,
        };
        if let Some((id, user, local)) = shared {
            video_tracks.push(CallVideoTrack {
                id: id.into(),
                user_id: user.into(),
                device_id: "PREVIEW".into(),
                local,
                source: VideoSource::Screen,
            });
            if let Some(frame) = preview_screen(1600, 1000) {
                self.preview_frames
                    .insert(id.into(), LatestFrame::fixed(frame));
            }
        }
        if cameras_on {
            for (index, p) in participants.iter().enumerate() {
                let local = p.user_id == self.snapshot.user_id;
                let id = format!("preview-camera-{index}");
                video_tracks.push(CallVideoTrack {
                    id: id.clone(),
                    user_id: p.user_id.clone(),
                    device_id: p.device_id.clone(),
                    local,
                    source: VideoSource::Camera,
                });
                if let Some(frame) = preview_camera(1280, 720, index as u32) {
                    self.preview_frames.insert(id, LatestFrame::fixed(frame));
                }
            }
        }
        self.snapshot.call = Some(CallState {
            ticket: None,
            screen_backend: (flag == "--preview-call-picker")
                .then_some(twayblade_core::calls::ScreenBackend::X11),
            screen_sources: if flag == "--preview-call-picker" {
                use twayblade_core::calls::{ScreenSelection, ScreenSource, SourceKind};
                // Generated stills; the last window has none, as when a
                // source gives no frame in time.
                let full = preview_screen(1600, 1000);
                let still = |width: u32, height: u32| {
                    let full = full.as_ref()?;
                    let mut bgra = Vec::with_capacity((width * height * 4) as usize);
                    for y in 0..height {
                        for x in 0..width {
                            let at = ((y * full.height / height) * full.width
                                + x * full.width / width)
                                as usize
                                * 4;
                            bgra.extend_from_slice(&full.bgra[at..at + 4]);
                        }
                    }
                    Some(twayblade_core::calls::Thumbnail {
                        width,
                        height,
                        bgra: bgra.into(),
                    })
                };
                [
                    (
                        SourceKind::Screen,
                        1,
                        Some("DP-1 · 2560 × 1440"),
                        still(192, 108),
                    ),
                    (
                        SourceKind::Screen,
                        2,
                        Some("HDMI-1 · 1920 × 1080"),
                        still(192, 108),
                    ),
                    (
                        SourceKind::Window,
                        3,
                        Some("Design review · Firefox"),
                        still(192, 120),
                    ),
                    (SourceKind::Window, 4, Some("Terminal"), None),
                ]
                .map(|(kind, id, title, thumbnail)| ScreenSource {
                    selection: ScreenSelection { kind, id },
                    title: title.map(Into::into),
                    thumbnail,
                })
                .to_vec()
            } else {
                Vec::new()
            },
            events,
            local: LocalCallState {
                muted: false,
                video: false,
                input_device: Some("builtin".into()),
                output_device: Some("speakers".into()),
                camera: None,
                screen: if sharing {
                    ScreenShare::Live
                } else {
                    ScreenShare::Off
                },
            },
            devices: CallDevices {
                inputs: vec![device("builtin", "MacBook Pro Microphone")],
                outputs: vec![device("speakers", "MacBook Pro Speakers")],
                cameras: vec![device("facetime", "FaceTime HD Camera")],
            },
            video_tracks: video_tracks.clone(),
            room_id: room.into(),
            kind: if direct {
                CallKind::Direct
            } else {
                CallKind::Group
            },
            phase,
            participants: participants.clone(),
            muted: Some(false),
            video: Some(false),
            screen_sharing: Some(sharing),
            error: (flag == "--preview-call-ended").then_some(CallError::ReconnectTimeout),
        });
        self.snapshot.room_calls.insert(
            room.into(),
            ActiveCall {
                participants,
                started_ms: Some(now - 754_000),
                expires_ms: None,
            },
        );
        // A call in another room shows the join button there.
        if flag == "--preview-call" {
            self.snapshot.room_calls.insert(
                "research".into(),
                ActiveCall {
                    participants: vec![person("@sam:preview.invalid", Some("Sam"), None, false)],
                    started_ms: Some(now),
                    expires_ms: None,
                },
            );
        }
        self.navigation.selected_room = Some(room.into());
        self.snapshot.selected_room = room.into();
    }
}

/// A generated stand-in for a camera: a soft gradient with a head and
/// shoulders silhouette. Preview captures only.
fn preview_camera(width: u32, height: u32, seed: u32) -> Option<twayblade_core::calls::VideoFrame> {
    let tints = [
        (70u32, 110u32, 150u32),
        (120, 90, 140),
        (90, 130, 110),
        (150, 110, 90),
    ];
    let (tr, tg, tb) = tints[seed as usize % tints.len()];
    let mut bgra = vec![0u8; width as usize * height as usize * 4];
    let (cx, cy) = (width as f32 / 2., height as f32 * 0.42);
    for y in 0..height {
        for x in 0..width {
            let fade = 0.55 + 0.45 * (y as f32 / height as f32);
            let (dx, dy) = (x as f32 - cx, y as f32 - cy);
            let head = (dx * dx + dy * dy).sqrt() < height as f32 * 0.17;
            let (sx, sy) = (
                dx / (width as f32 * 0.24),
                (y as f32 - height as f32 * 0.98) / (height as f32 * 0.42),
            );
            let body = sx * sx + sy * sy < 1.;
            let (r, g, b) = if head || body {
                (225u32, 205u32, 190u32)
            } else {
                (
                    (tr as f32 * fade) as u32,
                    (tg as f32 * fade) as u32,
                    (tb as f32 * fade) as u32,
                )
            };
            let at = (y as usize * width as usize + x as usize) * 4;
            bgra[at..at + 4].copy_from_slice(&[b as u8, g as u8, r as u8, 255]);
        }
    }
    twayblade_core::calls::VideoFrame::new(width, height, 0, 0, bgra).ok()
}

/// A generated stand-in for a shared window: title bar, sidebar, text lines
/// and one accent block. Preview captures only.
fn preview_screen(width: u32, height: u32) -> Option<twayblade_core::calls::VideoFrame> {
    let mut bgra = vec![0u8; width as usize * height as usize * 4];
    let lines = |y: u32| y % 34 < 12;
    for y in 0..height {
        for x in 0..width {
            let (r, g, b) = if y < 52 {
                (222, 222, 226)
            } else if x < 340 {
                if (90..440).contains(&y) && lines(y - 90) && (32..280).contains(&x) {
                    (172, 172, 180)
                } else {
                    (236, 236, 240)
                }
            } else if (120..420).contains(&y) && (420..1100).contains(&x) {
                (64, 120, 230)
            } else if (480..940).contains(&y)
                && lines(y - 480)
                && x >= 420
                && x < width - 180 - (y % 7) * 60
            {
                (190, 190, 196)
            } else {
                (250, 250, 252)
            };
            let at = (y as usize * width as usize + x as usize) * 4;
            bgra[at..at + 4].copy_from_slice(&[b, g, r, 255]);
        }
    }
    twayblade_core::calls::VideoFrame::new(width, height, 0, 0, bgra).ok()
}
