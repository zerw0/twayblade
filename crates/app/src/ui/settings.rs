//! One separate Settings window backed by the retained main view.
use crate::theme::scaled_px as px;
use crate::{
    app::{MatrixApp, OpenSettings},
    preferences::{ChatStyle, Density, Preferences, Scheme, SidebarDensity, Timestamps, Variant},
};
use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    component::{
        button::{Button, ButtonVariants},
        input::{Input, InputState},
        *,
    },
    prelude::FluentBuilder,
    *,
};
use twayblade_core::profile::ProfileUpdate;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Section {
    General,
    Notifications,
    #[default]
    Appearance,
    Chat,
    Keyboard,
    Security,
    Account,
}
impl Section {
    pub const ALL: [Self; 7] = [
        Self::General,
        Self::Notifications,
        Self::Appearance,
        Self::Chat,
        Self::Keyboard,
        Self::Security,
        Self::Account,
    ];
    /// Sections with working settings. General remains hidden until it has controls.
    pub const VISIBLE: [Self; 6] = [
        Self::Notifications,
        Self::Appearance,
        Self::Chat,
        Self::Keyboard,
        Self::Security,
        Self::Account,
    ];
    fn icon(self) -> gpui_kit::assets::IconName {
        use gpui_kit::assets::IconName;
        match self {
            Self::General => IconName::Settings,
            Self::Notifications => IconName::Bell,
            Self::Appearance => IconName::Palette,
            Self::Chat => IconName::MessagesSquare,
            Self::Keyboard => IconName::Keyboard,
            Self::Security => IconName::ShieldCheck,
            Self::Account => IconName::UserRound,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Notifications => "Notifications",
            Self::Appearance => "Appearance",
            Self::Chat => "Chat",
            Self::Keyboard => "Keyboard",
            Self::Security => "Security",
            Self::Account => "Account",
        }
    }
}
pub(crate) struct SettingsWindow {
    pub section: Section,
    main: Entity<MatrixApp>,
    _observe: Subscription,
}
impl MatrixApp {
    pub(crate) fn notification_config(&self) {
        if self.preview {
            return;
        }
        if let Some(backend) = &self.backend {
            backend.set_notification_config(crate::backend::NotificationConfig {
                device: self.preferences.notifications.clone(),
                context: twayblade_core::notifications::NotificationContext {
                    enabled: self.preferences.notifications.enabled,
                    active: self.notification_window_active,
                    focused_room: self.navigation.selected_room.clone(),
                },
                unread: self.snapshot.unread_total,
            });
        }
    }

