use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::Arc,
};

use gpui_kit::component::{
    input::{InputEvent, InputState, TextareaState},
    message_scroller::MessageScrollerState,
    *,
};
use gpui_kit::*;

use crate::{
    backend::{Backend, LoginOptions, Profile, Snapshot, TimelineAction},
    model::{self, Navigation},
};

gpui_kit::actions!(
    matrix_client,
    [Quit, FindConversation, CloseProfile, ToggleDetails]
);

pub(super) struct ProfilePage {
    pub id: String,
    pub name: String,
    pub profile: Option<Profile>,
    pub loading: bool,
    pub error: String,
}

pub(super) struct MatrixApp {
    pub snapshot: Snapshot,
    pub navigation: Navigation,
    pub groups: Vec<Range<usize>>,
    pub homeserver: Entity<InputState>,
    pub username: Entity<InputState>,
    pub password: Entity<InputState>,
    pub search: Entity<InputState>,
    pub composer: Entity<TextareaState>,
    composers: HashMap<String, (Entity<TextareaState>, Subscription)>,
    pub scroller: Entity<MessageScrollerState>,
    pub focus: FocusHandle,
    pub options: Option<LoginOptions>,
    pub status: String,
    pub busy: bool,
    pub paginating: bool,
    pub pagination_exhausted: bool,
    pub(super) pagination_request: u64,
    was_at_top: bool,
    group_boundaries: HashSet<String>,
    pub sending: HashSet<String>,
    pub pending_actions: HashSet<(String, String)>,
    pub latest_visible: Option<(String, String)>,
    receipt_pending: Option<(String, String)>,
    last_receipt: Option<(String, String)>,
    receipt_retry_after: std::time::Instant,
    pub profile: Option<ProfilePage>,
    /// Room details panel; the profile view replaces it while open.
    pub details_open: bool,
    pub preview: bool,
    pub backend: Option<Arc<Backend>>,
    profile_request: u64,
    started: bool,
    _subscriptions: Vec<Subscription>,
    selection_task: Option<Task<()>>,
}

