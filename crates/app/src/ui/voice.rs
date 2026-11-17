//! Voice messages (#184): recording from the composer's ＋ menu and inline
//! playback with a waveform. Audio I/O lives in `twayblade_media::voice`.
use super::*;
use crate::backend::TimelineAction;
use std::sync::Arc;
use std::time::Duration;
use twayblade_media::voice::{self as audio, Player, Recorder};

/// Bars drawn for a waveform.
const BARS: usize = 36;

/// A voice message being played, and its decoded audio.
pub(crate) struct VoicePlayback {
    pub id: String,
    pub player: Option<Player>,
    pub pcm: Arc<Vec<i16>>,
    /// Where playback resumes after a pause, in samples.
    pub resume: usize,
}

pub(crate) fn clock(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// `points` scaled to `bars` values (0–1), taking the loudest of each span.
pub(crate) fn bars(points: &[f32], bars: usize) -> Vec<f32> {
    if points.is_empty() {
        return vec![0.15; bars];
    }
    (0..bars)
        .map(|bar| {
            let from = bar * points.len() / bars;
            let to = ((bar + 1) * points.len() / bars)
                .max(from + 1)
                .min(points.len());
            points[from..to].iter().copied().fold(0f32, f32::max)
        })
        .collect()
}

impl MatrixApp {
    /// Redraws while recording or playing, ten times a second.
    fn tick_voice(&mut self, cx: &mut Context<Self>) {
        if self.voice_ticking {
            return;
        }
        self.voice_ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let active = this
                    .update(cx, |this, cx| {
                        if this.voice_recorder.as_ref().is_some_and(Recorder::stopped) {
                            this.status = "Recording stopped.".into();
                        }
                        if let Some(playing) = &mut this.voice_playback
                            && playing.player.as_ref().is_some_and(Player::finished)
                        {
                            playing.player = None;
                            playing.resume = 0;
                        }
                        cx.notify();
                        let active = this.voice_recorder.is_some()
                            || this
                                .voice_playback
                                .as_ref()
                                .is_some_and(|p| p.player.is_some());
                        if !active {
                            this.voice_ticking = false;
                        }
                        active
                    })
                    .unwrap_or(false);
                if !active {
                    break;
                }
            }
        })
        .detach();
    }

    /// Asks for the microphone, then starts recording.
    pub(crate) fn start_voice_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview || self.voice_recorder.is_some() || self.voice_starting {
            return;
        }
        self.voice_starting = true;
        self.voice_playback = None;
        let task = cx.background_spawn(async move {
            use twayblade_platform::media_permissions::{CapturePermission, request};
            request(CapturePermission::Microphone)
                .await
                .map_err(|_| "Allow Twayblade to use the microphone in System Settings › Privacy & Security.")?;
            Recorder::start().map_err(|error| match error {
                audio::VoiceError::NoDevice => "No microphone is connected.",
                _ => "The microphone could not be started.",
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.voice_starting = false;
                match result {
                    Ok(recorder) => {
                        this.voice_recorder = Some(recorder);
                        this.status.clear();
                        this.tick_voice(cx);
                    }
                    Err(message) => this.status = message.into(),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn cancel_voice_recording(&mut self, cx: &mut Context<Self>) {
        self.voice_recorder = None;
        cx.notify();
    }

    fn send_voice_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(recorder) = self.voice_recorder.take() else {
            return;
        };
        let Some(room) = self.navigation.selected_room.clone() else {
            return;
        };
        let task = cx.background_spawn(async move { recorder.finish() });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(recording) if this.navigation.selected_room.as_ref() == Some(&room) => {
                        this.timeline_action(
                            format!("voice-{}", recording.duration.as_millis()),
                            TimelineAction::SendVoice {
                                ogg: recording.ogg,
                                duration: recording.duration,
                                waveform: recording.waveform,
                            },
                            window,
                            cx,
                        );
                    }
                    Ok(_) => {}
                    Err(audio::VoiceError::Empty) => {
                        this.status = "The recording was too short to send.".into()
                    }
                    Err(_) => this.status = "The recording could not be prepared.".into(),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Replaces the composer while recording: cancel, time, level, send.
    pub(crate) fn voice_recording_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let (elapsed, level, stopped) = self
            .voice_recorder
            .as_ref()
            .map(|r| (r.elapsed(), r.level(), r.stopped()))
            .unwrap_or_default();
        let palette = theme::palette(cx);
        h_flex()
            .id("voice-recording")
            .role(Role::Status)
            .aria_label(format!("Recording voice message, {}", clock(elapsed)))
            .w_full()
            .h(px(42.))
            .px(px(6.))
            .gap(px(10.))
            .rounded(px(9.))
            .border_1()
            .border_color(cx.theme().input)
            .bg(palette.field)
            .child(
                Button::new("voice-cancel")
                    .ghost()
                    .size(px(28.))
                    .icon(IconName::Trash)
                    .accessibility_label("Discard voice message")
                    .tooltip("Discard")
                    .on_click(cx.listener(|this, _, _, cx| this.cancel_voice_recording(cx))),
            )
            .child(div().size(px(8.)).rounded_full().bg(if stopped {
                muted
            } else {
                palette.error
            }))
            .child(div().w(px(44.)).text_size(px(13.)).child(clock(elapsed)))
            .child(
                // Input level, so you can see the microphone hears you.
                div()
                    .flex_1()
                    .h(px(4.))
                    .rounded(px(2.))
                    .bg(cx.theme().border)
                    .child(
                        div()
                            .h_full()
                            .rounded(px(2.))
                            .w(relative((level * 4.).min(1.)))
                            .bg(cx.theme().primary),
                    ),
            )
            .child(
                Button::new("voice-send")
                    .primary()
                    .h(px(26.))
                    .w(px(30.))
                    .rounded(px(6.))
                    .icon(IconName::Send)
                    .accessibility_label("Send voice message")
                    .tooltip("Send  ↩")
                    .on_click(
                        cx.listener(|this, _, window, cx| this.send_voice_recording(window, cx)),
                    ),
            )
            .test_support()
            .into_any_element()
    }

    /// Plays or pauses a voice message, loading its audio the first time.
    fn toggle_voice(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(playing) = &mut self.voice_playback
            && playing.id == id
        {
            if let Some(player) = playing.player.take() {
                playing.resume = player.position();
            } else {
                match Player::start(playing.pcm.clone(), playing.resume) {
                    Ok(player) => playing.player = Some(player),
                    Err(_) => self.status = "No speaker or headphones are available.".into(),
                }
                self.tick_voice(cx);
            }
            cx.notify();
            return;
        }
        if self.preview || self.voice_loading.is_some() {
            return;
        }
        let Some(room) = self.navigation.selected_room.clone() else {
            return;
        };
        self.voice_playback = None;
        self.voice_loading = Some(id.clone());
        let account = self.snapshot.account_instance;
        self.job(
            window,
            cx,
            move |backend| {
                backend
                    .voice_audio(room, account, id.clone())
                    .map(|pcm| (id, pcm))
            },
            |this, result, _, cx| {
                this.voice_loading = None;
                match result {
                    Ok((id, pcm)) => {
                        let pcm = Arc::new(pcm);
                        match Player::start(pcm.clone(), 0) {
                            Ok(player) => {
                                this.voice_playback = Some(VoicePlayback {
                                    id,
                                    player: Some(player),
                                    pcm,
                                    resume: 0,
                                });
                                this.tick_voice(cx);
                            }
                            Err(_) => {
                                this.status = "No speaker or headphones are available.".into()
                            }
                        }
                    }
                    Err(error) => this.status = error.to_string(),
                }
                cx.notify();
            },
        );
        cx.notify();
    }

    /// A voice message in the timeline: play/pause, waveform, time.
    pub(crate) fn voice_bubble(
        &self,
        message: &crate::backend::Message,
        voice: &twayblade_core::conversation::Voice,
        outgoing: bool,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let total = Duration::from_millis(voice.duration_ms);
        let playing = self.voice_playback.as_ref().filter(|p| p.id == message.id);
        let running = playing.is_some_and(|p| p.player.is_some());
        let loading = self.voice_loading.as_ref() == Some(&message.id);
        let progress = playing.map_or(0., |p| {
            let at = p.player.as_ref().map_or(p.resume, Player::position);
            at as f32 / p.pcm.len().max(1) as f32
        });
        let shown = if playing.is_some() {
            Duration::from_secs_f32(total.as_secs_f32() * progress)
        } else {
            total
        };
        let foreground = if outgoing {
            cx.theme().primary_foreground
        } else {
            cx.theme().foreground
        };
        let id = message.id.clone();
        let heights = bars(&voice.waveform, BARS);
        let played = (progress * BARS as f32) as usize;
        let waveform = h_flex().h(px(24.)).gap(px(2.)).items_center().children(
            heights.into_iter().enumerate().map(|(index, height)| {
                div()
                    .w(px(3.))
                    .h(px(4. + height * 20.))
                    .rounded(px(1.5))
                    .bg(foreground.opacity(if index < played { 1. } else { 0.4 }))
            }),
        );
        Bubble::new()
            .max_w_full()
            .alignment(if outgoing {
                MessageAlignment::End
            } else {
                MessageAlignment::Start
            })
            .with_variant(if self.preferences.chat.style != ChatStyle::Bubbles {
                BubbleVariant::Ghost
            } else if outgoing {
                BubbleVariant::Filled
            } else {
                BubbleVariant::Secondary
            })
            .content(
                BubbleContent::new().child(
                    h_flex()
                        .flex_none()
                        .gap(px(10.))
                        .items_center()
                        .child(
                            Button::new(SharedString::from(format!("voice-play-{}", message.id)))
                                .ghost()
                                .size(px(28.))
                                .rounded_full()
                                .text_color(foreground)
                                .icon(if running {
                                    IconName::Pause
                                } else {
                                    IconName::Play
                                })
                                .loading(loading)
                                .accessibility_label(if running {
                                    "Pause voice message"
                                } else {
                                    "Play voice message"
                                })
                                .disabled(disabled || message.event_id.is_none())
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.toggle_voice(id.clone(), window, cx)
                                })),
                        )
                        .child(waveform)
                        .child(
                            div()
                                .flex_none()
                                .text_size(px(12.))
                                .text_color(foreground.opacity(0.8))
                                .child(clock(shown)),
                        ),
                ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{bars, clock};
    use std::time::Duration;

    #[test]
    fn bars_take_the_loudest_point_of_each_span() {
        assert_eq!(bars(&[0.1, 0.9, 0.2, 0.4], 2), vec![0.9, 0.4]);
        assert_eq!(bars(&[0.5], 3), vec![0.5, 0.5, 0.5]);
        assert_eq!(bars(&[], 2), vec![0.15, 0.15]);
        assert_eq!(clock(Duration::from_secs(75)), "1:15");
    }
}
