#![allow(
    dead_code,
    reason = "Call contracts for the UI contributor; native call views are owned by the UI contributor"
)]
use super::{Account, Backend, Snapshot, State, publish};
use std::sync::{Arc, Mutex};
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
    Screen(bool),
    ScreenSources(twayblade_core::calls::SourceKind),
    ShareScreen(CallTicket, twayblade_core::calls::ScreenSelection),
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
    /// Source listing is read-only. Capture the returned ticket with the menu;
    /// use it for the eventual Share action, not a newly fetched call identity.
    pub(crate) fn request_screen_sources(
        &self,
        expected: u128,
        kind: twayblade_core::calls::SourceKind,
    ) -> Result<CallTicket, CallError> {
        let state = self.state.lock().map_err(|_| CallError::StaleRequest)?;
        if state.account.as_ref().map(Account::instance_id) != Some(expected) {
            return Err(CallError::StaleRequest);
        }
        let ticket = CallTicket {
            account: expected,
            request: state.call_request,
        };
        state
            .call_control
            .as_ref()
            .ok_or(CallError::ControlUnavailable)?
            .try_send(Control::ScreenSources(kind))
            .map_err(|_| CallError::ControlUnavailable)?;
        Ok(ticket)
    }
    pub(crate) fn share_screen_source(
        &self,
        ticket: CallTicket,
        selection: twayblade_core::calls::ScreenSelection,
    ) -> Result<(), CallError> {
        let state = self.state.lock().map_err(|_| CallError::StaleRequest)?;
        if !state.accepts_call(ticket) {
            return Err(CallError::StaleRequest);
        }
        state
            .call_control
            .as_ref()
            .ok_or(CallError::ControlUnavailable)?
            .try_send(Control::ShareScreen(ticket, selection))
            .map_err(|_| CallError::ControlUnavailable)
    }
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
        launch_call(
            self.runtime.handle(),
            &self.state,
            &self.updates,
            expected_account,
            room,
            video,
            join,
            None,
        )
    }
    pub(crate) fn accept_call(
        &self,
        expected: u128,
        room: String,
        video: bool,
    ) -> Result<(), CallError> {
        let invitation = self
            .updates
            .borrow()
            .incoming_calls
            .iter()
            .find(|c| c.room_id == room)
            .map(|c| c.event_id.clone())
            .ok_or(CallError::StaleRequest)?;
        launch_call(
            self.runtime.handle(),
            &self.state,
            &self.updates,
            expected,
            room,
            video,
            true,
            Some(&invitation),
        )
    }
    pub(crate) fn incoming_action(
        &self,
        expected: u128,
        action: twayblade_core::calls::IncomingAction,
    ) -> Result<(), CallError> {
        let state = self.state.lock().map_err(|_| CallError::StaleRequest)?;
        if state.account.as_ref().map(Account::instance_id) != Some(expected) {
            return Err(CallError::StaleRequest);
        }
        state
            .incoming_control
            .as_ref()
            .ok_or(CallError::ControlUnavailable)?
            .try_send(action)
            .map_err(|_| CallError::ControlUnavailable)
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
                call.screen_sources.clear();
            }
        });
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
fn launch_call(
    runtime: &tokio::runtime::Handle,
    shared: &Arc<Mutex<State>>,
    updates: &watch::Sender<Snapshot>,
    expected_account: u128,
    room: String,
    video: bool,
    join: bool,
    invitation: Option<&str>,
) -> Result<(), CallError> {
    let mut state = shared.lock().map_err(|_| CallError::StaleRequest)?;
    let account = state
        .account
        .clone()
        .filter(|a| a.instance_id() == expected_account)
        .ok_or(CallError::StaleRequest)?;
    if let Some(id) = invitation
        && !updates.borrow().incoming_calls.iter().any(|c| {
            c.event_id == id
                && c.room_id == room
                && c.expires > twayblade_core::calls::ringing::now_ms()
        })
    {
        return Err(CallError::StaleRequest);
    }
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
    publish(updates, |v| {
        v.call = Some(CallState {
            screen_backend: None,
            screen_sources: Vec::new(),
            events: Vec::new(),
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
    let shared = shared.clone();
    let updates = updates.clone();
    state.call_task = Some(runtime.spawn(async move {
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
                call.screen_sources.clear();
            }
        });
    }));
    Ok(())
}
pub(super) fn observe(backend: &Backend, state: &mut State, account: Account) {
    use futures_util::StreamExt;
    let updates = backend.updates.clone();
    let shared = backend.state.clone();
    let instance = account.instance_id();
    let (actions, receive) = mpsc::channel(32);
    state.incoming_control = Some(actions);
    state
        .tasks
        .push(backend.runtime.spawn(incoming_notifications(
            account.clone(),
            backend.state.clone(),
            backend.updates.clone(),
            backend.notification_config.subscribe(),
        )));
    state.tasks.push(backend.runtime.spawn(async move {
        let activity = account.call_activity(receive);
        tokio::pin!(activity);
        while let Some(activity) = activity.next().await {
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
                    call.screen_sources.clear();
                }
                v.room_calls = activity.room_calls;
                v.incoming_calls = activity.incoming_calls;
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
    {
        let state = shared.lock().map_err(|_| CallError::StaleRequest)?;
        if !state.accepts_call(ticket) {
            return Err(CallError::StaleRequest);
        }
    }
    let mut native = NativeCall::start(&account, &room, video).await?;
    if !join {
        // A failed ring leaves a working call that others can still join.
        let _ = tokio::time::timeout(std::time::Duration::from_secs(10), native.ring(video)).await;
    }
    let mut call = native.drive();
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
                    Some(Control::Screen(v))=>NativeControl::Screen(v),
                    Some(Control::ScreenSources(kind))=>NativeControl::ScreenSources(kind),
                    Some(Control::ShareScreen(expected,selection))=>{
                        if expected != ticket {continue;}
                        NativeControl::ShareScreen(selection)
                    },
                    _=>{call.leave().await?;return Ok(EndReason::Left);}
                };
                call.control(control)?;
                update(&shared,&updates,ticket,|state|*state=call.updates.borrow().clone())?;
            }
        }
    }
}