impl MatrixApp {
    pub fn new(
        preview: bool,
        home: bool,
        large: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let homeserver = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("matrix.example.org")
                .default_value("matrix.org")
        });
        let username =
            cx.new(|cx| InputState::new(window, cx).placeholder("Username or Matrix ID"));
        let password = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Password")
                .masked(true)
        });
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search conversations"));
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Message")
                .auto_grow(1, 6)
                .submit_on_enter(true)
        });
        let snapshot = if preview {
            model::preview(home, if large { 10_000 } else { 6 })
        } else {
            Snapshot::default()
        };
        let groups = model::grouped(&snapshot.messages);
        let scroller = cx.new(|cx| MessageScrollerState::new(groups.len(), cx));
        let mut subscriptions =
            vec![cx.subscribe_in(&search, window, |_, _, _: &InputEvent, _, cx| cx.notify())];
        for input in [&homeserver, &username, &password] {
            subscriptions.push(cx.subscribe_in(
                input,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        if this.options.is_some() {
                            this.sign_in(window, cx);
                        } else {
                            this.discover(window, cx);
                        }
                    }
                    cx.notify();
                },
            ));
        }
        subscriptions.push(cx.observe_in(&scroller, window, |this, _, window, cx| {
            this.maybe_load_earlier(window, cx);
            this.latest_visible = None;
            cx.notify();
        }));
        subscriptions.push(cx.observe_window_activation(window, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe_window_appearance(window, |_, window, cx| {
            Theme::sync_system_appearance(Some(window), cx);
            crate::theme::apply(cx);
        }));
        let mut this = Self {
            navigation: Navigation {
                selected_space: (preview && !home).then(|| "design".into()),
                selected_room: preview.then(|| snapshot.selected_room.clone()),
                ..Default::default()
            },
            snapshot,
            groups,
            homeserver,
            username,
            password,
            search,
            composer,
            composers: HashMap::new(),
            scroller,
            focus: cx.focus_handle(),
            options: None,
            status: String::new(),
            busy: !preview,
            paginating: false,
            pagination_exhausted: false,
            pagination_request: 0,
            was_at_top: false,
            group_boundaries: HashSet::new(),
            sending: HashSet::new(),
            pending_actions: HashSet::new(),
            latest_visible: None,
            receipt_pending: None,
            last_receipt: None,
            receipt_retry_after: std::time::Instant::now(),
            profile: None,
            details_open: false,
            preview,
            backend: None,
            profile_request: 0,
            started: preview,
            _subscriptions: subscriptions,
            selection_task: None,
        };
        if let Some(room) = this.navigation.selected_room.clone() {
            this.activate_composer(room, window, cx);
        }
        if !preview {
            this.start(window, cx);
        }
        this
    }

    fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.status = "Opening secure session…".into();
        cx.spawn_in(window, async move |view, cx| {
            let result = cx
                .background_spawn(async {
                    let backend = Backend::new().map_err(|error| error.to_string())?;
                    backend.restore().map_err(|error| error.to_string())?;
                    Ok::<_, String>((backend.snapshot(), backend))
                })
                .await;
            let _ = view.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok((snapshot, backend)) => {
                        this.backend = Some(backend.clone());
                        this.status = if snapshot.user_id.is_empty() {
                            "Sign in to your homeserver.".into()
                        } else {
                            String::new()
                        };
                        this.apply(snapshot, window, cx);
                        let mut updates = backend.subscribe();
                        // Deliver anything published since `snapshot()`; stale revisions are ignored.
                        updates.mark_changed();
                        cx.spawn_in(window, async move |view, cx| {
                            // Wakes only on a published change; ends with the view or backend.
                            while updates.changed().await.is_ok() {
                                let snapshot = updates.borrow_and_update().clone();
                                if view
                                    .update_in(cx, |this, window, cx| {
                                        this.apply(snapshot, window, cx)
                                    })
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        })
                        .detach();
                    }
                    Err(error) => this.status = error,
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn job<T: Send + 'static>(
        &self,
        window: &Window,
        cx: &Context<Self>,
        job: impl FnOnce(Arc<Backend>) -> T + Send + 'static,
        complete: impl FnOnce(&mut Self, T, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        cx.spawn_in(window, async move |view, cx| {
            let result = cx.background_spawn(async move { job(backend) }).await;
            let _ = view.update_in(cx, |this, window, cx| {
                complete(this, result, window, cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn apply(
        &mut self,
        mut snapshot: Snapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.started && snapshot.revision <= self.snapshot.revision {
            return;
        }
        self.started = true;
        if self.snapshot.user_id != snapshot.user_id
            || self.snapshot.account_instance != snapshot.account_instance
        {
            // Status from sign-in or a previous account must not enter this workspace.
            self.status.clear();
            self.reset_pagination();
            self.navigation = Navigation::default();
            self.composers.clear();
            self.sending.clear();
            self.pending_actions.clear();
            self.latest_visible = None;
            self.receipt_pending = None;
            self.last_receipt = None;
            self.close_profile(cx);
            self.composer
                .update(cx, |state, cx| state.set_value("", window, cx));
        }
        if self.navigation.reconcile(&snapshot) {
            self.queue_selection(None, window, cx);
        }
        if self.navigation.selected_room.as_deref().unwrap_or_default() != snapshot.selected_room {
            snapshot.messages.clear();
            snapshot.timeline_ready = false;
            snapshot.members.clear();
            snapshot.members_state = crate::backend::MembersState::Loading;
            snapshot.typing.clear();
        }
        if snapshot.selected_room == self.snapshot.selected_room {
            if let Some(first) = self.snapshot.messages.first()
                && snapshot
                    .messages
                    .iter()
                    .position(|message| message.id == first.id)
                    .is_some_and(|index| index > 0)
            {
                self.group_boundaries.insert(first.id.clone());
            }
            self.group_boundaries
                .retain(|id| snapshot.messages.iter().any(|message| &message.id == id));
        } else {
            self.group_boundaries.clear();
        }
        let groups = model::grouped_with_boundaries(&snapshot.messages, &self.group_boundaries);
        let old_ids: Vec<_> = self
            .groups
            .iter()
            .map(|group| self.snapshot.messages[group.start].id.as_str())
            .collect();
        let new_ids: Vec<_> = groups
            .iter()
            .map(|group| snapshot.messages[group.start].id.as_str())
            .collect();
        let prefix = old_ids
            .iter()
            .zip(&new_ids)
            .take_while(|(old, new)| old == new)
            .count();
        let suffix = old_ids[prefix..]
            .iter()
            .rev()
            .zip(new_ids[prefix..].iter().rev())
            .take_while(|(old, new)| old == new)
            .count();
        self.scroller.update(cx, |state, cx| {
            if state.item_count() == 0 {
                state.reset(groups.len(), cx);
            } else {
                state.splice(
                    prefix..old_ids.len() - suffix,
                    new_ids.len() - prefix - suffix,
                    cx,
                );
                state.remeasure(cx);
            }
        });
        self.groups = groups;
        self.snapshot = snapshot;
        cx.notify();
    }

    pub fn discover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.preview || self.backend.is_none() {
            return;
        }
        self.busy = true;
        self.status = "Finding sign-in options…".into();
        let address = self.homeserver.read(cx).value().to_string();
        self.job(
            window,
            cx,
            move |backend| backend.discover(address),
            |this, result, window, cx| {
                this.busy = false;
                match result {
                    Ok(options) => {
                        this.homeserver.update(cx, |state, cx| {
                            state.set_value(options.homeserver.clone(), window, cx)
                        });
                        this.status = if options.password {
                            ""
                        } else if options.browser {
                            "Browser sign-in is not available in this build yet."
                        } else {
                            "No supported sign-in option was found."
                        }
                        .into();
                        this.options = Some(options);
                    }
                    Err(error) => this.status = error.to_string(),
                }
            },
        );
        cx.notify();
    }

    pub fn sign_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy
            || self.preview
            || self
                .options
                .as_ref()
                .is_none_or(|options| !options.password)
        {
            return;
        }
        let address = self.homeserver.read(cx).value().to_string();
        let username = self.username.read(cx).value().to_string();
        let password = self.password.read(cx).value().to_string();
        if username.trim().is_empty() || password.is_empty() {
            return;
        }
        self.password
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.busy = true;
        self.status = "Signing in…".into();
        self.job(
            window,
            cx,
            move |backend| backend.login(address, username, password),
            |this, result, _, _| {
                this.busy = false;
                if let Err(error) = result {
                    this.status = error.to_string();
                }
            },
        );
        cx.notify();
    }

    pub fn select_space(
        &mut self,
        id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigation.selected_space = id;
        self.search
            .update(cx, |state, cx| state.set_value("", window, cx));
        if self.navigation.reconcile(&self.snapshot) {
            self.reset_pagination();
            self.groups.clear();
            self.snapshot.messages.clear();
            self.scroller.update(cx, |state, cx| state.reset(0, cx));
            self.queue_selection(None, window, cx);
        }
        cx.notify();
    }

    pub fn select_room(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.navigation.selected_room.as_ref() == Some(&id) {
            return;
        }
        self.reset_pagination();
        self.navigation.selected_room = Some(id.clone());
        self.groups.clear();
        self.snapshot.messages.clear();
        self.snapshot.timeline_ready = false;
        self.latest_visible = None;
        if !self.preview {
            self.snapshot.members.clear();
            self.snapshot.members_state = crate::backend::MembersState::Loading;
            self.snapshot.typing.clear();
        }
        self.activate_composer(id.clone(), window, cx);
        self.scroller.update(cx, |state, cx| state.reset(0, cx));
        if self.preview {
            self.snapshot.selected_room = id.clone();
            self.snapshot.timeline_ready = true;
        }
        self.queue_selection(Some(id), window, cx);
        cx.notify();
    }

    fn activate_composer(&mut self, room: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((state, _)) = self.composers.get(&room) {
            self.composer = state.clone();
            return;
        }
        let draft = self
            .navigation
            .drafts
            .get(&room)
            .cloned()
            .unwrap_or_default();
        let state = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Message")
                .auto_grow(1, 6)
                .submit_on_enter(true)
                .default_value(draft)
        });
        let target = room.clone();
        let subscription = cx.subscribe_in(
            &state,
            window,
            move |this, state, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::Change => {
                        this.navigation
                            .drafts
                            .insert(target.clone(), state.read(cx).value().to_string());
                        if !this.preview
                            && this.navigation.selected_room.as_ref() == Some(&target)
                            && let Some(backend) = &this.backend
                        {
                            backend.set_typing(
                                target.clone(),
                                !state.read(cx).value().trim().is_empty(),
                            );
                        }
                    }
                    InputEvent::PressEnter { shift: false, .. }
                        if this.navigation.selected_room.as_ref() == Some(&target) =>
                    {
                        this.send(window, cx)
                    }
                    _ => {}
                }
                cx.notify();
            },
        );
        self.composer = state.clone();
        self.composers.insert(room, (state, subscription));
    }

    fn queue_selection(&mut self, room: Option<String>, window: &Window, cx: &Context<Self>) {
        if self.preview {
            return;
        }
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let previous = self.selection_task.take();
        self.selection_task = Some(cx.spawn_in(window, async move |view, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            let target = room.clone();
            let result = cx
                .background_spawn(async move {
                    match room {
                        Some(room) => backend.select_room(room),
                        None => backend.clear_selection(),
                    }
                })
                .await;
            let _ = view.update_in(cx, |this, _, cx| {
                if this.navigation.selected_room == target
                    && let Err(error) = result
                {
                    this.status = error.to_string();
                    cx.notify();
                }
            });
        }));
    }

    pub fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview || self.busy || !self.snapshot.timeline_ready {
            return;
        }
        let Some(room) = self.navigation.selected_room.clone() else {
            return;
        };
        let body = self.composer.read(cx).value().to_string();
        if body.trim().is_empty() || self.sending.contains(&room) {
            return;
        }
        if let Some(backend) = &self.backend {
            backend.set_typing(room.clone(), false);
        }
        self.sending.insert(room.clone());
        let account = self.snapshot.user_id.clone();
        let sent_body = body.clone();
        let sent_room = room.clone();
        self.job(
            window,
            cx,
            move |backend| backend.send_text(sent_room, sent_body),
            move |this, result, window, cx| {
                if this.snapshot.user_id != account {
                    return;
                }
                this.sending.remove(&room);
                match result {
                    Ok(()) => {
                        if let Some((state, _)) = this.composers.get(&room)
                            && state.read(cx).value().as_str() == body
                        {
                            this.navigation.drafts.remove(&room);
                            state.update(cx, |state, cx| state.set_value("", window, cx));
                        }
                    }
                    Err(error) => this.status = error.to_string(),
                }
            },
        );
        cx.notify();
    }

    pub fn report_latest_visibility(
        &mut self,
        room: String,
        event_id: String,
        account: u128,
        visible: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.snapshot.account_instance != account
            || self.navigation.selected_room.as_ref() != Some(&room)
            || self
                .snapshot
                .messages
                .iter()
                .rev()
                .find_map(|message| message.event_id.as_ref())
                != Some(&event_id)
        {
            return;
        }
        let key = (room.clone(), event_id.clone());
        self.latest_visible = visible.then(|| key.clone());
        if self.preview
            || !visible
            || !window.is_window_active()
            || self.backend.is_none()
            || !self.snapshot.timeline_ready
            || self.receipt_pending.is_some()
            || self.last_receipt.as_ref() == Some(&key)
            || std::time::Instant::now() < self.receipt_retry_after
        {
            return;
        }
        self.receipt_pending = Some(key.clone());
        self.job(
            window,
            cx,
            move |backend| {
                backend.timeline_action(room, account, TimelineAction::Read { event_id })
            },
            move |this, result, _, _| {
                if this.snapshot.account_instance != account
                    || this.receipt_pending.as_ref() != Some(&key)
                {
                    return;
                }
                this.receipt_pending = None;
                if result.is_ok() {
                    this.last_receipt = Some(key);
                } else {
                    this.receipt_retry_after =
                        std::time::Instant::now() + std::time::Duration::from_secs(10);
                }
            },
        );
    }

    pub fn timeline_action(
        &mut self,
        id: String,
        action: TimelineAction,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.preview || !self.snapshot.timeline_ready {
            return;
        }
        let Some(room) = self.navigation.selected_room.clone() else {
            return;
        };
        let pending = (room.clone(), id);
        if !self.pending_actions.insert(pending.clone()) {
            return;
        }
        let account_instance = self.snapshot.account_instance;
        self.job(
            window,
            cx,
            move |backend| backend.timeline_action(room, account_instance, action),
            move |this, result, _, _| {
                if this.snapshot.account_instance != account_instance {
                    return;
                }
                this.pending_actions.remove(&pending);
                if this.navigation.selected_room.as_ref() == Some(&pending.0) {
                    match result {
                        Ok(()) => this.status.clear(),
                        Err(error) => this.status = error.to_string(),
                    }
                }
            },
        );
        cx.notify();
    }

    fn reset_pagination(&mut self) {
        self.pagination_request = self.pagination_request.wrapping_add(1);
        self.paginating = false;
        self.pagination_exhausted = false;
        self.was_at_top = false;
        self.group_boundaries.clear();
    }

    fn maybe_load_earlier(&mut self, window: &Window, cx: &mut Context<Self>) {
        let state = self.scroller.read(cx);
        let top = state.logical_scroll_top();
        let at_top = state.is_scrolled_up() && top.item_ix == 0 && top.offset_in_item <= px(1.);
        let entered = at_top && !self.was_at_top;
        self.was_at_top = at_top;
        if entered {
            self.load_earlier(window, cx);
        }
    }

    pub fn load_earlier(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.preview
            || self.backend.is_none()
            || self.paginating
            || self.pagination_exhausted
            || !self.snapshot.timeline_ready
        {
            return;
        }
        let Some(room) = self.navigation.selected_room.clone() else {
            return;
        };
        self.paginating = true;
        self.pagination_request = self.pagination_request.wrapping_add(1);
        let request = self.pagination_request;
        let account = self.snapshot.account_instance;
        let target = room.clone();
        self.job(
            window,
            cx,
            move |backend| backend.load_earlier(room, account),
            move |this, result, _, _| {
                if this.pagination_request != request
                    || this.snapshot.account_instance != account
                    || this.navigation.selected_room.as_ref() != Some(&target)
                {
                    return;
                }
                this.paginating = false;
                match result {
                    Ok(true) => {
                        this.pagination_exhausted = true;
                        this.status = "Beginning of conversation".into();
                    }
                    Ok(false) => {}
                    Err(error) => this.status = error.to_string(),
                }
            },
        );
        cx.notify();
    }

    pub fn open_profile(
        &mut self,
        id: String,
        name: String,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.profile_request = self.profile_request.wrapping_add(1);
        let request = self.profile_request;
        self.profile = Some(ProfilePage {
            id: id.clone(),
            name: name.clone(),
            profile: None,
            loading: !self.preview,
            error: String::new(),
        });
        if self.preview {
            cx.notify();
            return;
        }
        let room = self.navigation.selected_room.clone();
        self.job(
            window,
            cx,
            move |backend| backend.profile(id, room),
            move |this, result, _, _| {
                if this.profile_request != request {
                    return;
                }
                if let Some(page) = &mut this.profile {
                    page.loading = false;
                    match result {
                        Ok(profile) => page.profile = Some(profile),
                        Err(error) => page.error = error.to_string(),
                    }
                }
            },
        );
        cx.notify();
    }

    pub fn toggle_details(&mut self, cx: &mut Context<Self>) {
        if self.profile.is_some() {
            self.close_profile(cx);
            self.details_open = true;
        } else {
            self.details_open = !self.details_open;
        }
        cx.notify();
    }

    /// Escape: close the profile first, then the details panel.
    pub fn dismiss_panel(&mut self, cx: &mut Context<Self>) {
        if self.profile.is_some() {
            self.close_profile(cx);
        } else if self.details_open {
            self.details_open = false;
            cx.notify();
        }
    }

    pub fn close_profile(&mut self, cx: &mut Context<Self>) {
        self.profile_request = self.profile_request.wrapping_add(1);
        self.profile = None;
        cx.notify();
    }

    pub fn logout(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.busy || self.preview {
            return;
        }
        self.busy = true;
        self.job(
            window,
            cx,
            |backend| backend.logout(),
            |this, result, _, cx| {
                this.busy = false;
                match result {
                    Ok(()) => {
                        this.options = None;
                        this.status = "Signed out.".into();
                        this.close_profile(cx);
                    }
                    Err(error) => this.status = error.to_string(),
                }
            },
        );
        cx.notify();
    }
}

impl Drop for MatrixApp {
    fn drop(&mut self) {
        if let Some(backend) = &self.backend {
            backend.stop();
        }
    }
}
