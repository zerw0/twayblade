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
    backend::{Backend, LoginOptions, Profile, Snapshot},
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
    pub sending: HashSet<String>,
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
            sending: HashSet::new(),
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
                        this.status = "Sign in to your homeserver.".into();
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

    fn apply(&mut self, mut snapshot: Snapshot, window: &mut Window, cx: &mut Context<Self>) {
        if self.started && snapshot.revision <= self.snapshot.revision {
            return;
        }
        self.started = true;
        if self.snapshot.user_id != snapshot.user_id {
            self.navigation = Navigation::default();
            self.composers.clear();
            self.sending.clear();
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
        }
        let groups = model::grouped(&snapshot.messages);
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
        self.navigation.selected_room = Some(id.clone());
        self.groups.clear();
        self.snapshot.messages.clear();
        self.snapshot.timeline_ready = false;
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

    pub fn load_earlier(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.preview || self.paginating || !self.snapshot.timeline_ready {
            return;
        }
        let Some(room) = self.navigation.selected_room.clone() else {
            return;
        };
        self.paginating = true;
        self.job(
            window,
            cx,
            move |backend| backend.load_earlier(room),
            |this, result, _, _| {
                this.paginating = false;
                match result {
                    Ok(true) => this.status = "Beginning of conversation".into(),
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
