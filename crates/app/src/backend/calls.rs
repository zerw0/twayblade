#![allow(
    dead_code,
    reason = "Call contracts for the UI contributor; native call views are owned by the UI contributor"
)]
use super::{Account, Backend, Snapshot, State, publish};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{mpsc, watch};
const QUEUE_CAPACITY: usize = 32;
use twayblade_core::calls::{
    CallError, CallKind, CallPhase, CallState, CallTicket, EndReason, NativeCall, NativeControl,
};

pub(crate) enum Control {
    Muted(bool),
    Video(bool),
    Input(String),
    Output(String),
    Camera(String),
    Leave,
}
impl State {
    pub(super) fn cancel_call(&mut self) {
        self.call_request = self.call_request.wrapping_add(1);
        self.call_control = None;
        self.call_video = Default::default();
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
    pub(crate) fn call_video_handle(
        &self,
        expected: u128,
        id: &str,
    ) -> Option<twayblade_core::calls::LatestFrame> {
        let state = self.state.lock().ok()?;
        if state.account.as_ref()?.instance_id() != expected {
            return None;
        }
        state.call_video.get(id)
    }

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
                local: Default::default(),
                devices: Default::default(),
                video_tracks: Vec::new(),
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
                if let Some(call) = &mut v.call
                    && room_left
                {
                    call.phase = CallPhase::Ended(EndReason::RoomLeft);
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
    let _ = join; // Start and join use the same room-scoped MatrixRTC session.
    {
        let state = shared.lock().map_err(|_| CallError::StaleRequest)?;
        if !state.accepts_call(ticket) {
            return Err(CallError::StaleRequest);
        }
    }
    let mut call = NativeCall::start(&account, &room, video).await?.drive();
    {
        let mut state = shared.lock().map_err(|_| CallError::StaleRequest)?;
        if !state.accepts_call(ticket) {
            return Err(CallError::StaleRequest);
        }
        state.call_video = call.video_tracks();
    }
    update(&shared, &updates, ticket, |state| {
        *state = call.updates.borrow().clone()
    })?;
    loop {
        tokio::select! {
            result=call.updates.changed()=>{
                result.map_err(|_|CallError::SessionClosed)?;
                let snapshot=call.updates.borrow().clone();
                update(&shared,&updates,ticket,|state|*state=snapshot.clone())?;
                if let CallPhase::Ended(reason)=snapshot.phase{return snapshot.error.map_or(Ok(reason),Err);}
            },
            control=controls.recv()=>{
                let control=match control{
                    Some(Control::Muted(v))=>NativeControl::Muted(v),
                    Some(Control::Video(v))=>NativeControl::Video(v),
                    Some(Control::Input(id))=>NativeControl::Input(id),
                    Some(Control::Output(id))=>NativeControl::Output(id),
                    Some(Control::Camera(id))=>NativeControl::Camera(id),
                    _=>{call.leave().await?;return Ok(EndReason::Left);}
                };
                call.control(control)?;
                update(&shared,&updates,ticket,|state|*state=call.updates.borrow().clone())?;
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