    fn device_notifications(&self, cx: &mut Context<Self>) -> AnyElement {
        use twayblade_platform::notifications::Permission;
        let permission = if self.preview {
            Permission::Granted
        } else {
            self.snapshot.notification_permission
        };
        let label = match permission {
            Permission::Granted => "Allowed by the system",
            Permission::Denied => "Denied by the system",
            Permission::NotRequested => "Permission will be requested when enabled",
            Permission::Unavailable => {
                if cfg!(target_os = "macos") {
                    "Unavailable here — run the app bundle"
                } else {
                    "System notifications are unavailable"
                }
            }
            Permission::Unknown => "Checking system permission…",
        };
        let values = &self.preferences.notifications;
        let rows = [
            (
                "device-notifications",
                "Enable notifications",
                values.enabled,
            ),
            ("notification-sound", "Sound", values.sound),
            ("notification-preview", "Message preview", values.preview),
            ("notification-badge", "Dock / taskbar badge", values.badge),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (id, label, checked))| {
            form_row(
                label,
                switch::Switch::new(id)
                    .accessibility_label(label)
                    .checked(checked)
                    .on_click(cx.listener(move |this, checked: &bool, window, cx| {
                        match index {
                            0 => this.preferences.notifications.enabled = *checked,
                            1 => this.preferences.notifications.sound = *checked,
                            2 => this.preferences.notifications.preview = *checked,
                            _ => this.preferences.notifications.badge = *checked,
                        }
                        this.save_preferences(window, cx);
                        this.notification_config();
                        cx.notify();
                    })),
                cx,
            )
        })
        .collect::<Vec<_>>();
        v_flex()
            .gap(px(8.))
            .child(div().text_size(px(13.)).child("This device"))
            .child(group(rows, cx))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(cx.theme().muted_foreground)
                    .child(label),
            )
            .when(permission == Permission::Denied, |view| {
                view.child(
                    Button::new("open-notification-system-settings")
                        .small()
                        .label("Open System Settings")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.job(
                                window,
                                cx,
                                |backend| backend.open_notification_settings(),
                                |this, result, _, cx| {
                                    if let Err(error) = result {
                                        this.settings_status = error.to_string();
                                        cx.notify();
                                    }
                                },
                            )
                        })),
                )
            })
            .when(self.snapshot.notification_failed, |view| {
                view.child(div().text_size(px(12.)).child(
                    "A notification could not be delivered. Check system notification settings.",
                ))
            })
            .into_any_element()
    }

    pub(crate) fn notification_sample_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let preview = self.preferences.notifications.preview;
        v_flex()
            .id("notification-sample")
            .absolute()
            .top(px(58.))
            .right(px(18.))
            .w(px(340.))
            .p(px(14.))
            .gap(px(6.))
            .rounded(px(10.))
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(cx.theme().muted_foreground)
                    .child("Sample data · Notification preview"),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(if preview {
                        "Alex · General"
                    } else {
                        "Twayblade"
                    }),
            )
            .child(div().text_size(px(13.)).child(if preview {
                "The sample review is ready."
            } else {
                "New message"
            }))
            .child(
                Button::new("sample-notification-open")
                    .small()
                    .label("Open conversation")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.activate_notification(
                            twayblade_platform::notifications::Activation {
                                account: this.snapshot.account_instance,
                                room: "general".into(),
                                thread: Some("$sample-0:preview.invalid".into()),
                            },
                            window,
                            cx,
                        );
                    })),
            )
            .into_any_element()
    }

    pub(crate) fn change_notification_rule(
        &mut self,
        change: twayblade_core::notifications::RuleChange,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use twayblade_core::notifications::{RoomMode, RuleChange};
        if self.notification_busy {
            return;
        }
        if self.preview {
            let rules = &mut self.snapshot.notifications;
            match change {
                RuleChange::Default(class, level) => rules.defaults[class.index()] = level,
                RuleChange::AddKeyword(word) => {
                    let word = word.trim().to_owned();
                    if !word.is_empty() && !rules.keywords.contains(&word) {
                        rules.keywords.push(word);
                    }
                }
                RuleChange::RemoveKeyword(word) => rules.keywords.retain(|value| *value != word),
                RuleChange::Room(room, mode) => {
                    rules.rooms.insert(room.clone(), mode);
                    if let Some(room) = self
                        .snapshot
                        .rooms
                        .iter_mut()
                        .find(|value| value.id == room)
                    {
                        room.muted = mode == RoomMode::Mute;
                        if room.muted {
                            room.unread = 0;
                        }
                    }
                }
            }
            self.settings_status = "Sample data · Notification rule preview".into();
            cx.notify();
            return;
        }
        if !self.snapshot.notifications.ready {
            return;
        }
        self.notification_busy = true;
        self.settings_status = "Saving notification settings…".into();
        let account = self.snapshot.account_instance;
        self.job(
            window,
            cx,
            move |backend| backend.notification_rule(change),
            move |this, result, _, cx| {
                if this.snapshot.account_instance != account {
                    return;
                }
                this.notification_busy = false;
                this.settings_status = match result {
                    Ok(()) => "Notification settings saved.".into(),
                    Err(error) => error.to_string(),
                };
                cx.notify();
            },
        );
    }

    fn notification_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        use twayblade_core::notifications::{DefaultLevel, RoomClass, RuleChange};
        if !self.preview && !self.snapshot.notifications.ready {
            return div()
                .text_size(px(13.))
                .child("Notification settings are loading. Check your connection if this persists.")
                .into_any_element();
        }
        let rules = &self.snapshot.notifications;
        let labels = [
            "Direct chats",
            "Encrypted direct chats",
            "Group rooms",
            "Encrypted group rooms",
        ];
        let ids = [
            ["notify-dm-all", "notify-dm-mentions"],
            ["notify-encrypted-dm-all", "notify-encrypted-dm-mentions"],
            ["notify-group-all", "notify-group-mentions"],
            [
                "notify-encrypted-group-all",
                "notify-encrypted-group-mentions",
            ],
        ];
        let rows = RoomClass::ALL
            .into_iter()
            .enumerate()
            .map(|(index, class)| {
                let level = rules.defaults[index];
                form_row(
                    labels[index],
                    segmented(
                        [
                            (ids[index][0], "All messages", level == DefaultLevel::All),
                            (
                                ids[index][1],
                                "Mentions and keywords",
                                level == DefaultLevel::Mentions,
                            ),
                        ],
                        cx,
                        move |this, index, window, cx| {
                            this.change_notification_rule(
                                RuleChange::Default(
                                    class,
                                    if index == 0 {
                                        DefaultLevel::All
                                    } else {
                                        DefaultLevel::Mentions
                                    },
                                ),
                                window,
                                cx,
                            );
                        },
                    ),
                    cx,
                )
            })
            .collect::<Vec<_>>();
        let mut keywords = v_flex().gap(px(4.));
        for (index, word) in rules.keywords.iter().enumerate() {
            let remove = word.clone();
            keywords = keywords.child(
                h_flex()
                    .px(px(12.))
                    .py(px(4.))
                    .gap(px(8.))
                    .child(div().flex_1().text_size(px(13.)).child(word.clone()))
                    .child(
                        Button::new(SharedString::from(format!("remove-keyword-{index}")))
                            .ghost()
                            .xsmall()
                            .icon(gpui_kit::assets::IconName::X)
                            .accessibility_label(format!("Remove keyword {word}"))
                            .tooltip("Remove keyword")
                            .disabled(self.notification_busy)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.change_notification_rule(
                                    RuleChange::RemoveKeyword(remove.clone()),
                                    window,
                                    cx,
                                )
                            })),
                    ),
            );
        }
        v_flex().gap(px(14.))
            .child(group(rows, cx))
            .child(div().text_size(px(13.)).child("Keywords"))
            .child(keywords)
            .child(h_flex().id("notification-keyword-entry").gap(px(8.)).child(Input::new(&self.notification_keyword).disabled(self.notification_busy)).child(
                Button::new("add-notification-keyword").small().label("Add").disabled(self.notification_busy)
                .on_click(cx.listener(|this, _, window, cx| {
                    let word = this.notification_keyword.read(cx).value().to_string();
                    if word.trim().is_empty() { return; }
                    this.change_notification_rule(RuleChange::AddKeyword(word), window, cx);
                    this.notification_keyword.update(cx, |input, cx| input.set_value("", window, cx));
                }))
            ))
            .child(div().text_size(px(12.)).text_color(cx.theme().muted_foreground).child("These notification rules sync with your other Matrix apps. Mute is available for individual rooms."))
            .child(self.device_notifications(cx))
            .into_any_element()
    }

    pub(crate) fn room_notification_controls(
        &self,
        room: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use twayblade_core::notifications::{RoomMode, RuleChange};
        if !self.preview && !self.snapshot.notifications.ready {
            return div().into_any_element();
        }
        let mode = self.snapshot.notifications.room_mode(room);
        let room = room.to_owned();
        v_flex()
            .px(px(6.))
            .py(px(8.))
            .gap(px(6.))
            .child(div().text_size(px(13.)).child("Notifications"))
            .child(segmented(
                [
                    (
                        "room-notifications-default",
                        "Default",
                        mode == RoomMode::Default,
                    ),
                    ("room-notifications-all", "All", mode == RoomMode::All),
                    (
                        "room-notifications-mentions",
                        "Mentions",
                        mode == RoomMode::Mentions,
                    ),
                    ("room-notifications-mute", "Mute", mode == RoomMode::Mute),
                ],
                cx,
                move |this, index, window, cx| {
                    this.change_notification_rule(
                        RuleChange::Room(
                            room.clone(),
                            [
                                RoomMode::Default,
                                RoomMode::All,
                                RoomMode::Mentions,
                                RoomMode::Mute,
                            ][index],
                        ),
                        window,
                        cx,
                    )
                },
            ))
            .into_any_element()
    }

    pub(crate) fn open_settings(&mut self, section: Section, cx: &mut Context<Self>) {
        let main = cx.entity();
        // GPUI draws a newly opened window immediately. Release the main entity
        // before creating it, because Settings renders controls from that entity.
        cx.defer(move |cx| Self::open_settings_window(main, section, cx));
    }
    fn open_settings_window(main: Entity<Self>, section: Section, cx: &mut App) {
        let existing = main.read(cx).settings.clone();
        let section = if main.read(cx).security_key.is_some() {
            Section::Security
        } else {
            section
        };
        if let Some((handle, view)) = existing
            && view
                .update(cx, |root, cx| {
                    root.section = section;
                    cx.notify();
                })
                .is_ok()
            && handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
        {
            return;
        }
        let result = gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::centered(size(px(880.), px(700.)), cx)),
                window_min_size: Some(size(px(760.), px(540.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Settings".into()),
                    appears_transparent: cfg!(target_os = "macos"),
                    traffic_light_position: cfg!(target_os = "macos")
                        .then(|| point(px(16.), px(14.))),
                }),
                app_owns_titlebar_drag: cfg!(target_os = "macos"),
                ..Default::default()
            },
            cx,
            |window, cx| {
                let weak = main.downgrade();
                window.on_window_should_close(cx, move |_window, cx| {
                    weak.update(cx, |main, cx| {
                        if main.security_key.is_some() {
                            return false;
                        }
                        main.security_restore = false;
                        main.security_device = None;
                        main.security_input =
                            cx.new(|cx| InputState::new(_window, cx).masked(true));
                        main.settings = None;
                        true
                    })
                    .unwrap_or(true)
                });
                cx.new(|cx| {
                    let observe = cx.observe(&main, |_, _, cx| cx.notify());
                    Self::settings_root(section, main.clone(), observe)
                })
            },
        );
        match result {
            Ok((window, view)) => main.update(cx, |main, cx| {
                main.settings = Some((window, view.downgrade()));
                cx.notify();
            }),
            Err(_) => main.update(cx, |main, cx| {
                main.settings_status = "Settings could not be opened.".into();
                cx.notify();
            }),
        }
    }
    fn settings_root(
        section: Section,
        main: Entity<MatrixApp>,
        observe: Subscription,
    ) -> SettingsWindow {
        SettingsWindow {
            section,
            main,
            _observe: observe,
        }
    }
    pub(crate) fn reset_settings(
        &mut self,
        section: Section,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if section == Section::Chat {
            self.preferences.chat = Default::default();
            self.chat_changed(window, cx);
            return;
        }
        if section == Section::Appearance {
            self.preferences.appearance = Default::default();
            for selector in &self.font_selectors {
                selector.update(cx, |state, cx| {
                    state.set_selected_value(&"System default".to_string(), window, cx)
                });
            }
            self.appearance_changed(window, cx);
            return;
        }
        // Reset local form state only. Reset never mutates a Matrix account or keys.
        if section == Section::Security {
            if self.security_key.is_some() {
                return;
            }
            self.security_restore = false;
            self.security_device = None;
            self.security_reset_confirmation = false;
            self.security_input = cx.new(|cx| InputState::new(window, cx).masked(true));
        }
        if section == Section::Account {
            self.account_name = None;
        }
        self.settings_status =
            "Defaults restored. Server account and security data are retained.".into();
        self.save_preferences(window, cx);
        cx.notify();
    }
    pub(crate) fn save_preferences(&mut self, window: &Window, cx: &Context<Self>) {
        if self.preview || !self.preferences_writable {
            return;
        }
        let value = self.preferences.clone();
        use std::sync::atomic::Ordering;
        let revision = self.preference_revision.fetch_add(1, Ordering::SeqCst) + 1;
        let latest = self.preference_revision.clone();
        let lock = self.preference_write_lock.clone();
        cx.spawn_in(window, async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    let _guard = lock
                        .lock()
                        .map_err(|_| std::io::Error::other("Preferences writer unavailable"))?;
                    if latest.load(Ordering::SeqCst) != revision {
                        return Ok(());
                    }
                    Preferences::path()
                        .ok_or_else(|| std::io::Error::other("No config directory"))
                        .and_then(|path| value.save(&path))
                })
                .await;
            if result.is_err() {
                let _ = view.update(cx, |this, cx| {
                    this.settings_status = "Preferences could not be saved.".into();
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub(crate) fn change_scale(&mut self, delta: i16, window: &mut Window, cx: &mut Context<Self>) {
        self.preferences.appearance.scale =
            (self.preferences.appearance.scale as i16 + delta).clamp(85, 150) as u16;
        self.appearance_changed(window, cx);
    }
    pub(crate) fn reset_scale(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.preferences.appearance.scale = 100;
        self.appearance_changed(window, cx);
    }
    pub(crate) fn appearance_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.preview {
            Theme::sync_system_appearance(Some(window), cx);
        }
        crate::theme::set_appearance(self.preferences.appearance.clone(), cx);
        if let Some([r, g, b]) = self.preferences.appearance.accent {
            let value = Hsla::from(Rgba {
                r: f32::from(r) / 255.,
                g: f32::from(g) / 255.,
                b: f32::from(b) / 255.,
                a: 1.,
            });
            self.accent_picker
                .update(cx, |picker, cx| picker.set_value(value, window, cx));
        }
        let custom = self.preferences.appearance.custom_theme.unwrap_or_default();
        for (picker, value) in self.theme_pickers.iter().zip(custom.colors()) {
            picker.update(cx, |picker, cx| {
                picker.set_value(crate::theme::swatch(value), window, cx)
            });
        }
        self.scroller.update(cx, |state, cx| state.remeasure(cx));
        if let Some(thread) = &self.thread {
            thread.scroller.update(cx, |state, cx| state.remeasure(cx));
        }
        if self.preferences_writable {
            self.settings_status.clear();
        }
        self.save_preferences(window, cx);
        cx.notify();
    }
    pub(crate) fn load_preset_to_custom(
        &mut self,
        scheme: Scheme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dark = cx.theme().is_dark();
        self.preferences.appearance.custom_theme = Some(
            crate::preferences::CustomTheme::from_colors(crate::theme::surfaces(scheme, dark)),
        );
        self.preferences.appearance.scheme = Scheme::Custom;
        self.appearance_changed(window, cx);
    }
    pub(crate) fn reset_custom_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.preferences.appearance.custom_theme = Some(Default::default());
        self.preferences.appearance.scheme = Scheme::Custom;
        self.appearance_changed(window, cx);
    }
    fn appearance_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let appearance = &self.preferences.appearance;
        let dark = cx.theme().is_dark();
        let muted = cx.theme().muted_foreground;
        // Scheme tiles, like the Appearance thumbnails in System Settings.
        let mut tiles = h_flex().gap(px(12.)).flex_wrap();
        for scheme in Scheme::ALL {
            let selected = appearance.scheme == scheme;
            let preview_dark = match scheme {
                Scheme::Light => false,
                Scheme::Dark => true,
                _ => dark,
            };
            let [background, sidebar, text] =
                crate::theme::scheme_preview(scheme, preview_dark, appearance.custom_theme);
            let label = scheme_label(scheme);
            tiles = tiles.child(
                Button::new(SharedString::from(format!("scheme-{}", scheme.name())))
                    .ghost()
                    .p_0()
                    .h_auto()
                    .w(px(72.))
                    .accessibility_label(label)
                    .child(
                        v_flex()
                            .w(px(72.))
                            .items_center()
                            .gap(px(4.))
                            .child(
                                h_flex()
                                    .w(px(56.))
                                    .h(px(36.))
                                    .rounded(px(6.))
                                    .overflow_hidden()
                                    .border_2()
                                    .border_color(if selected {
                                        cx.theme().primary
                                    } else {
                                        cx.theme().border
                                    })
                                    .bg(background)
                                    .child(div().w(px(16.)).h_full().bg(sidebar))
                                    .child(
                                        v_flex()
                                            .flex_1()
                                            .p(px(5.))
                                            .gap(px(3.))
                                            .child(
                                                div().w(px(22.)).h(px(3.)).rounded(px(2.)).bg(text),
                                            )
                                            .child(
                                                div()
                                                    .w(px(14.))
                                                    .h(px(6.))
                                                    .ml_auto()
                                                    .rounded(px(3.))
                                                    .bg(cx.theme().primary),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .text_center()
                                    .text_size(px(11.))
                                    .text_color(if selected {
                                        cx.theme().foreground
                                    } else {
                                        muted
                                    })
                                    .child(label),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.preferences.appearance.scheme = scheme;
                        this.appearance_changed(window, cx);
                    })),
            );
        }
        let has_variants = !matches!(
            appearance.scheme,
            Scheme::System | Scheme::Light | Scheme::Dark | Scheme::Custom
        );
        let variant = segmented(
            [
                (
                    "variant-System",
                    "Auto",
                    appearance.variant == Variant::System,
                ),
                (
                    "variant-Light",
                    "Light",
                    appearance.variant == Variant::Light,
                ),
                ("variant-Dark", "Dark", appearance.variant == Variant::Dark),
            ],
            cx,
            |this, index, window, cx| {
                this.preferences.appearance.variant =
                    [Variant::System, Variant::Light, Variant::Dark][index];
                this.appearance_changed(window, cx);
            },
        );
        // Accent: System first, then the swatches, then a custom picker.
        let mut accents = h_flex().gap(px(6.)).items_center();
        let system_selected = appearance.accent.is_none();
        accents = accents.child(
            Button::new("system-accent")
                .ghost()
                .p_0()
                .size(px(22.))
                .rounded(px(11.))
                .accessibility_label("System accent")
                .tooltip("System")
                .child(accent_dot(cx.theme().primary, system_selected, true, cx))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.preferences.appearance.accent = None;
                    this.appearance_changed(window, cx);
                })),
        );
        for (label, color) in crate::theme::SWATCHES {
            let fill = crate::theme::swatch(color);
            let bytes = crate::theme::accent_bytes(fill);
            let selected = appearance.accent == Some(bytes);
            accents = accents.child(
                Button::new(SharedString::from(format!("accent-{label}")))
                    .ghost()
                    .p_0()
                    .size(px(22.))
                    .rounded(px(11.))
                    .accessibility_label(format!("{label} accent"))
                    .tooltip(label)
                    .child(accent_dot(fill, selected, false, cx))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.preferences.appearance.accent = Some(bytes);
                        this.appearance_changed(window, cx);
                    })),
            );
        }
        accents = accents.child(
            color_picker::ColorPicker::new(&self.accent_picker)
                .small()
                .accessibility_label("Choose custom accent colour"),
        );
        let custom_theme = appearance.custom_theme.unwrap_or_default();
        let color_row = |id: &'static str,
                         label: &'static str,
                         color_val: u32,
                         picker: &Entity<color_picker::ColorPickerState>,
                         cx: &App| {
            form_row(
                label,
                h_flex()
                    .id(SharedString::from(format!("custom-color-{id}")))
                    .gap(px(8.))
                    .items_center()
                    .child(
                        div()
                            .size(px(18.))
                            .rounded(px(4.))
                            .border_1()
                            .border_color(cx.theme().border)
                            .bg(crate::theme::swatch(color_val)),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("#{:06x}", color_val)),
                    )
                    .child(
                        color_picker::ColorPicker::new(picker)
                            .small()
                            .accessibility_label(format!("Choose {label} colour")),
                    ),
                cx,
            )
        };
        let presets = [
            ("preset-catppuccin", "Catppuccin", Scheme::Catppuccin),
            ("preset-nord", "Nord", Scheme::Nord),
            ("preset-gruvbox", "Gruvbox", Scheme::Gruvbox),
            ("preset-solarized", "Solarized", Scheme::Solarized),
            ("preset-dark", "Dark", Scheme::Dark),
            ("preset-light", "Light", Scheme::Light),
        ]
        .into_iter()
        .map(|(id, name, scheme)| {
            Button::new(id)
                .small()
                .label(name)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.load_preset_to_custom(scheme, window, cx);
                }))
        });

        let theme_editor_rows = vec![
            v_flex()
                .px(px(12.))
                .py(px(10.))
                .gap(px(8.))
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(
                            v_flex()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("Theme Editor"),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(muted)
                                        .child("Customize theme surfaces and palettes, or load a preset template to edit."),
                                ),
                        )
                        .child(
                            Button::new("reset-custom-theme")
                                .small()
                                .label("Reset Custom")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.reset_custom_theme(window, cx);
                                })),
                        ),
                )
                .child(
                    h_flex()
                        .gap(px(6.))
                        .items_center()
                        .flex_wrap()
                        .child(div().text_size(px(11.)).text_color(muted).child("Load template:"))
                        .children(presets),
                ),
        ];
        let mut theme_editor_rows = theme_editor_rows;
        for (((id, label), value), picker) in crate::preferences::CustomTheme::FIELDS
            .into_iter()
            .zip(custom_theme.colors())
            .zip(&self.theme_pickers)
        {
            theme_editor_rows.push(color_row(id, label, value, picker, cx));
        }
        if crate::theme::custom_palette(custom_theme).adjusted {
            theme_editor_rows.push(
                div()
                    .px(px(12.))
                    .py(px(8.))
                    .text_size(px(11.))
                    .text_color(muted)
                    .child(
                        "Some text colours are too faint on these backgrounds, \
                         so Twayblade shows a more readable shade.",
                    ),
            );
        }

        let fonts = ["Interface font", "Message font", "Code font"]
            .into_iter()
            .enumerate()
            .map(|(index, label)| {
                form_row(
                    label,
                    div().w(px(220.)).child(
                        select::Select::new(&self.font_selectors[index])
                            .small()
                            .accessibility_label(label),
                    ),
                    cx,
                )
            })
            .collect::<Vec<_>>();
        let density = appearance.sidebar_density;
        let sidebar_density = segmented(
            [
                (
                    "sidebar-density-Compact",
                    "Compact",
                    density == SidebarDensity::Compact,
                ),
                (
                    "sidebar-density-Regular",
                    "Regular",
                    density == SidebarDensity::Regular,
                ),
                (
                    "sidebar-density-Comfortable",
                    "Comfortable",
                    density == SidebarDensity::Comfortable,
                ),
            ],
            cx,
            |this, index, window, cx| {
                this.preferences.appearance.sidebar_density = [
                    SidebarDensity::Compact,
                    SidebarDensity::Regular,
                    SidebarDensity::Comfortable,
                ][index];
                this.appearance_changed(window, cx);
            },
        );

        let typography_rows = std::iter::once(
            v_flex()
                .px(px(12.))
                .py(px(10.))
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Typography & Fonts"),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(muted)
                        .child("Choose font families and sizing for interface controls, chat messages, and code."),
                ),
        )
        .chain(fonts)
        .collect::<Vec<_>>();

        let message_font = crate::theme::message_font(cx);
        let preview_card = v_flex()
            .px(px(12.))
            .py(px(10.))
            .gap(px(10.))
            .child(
                v_flex()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Theme & Typography Preview"),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(muted)
                            .child("Live preview of current theme colors, interface font, message font, and code font."),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(cx.theme().border)
                    .overflow_hidden()
                    .bg(cx.theme().background)
                    .child(
                        v_flex()
                            .w(px(130.))
                            .p(px(8.))
                            .gap(px(6.))
                            .bg(cx.theme().sidebar)
                            .border_r_1()
                            .border_color(cx.theme().border)
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(cx.theme().sidebar_foreground)
                                    .child("CHANNELS"),
                            )
                            .child(
                                h_flex()
                                    .px(px(6.))
                                    .py(px(3.))
                                    .rounded(px(4.))
                                    .bg(cx.theme().accent)
                                    .text_color(cx.theme().accent_foreground)
                                    .text_size(px(12.))
                                    .child("# general"),
                            )
                            .child(
                                h_flex()
                                    .px(px(6.))
                                    .py(px(3.))
                                    .text_color(cx.theme().muted_foreground)
                                    .text_size(px(12.))
                                    .child("# random"),
                            ),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .p(px(12.))
                            .gap(px(10.))
                            .child(
                                h_flex()
                                    .justify_between()
                                    .items_center()
                                    .border_b_1()
                                    .border_color(cx.theme().border)
                                    .pb(px(6.))
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("# general"),
                                    )
                                    .child(
                                        div()
                                            .px(px(8.))
                                            .py(px(2.))
                                            .rounded(px(4.))
                                            .bg(cx.theme().primary)
                                            .text_color(cx.theme().primary_foreground)
                                            .text_size(px(11.))
                                            .child("Accent"),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .size(px(28.))
                                            .rounded(px(14.))
                                            .bg(cx.theme().primary)
                                            .text_color(cx.theme().primary_foreground)
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .text_size(px(11.))
                                            .font_weight(FontWeight::BOLD)
                                            .child("AR"),
                                    )
                                    .child(
                                        v_flex()
                                            .flex_1()
                                            .gap(px(4.))
                                            .child(
                                                h_flex()
                                                    .gap(px(6.))
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .text_size(px(12.))
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .child("Alex Rivera"),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(10.))
                                                            .text_color(muted)
                                                            .child("10:42 AM"),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .font_family(message_font)
                                                    .text_size(px(f32::from(appearance.message_size)))
                                                    .child("Welcome to Twayblade! Customize your themes, colors, and fonts."),
                                            )
                                            .child(
                                                div()
                                                    .p(px(6.))
                                                    .rounded(px(4.))
                                                    .bg(cx.theme().secondary)
                                                    .text_color(cx.theme().secondary_foreground)
                                                    .font_family(cx.theme().mono_font_family.clone())
                                                    .text_size(px(11.5))
                                                    .child("let theme = Theme::catppuccin();"),
                                            ),
                                    ),
                            ),
                    ),
            );

        v_flex()
            .gap(px(18.))
            .child(group(
                // The thumbnails need the full width, so the label sits above them.
                std::iter::once(
                    v_flex()
                        .px(px(12.))
                        .py(px(10.))
                        .gap(px(10.))
                        .child(div().text_size(px(13.)).child("Appearance & Themes"))
                        .child(tiles),
                )
                // Only curated palettes have light/dark variants to choose from.
                .chain(has_variants.then(|| form_row("Palette variant", variant, cx)))
                .chain(std::iter::once(form_row("Accent colour", accents, cx)))
                .collect::<Vec<_>>(),
                cx,
            ))
            .child(group(theme_editor_rows, cx))
            .child(group(
                [
                    form_row(
                        "Interface size",
                        stepper(
                            "scale",
                            format!("{}%", appearance.scale),
                            appearance.scale > 85,
                            appearance.scale < 150,
                            "⌘− / ⌘+ / ⌘0",
                            cx,
                            |this, delta, window, cx| this.change_scale(delta * 5, window, cx),
                        ),
                        cx,
                    ),
                    form_row(
                        "Message text size",
                        stepper(
                            "message-size",
                            format!("{} pt", appearance.message_size),
                            appearance.message_size > 10,
                            appearance.message_size < 28,
                            "Independent of interface size",
                            cx,
                            |this, delta, window, cx| {
                                const SIZES: [u16; 10] = [10, 11, 12, 13, 14, 16, 18, 20, 24, 28];
                                let current = this.preferences.appearance.message_size;
                                let ix =
                                    SIZES.iter().position(|s| *s >= current).unwrap_or(3) as i16;
                                let next = (ix + delta).clamp(0, SIZES.len() as i16 - 1) as usize;
                                this.preferences.appearance.message_size = SIZES[next];
                                this.appearance_changed(window, cx);
                            },
                        ),
                        cx,
                    ),
                    form_row("Sidebar density", sidebar_density, cx),
                ],
                cx,
            ))
            .child(group(typography_rows, cx))
            .child(group([preview_card], cx))
            .into_any_element()
    }

    pub(crate) fn chat_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.scroller.update(cx, |state, cx| state.remeasure(cx));
        if let Some(thread) = &self.thread {
            thread.scroller.update(cx, |state, cx| state.remeasure(cx));
        }
        self.save_preferences(window, cx);
        cx.notify();
    }
    fn chat_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let chat = &self.preferences.chat;
        let style = segmented(
            [
                ("style-Bubbles", "Bubbles", chat.style == ChatStyle::Bubbles),
                ("style-Irc", "IRC", chat.style == ChatStyle::Irc),
                ("style-Minimal", "Minimal", chat.style == ChatStyle::Minimal),
            ],
            cx,
            |this, index, window, cx| {
                this.preferences.chat.style =
                    [ChatStyle::Bubbles, ChatStyle::Irc, ChatStyle::Minimal][index];
                this.chat_changed(window, cx);
            },
        );
        let density = segmented(
            [
                (
                    "density-Compact",
                    "Compact",
                    chat.density == Density::Compact,
                ),
                (
                    "density-Regular",
                    "Regular",
                    chat.density == Density::Regular,
                ),
            ],
            cx,
            |this, index, window, cx| {
                this.preferences.chat.density = [Density::Compact, Density::Regular][index];
                this.chat_changed(window, cx);
            },
        );
        let timestamps = segmented(
            [
                (
                    "timestamps-OnHover",
                    "On Hover",
                    chat.timestamps == Timestamps::OnHover,
                ),
                (
                    "timestamps-Always",
                    "Always",
                    chat.timestamps == Timestamps::Always,
                ),
            ],
            cx,
            |this, index, window, cx| {
                this.preferences.chat.timestamps = [Timestamps::OnHover, Timestamps::Always][index];
                this.chat_changed(window, cx);
            },
        );
        use twayblade_core::conversation::PreviewSize;
        let image_size = segmented(
            [
                (
                    "image-size-Default",
                    "Default",
                    chat.preview_size == PreviewSize::Default,
                ),
                (
                    "image-size-Large",
                    "Large",
                    chat.preview_size == PreviewSize::Large,
                ),
            ],
            cx,
            |this, index, window, cx| {
                this.set_timeline_preview_size([PreviewSize::Default, PreviewSize::Large][index]);
                this.chat_changed(window, cx);
            },
        );
        let avatars = switch::Switch::new("avatars")
            .checked(chat.avatars)
            .accessibility_label("Show avatars")
            .on_click(cx.listener(|this, checked: &bool, window, cx| {
                this.preferences.chat.avatars = *checked;
                this.chat_changed(window, cx);
            }));
        v_flex()
            .gap(px(18.))
            .child(group(
                [
                    form_row("Message style", style, cx),
                    form_row("Message density", density, cx),
                    form_row("Timestamps", timestamps, cx),
                    form_row("Images in conversations", image_size, cx),
                    form_row("Show avatars", avatars, cx),
                ],
                cx,
            ))
            .child(
                div()
                    .px(px(4.))
                    .text_size(px(11.))
                    .text_color(cx.theme().muted_foreground)
                    .child("IRC never shows avatars. Changes apply to the open conversation immediately."),
            )
            .into_any_element()
    }
    fn edit_account(&mut self, update: ProfileUpdate, window: &Window, cx: &mut Context<Self>) {
        if self.preview || self.account_edit_busy || self.backend.is_none() {
            return;
        }
        self.account_edit_busy = true;
        self.settings_status.clear();
        let account = self.snapshot.account_instance;
        self.job(
            window,
            cx,
            move |backend| backend.update_profile(account, update),
            move |this, result, window, cx| {
                this.account_edit_busy = false;
                if this.snapshot.account_instance != account {
                    return;
                }
                match result {
                    Ok(profile) => {
                        this.account_name = Some(cx.new(|cx| {
                            InputState::new(window, cx).default_value(profile.display_name)
                        }));
                        this.settings_status = "Account updated.".into();
                    }
                    Err(error) => this.settings_status = error.to_string(),
                }
                cx.notify();
            },
        );
        cx.notify();
    }
    fn choose_avatar(&mut self, window: &Window, cx: &mut Context<Self>) {
        let account = self.snapshot.account_instance;
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose account avatar".into()),
        });
        cx.spawn_in(window, async move |view, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = view.update_in(cx, |this, window, cx| {
                    if this.snapshot.account_instance == account {
                        this.edit_account(ProfileUpdate::Avatar(path), window, cx);
                    }
                });
            }
        })
        .detach();
    }
    fn account_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if self.account_name.is_none() {
            self.account_name =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder("Display name")));
            if !self.preview && self.backend.is_some() {
                let user = self.snapshot.user_id.clone();
                let account = self.snapshot.account_instance;
                self.job(
                    window,
                    cx,
                    move |backend| backend.profile(user, None),
                    move |this, result, window, cx| {
                        if this.snapshot.account_instance == account
                            && let Ok(profile) = result
                            && let Some(input) = &this.account_name
                            && input.read(cx).value().is_empty()
                        {
                            input.update(cx, |state, cx| {
                                state.set_value(profile.display_name, window, cx)
                            });
                        }
                    },
                );
            }
        }
        let disabled =
            self.preview || self.busy || self.account_edit_busy || self.snapshot.user_id.is_empty();
        v_flex()
            .gap_3()
            .child(div().child(self.snapshot.user_id.clone()))
            .child(div().child("Display name"))
            .child(Input::new(self.account_name.as_ref().unwrap()).disabled(disabled))
            .child(
                Button::new("save-account-name")
                    .label("Save display name")
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, window, cx| {
                        let name = this
                            .account_name
                            .as_ref()
                            .map(|input| input.read(cx).value().to_string())
                            .unwrap_or_default();
                        this.edit_account(ProfileUpdate::DisplayName(name), window, cx);
                    })),
            )
            .child(div().child("Avatar"))
            .child(
                Button::new("change-account-avatar")
                    .label("Choose avatar…")
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, window, cx| this.choose_avatar(window, cx))),
            )
            .child(
                Button::new("remove-account-avatar")
                    .label("Remove avatar")
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.edit_account(ProfileUpdate::RemoveAvatar, window, cx)
                    })),
            )
            .child(
                Button::new("account-sign-out")
                    .label("Sign Out")
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, window, cx| this.logout(window, cx))),
            )
            .into_any_element()
    }
}
impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let section = self.section;
        // Same sidebar as the main window: left-aligned icon rows, solid accent
        // selection, running up under the traffic lights.
        let rows =
            v_flex()
                .px(px(8.))
                .gap(px(1.))
                .children(Section::VISIBLE.map(|item| {
                    let selected = item == section;
                    let fg = if selected {
                        cx.theme().primary_foreground
                    } else {
                        cx.theme().foreground
                    };
                    Button::new(SharedString::from(format!("settings-{}", item.label())))
                        .ghost()
                        .w_full()
                        .h(px(28.))
                        .p_0()
                        .rounded(px(6.))
                        .accessibility_label(item.label())
                        .child(
                            h_flex()
                                .size_full()
                                .px(px(8.))
                                .gap(px(8.))
                                .rounded(px(6.))
                                .when(selected, |row| row.bg(cx.theme().primary))
                                .text_color(fg)
                                .child(Icon::new(item.icon()).size(px(15.)).text_color(
                                    if selected {
                                        fg
                                    } else {
                                        cx.theme().muted_foreground
                                    },
                                ))
                                .child(div().text_size(px(13.)).child(item.label())),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if this.main.read(cx).security_key.is_none() {
                                if this.section == Section::Security && item != Section::Security {
                                    this.main.update(cx, |main, cx| {
                                        main.security_input =
                                            cx.new(|cx| InputState::new(window, cx).masked(true));
                                        main.security_restore = false;
                                        main.security_device = None;
                                        main.security_reset_confirmation = false;
                                        cx.notify();
                                    });
                                }
                                this.section = item;
                                cx.notify();
                            }
                        }))
                }));
        let (sidebar_bar, content_bar, panel) = self.main.update(cx, |main, cx| {
            // 40pt, like the main window's title bar, so the window buttons
            // (centred at 20pt) line up with the section title.
            let sidebar_bar = main.toolbar("settings-sidebar-toolbar", cx).h(px(40.));
            let content_bar = main
                .toolbar("settings-toolbar", cx)
                .h(px(40.))
                .px(px(20.))
                .border_b_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(section.label()),
                );
            let body = match section {
                Section::Security => main.security_view(cx),
                Section::Account => main.account_settings(window, cx),
                Section::Appearance => main.appearance_settings(cx),
                Section::Chat => main.chat_settings(cx),
                Section::Keyboard => keyboard_settings(cx),
                Section::Notifications => main.notification_settings(cx),
                Section::General => div().into_any_element(),
            };
            let resettable = matches!(section, Section::Appearance | Section::Chat);
            let muted = cx.theme().muted_foreground;
            let panel = v_flex()
                .id("settings-body")
                .test_support()
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px(px(20.))
                .py(px(16.))
                .gap(px(14.))
                .when(main.preview, |v| {
                    v.child(
                        div()
                            .text_size(px(11.))
                            .text_color(muted)
                            .child("Sample data · Offline Settings preview"),
                    )
                })
                .child(body)
                .child(
                    h_flex()
                        .pt(px(4.))
                        .gap(px(8.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(11.))
                                .text_color(muted)
                                .child(main.settings_status.clone()),
                        )
                        .when(resettable, |row| {
                            row.child(
                                Button::new("reset-settings-section")
                                    .small()
                                    .label("Reset to Defaults")
                                    .disabled(
                                        main.account_edit_busy
                                            || main.busy
                                            || main.security_key.is_some(),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.reset_settings(section, window, cx)
                                    })),
                            )
                        }),
                );
            (sidebar_bar, content_bar, panel)
        });
        let sidebar = v_flex()
            .w(px(200.))
            .flex_none()
            .h_full()
            .bg(cx.theme().sidebar)
            .border_r_1()
            .border_color(cx.theme().border)
            .child(sidebar_bar)
            .child(rows);
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_size(px(13.))
            .key_context("Twayblade")
            .on_action(
                cx.listener(|this, _: &crate::app::IncreaseScale, window, cx| {
                    this.main
                        .update(cx, |main, cx| main.change_scale(5, window, cx))
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::app::DecreaseScale, window, cx| {
                    this.main
                        .update(cx, |main, cx| main.change_scale(-5, window, cx))
                }),
            )
            .on_action(cx.listener(|this, _: &crate::app::ResetScale, window, cx| {
                this.main
                    .update(cx, |main, cx| main.reset_scale(window, cx))
            }))
            .on_action(cx.listener(|_, _: &OpenSettings, window, _| window.activate_window()))
            .on_action(cx.listener(
                |this, _: &crate::shortcuts::ShowKeyboardShortcuts, window, cx| {
                    if this.main.read(cx).security_key.is_none() {
                        this.section = Section::Keyboard;
                        cx.notify();
                    }
                    window.activate_window();
                },
            ))
            .child(
                h_flex().size_full().child(sidebar).child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .child(content_bar)
                        .child(panel),
                ),
            )
    }
}

