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
    twayblade,
    [
        Quit,
        OpenSettings,
        IncreaseScale,
        DecreaseScale,
        ResetScale,
        FindConversation,
        CloseProfile,
        ToggleDetails,
        NewRoom,
        NewDirectMessage,
        JoinSpace,
        ToggleSidebar,
        NavigateBack,
        NavigateForward
    ]
);

pub(super) struct ProfilePage {
    pub id: String,
    pub name: String,
    pub profile: Option<Profile>,
    pub loading: bool,
    pub error: String,
}

pub(super) struct MatrixApp {
    pub settings: Option<(AnyWindowHandle, WeakEntity<crate::settings::SettingsWindow>)>,
    pub preferences: crate::preferences::Preferences,
    pub preferences_writable: bool,
    pub preference_revision: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub preference_write_lock: std::sync::Arc<std::sync::Mutex<()>>,
    pub accent_picker: Entity<color_picker::ColorPickerState>,
    pub font_selectors: [Entity<select::SelectState<select::SearchableVec<String>>>; 3],
    pub settings_status: String,
    pub notification_busy: bool,
    pub notification_keyword: Entity<InputState>,
    pub notification_sample: bool,
    pub notification_window_active: bool,
    pub last_notification_click: u64,
    pub notification_thread: Option<(String, String)>,
    pub account_name: Option<Entity<InputState>>,
    pub account_edit_busy: bool,
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
    /// The room list column; the space rail stays visible (⌃⌘S).
    pub sidebar_open: bool,
    /// Room navigation history for Back/Forward (⌘[ / ⌘]).
    pub(super) history_back: Vec<String>,
    pub(super) history_forward: Vec<String>,
    navigating_history: bool,
    /// Sidebar sections the user collapsed, by section key.
    pub(super) collapsed_sections: std::collections::HashSet<&'static str>,
    /// Selected tab of the room Info panel.
    pub(super) info_tab: InfoTab,
    /// The composer's current placeholder, so it is only reset on room change.
    pub(super) composer_placeholder: SharedString,
    /// The room the Files tab last asked the gallery for, so a failed load is
    /// retried by the user, not on every frame.
    pub(super) gallery_requested: String,
    /// Message search panel and its scope/filter.
    pub(super) search_open: bool,
    pub(super) search_all_rooms: bool,
    pub(super) search_has_file: bool,
    /// The last focused event scrolled into view, and the one highlighted.
    pub(super) revealed_event: Option<String>,
    /// The DM partners and members last handed to the presence observer.
    pub(super) presence_observed: Vec<String>,
    /// A destructive member action waiting for its second click: (user, action).
    pub(super) member_confirm: Option<(String, twayblade_core::room_state::MemberAction)>,
    /// Per-group content fingerprints matching `groups`, for targeted re-measure.
    group_signatures: Vec<u64>,
    /// Latest timeline event and your latest message, derived once per snapshot.
    latest_cache: std::cell::RefCell<Option<LatestIds>>,
    pub(super) avatars: crate::avatars::AvatarCache,
    /// The signed-in user's avatar URL, fetched once per account.
    pub own_avatar: Option<String>,
    /// Pressed inside a toolbar row; the next mouse move starts a window drag.
    pub window_drag: bool,
    /// The security reminder strip was dismissed for this session.
    pub security_notice_dismissed: bool,
    pub confirm_session_reset: bool,
    pub emoji_picker: Option<crate::emoji::Picker>,
    pub emoji_search: Entity<InputState>,
    pub emoji_preferences: crate::emoji::Preferences,
    pub attachment_draft: Option<twayblade_core::conversation::AttachmentDraft>,
    pub attachment_busy: bool,
    pub attachment_sample: bool,
    pub attachment_request: u64,
    pub security_open: bool,
    pub security_restore: bool,
    pub security_reset_confirmation: bool,
    pub security_device: Option<String>,
    pub security_input: Entity<InputState>,
    pub security_key: Option<zeroize::Zeroizing<String>>,
    pub security_key_copied: bool,
    pub oauth_cancel: Option<tokio::sync::watch::Sender<bool>>,
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
    pub thread: Option<crate::threads::ThreadPanel>,
    pub thread_drafts: HashMap<(String, String), (Entity<TextareaState>, Subscription)>,
    pub thread_request: u64,
    pub management: Option<crate::spaces_ui::Management>,
    pub management_request: u64,
    pub viewer: Option<crate::viewer::Viewer>,
    pub viewer_request: u64,
    pub reveal_image: Option<String>,
    pub image_focus: std::cell::RefCell<HashMap<String, FocusHandle>>,
    pub invite_pending: HashSet<String>,
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
        let emoji_search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search emoji or shortcode"));
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
        let mut subscriptions = vec![cx.subscribe_in(
            &search,
            window,
            |this, _, event: &InputEvent, window, cx| {
                // Typing filters conversations; Return searches messages.
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.run_message_search(window, cx);
                }
                cx.notify()
            },
        )];
        subscriptions.push(cx.subscribe_in(
            &emoji_search,
            window,
            |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change)
                    && let Some(picker) = &mut this.emoji_picker
                {
                    picker.selected = 0;
                }
                cx.notify();
            },
        ));
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
        subscriptions.push(cx.observe_window_activation(window, |this, window, cx| {
            this.notification_window_active = window.is_window_active();
            this.notification_config();
            cx.notify();
        }));
        if !preview {
            subscriptions.push(cx.observe_window_appearance(window, |_, window, cx| {
                Theme::sync_system_appearance(Some(window), cx);
                crate::theme::apply(cx);
            }));
        }
        let loaded = if preview {
            Ok(Default::default())
        } else {
            crate::preferences::Preferences::path()
                .map(|path| crate::preferences::Preferences::load(&path))
                .unwrap_or_else(|| Err(std::io::Error::other("No config directory")))
        };
        let preferences_writable = loaded.is_ok();
        let mut installed = cx.text_system().all_font_names();
        installed.sort();
        installed.dedup();
        installed.insert(0, "System default".into());
        let font_selectors = std::array::from_fn(|index| {
            let saved = loaded
                .as_ref()
                .ok()
                .and_then(|p: &crate::preferences::Preferences| match index {
                    0 => p.appearance.interface_font.as_ref(),
                    1 => p.appearance.message_font.as_ref(),
                    _ => p.appearance.code_font.as_ref(),
                })
                .filter(|name| installed.contains(name))
                .cloned()
                .unwrap_or_else(|| "System default".into());
            let selector = cx.new(|cx| {
                let mut state = select::SelectState::new(
                    select::SearchableVec::from(installed.clone()),
                    None,
                    window,
                    cx,
                )
                .searchable(true);
                state.set_selected_value(&saved, window, cx);
                state
            });
            subscriptions.push(cx.subscribe_in(
                &selector,
                window,
                move |this,
                      _,
                      event: &select::SelectEvent<select::SearchableVec<String>>,
                      window,
                      cx| {
                    let select::SelectEvent::Confirm(value) = event;
                    let value = value
                        .as_ref()
                        .filter(|name| name.as_str() != "System default")
                        .cloned();
                    match index {
                        0 => this.preferences.appearance.interface_font = value,
                        1 => this.preferences.appearance.message_font = value,
                        _ => this.preferences.appearance.code_font = value,
                    }
                    this.appearance_changed(window, cx);
                },
            ));
            selector
        });
        let accent_picker = cx.new(|cx| color_picker::ColorPickerState::new(window, cx));
        subscriptions.push(cx.subscribe_in(
            &accent_picker,
            window,
            |this, _, event: &color_picker::ColorPickerEvent, window, cx| {
                let color_picker::ColorPickerEvent::Change(value) = event;
                this.preferences.appearance.accent = value.map(crate::theme::accent_bytes);
                this.appearance_changed(window, cx);
            },
        ));
        let mut this = Self {
            accent_picker,
            font_selectors,
            preference_revision: Default::default(),
            preference_write_lock: Default::default(),
            settings: None,
            preferences: loaded.unwrap_or_default(),
            preferences_writable,
            settings_status: if preferences_writable {
                String::new()
            } else {
                "Preferences could not be read or backed up; saving is disabled to preserve the original file.".into()
            },
            account_name: None,
            notification_busy: false,
            notification_sample: false,
            notification_window_active: window.is_window_active(),
            last_notification_click: 0,
            notification_thread: None,
            notification_keyword: cx.new(|cx| InputState::new(window, cx).placeholder("Keyword")),
            account_edit_busy: false,
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
            sidebar_open: true,
            history_back: Vec::new(),
            history_forward: Vec::new(),
            navigating_history: false,
            collapsed_sections: Default::default(),
            info_tab: InfoTab::About,
            composer_placeholder: "Message".into(),
            gallery_requested: String::new(),
            search_open: false,
            search_all_rooms: true,
            search_has_file: false,
            revealed_event: None,
            presence_observed: Vec::new(),
            member_confirm: None,
            group_signatures: Vec::new(),
            latest_cache: Default::default(),
            avatars: Default::default(),
            own_avatar: None,
            window_drag: false,
            security_notice_dismissed: false,
            oauth_cancel: None,
            confirm_session_reset: false,
            emoji_picker: None,
            emoji_search,
            emoji_preferences: if preview {
                Default::default()
            } else {
                crate::emoji::Preferences::load()
            },
            attachment_draft: None,
            attachment_busy: false,
            attachment_sample: false,
            attachment_request: 0,
            security_open: false,
            security_restore: false,
            security_reset_confirmation: false,
            security_device: None,
            security_input: cx.new(|cx| InputState::new(window, cx).masked(true)),
            security_key: None,
            security_key_copied: false,
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
            thread: None,
            thread_drafts: HashMap::new(),
            thread_request: 0,
            management: None,
            management_request: 0,
            viewer: None,
            viewer_request: 0,
            reveal_image: None,
            image_focus: std::cell::RefCell::new(HashMap::new()),
            invite_pending: HashSet::new(),
            profile: None,
            details_open: false,
            preview,
            backend: None,
            profile_request: 0,
            started: preview,
            _subscriptions: subscriptions,
            selection_task: None,
        };
        crate::theme::set_appearance(this.preferences.appearance.clone(), cx);
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
                        this.notification_config();
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
        if self.snapshot.session_state != snapshot.session_state {
            self.options = None;
            self.status.clear();
            self.confirm_session_reset = false;
        }
        self.started = true;
        if self.snapshot.user_id != snapshot.user_id
            || self.snapshot.account_instance != snapshot.account_instance
        {
            self.notification_thread = None;
            self.own_avatar = None;
            if !snapshot.user_id.is_empty()
                && let Some(backend) = self.backend.clone()
            {
                let account = snapshot.account_instance;
                cx.spawn(async move |this, cx| {
                    let url = cx
                        .background_spawn(async move { backend.own_avatar_url() })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if this.snapshot.account_instance == account {
                            this.own_avatar = url;
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
            self.account_name = None;
            self.account_edit_busy = false;
            self.notification_busy = false;
            self.notification_keyword
                .update(cx, |input, cx| input.set_value("", window, cx));
            self.emoji_picker = None;
            self.clear_attachment();
            self.security_key = None;
            self.security_key_copied = false;
            self.security_open = false;
            self.security_restore = false;
            self.security_device = None;
            self.security_reset_confirmation = false;
            self.security_input = cx.new(|cx| InputState::new(window, cx).masked(true));
            // Status from sign-in or a previous account must not enter this workspace.
            self.status.clear();
            self.reset_pagination();
            self.navigation = Navigation::default();
            self.composers.clear();
            self.thread = None;
            self.thread_drafts.clear();
            if let Some(viewer) = self.viewer.take() {
                viewer.cancel.send_replace(true);
            }
            self.image_focus.borrow_mut().clear();
            self.reveal_image = None;
            self.management = None;
            self.invite_pending.clear();
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
        let signatures: Vec<u64> = groups
            .iter()
            .map(|group| model::group_signature(&snapshot.messages, group.clone()))
            .collect();
        let old_signatures = std::mem::take(&mut self.group_signatures);
        let inserted = new_ids.len() - prefix - suffix;
        self.scroller.update(cx, |state, cx| {
            if state.item_count() == 0 {
                state.reset(groups.len(), cx);
                return;
            }
            state.splice(prefix..old_ids.len() - suffix, inserted, cx);
            if old_signatures.len() != old_ids.len() {
                // No fingerprints for the old groups (snapshot set directly):
                // keep the anchor, measure everything once.
                state.remeasure(cx);
                return;
            }
            // Re-measure only surviving groups whose content changed; inserted
            // groups are measured on first layout.
            let mut changed = |new_ix: usize, old_ix: usize| {
                if old_signatures.get(old_ix) != signatures.get(new_ix) {
                    state.remeasure_items(new_ix..new_ix + 1, cx);
                }
            };
            for ix in 0..prefix {
                changed(ix, ix);
            }
            for k in 0..suffix {
                changed(new_ids.len() - suffix + k, old_ids.len() - suffix + k);
            }
        });
        self.group_signatures = signatures;
        self.update_thread(snapshot.thread.as_ref(), cx);
        self.groups = groups;
        self.snapshot = snapshot;
        self.notification_config();
        if let Some((serial, target)) = self.snapshot.notification_click.clone()
            && serial != self.last_notification_click
        {
            self.last_notification_click = serial;
            self.activate_notification(target, window, cx);
        }
        self.open_pending_notification_thread(window, cx);
        self.refresh_viewer(window, cx);
        cx.notify();
    }

    pub fn prepare_reauthentication(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.preview {
            return;
        }
        self.busy = true;
        self.job(
            window,
            cx,
            |backend| backend.recovery_options(),
            |this, result, window, cx| {
                this.busy = false;
                match result {
                    Ok(options) => {
                        this.homeserver.update(cx, |state, cx| {
                            state.set_value(options.homeserver.clone(), window, cx)
                        });
                        this.username.update(cx, |state, cx| {
                            state.set_value(this.snapshot.user_id.clone(), window, cx)
                        });
                        this.options = Some(options);
                        this.status.clear();
                    }
                    Err(error) => this.status = error.to_string(),
                }
            },
        );
    }

    pub fn discard_revoked_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.confirm_session_reset || self.busy {
            return;
        }
        if self.preview {
            self.snapshot = Snapshot::default();
            self.options = None;
            self.confirm_session_reset = false;
            self.status = "Sample data · Local session removed.".into();
            cx.notify();
            return;
        }
        self.busy = true;
        self.job(
            window,
            cx,
            |backend| backend.discard_revoked_session(),
            |this, result, _, _| {
                this.busy = false;
                if let Err(error) = result {
                    this.status = error.to_string();
                } else {
                    this.options = None;
                    this.confirm_session_reset = false;
                    this.status.clear();
                }
            },
        );
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
                        this.status = if options.password || options.browser || options.sso {
                            ""
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

    pub fn browser_sign_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.start_browser_sign_in(false, None, window, cx);
    }

    pub fn sso_sign_in(
        &mut self,
        provider: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_browser_sign_in(true, provider, window, cx);
    }

    fn start_browser_sign_in(
        &mut self,
        sso: bool,
        provider: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy
            || self.preview
            || self
                .options
                .as_ref()
                .is_none_or(|options| if sso { !options.sso } else { !options.browser })
        {
            return;
        }
        self.busy = true;
        self.status = "Opening your browser…".into();
        let (cancel, receiver) = tokio::sync::watch::channel(false);
        self.oauth_cancel = Some(cancel);
        let address = self.homeserver.read(cx).value().to_string();
        self.job(window, cx, move |backend| backend.begin_oauth(address, sso, provider), move |this, result, window, cx| {
            match result {
                Ok(url) => {
                    // Never retain the authorization URL in a snapshot or diagnostics.
                    if !*receiver.borrow() { cx.open_url(&url); }
                    this.status = "Complete sign-in in your browser. This window will wait for up to five minutes.".into();
                    this.job(window, cx, move |backend| backend.finish_oauth(receiver), |this, result, _, _| {
                        this.busy = false;
                        this.oauth_cancel = None;
                        this.status = result.err().map(|error| error.to_string()).unwrap_or_default();
                    });
                }
                Err(error) => {
                    this.busy = false;
                    this.oauth_cancel = None;
                    this.status = error.to_string();
                }
            }
        });
        cx.notify();
    }

    pub fn cancel_browser_sign_in(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = &self.oauth_cancel {
            cancel.send_replace(true);
        }
        if self.preview {
            self.oauth_cancel = None;
            self.busy = false;
            self.status = "Sample data · Browser sign-in cancelled.".into();
        } else {
            self.status = "Cancelling browser sign-in…".into();
        }
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
        self.management = None;
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

    pub(super) fn activate_notification(
        &mut self,
        target: twayblade_platform::notifications::Activation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if target.account != self.snapshot.account_instance
            || !self
                .snapshot
                .rooms
                .iter()
                .any(|room| room.id == target.room)
        {
            return;
        }
        self.navigation.selected_space = None;
        self.select_room(target.room.clone(), window, cx);
        cx.activate(true);
        window.activate_window();
        self.notification_sample = false;
        self.notification_thread = target.thread.map(|root| (target.room, root));
        self.open_pending_notification_thread(window, cx);
        cx.notify();
    }

    fn open_pending_notification_thread(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((room, _)) = &self.notification_thread else {
            return;
        };
        if self.navigation.selected_room.as_ref() != Some(room) {
            self.notification_thread = None;
            return;
        }
        if self.snapshot.selected_room != *room || !self.snapshot.timeline_ready {
            return;
        }
        let (_, root) = self.notification_thread.take().expect("pending thread");
        self.open_thread_event(root, window, cx);
    }

    pub fn select_room(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.select_room_target(id, None, window, cx);
    }
    fn select_room_target(
        &mut self,
        id: String,
        event: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.management = None;
        cx.notify();
        if event.is_none() && self.navigation.selected_room.as_ref() == Some(&id) {
            return;
        }
        self.emoji_picker = None;
        self.clear_attachment();
        self.reset_pagination();
        if !std::mem::take(&mut self.navigating_history) {
            if let Some(previous) = self.navigation.selected_room.clone() {
                self.history_back.push(previous);
                if self.history_back.len() > 100 {
                    self.history_back.remove(0);
                }
            }
            self.history_forward.clear();
        }
        self.navigation.selected_room = Some(id.clone());
        self.notification_config();
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
        self.queue_selection_focus(Some(id), event, window, cx);
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
                        this.update_emoji_completion(
                            state.read(cx).value().to_string(),
                            state.read(cx).cursor(),
                        );
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
        self.queue_selection_focus(room, None, window, cx);
    }
    fn queue_selection_focus(
        &mut self,
        room: Option<String>,
        event: Option<String>,
        window: &Window,
        cx: &Context<Self>,
    ) {
        self.thread = None;
        self.management = None;
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
                        Some(room) => match event {
                            Some(event) => backend.open_event(room, event),
                            None => backend.select_room(room),
                        },
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
            move |this, result, window, cx| {
                if this.pagination_request != request
                    || this.snapshot.account_instance != account
                    || this.navigation.selected_room.as_ref() != Some(&target)
                {
                    return;
                }
                this.paginating = false;
                let continue_images = matches!(&result, Ok(false));
                match result {
                    Ok(true) => {
                        this.pagination_exhausted = true;
                        this.status = "Beginning of conversation".into();
                    }
                    Ok(false) => {}
                    Err(error) => this.status = error.to_string(),
                }
                if this.viewer.as_ref().is_some_and(|v| v.previous_pending) {
                    if continue_images {
                        this.load_earlier(window, cx);
                    } else if let Some(v) = &mut this.viewer {
                        v.previous_pending = false;
                        v.status = this.status.clone();
                    }
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
        self.management = None;
        self.close_thread(cx);
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
        if self.thread.is_some() {
            self.close_thread(cx);
            self.details_open = true;
            cx.notify();
            return;
        }
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
        if self.management.is_some() {
            self.management = None;
            cx.notify();
            return;
        }
        if self.search_open {
            self.search_open = false;
            cx.notify();
            return;
        }
        if self.thread.is_some() {
            self.close_thread(cx);
            return;
        }
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

/// Cache key: snapshot revision, message count and last message ID, so direct
/// snapshot edits (previews, tests) can never serve stale values.
struct LatestIds {
    key: (u64, usize, Option<String>),
    event: Option<String>,
    own: Option<String>,
}

impl MatrixApp {
    fn latest_ids(&self) -> std::cell::Ref<'_, LatestIds> {
        let messages = &self.snapshot.messages;
        let key = (
            self.snapshot.revision,
            messages.len(),
            messages.last().map(|message| message.id.clone()),
        );
        let stale = self
            .latest_cache
            .borrow()
            .as_ref()
            .is_none_or(|cached| cached.key != key);
        if stale {
            let event = messages
                .iter()
                .rev()
                .find(|message| message.event_id.is_some())
                .map(|message| message.id.clone());
            let own = messages
                .iter()
                .rev()
                .find(|message| message.sender == self.snapshot.user_id)
                .map(|message| message.id.clone());
            *self.latest_cache.borrow_mut() = Some(LatestIds { key, event, own });
        }
        std::cell::Ref::map(self.latest_cache.borrow(), |cached| {
            cached.as_ref().expect("latest ids computed")
        })
    }

    /// The main timeline's latest event ID (for read receipts).
    pub(super) fn latest_event(&self) -> Option<String> {
        self.latest_ids().event.clone()
    }

    /// Your latest message in the main timeline (for the delivery label).
    pub(super) fn latest_own(&self) -> Option<String> {
        self.latest_ids().own.clone()
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum InfoTab {
    #[default]
    About,
    Members,
    Files,
}

impl MatrixApp {
    /// ⌘[ / ⌘]: move through visited rooms, skipping rooms that no longer exist.
    pub(super) fn navigate_history(
        &mut self,
        back: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        loop {
            let next = if back {
                self.history_back.pop()
            } else {
                self.history_forward.pop()
            };
            let Some(next) = next else { return };
            if !self.snapshot.rooms.iter().any(|room| room.id == next) {
                continue;
            }
            if let Some(current) = self.navigation.selected_room.clone() {
                if back {
                    self.history_forward.push(current);
                } else {
                    self.history_back.push(current);
                }
            }
            self.navigating_history = true;
            self.select_room(next, window, cx);
            return;
        }
    }

    pub(super) fn set_favourite(
        &mut self,
        room: String,
        favourite: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.preview {
            if let Some(entry) = self
                .snapshot
                .rooms
                .iter_mut()
                .find(|entry| entry.id == room)
            {
                entry.favourite = favourite;
            }
            cx.notify();
            return;
        }
        self.job(
            window,
            cx,
            move |backend| backend.set_favourite(room, favourite),
            |this, result, _, cx| {
                if let Err(error) = result {
                    this.status = error.to_string();
                }
                cx.notify();
            },
        );
    }
}

impl MatrixApp {
    /// Open (or create) a direct message with `user` and switch to it.
    pub(super) fn message_user(&mut self, user: String, window: &Window, cx: &mut Context<Self>) {
        if self.preview {
            return;
        }
        let account = self.snapshot.account_instance;
        self.job(
            window,
            cx,
            move |backend| backend.space_action(account, 0, crate::backend::SpaceAction::Dm(user)),
            move |this, result, window, cx| {
                if this.snapshot.account_instance != account {
                    return;
                }
                match result {
                    Ok(crate::backend::SpaceOutcome::Room(id)) => {
                        this.close_profile(cx);
                        this.select_room(id, window, cx);
                    }
                    Ok(_) => {}
                    Err(error) => this.status = error.to_string(),
                }
                cx.notify();
            },
        );
    }

    /// Start emoji verification with another user; the flow shows in Settings › Security.
    pub(super) fn verify_user(
        &mut self,
        user: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.preview {
            return;
        }
        self.perform_security(crate::backend::SecurityAction::VerifyUser(user), window, cx);
        self.open_settings(crate::settings::Section::Security, cx);
    }

    /// Ignore, kick, ban or (un)mute. Destructive ones need a second click.
    pub(super) fn member_action(
        &mut self,
        user: String,
        action: twayblade_core::room_state::MemberAction,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        use twayblade_core::room_state::MemberAction;
        let destructive = matches!(
            action,
            MemberAction::Kick | MemberAction::Ban | MemberAction::Ignore
        );
        if destructive && self.member_confirm.as_ref() != Some(&(user.clone(), action)) {
            self.member_confirm = Some((user, action));
            cx.notify();
            return;
        }
        self.member_confirm = None;
        if self.preview {
            cx.notify();
            return;
        }
        let Some(room) = self.navigation.selected_room.clone() else {
            return;
        };
        self.job(
            window,
            cx,
            move |backend| backend.member_action(room, user, action),
            move |this, result, _, cx| {
                this.status = match result {
                    Ok(()) => match action {
                        MemberAction::Ignore => {
                            "Ignored. Their messages are hidden everywhere.".into()
                        }
                        MemberAction::Unignore => "No longer ignored.".into(),
                        MemberAction::Kick => "Removed from the room.".into(),
                        MemberAction::Ban => "Banned from the room.".into(),
                        MemberAction::Mute(true) => "Muted in this room.".into(),
                        MemberAction::Mute(false) => "Unmuted in this room.".into(),
                    },
                    Err(error) => error.to_string(),
                };
                cx.notify();
            },
        );
    }
}

impl MatrixApp {
    #[allow(dead_code, reason = "Gallery UI entry point")]
    pub fn load_media(
        &mut self,
        kind: twayblade_core::media::MediaKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(backend) = &self.backend
            && let Some(room) = &self.navigation.selected_room
            && let Err(error) = backend.load_media(kind, self.snapshot.account_instance, room)
        {
            self.status = error.to_string();
            cx.notify();
        }
        let _ = window;
    }
    #[allow(dead_code, reason = "Gallery UI entry point")]
    pub fn load_more_media(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(backend) = &self.backend
            && let Some(room) = &self.navigation.selected_room
            && let Err(error) = backend.load_more_media(self.snapshot.account_instance, room)
        {
            self.status = error.to_string();
            cx.notify();
        }
        let _ = window;
    }
    fn gallery_entries(&self) -> Vec<crate::viewer::ImageEntry> {
        self.snapshot
            .media
            .items
            .iter()
            .map(|item| crate::viewer::ImageEntry {
                message: crate::backend::Message {
                    id: item.event_id.clone(),
                    event_id: Some(item.event_id.clone()),
                    sender: item.sender.clone(),
                    display_name: item.sender_name.clone(),
                    timestamp_ms: item.timestamp_ms,
                    kind: crate::backend::MessageKind::Image,
                    attachment: Some(crate::backend::Attachment {
                        filename: item.filename.clone(),
                        size: item.size,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                in_thread: false,
            })
            .collect()
    }
    #[allow(dead_code, reason = "Gallery UI entry point")]
    pub fn open_media_image(&mut self, event: &str, window: &mut Window, cx: &mut Context<Self>) {
        use crate::viewer::{ImageLoad, Viewer};
        if self.navigation.selected_room.as_deref() != Some(self.snapshot.media.room.as_str())
            || self.snapshot.media.room != self.snapshot.selected_room
            || self.snapshot.media.kind != twayblade_core::media::MediaKind::Images
        {
            return;
        }
        let entries = self.gallery_entries();
        let Some(index) = entries
            .iter()
            .position(|e| e.message.event_id.as_deref() == Some(event))
        else {
            return;
        };
        if let Some(old) = self.viewer.take() {
            old.cancel.send_replace(true);
        }
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        self.viewer_request = self.viewer_request.wrapping_add(1);
        self.viewer = Some(Viewer {
            room: self.snapshot.selected_room.clone(),
            entries,
            index,
            request: self.viewer_request,
            load: ImageLoad::Loading,
            zoom: 1.,
            pan: point(gpui_kit::px(0.), gpui_kit::px(0.)),
            drag: None,
            focus,
            cancel: tokio::sync::watch::channel(false).0,
            status: String::new(),
            previous_pending: false,
        });
        self.load_viewer(window, cx);
    }
    pub(super) fn image_entries(&self) -> Vec<crate::viewer::ImageEntry> {
        if self.snapshot.media.room == self.snapshot.selected_room
            && self.snapshot.media.kind == twayblade_core::media::MediaKind::Images
            && self.viewer.as_ref().is_some_and(|v| {
                self.snapshot.media.items.iter().any(|i| {
                    Some(&i.event_id)
                        == v.entries
                            .get(v.index)
                            .and_then(|e| e.message.event_id.as_ref())
                })
            })
        {
            return self.gallery_entries();
        }
        let mut entries: Vec<_> = self
            .snapshot
            .messages
            .iter()
            .filter(|m| m.kind == crate::backend::MessageKind::Image && m.event_id.is_some())
            .cloned()
            .map(|message| crate::viewer::ImageEntry {
                message,
                in_thread: false,
            })
            .collect();
        if let Some(thread) = &self.thread {
            let mut after = 0;
            for message in std::iter::once(&thread.root)
                .chain(&thread.messages)
                .filter(|m| m.kind == crate::backend::MessageKind::Image && m.event_id.is_some())
            {
                if let Some(ix) = entries
                    .iter()
                    .position(|e| e.message.event_id == message.event_id)
                {
                    after = ix + 1;
                    continue;
                }
                let ix = (after..entries.len())
                    .find(|&ix| entries[ix].message.timestamp_ms > message.timestamp_ms)
                    .unwrap_or(entries.len());
                entries.insert(
                    ix,
                    crate::viewer::ImageEntry {
                        message: message.clone(),
                        in_thread: true,
                    },
                );
                after = ix + 1;
            }
        }
        let mut seen = std::collections::HashSet::new();
        entries.retain(|e| seen.insert(e.message.event_id.clone()));
        entries
    }
}

#[allow(dead_code, reason = "Message search UI action contracts")]
impl MatrixApp {
    pub fn search_messages(
        &mut self,
        query: twayblade_core::search::SearchQuery,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(backend) = &self.backend
            && let Err(error) = backend.search(query)
        {
            self.status = error.to_string();
            cx.notify();
        }
        let _ = window;
    }
    pub fn search_more(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.job(window, cx, |b| b.search_more(), |_, _, _, _| {});
    }
    pub fn open_event(
        &mut self,
        room: String,
        event: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_room_target(room, Some(event), window, cx);
    }
}

#[allow(dead_code, reason = "Presence UI action contracts")]
impl MatrixApp {
    pub fn observe_presence(&self, users: Vec<String>) {
        if let Some(backend) = &self.backend {
            let _ = backend.observe_presence(users);
        }
    }
    pub fn set_presence(
        &mut self,
        state: twayblade_core::presence::State,
        status: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.job(
            window,
            cx,
            move |b| b.set_presence(state, status),
            |_, _, _, _| {},
        );
    }
}

impl MatrixApp {
    /// Search messages for the title-bar query, in this room or all rooms.
    pub(super) fn run_message_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.search.read(cx).value().trim().to_owned();
        if text.is_empty() {
            return;
        }
        self.search_open = true;
        if self.preview {
            cx.notify();
            return;
        }
        let room = (!self.search_all_rooms)
            .then(|| self.snapshot.selected_room.clone())
            .filter(|room| !room.is_empty())
            .and_then(|room| room.try_into().ok());
        self.search_messages(
            twayblade_core::search::SearchQuery {
                text,
                room,
                sender: None,
                has_file: self.search_has_file,
            },
            window,
            cx,
        );
        cx.notify();
    }
}