struct IncomingNotifications {
    pending: std::collections::HashMap<String, (String, tokio::task::AbortHandle)>,
}
impl Drop for IncomingNotifications {
    fn drop(&mut self) {
        let ids: Vec<_> = self
            .pending
            .drain()
            .map(|(_, (id, task))| {
                task.abort();
                id
            })
            .collect();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                for id in ids {
                    twayblade_platform::notifications::dismiss(&id).await;
                }
            });
        }
    }
}
/// Native notification delivery stays on the backend runtime. Ringtone playback
/// belongs to the views; no audio assets or repeating sound are started here.
async fn incoming_notifications(
    account: Account,
    shared: Arc<Mutex<State>>,
    updates: watch::Sender<Snapshot>,
    config: watch::Receiver<super::notifications::NotificationConfig>,
) {
    use std::collections::HashSet;
    use twayblade_platform::notifications as os;
    let mut snapshots = updates.subscribe();
    let mut delivered = IncomingNotifications {
        pending: Default::default(),
    };
    let mut seen = HashSet::new();
    let mut responses = tokio::task::JoinSet::new();
    loop {
        let snapshot = snapshots.borrow_and_update().clone();
        if snapshot.account_instance != account.instance_id() || snapshot.user_id.is_empty() {
            break;
        }
        let active: HashSet<_> = snapshot
            .incoming_calls
            .iter()
            .map(|c| c.event_id.clone())
            .collect();
        let removed: Vec<_> = delivered
            .pending
            .keys()
            .filter(|id| !active.contains(*id))
            .cloned()
            .collect();
        for key in removed {
            if let Some((id, task)) = delivered.pending.remove(&key) {
                task.abort();
                os::dismiss(&id).await;
            }
        }
        seen.retain(|id| active.contains(id));
        for call in &snapshot.incoming_calls {
            if !call.ringing || !seen.insert(call.event_id.clone()) {
                continue;
            }
            let live = config.borrow().clone();
            if !live.device.enabled
                || snapshot.notifications.room_mode(&call.room_id)
                    == twayblade_core::notifications::RoomMode::Mute
                || os::permission().await != os::Permission::Granted
            {
                continue;
            }
            let video = call.kind == twayblade_core::calls::ringing::CallIntent::Video;
            let (title, body) = if !live.device.preview {
                ("Twayblade".to_owned(), "Incoming call".to_owned())
            } else if call.room_name == call.caller_name {
                let kind = if video { "video call" } else { "call" };
                (call.caller_name.clone(), format!("Incoming {kind}"))
            } else {
                let kind = if video { "a video call" } else { "a call" };
                (
                    call.room_name.clone(),
                    format!("{} started {kind}", call.caller_name),
                )
            };
            let notification = os::Notification {
                title,
                body,
                sound: live.device.sound,
                action_label: "Open",
                target: os::Activation {
                    account: account.instance_id(),
                    room: call.room_id.clone(),
                    thread: None,
                },
                launch: Some(os::LaunchActivation::new(
                    account.notification_identity(),
                    call.room_id.clone(),
                    None,
                )),
            };
            if let Ok(notification) = os::notify_call(notification).await {
                let event = call.event_id.clone();
                let expires = call.expires;
                let abort = responses.spawn(async move {
                    let result = tokio::time::timeout(
                        std::time::Duration::from_millis(
                            expires.saturating_sub(twayblade_core::calls::ringing::now_ms()),
                        ),
                        notification.response,
                    )
                    .await
                    .ok()
                    .flatten();
                    (event, result)
                });
                delivered
                    .pending
                    .insert(call.event_id.clone(), (notification.id, abort));
            }
        }
        tokio::select! {
            changed = snapshots.changed() => {if changed.is_err() {break;}}
            Some(result) = responses.join_next(), if !responses.is_empty() => {
                if let Ok((event,Some(response))) = result {
                    // A click racing expiry/removal must never answer a later call.
                    let current = updates.borrow().clone();
                    if current.account_instance != account.instance_id() || !current.incoming_calls.iter().any(|c|c.event_id == event && c.expires > twayblade_core::calls::ringing::now_ms()) {continue;}
                    let target = response.target;
                    match response.action {
                        os::NotificationAction::Accept => {
                            let _ = launch_call(&tokio::runtime::Handle::current(), &shared, &updates, account.instance_id(), target.room.clone(), false, true, Some(&event));
                        }
                        os::NotificationAction::Decline => {
                            if let Ok(state) = shared.lock() && state.account.as_ref().map(Account::instance_id) == Some(account.instance_id()) && let Some(control) = &state.incoming_control { let _ = control.try_send(twayblade_core::calls::IncomingAction::Decline(target.room.clone())); }
                        }
                        os::NotificationAction::Open => {}
                    }
                    super::notifications::publish_click(&updates,target);
                }
            }
        }
    }
    for (_, (id, task)) in delivered.pending.drain() {
        task.abort();
        os::dismiss(&id).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn screen_choices_cannot_capture_after_account_or_call_teardown() {
        let backend = Backend::with_lock(None).unwrap();
        let ticket = CallTicket {
            account: 5,
            request: 0,
        };
        let choice = twayblade_core::calls::ScreenSelection {
            kind: twayblade_core::calls::SourceKind::Screen,
            id: 42,
        };
        assert_eq!(
            backend.request_screen_sources(5, choice.kind),
            Err(CallError::StaleRequest)
        );
        assert_eq!(
            backend.share_screen_source(ticket, choice),
            Err(CallError::StaleRequest)
        );
        backend.state.lock().unwrap().cancel_call();
        assert_eq!(
            backend.share_screen_source(ticket, choice),
            Err(CallError::StaleRequest)
        );
    }
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
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                while !aborted.is_finished() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
        });
    }
}
