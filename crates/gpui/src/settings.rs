//! One separate Settings window backed by the retained main view.
use crate::theme::scaled_px as px;
use crate::{
    app::{MatrixApp, OpenSettings},
    preferences::{ChatStyle, Density, Preferences, Scheme, Timestamps, Variant},
};
use gpui_kit::{
    component::{
        button::{Button, ButtonVariants},
        input::{Input, InputState},
        *,
    },
    prelude::FluentBuilder,
    *,
};
use matrix_client_core::profile::ProfileUpdate;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Section {
    General,
    Notifications,
    #[default]
    Appearance,
    Chat,
    Security,
    Account,
}
impl Section {
    pub const ALL: [Self; 6] = [
        Self::General,
        Self::Notifications,
        Self::Appearance,
        Self::Chat,
        Self::Security,
        Self::Account,
    ];
    /// Sections with working settings. General and Notifications return when
    /// they have real controls (notifications are M4 task 5).
    pub const VISIBLE: [Self; 4] = [Self::Appearance, Self::Chat, Self::Security, Self::Account];
    fn icon(self) -> gpui_kit::assets::IconName {
        use gpui_kit::assets::IconName;
        match self {
            Self::General => IconName::Settings,
            Self::Notifications => IconName::Bell,
            Self::Appearance => IconName::Palette,
            Self::Chat => IconName::MessagesSquare,
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
            Self::Security => "Security",
            Self::Account => "Account",
        }
    }
}
pub(super) struct SettingsWindow {
    pub section: Section,
    main: Entity<MatrixApp>,
    _observe: Subscription,
}
impl MatrixApp {
    pub(super) fn open_settings(&mut self, section: Section, cx: &mut Context<Self>) {
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
                        .then(|| point(px(18.), px(18.))),
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
    pub(super) fn reset_settings(
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
    pub(super) fn save_preferences(&mut self, window: &Window, cx: &Context<Self>) {
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

    pub(super) fn change_scale(&mut self, delta: i16, window: &mut Window, cx: &mut Context<Self>) {
        self.preferences.appearance.scale =
            (self.preferences.appearance.scale as i16 + delta).clamp(85, 150) as u16;
        self.appearance_changed(window, cx);
    }
    pub(super) fn reset_scale(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.preferences.appearance.scale = 100;
        self.appearance_changed(window, cx);
    }
    pub(super) fn appearance_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            let [background, sidebar, text] = crate::theme::scheme_preview(scheme, preview_dark);
            let label = scheme_label(scheme);
            tiles = tiles.child(
                Button::new(SharedString::from(format!("scheme-{}", scheme.name())))
                    .ghost()
                    .p_0()
                    .h_auto()
                    .accessibility_label(label)
                    .child(
                        v_flex()
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
            Scheme::System | Scheme::Light | Scheme::Dark
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
        v_flex()
            .gap(px(18.))
            .child(group(
                // The thumbnails need the full width, so the label sits above them.
                std::iter::once(
                    v_flex()
                        .px(px(12.))
                        .py(px(10.))
                        .gap(px(10.))
                        .child(div().text_size(px(13.)).child("Appearance"))
                        .child(tiles),
                )
                // Only curated palettes have light/dark variants to choose from.
                .chain(has_variants.then(|| form_row("Palette variant", variant, cx)))
                .chain(std::iter::once(form_row("Accent colour", accents, cx)))
                .collect::<Vec<_>>(),
                cx,
            ))
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
                ],
                cx,
            ))
            .child(group(fonts, cx))
            .into_any_element()
    }

    pub(super) fn chat_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                    form_row("Density", density, cx),
                    form_row("Timestamps", timestamps, cx),
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
            let sidebar_bar = main.toolbar("settings-sidebar-toolbar", cx);
            let content_bar = main
                .toolbar("settings-toolbar", cx)
                .px(px(20.))
                .border_b_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::BOLD)
                        .child(section.label()),
                );
            let body = match section {
                Section::Security => main.security_view(cx),
                Section::Account => main.account_settings(window, cx),
                Section::Appearance => main.appearance_settings(cx),
                Section::Chat => main.chat_settings(cx),
                Section::General | Section::Notifications => div().into_any_element(),
            };
            let resettable = matches!(section, Section::Appearance | Section::Chat);
            let muted = cx.theme().muted_foreground;
            let panel = v_flex()
                .id("settings-body")
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
            .key_context("MatrixClient")
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
        Scheme::HighContrast => "High Contrast",
        Scheme::Solarized => "Solarized",
        Scheme::Gruvbox => "Gruvbox",
        Scheme::Nord => "Nord",
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
    let mut control = h_flex()
        .p(px(2.))
        .gap(px(2.))
        .rounded(px(7.))
        .bg(cx.theme().secondary);
    for (index, (id, label, selected)) in options.into_iter().enumerate() {
        let on_pick = on_pick.clone();
        let button = Button::new(id)
            .xsmall()
            .h(px(22.))
            .px(px(10.))
            .rounded(px(5.))
            .label(label)
            .on_click(cx.listener(move |this, _, window, cx| on_pick(this, index, window, cx)));
        control = control.child(if selected {
            button.primary()
        } else {
            button.ghost()
        });
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