fn scheme_label(scheme: Scheme) -> &'static str {
    match scheme {
        Scheme::System => "System",
        Scheme::Light => "Light",
        Scheme::Dark => "Dark",
        Scheme::Catppuccin => "Catppuccin",
        Scheme::Nord => "Nord",
        Scheme::Gruvbox => "Gruvbox",
        Scheme::Solarized => "Solarized",
        Scheme::HighContrast => "High Contrast",
        Scheme::Custom => "Custom",
    }
}

/// A rounded group of form rows separated by hairlines, as in System Settings.
fn group(rows: impl IntoIterator<Item = Div>, cx: &App) -> Div {
    let mut group = v_flex()
        .rounded(px(8.))
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().secondary.opacity(0.35));
    for (ix, row) in rows.into_iter().enumerate() {
        group = group.child(row.when(ix > 0, |row| {
            row.border_t_1().border_color(cx.theme().border)
        }));
    }
    group
}

/// Label on the left, control on the right.
fn form_row(label: &'static str, control: impl IntoElement, cx: &App) -> Div {
    h_flex()
        .min_h(px(40.))
        .px(px(12.))
        .py(px(6.))
        .gap(px(12.))
        .child(
            div()
                .flex_none()
                .w(px(150.))
                .text_size(px(13.))
                .text_color(cx.theme().foreground)
                .child(label),
        )
        .child(h_flex().flex_1().min_w_0().justify_end().child(control))
}

