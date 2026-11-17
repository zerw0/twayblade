#![recursion_limit = "512"]
// Release builds start without a console window on Windows; diagnostics go to
// the log file. Debug builds keep the console for development output.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
extern crate gpui_kit as gpui;

mod app;
mod backend;
mod composer_format;
mod model;
mod preferences;
mod shortcuts;
#[cfg(all(test, feature = "capture"))]
mod tests;
mod theme;
mod ui;

use app::{
    CloseProfile, DecreaseScale, FindConversation, IncreaseScale, JoinSpace, MatrixApp,
    NewDirectMessage, NewRoom, OpenSettings, Quit, ResetScale, ToggleDetails,
};
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;

/// The reverse-DNS ID shared by the desktop entry, AppStream data and icons.
pub(crate) const APP_ID: &str = "com.twayblade.desktop";

fn main() {
    if twayblade_platform::diagnostics::init().is_err() {
        eprintln!("Diagnostics could not start; logging is disabled.");
    }
    let args: Vec<_> = std::env::args().collect();
    let preview = args.iter().any(|arg| arg.starts_with("--preview"));
    let capture = args
        .iter()
        .position(|arg| arg == "--capture")
        .and_then(|ix| args.get(ix + 1))
        .cloned();
    if capture.is_some() && !preview {
        eprintln!("Capture requires an offline preview.");
        return;
    }
    #[cfg(not(feature = "capture"))]
    if capture.is_some() {
        eprintln!("Build with --features capture to capture an offline preview.");
        return;
    }
    if !preview {
        twayblade_platform::notifications::initialize_activation();
    }
    // Before anything reads preferences or takes the instance lock.
    if !preview {
        twayblade_core::auth::migrate_legacy_storage();
    }
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            // Kit just made GPUI the notification delegate; take clicks back.
            if !preview {
                twayblade_platform::notifications::initialize_activation();
            }
            Theme::sync_system_appearance(None, cx);
            if preview {
                if args.iter().any(|arg| arg == "--light") { Theme::change(ThemeMode::Light, None, cx); }
                if args.iter().any(|arg| arg == "--dark") { Theme::change(ThemeMode::Dark, None, cx); }
            }
            // Base type anchors Kit's rem scale; window constraints below are OS pixels.
            theme::apply(cx);
            cx.bind_keys(shortcuts::bindings());
            cx.on_action(|_: &Quit, cx| cx.quit());
            use gpui_kit::component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
            cx.set_menus([
                Menu {
                    name: "Twayblade".into(),
                    items: if cfg!(target_os = "macos") { vec![MenuItem::action("Settings…", OpenSettings), MenuItem::Separator, MenuItem::action("Quit Twayblade", Quit)] } else { vec![MenuItem::action("Quit Twayblade", Quit)] },
                    disabled: false,
                },
                Menu { name: "File".into(), items: vec![MenuItem::action("New Direct Message…", NewDirectMessage),MenuItem::action("Create Room…", NewRoom),MenuItem::action("Join Space…", JoinSpace)].into_iter().chain((!cfg!(target_os = "macos")).then(|| MenuItem::action("Settings…", OpenSettings))).collect(), disabled: false },
                Menu {
                    name: "Edit".into(),
                    items: vec![
                        MenuItem::action("Undo", Undo),
                        MenuItem::action("Redo", Redo),
                        MenuItem::Separator,
                        MenuItem::action("Cut", Cut),
                        MenuItem::action("Copy", Copy),
                        MenuItem::action("Paste", Paste),
                        MenuItem::action("Select All", SelectAll),
                        MenuItem::Separator,
                        MenuItem::action("Find Conversation", FindConversation),
                    ],
                    disabled: false,
                },
                Menu { name: "Image".into(), items: vec![MenuItem::action("Previous Image",ui::viewer::PreviousImage),MenuItem::action("Next Image",ui::viewer::NextImage),MenuItem::Separator,MenuItem::action("Zoom In",ui::viewer::ZoomIn),MenuItem::action("Zoom Out",ui::viewer::ZoomOut),MenuItem::action("Fit to Window",ui::viewer::FitImage),MenuItem::Separator,MenuItem::action("Save Image…",ui::viewer::SaveImage),MenuItem::action("Copy Image",ui::viewer::CopyImage),MenuItem::action("Open in Default App…",ui::viewer::OpenImage),MenuItem::action("Show in Timeline",ui::viewer::RevealImage),MenuItem::action("Close Image",ui::viewer::CloseViewer)], disabled: false },
                Menu {
                    name: "View".into(),
                    items: vec![MenuItem::action("Increase Interface Scale", IncreaseScale), MenuItem::action("Decrease Interface Scale", DecreaseScale), MenuItem::action("Actual Interface Size", ResetScale), MenuItem::Separator, MenuItem::action("Room Details", ToggleDetails), MenuItem::action("Close Panel", CloseProfile)],
                    disabled: false,
                },
                Menu {
                    name: "Call".into(),
                    items: vec![
                        MenuItem::action("Mute or Unmute", shortcuts::ToggleMute),
                        MenuItem::action("Turn Camera On or Off", shortcuts::ToggleCamera),
                        MenuItem::action("Share Screen or Stop Sharing", shortcuts::ToggleScreenShare),
                        MenuItem::Separator,
                        MenuItem::action("Answer", shortcuts::AnswerCall),
                        MenuItem::action("Decline", shortcuts::DeclineCall),
                        MenuItem::Separator,
                        MenuItem::action("Leave Call", shortcuts::LeaveCall),
                    ],
                    disabled: false,
                },
                Menu {
                    name: "Help".into(),
                    items: vec![MenuItem::action("Keyboard Shortcuts", shortcuts::ShowKeyboardShortcuts)],
                    disabled: false,
                },
            ]);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let options = WindowOptions {
                // Linux desktops match the window to its desktop entry and icon by this ID.
                app_id: Some(APP_ID.into()),
                window_bounds: Some(WindowBounds::centered(size(px(1120.), px(760.)), cx)),
                window_min_size: Some(size(px(820.), px(540.))),
                // macOS: unified toolbar. The sidebar runs under the traffic lights and
                // our toolbar rows move the window. Linux/Windows keep standard
                // decorations, which supply their own window controls.
                titlebar: Some(TitlebarOptions {
                    title: Some("Twayblade".into()),
                    appears_transparent: cfg!(target_os = "macos"),
                    traffic_light_position: cfg!(target_os = "macos")
                        .then(|| point(px(16.), px(9.))),
                }),
                app_owns_titlebar_drag: cfg!(target_os = "macos"),
                ..Default::default()
            };
            let home = args.iter().any(|arg| arg == "--preview-home");
            let large = args.iter().any(|arg| arg == "--preview-large");
            let profile = args.iter().any(|arg| arg == "--preview-profile");
            let (main_window, view) = gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| {
                    let mut view = MatrixApp::new(preview, home, large, window, cx);
                    if let Some(scale) = args.iter().position(|arg| arg == "--preview-scale").and_then(|ix| args.get(ix + 1)).and_then(|value| value.parse::<u16>().ok()) { view.preferences.appearance.scale = (scale.clamp(85,150) / 5) * 5; theme::set_appearance(view.preferences.appearance.clone(), cx); }
                    if let Some(name) = args.iter().position(|arg| arg == "--preview-scheme").and_then(|ix| args.get(ix + 1)) && let Some(scheme) = preferences::Scheme::ALL.into_iter().find(|scheme| scheme.name() == name) { view.preferences.appearance.scheme = scheme; theme::set_appearance(view.preferences.appearance.clone(), cx); }
                    if args.iter().any(|arg| arg == "--preview-style-irc") { view.preferences.chat.style = preferences::ChatStyle::Irc; }
                    if args.iter().any(|arg| arg == "--preview-style-minimal") { view.preferences.chat.style = preferences::ChatStyle::Minimal; }
                    if args.iter().any(|arg| arg == "--preview-timestamps") { view.preferences.chat.timestamps = preferences::Timestamps::Always; }
                    if args.iter().any(|arg| arg == "--preview-signin") {
                        view.snapshot = backend::Snapshot::default();
                        view.groups.clear();
                        view.status = "Offline sign-in preview".into();
                    }
                    if args.iter().any(|arg| arg == "--preview-servers") {
                        view.snapshot = backend::Snapshot::default();
                        view.groups.clear();
                        view.choosing_server = true;
                        view.status = "Offline sign-in preview".into();
                    }
                    if args.iter().any(|arg| arg == "--preview-oauth-wait") {
                        view.snapshot = backend::Snapshot::default();
                        view.groups.clear();
                        view.options = Some(backend::LoginOptions { homeserver: "matrix.example.org".into(), password: false, browser: true, sso: false, providers: Vec::new() });
                        view.busy = true;
                        view.oauth_cancel = Some(tokio::sync::watch::channel(false).0);
                        view.status = "Sample data · Complete sign-in in your browser. This window will wait for up to five minutes.".into();
                    }
                    if args.iter().any(|arg| arg == "--preview-sso") {
                        view.snapshot = backend::Snapshot::default();
                        view.groups.clear();
                        view.options = Some(backend::LoginOptions { homeserver: "matrix.example.org".into(), password: false, browser: false, sso: true, providers: vec![twayblade_core::IdentityProvider { id: "sample".into(), name: "Example Identity".into(), icon_png: None }] });
                        view.status = "Sample data · Choose your identity provider.".into();
                    }
                    if args.iter().any(|arg| arg == "--preview-update-download" || arg == "--preview-update-notice") {
                        // Sample data: an available release with this Mac's installer downloaded and checked.
                        view.snapshot.updater = twayblade_platform::updates::Snapshot { phase: twayblade_platform::updates::Phase::Available, available: Some(twayblade_platform::updates::Available { version: "0.2.0".into(), prerelease: true, changelog: "Sample release notes.".into(), release_url: "https://github.com/zerw0/twayblade/releases/tag/v0.2.0".into(), upgrade_command: None, download: Some(twayblade_platform::updates::Download { file: "Twayblade-0.2.0-arm64.dmg".into(), size: 1, url: String::new(), sums_url: String::new() }) }), ..Default::default() };
                        view.snapshot.update_download = backend::UpdateDownload::Ready { version: "0.2.0".into(), path: "Twayblade-0.2.0-arm64.dmg".into() };
                    }
                    if args.iter().any(|arg| arg == "--preview-selection") {
                        view.composer.update(cx, |state, cx| {
                            state.set_value("Let's ship the release notes today", window, cx);
                            state.set_selected_range(10..23, cx);
                            state.focus(window, cx);
                        });
                    }
                    if args.iter().any(|arg| arg.starts_with("--preview-add-account") || arg == "--preview-sign-out" || arg == "--preview-accounts-menu") {
                        let me = view.snapshot.user_id.clone();
                        view.snapshot.accounts = vec![
                            backend::AccountEntry { id: "sample-1".into(), user_id: me, homeserver: "https://matrix.example.org/".into(), active: true, ..Default::default() },
                            backend::AccountEntry { id: "sample-2".into(), user_id: "@sam.work:example.com".into(), homeserver: "https://example.com/".into(), active: false , instance: 2, unread: 4, mentions: 1 },
                        ];
                    }
                    if args.iter().any(|arg| arg == "--preview-add-account") {
                        view.adding_account = true;
                        view.choosing_server = true;
                    }
                    if args.iter().any(|arg| arg == "--preview-add-account-options") {
                        view.adding_account = true;
                        view.options = Some(backend::LoginOptions { homeserver: "matrix.example.org".into(), password: true, browser: true, sso: false, providers: Vec::new() });
                    }
                    if args.iter().any(|arg| arg == "--preview-sign-out") { view.open_management(ui::spaces::ManageKind::SignOut, window, cx); }
                    if args.iter().any(|arg| arg == "--preview-subspaces") {
                        let root = &mut view.snapshot.spaces[0];
                        root.child_ids.remove("research");
                        root.child_ids.remove("critique");
                        root.child_ids.insert("projects".into());
                        root.room_ids.extend(["projects".into(), "planning".into()]);
                        view.snapshot.spaces.extend([
                            backend::Space { id: "projects".into(), name: "Projects".into(), level: 1,
                                child_ids: ["research", "planning"].into_iter().map(str::to_owned).collect(),
                                room_ids: ["research", "planning", "critique"].into_iter().map(str::to_owned).collect(), ..Default::default() },
                            backend::Space { id: "planning".into(), name: "Planning".into(), level: 1,
                                child_ids: ["critique".into()].into_iter().collect(), room_ids: ["critique".into()].into_iter().collect(), ..Default::default() },
                        ]);
                        view.expanded_subspaces.insert("projects".into());
                    }
                    // Sample latest messages for the sidebar's preview lines.
                    {
                        let now = chrono::Utc::now().timestamp_millis() as u64;
                        let samples = [("Robin", "Standup moved to 10:30"), ("Alex", "Can you check the new mockups?"), ("Sam", ""), ("You", "Pushed the release notes draft"), ("Robin", "Thanks, looks good"), ("Alex", "On my way")];
                        for (room, (sender, text)) in view.snapshot.rooms.iter().zip(samples.iter().cycle()) {
                            let body = if text.is_empty() { twayblade_core::activity::PreviewBody::Image } else { twayblade_core::activity::PreviewBody::Text((*text).into()) };
                            view.snapshot.activity_previews.insert(room.id.clone(), twayblade_core::activity::RoomPreview { room: room.id.clone(), sender: format!("@{}:example.org", sender.to_lowercase()), sender_name: (*sender).into(), own: *sender == "You", body, timestamp_ms: now });
                        }
                    }
                    if args.iter().any(|arg| arg == "--preview-activity") {
                        view.snapshot.invites = vec![twayblade_core::space_actions::Invite { id: "sample-room".into(), name: "Planning".into(), ..Default::default() }];
                        for (room, mentions) in view.snapshot.rooms.iter_mut().zip([2u64, 0, 0, 1]) { room.mentions = mentions; room.unread = room.unread.max(mentions).max(1); }
                        view.notifications_open = true;
                        // Sample data only: a missed call and latest-message previews.
                        let now = chrono::Utc::now().timestamp_millis() as u64;
                        view.snapshot.missed_calls = vec![backend::MissedCall { room: "alex".into(), room_name: "Alex".into(), caller: "@alex:example.org".into(), caller_name: "Alex".into(), video: true, at: now - 12 * 60_000 }];
                        let samples = [("Alex", "Can you check the new mockups before standup?"), ("Sam", "Pushed the release notes draft"), ("Alex", ""), ("Robin", "Thanks, looks good")];
                        for (room, (sender, text)) in view.snapshot.rooms.iter().zip(samples) {
                            let body = if text.is_empty() { twayblade_core::activity::PreviewBody::Image } else { twayblade_core::activity::PreviewBody::Text(text.into()) };
                            view.snapshot.activity_previews.insert(room.id.clone(), twayblade_core::activity::RoomPreview { room: room.id.clone(), sender: format!("@{}:example.org", sender.to_lowercase()), sender_name: sender.into(), own: false, body, timestamp_ms: now });
                        }
                    }
                    if args.iter().any(|arg| arg == "--preview-signed-out") {
                        view.snapshot.session_state = twayblade_core::auth::SessionState::HardLoggedOut;
                    }
                    if args.iter().any(|arg| arg == "--preview-soft-logout") {
                        view.snapshot.session_state = twayblade_core::auth::SessionState::SoftLoggedOut;
                    }
                    if args.iter().any(|arg| arg == "--preview-verify" || arg == "--preview-recovery") {
                        view.security_open = true;
                        view.snapshot.security.ready = true;
                        view.snapshot.security.has_identity = true;
                        view.snapshot.security.backup_exists = true;
                        if args.iter().any(|arg| arg == "--preview-verify") {
                            view.snapshot.security.verification = Some(twayblade_core::security::VerificationView { flow: "sample".into(), device: "Sample second device".into(), incoming: false, phase: twayblade_core::security::VerificationPhase::Emojis([("🐶", "Dog"), ("🌳", "Tree"), ("🚀", "Rocket"), ("🎸", "Guitar"), ("🍎", "Apple"), ("🔔", "Bell"), ("🦋", "Butterfly")].into_iter().map(|(symbol, name)| (symbol.into(), name.into())).collect()) });
                        } else { view.security_restore = true; }
                    }
                    if args.iter().any(|arg| arg == "--preview-offline") {
                        view.preview_offline();
                    }
                    if args.iter().any(|arg| arg == "--preview-search") {
                        view.search_open = true;
                    }
                    if args.iter().any(|arg| arg == "--preview-files") {
                        view.details_open = true;
                        view.info_tab = app::InfoTab::Files;
                    }
                    if args.iter().any(|arg| arg == "--preview-info") {
                        view.details_open = true;
                    }
                    if args.iter().any(|arg| arg == "--preview-members") {
                        view.details_open = true;
                        view.info_tab = app::InfoTab::Members;
                    }
                    if args.iter().any(|arg| arg == "--preview-emoji") {
                        view.status = "Sample data · Emoji picker".into();
                        view.open_emoji(None, window, cx);
                    }
                    if args.iter().any(|arg| arg == "--preview-voice") {
                        view.status = "Sample data · Voice message".into();
                        if let Some(message) = view.snapshot.messages.iter_mut().rev().nth(1) {
                            message.kind = backend::MessageKind::Voice;
                            message.rich = None;
                            message.attachment = None;
                            message.voice = Some(twayblade_core::conversation::Voice {
                                duration_ms: 23_000,
                                waveform: (0..100).map(|n| (n as f32 * 0.37).sin().abs() * 0.8 + 0.1).collect(),
                            });
                        }
                    }
                    if args.iter().any(|arg| arg == "--preview-stickers") {
                        view.open_sticker_picker(window, cx);
                    }
                    if args.iter().any(|arg| arg == "--preview-create-poll") {
                        view.open_create_poll(window, cx);
                    }
                    if args.iter().any(|arg| arg == "--preview-poll") {
                        view.status = "Sample data · Polls".into();
                        let own = view.snapshot.user_id.clone();
                        ui::polls::sample_polls(&mut view.snapshot.messages, &own);
                    }
                    if args.iter().any(|arg| arg == "--preview-attachments") {
                        view.status = "Sample data · Attachment preview".into();
                        view.attachment_sample = true;
                        for message in &mut view.snapshot.messages { if message.kind == backend::MessageKind::Image && let Some(attachment) = &mut message.attachment { attachment.preview_png = Some(std::sync::Arc::new(include_bytes!("../assets/attachment-sample.png").to_vec())); } }
                        if let Some(message) = view.snapshot.messages.last_mut() {
                            message.kind = backend::MessageKind::File;
                            message.body.clear();
                            message.attachment = Some(twayblade_core::conversation::Attachment { filename: "Travel notes.pdf".into(), size: Some(234567), preview_png: None, download_needed: false });
                            message.delivery = backend::DeliveryState::Queued;
                            message.upload_percent = Some(42);
                        }
                    }
                    if args.iter().any(|arg| arg == "--preview-viewer") { view.preview_viewer(window,cx); }
                    if args.iter().any(|arg| arg == "--preview-leave") { view.open_management(ui::spaces::ManageKind::LeaveRoom("general".into()),window,cx); }
                    if args.iter().any(|arg| arg == "--preview-spotlight") { view.spotlight_open = true; }
                    if args.iter().any(|arg| arg == "--preview-mention") {
                        let text = "Thanks @";
                        view.composer.update(cx, |state, cx| state.set_value(text, window, cx));
                        view.update_mention_completion(false, text.into(), text.len());
                    }
                    if args.iter().any(|arg| arg == "--preview-explore") { view.open_explore(window,cx); }
                    if args.iter().any(|arg| arg == "--preview-create-space") { view.open_management(ui::spaces::ManageKind::CreateSpace,window,cx); }
                    if args.iter().any(|arg| arg == "--preview-dm") { view.open_management(ui::spaces::ManageKind::Dm,window,cx); }
                    if args.iter().any(|arg| arg == "--preview-invite-contact") { view.open_management(ui::spaces::ManageKind::InviteContact,window,cx); }
                    if args.iter().any(|arg| arg == "--preview-archive") { for room in view.snapshot.rooms.iter_mut().filter(|room| room.id == "announcements") { room.archived = true; } view.show_archive(window, cx); }
                    if args.iter().any(|arg| arg == "--preview-message-focus") { view.focus_next_area(true, window, cx); view.focus_next_area(true, window, cx); if let Some(id) = view.message_cursor(false).map(str::to_owned) { view.step_message(false, &id, ui::focus::Step::Previous, window, cx); } }
                    if args.iter().any(|arg| arg == "--preview-directory") { view.open_management(ui::spaces::ManageKind::Directory("design".into()),window,cx); }
                    if args.iter().any(|arg| arg == "--preview-invites") { view.status = "Sample data · Invitations preview".into(); view.snapshot.invites = vec![twayblade_core::space_actions::Invite { id: "sample-room".into(), name: "Planning".into(), ..Default::default() },twayblade_core::space_actions::Invite { id: "sample-dm".into(), name: "Sam".into(), direct: true, ..Default::default() },twayblade_core::space_actions::Invite { id: "sample-space".into(), name: "Product team".into(), space: true, ..Default::default() }]; }
                    if args.iter().any(|arg| arg == "--preview-notification") { view.notification_sample = true;
                        if args.iter().any(|arg| arg == "--preview-no-message-preview") { view.preferences.notifications.preview = false; } }
                    if let Some(flag) = args.iter().find(|arg| arg.starts_with("--preview-call") || *arg == "--preview-incoming") { view.preview_call(flag); }
                    if args.iter().any(|arg| arg == "--preview-thread") { view.open_thread("sample-0".into(), window, cx); }
                    if args.iter().any(|arg| arg == "--preview-details") {
                        view.details_open = true;
                        if let Some(room) = view.snapshot.rooms.iter_mut().find(|room| room.id == view.snapshot.selected_room) { room.encrypted = false; }
                    }
                    if profile {
                        view.open_profile(
                            "@alex:preview.invalid".into(),
                            "Alex".into(),
                            window,
                            cx,
                        );
                    }
                    view
                })
            })
            .expect("The application window could not open");
            if args.iter().any(|arg| arg == "--preview-settings") {
                view.update(cx, |view, cx| view.open_settings(args.iter().position(|arg| arg == "--preview-settings").and_then(|ix| args.get(ix + 1)).and_then(|name| ui::settings::Section::ALL.into_iter().find(|section| section.label().eq_ignore_ascii_case(name))).unwrap_or_default(), cx));
            }
            let accent_view = view.downgrade();
            cx.spawn(async move |cx| {
                loop {
                    let accent = cx.background_spawn(async { twayblade_platform::system_accent() }).await;
                    if accent_view.update(cx, |_, cx| {
                        if theme::set_system_accent(accent, cx) { theme::apply(cx); }
                    }).is_err() { break; }
                    if preview { break; }
                    cx.background_executor().timer(std::time::Duration::from_secs(2)).await;
                }
            }).detach();
            #[cfg(feature = "capture")]
            if let Some(path) = capture {
                let weak = view.downgrade();
                let open_account_menu = args.iter().any(|arg| arg == "--preview-accounts-menu");
                cx.spawn(async move |cx| {
                    if open_account_menu {
                        // Open the avatar menu the way a click would.
                        cx.background_executor()
                            .timer(std::time::Duration::from_secs(2))
                            .await;
                        let _ = main_window.update(cx, |_, window, cx| {
                            use gpui_kit::test::TestWindowExt;
                            window.click("my-profile", cx);
                        });
                    }
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(if open_account_menu { 1 } else { 3 }))
                        .await;
                    let settings = cx.update(|cx| weak.upgrade().and_then(|main| main.read(cx).settings.as_ref().map(|entry| entry.0)));
                    let result = settings.unwrap_or(main_window).update(cx, |_, window, cx| { window.refresh(); window.draw(cx).clear(cx); window.render_to_image() });
                    match result {
                        Ok(Ok(image)) => {
                            if image.save(path).is_err() {
                                eprintln!("Preview capture could not be saved.");
                            }
                        }
                        _ => eprintln!("Preview capture is unavailable on this renderer."),
                    }
                    cx.update(|cx| cx.quit());
                })
                .detach();
            }
            #[cfg(not(feature = "capture"))]
            let _ = (view, main_window);
            cx.activate(true);
        });
}
