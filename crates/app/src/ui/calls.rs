//! Native call views: the header call buttons, the call stage above the
//! timeline, the in-another-room strip, the incoming-call card and device
//! notices. Call state comes from the backend snapshot; video from bounded
//! latest-frame handles, each drawn by its own small view so only it repaints.
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

/// More than this many people share one "+N" tile.
const MAX_TILES: usize = 8;
/// Height of a shared screen shown large above the tiles.
const SCREEN_HEIGHT: f32 = 300.;
/// Tiles in the column beside a shared screen, the last one "+N" past that.
const SIDE_TILES: usize = 3;

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

fn live(call: &CallState) -> bool {
    !matches!(call.phase, CallPhase::Ended(_))
}

/// User wording for call failures; core returns only typed errors.
pub(crate) fn call_error_text(error: CallError) -> &'static str {
    match error {
        CallError::NoFoci | CallError::UnsupportedServer => "Your server doesn't support calls.",
        CallError::MediaE2eeRequired => "The call couldn't be encrypted end to end.",
        CallError::KeyRejected => "Encryption keys couldn't be exchanged.",
        CallError::TransportFailed => "The connection was lost.",
        CallError::MembershipForbidden => "You don't have permission to call in this room.",
        CallError::DeviceUnavailable => "Your microphone or camera isn't available.",
        CallError::ScreenUnavailable => "Screen sharing couldn't start.",
        CallError::PermissionDenied => {
            "Twayblade can't use your microphone or camera. Allow it in System Settings → Privacy & Security."
        }
        CallError::PermissionRestricted => {
            "Microphone and camera access is restricted on this Mac."
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

impl MatrixApp {
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

    fn fresh_call(&mut self) {
        self.call_end_dismissed = false;
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

    /// Keeps one video view per shown track and a 1 s clock while in a call.
    pub(crate) fn sync_call_views(&mut self, cx: &mut Context<Self>) {
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
        let joined = self
            .snapshot
            .room_calls
            .get(room_id)
            .map_or(0, |call| call.participants.len());
        if joined > 0 && !busy {
            let room = room_id.to_owned();
            let label = format!("Join call · {joined}");
            return Some(
                Button::new("call-join")
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
            .child(div().truncate().child(name.clone()));
        div()
            .id(SharedString::from(format!(
                "call-tile-{}-{}",
                participant.user_id, participant.device_id
            )))
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
            .into_any_element()
    }

    fn track_owner(&self, call: &CallState, track: &CallVideoTrack) -> String {
        call.participants
            .iter()
            .find(|p| p.user_id == track.user_id && p.device_id == track.device_id)
            .and_then(|p| p.display_name.clone())
            .unwrap_or_else(|| track.user_id.clone())
    }

    /// Someone else's shared screen, large above the tiles. With several,
    /// the chosen one (or the first) is shown and the others can be picked.
    fn screen_presentation(&self, call: &CallState, cx: &mut Context<Self>) -> Option<AnyElement> {
        let screens: Vec<&CallVideoTrack> = call
            .video_tracks
            .iter()
            .filter(|t| {
                t.source == VideoSource::Screen && !t.local && self.call_tiles.contains_key(&t.id)
            })
            .collect();
        let shown = screens
            .iter()
            .find(|t| self.call_screen_focus.as_ref() == Some(&t.id))
            .or(screens.first())?;
        let view = self.call_tiles.get(&shown.id)?.clone();
        let owner = self.track_owner(call, shown);
        let switcher = (screens.len() > 1).then(|| {
            h_flex()
                .absolute()
                .top(px(8.))
                .right(px(8.))
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
                            cx.notify();
                        }))
                }))
        });
        Some(
            div()
                .id("call-screen")
                .relative()
                .flex_1()
                .min_w_0()
                .h(px(SCREEN_HEIGHT))
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
        let presentation = if ended {
            None
        } else {
            self.screen_presentation(call, cx)
        };
        // You first, then everyone else in membership order.
        let mut people: Vec<&Participant> = call.participants.iter().collect();
        people.sort_by_key(|p| !self.is_local(p));
        let has_video = call
            .video_tracks
            .iter()
            .any(|t| t.source == VideoSource::Camera && self.call_tiles.contains_key(&t.id));
        // A shared screen takes the width; people sit in a column beside it.
        let side = presentation.is_some();
        let size = match (people.len(), has_video) {
            _ if side => (132., (SCREEN_HEIGHT - 2. * 8.) / SIDE_TILES as f32),
            (_, false) => (132., 96.),
            (0..=2, true) => (260., 162.),
            (3..=4, true) => (200., 125.),
            _ => (156., 98.),
        };
        let limit = if side { SIDE_TILES } else { MAX_TILES };
        let overflow = people.len().saturating_sub(limit);
        let shown = if overflow > 0 {
            limit - 1
        } else {
            people.len()
        };
        let ringing_direct =
            matches!(call.phase, CallPhase::Ringing { .. }) && call.kind == CallKind::Direct;
        let tiles = (!ended && !people.is_empty()).then(|| {
            if side {
                v_flex().flex_none().gap(px(8.))
            } else {
                h_flex()
                    .flex_wrap()
                    .justify_center()
                    .gap(px(8.))
                    .px(px(16.))
                    .pt(px(12.))
            }
            .when(call.phase == CallPhase::Reconnecting, |row| {
                row.opacity(0.5)
            })
            .children(
                people
                    .iter()
                    .take(shown)
                    .map(|p| self.call_tile(p, call, size, cx)),
            )
            // Beside someone else's screen, the status line says you share.
            .children(
                (!side)
                    .then(|| self.own_screen_tile(call, size, cx))
                    .flatten(),
            )
            .when(ringing_direct, |row| {
                row.child(self.callee_tile(call, size, cx))
            })
            .when(overflow > 0, |row| {
                row.child(
                    v_flex()
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
            })
        });
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
        let status = v_flex()
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
                .child(
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
                .child(
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
                        .selected(screen != ScreenShare::Off)
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
                            let result = this.set_screen_share(screen == ScreenShare::Off);
                            this.call_result(result, cx)
                        })),
                )
                .child(self.call_devices_menu(call, cx))
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
        motion::rise(
            v_flex()
                .id("call-stage")
                .flex_none()
                .border_b_1()
                .border_color(cx.theme().border)
                .map(|stage| match presentation {
                    Some(screen) => stage.child(
                        h_flex()
                            .px(px(16.))
                            .pt(px(12.))
                            .gap(px(8.))
                            .child(screen)
                            .children(tiles),
                    ),
                    None => stage.children(tiles),
                })
                .children(notice)
                .child(
                    h_flex()
                        .h(px(52.))
                        .px(px(16.))
                        .gap(px(12.))
                        .child(status)
                        .child(controls),
                ),
            "call-stage-motion",
            self.snapshot.reduce_motion,
        )
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
                self.set_screen_share(screen == ScreenShare::Off)
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
            if flag == "--preview-call-group" {
                for (user, name) in [
                    ("@kai:preview.invalid", "Kai"),
                    ("@mira:preview.invalid", "Mira"),
                ] {
                    participants.push(person(user, Some(name), Some(false), false));
                }
            }
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
        self.snapshot.call = Some(CallState {
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