/// A macOS-style segmented control. Each entry is (element id, label, selected).
fn segmented<const N: usize>(
    options: [(&'static str, &'static str, bool); N],
    cx: &mut Context<MatrixApp>,
    on_pick: impl Fn(&mut MatrixApp, usize, &mut Window, &mut Context<MatrixApp>) + Clone + 'static,
) -> Div {
    let pill = crate::theme::palette(cx).pill;
    let mut control = h_flex()
        .p(px(2.))
        .gap(px(2.))
        .rounded(px(7.))
        .bg(cx.theme().secondary);
    for (index, (id, label, selected)) in options.into_iter().enumerate() {
        let on_pick = on_pick.clone();
        // Segments share the width evenly, so a stretched control never shows
        // an empty trailing area that reads as a blank option.
        control = control.child(
            Button::new(id)
                .ghost()
                .xsmall()
                .flex_1()
                .h(px(22.))
                .px(px(10.))
                .rounded(px(5.))
                .label(label)
                .selected(selected)
                .when(selected, |button| button.bg(pill).shadow_xs())
                .on_click(cx.listener(move |this, _, window, cx| on_pick(this, index, window, cx))),
        );
    }
    control
}

/// − value + with a quiet hint, for ranges too long for a segmented control.
fn stepper(
    id: &'static str,
    value: String,
    can_decrease: bool,
    can_increase: bool,
    hint: &'static str,
    cx: &mut Context<MatrixApp>,
    on_step: impl Fn(&mut MatrixApp, i16, &mut Window, &mut Context<MatrixApp>) + Clone + 'static,
) -> Div {
    let down = on_step.clone();
    h_flex()
        .gap(px(8.))
        .child(
            div()
                .text_size(px(11.))
                .text_color(cx.theme().muted_foreground)
                .child(hint),
        )
        .child(
            h_flex()
                .p(px(2.))
                .gap(px(2.))
                .rounded(px(7.))
                .bg(cx.theme().secondary)
                .child(
                    Button::new(SharedString::from(format!("{id}-down")))
                        .ghost()
                        .xsmall()
                        .h(px(22.))
                        .w(px(26.))
                        .icon(gpui_kit::assets::IconName::Minus)
                        .accessibility_label("Decrease")
                        .disabled(!can_decrease)
                        .on_click(
                            cx.listener(move |this, _, window, cx| down(this, -1, window, cx)),
                        ),
                )
                .child(
                    div()
                        .min_w(px(52.))
                        .text_center()
                        .text_size(px(12.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(value),
                )
                .child(
                    Button::new(SharedString::from(format!("{id}-up")))
                        .ghost()
                        .xsmall()
                        .h(px(22.))
                        .w(px(26.))
                        .icon(gpui_kit::assets::IconName::Plus)
                        .accessibility_label("Increase")
                        .disabled(!can_increase)
                        .on_click(
                            cx.listener(move |this, _, window, cx| on_step(this, 1, window, cx)),
                        ),
                ),
        )
}

fn accent_dot(fill: Hsla, selected: bool, system: bool, cx: &App) -> Div {
    div()
        .size(px(22.))
        .rounded(px(11.))
        .p(px(2.))
        .border_2()
        .border_color(if selected {
            cx.theme().foreground.opacity(0.6)
        } else {
            gpui_kit::transparent_black()
        })
        .child(
            div()
                .size_full()
                .rounded(px(9.))
                .bg(fill)
                // The System dot carries a white centre so it reads as "automatic".
                .when(system, |dot| {
                    dot.flex()
                        .items_center()
                        .justify_center()
                        .child(div().size(px(6.)).rounded(px(3.)).bg(gpui_kit::white()))
                }),
        )
}

/// Settings › Keyboard: every shortcut, generated from the bindings.
fn keyboard_settings(cx: &App) -> AnyElement {
    use crate::shortcuts::{Area, list};
    // Message box rows in the order people reach for them.
    const COMPOSER_ORDER: [&str; 20] = [
        "Send message",
        "New line",
        "Bold",
        "Italic",
        "Code",
        "Quote",
        "Link",
        "Edit your previous message",
        "Edit your next message",
        "Cancel edit",
        "Previous sent message",
        "Next sent message",
        "Undo",
        "Redo",
        "Move to start",
        "Move to end",
        "Cut",
        "Copy",
        "Paste",
        "Select all",
    ];
    let entries = list();
    let sections = [
        (Area::Composer, "Message box"),
        (Area::Navigation, "Navigation"),
        (Area::Window, "Window"),
        (Area::Viewer, "Image viewer"),
    ];
    let mut body = v_flex().gap(px(6.));
    for (index, (area, title)) in sections.into_iter().enumerate() {
        // One row per action; alternative keys share it.
        let mut rows: Vec<(&'static str, Vec<Vec<String>>)> = Vec::new();
        for entry in entries.iter().filter(|e| e.area == area) {
            match rows.iter_mut().find(|(label, _)| *label == entry.label) {
                Some((_, keys)) => keys.push(entry.keys.clone()),
                None => rows.push((entry.label, vec![entry.keys.clone()])),
            }
        }
        if area == Area::Composer {
            rows.sort_by_key(|(label, _)| {
                COMPOSER_ORDER
                    .iter()
                    .position(|l| l == label)
                    .unwrap_or(usize::MAX)
            });
        }
        body = body
            .child(
                div()
                    .px(px(4.))
                    .when(index > 0, |d| d.pt(px(12.)))
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(group(
                rows.into_iter()
                    .map(|(label, chords)| shortcut_row(label, chords, cx)),
                cx,
            ));
    }
    body.child(
        div()
            .px(px(4.))
            .pt(px(8.))
            .text_size(px(11.))
            .text_color(cx.theme().muted_foreground)
            .child("Shortcuts can't be changed yet."),
    )
    .into_any_element()
}

fn shortcut_row(label: &'static str, chords: Vec<Vec<String>>, cx: &App) -> Div {
    let muted = cx.theme().muted_foreground;
    let mut keys = h_flex().flex_none().gap(px(6.)).items_center();
    for (index, chord) in chords.into_iter().enumerate() {
        if index > 0 {
            keys = keys.child(div().text_size(px(11.)).text_color(muted).child("or"));
        }
        keys = keys.child(
            h_flex()
                .gap(px(3.))
                .children(chord.into_iter().map(|key| key_cap(key, cx))),
        );
    }
    h_flex()
        .min_h(px(34.))
        .px(px(12.))
        .py(px(5.))
        .gap(px(12.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(13.))
                .text_color(cx.theme().foreground)
                .child(label),
        )
        .child(keys)
}

/// One key, drawn as a key cap.
fn key_cap(key: String, cx: &App) -> Div {
    h_flex()
        .flex_none()
        .justify_center()
        .min_w(px(22.))
        .h(px(20.))
        .px(px(5.))
        .rounded(px(4.))
        .border_1()
        .border_b_2()
        .border_color(cx.theme().border)
        .bg(cx.theme().background)
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().foreground)
        .child(key)
}
