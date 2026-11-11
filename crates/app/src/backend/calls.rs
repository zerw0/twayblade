#![allow(
    dead_code,
    reason = "Call contracts for the UI contributor; no call views in this spike"
)]
use super::{Account, Backend, Snapshot, State, publish};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{mpsc, watch};
use twayblade_call_host::{Failure, Frame, QUEUE_CAPACITY};
use twayblade_core::calls::{
    self, CallError, CallKind, CallPhase, CallState, CallTicket, EndReason,
};

pub(crate) enum Control {
    Muted(bool),
    Video(bool),
    Leave,
}
impl State {
    pub(super) fn cancel_call(&mut self) {
        self.call_request = self.call_request.wrapping_add(1);
        self.call_control = None;
        if let Some(task) = self.call_task.take() {
            task.abort();
        }
    }
    fn accepts_call(&self, ticket: CallTicket) -> bool {
        ticket.matches(
            self.account.as_ref().map(Account::instance_id),
            self.call_request,
        )
    }
}
impl Backend {
    pub(crate) fn start_call(
        &self,
        expected_account: u128,
        room: String,
        video: bool,
        join: bool,
    ) -> Result<(), CallError> {
        let mut state = self.state.lock().map_err(|_| CallError::StaleRequest)?;
        let account = state
            .account
            .clone()
            .filter(|a| a.instance_id() == expected_account)
            .ok_or(CallError::StaleRequest)?;
        if state.call_control.is_some() {
            return Err(CallError::AlreadyActive);
        }
        if !account.call_room_joined(&room) {
            return Err(CallError::InvalidRoom);
        }
        state.cancel_call();
        let ticket = CallTicket {
            account: expected_account,
            request: state.call_request,
        };
        let (tx, rx) = mpsc::channel(QUEUE_CAPACITY);
        state.call_control = Some(tx);
        publish(&self.updates, |v| {
            v.call = Some(CallState {
                room_id: room.clone(),
                kind: CallKind::Group,
                phase: CallPhase::Starting,
                participants: Vec::new(),
                muted: None,
                video: None,
                screen_sharing: None,
                error: None,
            })
        });
        let shared = self.state.clone();
        let updates = self.updates.clone();
        state.call_task = Some(self.runtime.spawn(async move {
            let result = run(
                account,
                room,
                video,
                join,
                ticket,
                shared.clone(),
                updates.clone(),
                rx,
            )
            .await;
            let Ok(mut state) = shared.lock() else {
                return;
            };
            if !state.accepts_call(ticket) {
                return;
            }
            state.call_control = None;
            publish(&updates, |v| {
                if let Some(call) = &mut v.call {
                    call.phase = CallPhase::Ended(match result {
                        Ok(reason) => reason,
                        Err(_) => EndReason::Failed,
                    });
                    call.error = result.err();
                }
            });
        }));
        Ok(())
    }
    pub(crate) fn call_control(&self, expected: u128, control: Control) -> Result<(), CallError> {
        let state = self.state.lock().map_err(|_| CallError::StaleRequest)?;
        if state.account.as_ref().map(Account::instance_id) != Some(expected) {
            return Err(CallError::StaleRequest);
        }
        state
            .call_control
            .as_ref()
            .ok_or(CallError::ControlUnavailable)?
            .try_send(control)
            .map_err(|_| CallError::ControlUnavailable)
    }
    pub(crate) fn leave_call(&self, expected: u128) -> Result<(), CallError> {
        let mut state = self.state.lock().map_err(|_| CallError::StaleRequest)?;
        if state.account.as_ref().map(Account::instance_id) != Some(expected) {
            return Err(CallError::StaleRequest);
        }
        state.cancel_call();
        publish(&self.updates, |v| {
            if let Some(call) = &mut v.call {
                call.phase = CallPhase::Ended(EndReason::Left);
            }
        });
        Ok(())
    }
}
pub(super) fn observe(backend: &Backend, state: &mut State, account: Account) {
    let updates = backend.updates.clone();
    let shared = backend.state.clone();
    let instance = account.instance_id();
    state.tasks.push(backend.runtime.spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tick.tick().await;
            let calls = account.room_calls().await;
            let Ok(mut state) = shared.lock() else {
                return;
            };
            if state.account.as_ref().map(Account::instance_id) != Some(instance) {
                return;
            }
            let room_left = updates
                .borrow()
                .call
                .as_ref()
                .is_some_and(|call| !account.call_room_joined(&call.room_id));
            if room_left {
                state.cancel_call();
            }
            publish(&updates, |v| {
                if let Some(call) = &mut v.call {
                    if room_left {
                        call.phase = CallPhase::Ended(EndReason::RoomLeft);
                    }
                    call.participants = calls
                        .get(&call.room_id)
                        .map(|c| c.participants.clone())
                        .unwrap_or_default();
                }
                v.room_calls = calls;
            });
        }
    }));
}
fn update(
    shared: &Mutex<State>,
    updates: &watch::Sender<Snapshot>,
    ticket: CallTicket,
    f: impl FnOnce(&mut CallState),
) -> Result<(), CallError> {
    let state = shared.lock().map_err(|_| CallError::StaleRequest)?;
    if !state.accepts_call(ticket) {
        return Err(CallError::StaleRequest);
    }
    publish(updates, |v| {
        if let Some(call) = &mut v.call {
            f(call);
        }
    });
    Ok(())
}
#[allow(clippy::too_many_arguments)]
async fn run(
    account: Account,
    room: String,
    video: bool,
    join: bool,
    ticket: CallTicket,
    shared: Arc<Mutex<State>>,
    updates: watch::Sender<Snapshot>,
    mut controls: mpsc::Receiver<Control>,
) -> Result<EndReason, CallError> {
    let widget = tokio::time::timeout(
        Duration::from_secs(30),
        account.call_widget(&room, join, video),
    )
    .await
    .map_err(|_| CallError::UnsupportedServer)??;
    let direct = widget.room.is_direct().await.unwrap_or(false);
    update(&shared, &updates, ticket, |c| {
        c.kind = if direct {
            CallKind::Direct
        } else {
            CallKind::Group
        }
    })?;
    // Check admission after discovery and before the only runtime spawn path.
    let mut runtime = {
        let state = shared.lock().map_err(|_| CallError::StaleRequest)?;
        if !state.accepts_call(ticket) {
            return Err(CallError::StaleRequest);
        }
        // spawn_runtime has no suspension before spawning/configuration; its
        // synchronous counterpart keeps ticket admission atomic with spawn.
        let runtime =
            twayblade_call_host::supervisor::Supervisor::spawn(&calls::runtime_executable()?)
                .map_err(|_| CallError::RuntimeExited)?;
        runtime
            .send
            .try_send(Frame::Configure {
                url: widget.url.clone(),
                jwt_service: widget.jwt_service.clone(),
            })
            .map_err(|_| CallError::RuntimeExited)?;
        runtime
    };
    let mut outstanding = HashSet::new();
    let mut sdk_requests = HashSet::new();
    let mut control_request: Option<String> = None;
    let mut control_deadline = tokio::time::Instant::now();
    let mut sequence = 0u64;
    loop {
        tokio::select! {
            exit = &mut runtime.exited => return match exit {
                Ok(twayblade_call_host::supervisor::Exit::Closed) => Ok(EndReason::RuntimeClosed),
                Ok(exit) => Err(calls::runtime_error(exit)),
                Err(_) => Err(CallError::RuntimeExited),
            },
            _ = tokio::time::sleep_until(control_deadline), if control_request.is_some() => return Err(CallError::ControlUnavailable),
            control = controls.recv(), if control_request.is_none() => {
                let data = match control {
                    Some(Control::Muted(muted)) => serde_json::json!({"audio_enabled":!muted}),
                    Some(Control::Video(video)) => serde_json::json!({"video_enabled":video}),
                    _ => return Ok(EndReason::Left),
                };
                sequence += 1;
                let request = format!("twayblade-{sequence}");
                control_request = Some(request.clone());
                control_deadline = tokio::time::Instant::now() + Duration::from_secs(5);
                runtime.send.try_send(Frame::Widget(serde_json::json!({"api":"toWidget","widgetId":widget.widget_id,"requestId":request,"action":"io.element.device_mute","data":data}))).map_err(|_| CallError::WidgetRejected)?;
            },
            frame = runtime.receive.recv() => {
                match frame {
                    Some(Frame::Ready) => update(&shared, &updates, ticket, |c| c.phase = CallPhase::Joining)?,
                    Some(Frame::ServiceHealth(true)) => {},
                    Some(Frame::ServiceHealth(false)) => return Err(CallError::UnsupportedServer),
                    Some(Frame::Environment { secure_context, media_devices, script_transform }) => {
                        if !secure_context || !media_devices || !script_transform { return Err(CallError::UnsupportedRuntime); }
                    }
                    Some(Frame::Failed(failure)) => return Err(if failure == Failure::PermissionDenied { CallError::PermissionDenied } else { CallError::WidgetRejected }),
                    Some(Frame::Widget(value)) => {
                        if !widget.accepts_message(&value).await { return Err(CallError::WidgetRejected); }
                        let action = value.get("action").and_then(serde_json::Value::as_str).unwrap_or("");
                        let response = value.get("response");
                        if action == "im.vector.hangup" || action == "io.element.close" { return Ok(EndReason::Left); }
                        if action == "io.element.join" && response.is_none() {
                            // This is Element Call's own joined-session notification,
                            // not an inference from presence or process readiness.
                            update(&shared, &updates, ticket, |c| c.phase = CallPhase::Connected)?;
                            let mut ack = value.clone(); ack["response"] = serde_json::json!({});
                            runtime.send.try_send(Frame::Widget(ack)).map_err(|_| CallError::WidgetRejected)?;
                            continue;
                        }
                        if action == "io.element.device_mute" {
                            let is_response = response.is_some();
                            if is_response && value.get("requestId").and_then(serde_json::Value::as_str) != control_request.as_deref() { return Err(CallError::WidgetRejected); }
                            let data = response.unwrap_or(&value["data"]);
                            update(&shared, &updates, ticket, |c| {
                                if let Some(audio) = data.get("audio_enabled").and_then(serde_json::Value::as_bool) { c.muted = Some(!audio); }
                                if let Some(video) = data.get("video_enabled").and_then(serde_json::Value::as_bool) { c.video = Some(video); }
                            })?;
                            if is_response { control_request = None; } else {
                                let mut ack = value.clone(); ack["response"] = serde_json::json!({});
                                runtime.send.try_send(Frame::Widget(ack)).map_err(|_| CallError::WidgetRejected)?;
                            }
                            continue;
                        }
                        if response.is_none() {
                            if outstanding.len() >= QUEUE_CAPACITY { return Err(CallError::WidgetRejected); }
                            let id = value.get("requestId").and_then(serde_json::Value::as_str).ok_or(CallError::WidgetRejected)?.to_owned();
                            if !outstanding.insert(id) { return Err(CallError::WidgetRejected); }
                        } else {
                            let id = value.get("requestId").and_then(serde_json::Value::as_str).ok_or(CallError::WidgetRejected)?;
                            if !sdk_requests.remove(id) { return Err(CallError::WidgetRejected); }
                        }
                        if !widget.handle.send(value.to_string()) { return Err(CallError::WidgetRejected); }
                    }
                    _ => return Err(CallError::RuntimeExited),
                }
            },
            message = widget.handle.recv() => {
                let message = message.ok_or(CallError::WidgetRejected)?;
                if message.len() > twayblade_call_host::MAX_FRAME { return Err(CallError::WidgetRejected); }
                let value: serde_json::Value = serde_json::from_str(&message).map_err(|_| CallError::WidgetRejected)?;
                if !calls::policy::outgoing_room_scoped(&value, &room) {
                    let mut ack = value; ack["response"] = serde_json::json!({});
                    if !widget.handle.send(ack.to_string()) { return Err(CallError::WidgetRejected); }
                    continue;
                }
                if value.get("response").is_some() && let Some(id) = value.get("requestId").and_then(serde_json::Value::as_str) { outstanding.remove(id); }
                if value.get("response").is_none() {
                    let id = value.get("requestId").and_then(serde_json::Value::as_str).ok_or(CallError::WidgetRejected)?;
                    if sdk_requests.len() >= QUEUE_CAPACITY || !sdk_requests.insert(id.to_owned()) { return Err(CallError::WidgetRejected); }
                }
                runtime.send.try_send(Frame::Widget(value)).map_err(|_| CallError::WidgetRejected)?;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelling_a_call_invalidates_pending_work_and_drops_controls() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let mut state = State::default();
        let task = runtime.spawn(std::future::pending());
        let aborted = task.abort_handle();
        let (tx, mut rx) = mpsc::channel(QUEUE_CAPACITY);
        state.call_task = Some(task);
        state.call_control = Some(tx);
        let request = state.call_request;
        state.cancel_call();
        assert_ne!(state.call_request, request);
        assert!(state.call_control.is_none());
        runtime.block_on(async {
            assert!(rx.recv().await.is_none());
            tokio::time::timeout(Duration::from_secs(2), async {
                while !aborted.is_finished() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
        });
    }
}
