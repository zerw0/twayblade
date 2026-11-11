#![recursion_limit = "512"]
extern crate gpui_kit as gpui;

mod app;
mod attachments;
mod avatars;
mod backend;
mod emoji;
mod model;
mod motion;
mod preferences;
mod security;
mod settings;
mod spaces_ui;
#[cfg(all(test, feature = "capture"))]
mod tests;
mod theme;
mod threads;
mod viewer;
mod workspace;

use app::{
    CloseProfile, DecreaseScale, FindConversation, IncreaseScale, JoinSpace, MatrixApp,
    NewDirectMessage, NewRoom, OpenSettings, Quit, ResetScale, ToggleDetails,
};
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;

fn main() {
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
        matrix_client_platform::notifications::initialize_activation();
    }
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            Theme::sync_system_appearance(None, cx);
            if preview {
                if args.iter().any(|arg| arg == "--light") { Theme::change(ThemeMode::Light, None, cx); }
                if args.iter().any(|arg| arg == "--dark") { Theme::change(ThemeMode::Dark, None, cx); }
            }
            // Base type anchors Kit's rem scale; window constraints below are OS pixels.
            theme::apply(cx);
            cx.bind_keys([
                KeyBinding::new(if cfg!(target_os = "macos") { "cmd-," } else { "ctrl-," }, OpenSettings, None),
                KeyBinding::new(
                    if cfg!(target_os = "macos") {
                        "cmd-q"
                    } else {
                        "ctrl-q"
                    },
                    Quit,
                    None,
                ),
                KeyBinding::new(
                    if cfg!(target_os = "macos") {
                        "cmd-f"
                    } else {
                        "ctrl-f"
                    },
                    FindConversation,
                    Some("MatrixClient"),
                ),
                KeyBinding::new("escape", CloseProfile, Some("MatrixClient")),
                KeyBinding::new(if cfg!(target_os = "macos") { "cmd-k" } else { "ctrl-k" }, FindConversation, Some("MatrixClient")),
                KeyBinding::new(if cfg!(target_os = "macos") { "ctrl-cmd-s" } else { "ctrl-alt-s" }, app::ToggleSidebar, Some("MatrixClient")),
                KeyBinding::new(if cfg!(target_os = "macos") { "cmd-[" } else { "alt-left" }, app::NavigateBack, Some("MatrixClient")),
                KeyBinding::new(if cfg!(target_os = "macos") { "cmd-]" } else { "alt-right" }, app::NavigateForward, Some("MatrixClient")),
                KeyBinding::new(if cfg!(target_os = "macos") { "alt-cmd-i" } else { "ctrl-alt-i" }, ToggleDetails, Some("MatrixClient")),
                KeyBinding::new(if cfg!(target_os = "macos") { "cmd-n" } else { "ctrl-n" }, NewDirectMessage, Some("MatrixClient")),
                KeyBinding::new(if cfg!(target_os = "macos") { "cmd-shift-n" } else { "ctrl-shift-n" }, NewRoom, Some("MatrixClient")),
                KeyBinding::new(
                    if cfg!(target_os = "macos") {
                        "cmd-i"
                    } else {
                        "ctrl-i"
                    },
                    ToggleDetails,
                    Some("MatrixClient"),
                ),
            ]);
            cx.bind_keys([
                KeyBinding::new(if cfg!(target_os = "macos") { "cmd-+" } else { "ctrl-+" }, IncreaseScale, Some("MatrixClient")),
                KeyBinding::new(if cfg!(target_os = "macos") { "cmd-=" } else { "ctrl-=" }, IncreaseScale, Some("MatrixClient")),
                KeyBinding::new(if cfg!(target_os = "macos") { "cmd--" } else { "ctrl--" }, DecreaseScale, Some("MatrixClient")),
                KeyBinding::new(if cfg!(target_os = "macos") { "cmd-0" } else { "ctrl-0" }, ResetScale, Some("MatrixClient")),
            ]);
            cx.bind_keys(viewer::bindings());
            cx.on_action(|_: &Quit, cx| cx.quit());
            use gpui_kit::component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
            cx.set_menus([
                Menu {
                    name: "Matrix Client".into(),
                    items: if cfg!(target_os = "macos") { vec![MenuItem::action("Settings…", OpenSettings), MenuItem::Separator, MenuItem::action("Quit Matrix Client", Quit)] } else { vec![MenuItem::action("Quit Matrix Client", Quit)] },
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
                Menu { name: "Image".into(), items: vec![MenuItem::action("Previous Image",viewer::PreviousImage),MenuItem::action("Next Image",viewer::NextImage),MenuItem::Separator,MenuItem::action("Zoom In",viewer::ZoomIn),MenuItem::action("Zoom Out",viewer::ZoomOut),MenuItem::action("Fit to Window",viewer::FitImage),MenuItem::Separator,MenuItem::action("Save Image…",viewer::SaveImage),MenuItem::action("Copy Image",viewer::CopyImage),MenuItem::action("Open in Default App…",viewer::OpenImage),MenuItem::action("Show in Timeline",viewer::RevealImage),MenuItem::action("Close Image",viewer::CloseViewer)], disabled: false },
                Menu {
                    name: "View".into(),
                    items: vec![MenuItem::action("Increase Interface Scale", IncreaseScale), MenuItem::action("Decrease Interface Scale", DecreaseScale), MenuItem::action("Actual Interface Size", ResetScale), MenuItem::Separator, MenuItem::action("Room Details", ToggleDetails), MenuItem::action("Close Panel", CloseProfile)],
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
                window_bounds: Some(WindowBounds::centered(size(px(1120.), px(760.)), cx)),
                window_min_size: Some(size(px(820.), px(540.))),
                // macOS: unified toolbar. The sidebar runs under the traffic lights and
                // our toolbar rows move the window. Linux/Windows keep standard
                // decorations, which supply their own window controls.
                titlebar: Some(TitlebarOptions {
                    title: Some("Matrix Client".into()),
                    appears_transparent: cfg!(target_os = "macos"),
                    traffic_light_position: cfg!(target_os = "macos")
                        .then(|| point(px(16.), px(14.))),
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
                    if args.iter().any(|arg| arg == "--preview-signin") {
                        view.snapshot = backend::Snapshot::default();
                        view.groups.clear();
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
                        view.options = Some(backend::LoginOptions { homeserver: "matrix.example.org".into(), password: false, browser: false, sso: true, providers: vec![matrix_client_core::IdentityProvider { id: "sample".into(), name: "Example Identity".into(), icon_png: None }] });
                        view.status = "Sample data · Choose your identity provider.".into();
                    }
                    if args.iter().any(|arg| arg == "--preview-signed-out") {
                        view.snapshot.session_state = matrix_client_core::auth::SessionState::HardLoggedOut;
                    }
                    if args.iter().any(|arg| arg == "--preview-soft-logout") {
                        view.snapshot.session_state = matrix_client_core::auth::SessionState::SoftLoggedOut;
                    }
                    if args.iter().any(|arg| arg == "--preview-verify" || arg == "--preview-recovery") {
                        view.security_open = true;
                        view.snapshot.security.ready = true;
                        view.snapshot.security.has_identity = true;
                        view.snapshot.security.backup_exists = true;
                        if args.iter().any(|arg| arg == "--preview-verify") {
                            view.snapshot.security.verification = Some(matrix_client_core::security::VerificationView { flow: "sample".into(), device: "Sample second device".into(), incoming: false, phase: matrix_client_core::security::VerificationPhase::Emojis([("🐶", "Dog"), ("🌳", "Tree"), ("🚀", "Rocket"), ("🎸", "Guitar"), ("🍎", "Apple"), ("🔔", "Bell"), ("🦋", "Butterfly")].into_iter().map(|(symbol, name)| (symbol.into(), name.into())).collect()) });
                        } else { view.security_restore = true; }
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
                    if args.iter().any(|arg| arg == "--preview-attachments") {
                        view.status = "Sample data · Attachment preview".into();
                        view.attachment_sample = true;
                        for message in &mut view.snapshot.messages { if message.kind == backend::MessageKind::Image && let Some(attachment) = &mut message.attachment { attachment.preview_png = Some(std::sync::Arc::new(include_bytes!("../assets/attachment-sample.png").to_vec())); } }
                        if let Some(message) = view.snapshot.messages.last_mut() {
                            message.kind = backend::MessageKind::File;
                            message.body.clear();
                            message.attachment = Some(matrix_client_core::conversation::Attachment { filename: "Travel notes.pdf".into(), size: Some(234567), preview_png: None });
                            message.delivery = backend::DeliveryState::Queued;
                            message.upload_percent = Some(42);
                        }
                    }
                    if args.iter().any(|arg| arg == "--preview-viewer") { view.preview_viewer(window,cx); }
                    if args.iter().any(|arg| arg == "--preview-directory") { view.open_management(spaces_ui::ManageKind::Directory("design".into()),window,cx); }
                    if args.iter().any(|arg| arg == "--preview-invites") { view.status = "Sample data · Invitations preview".into(); view.snapshot.invites = vec![matrix_client_core::space_actions::Invite { id: "sample-room".into(), name: "Planning".into(), ..Default::default() },matrix_client_core::space_actions::Invite { id: "sample-dm".into(), name: "Sam".into(), direct: true, ..Default::default() },matrix_client_core::space_actions::Invite { id: "sample-space".into(), name: "Product team".into(), space: true, ..Default::default() }]; }
                    if args.iter().any(|arg| arg == "--preview-notification") { view.notification_sample = true;
                        if args.iter().any(|arg| arg == "--preview-no-message-preview") { view.preferences.notifications.preview = false; } }
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
                view.update(cx, |view, cx| view.open_settings(args.iter().position(|arg| arg == "--preview-settings").and_then(|ix| args.get(ix + 1)).and_then(|name| settings::Section::ALL.into_iter().find(|section| section.label().eq_ignore_ascii_case(name))).unwrap_or_default(), cx));
            }
            let accent_view = view.downgrade();
            cx.spawn(async move |cx| {
                loop {
                    let accent = cx.background_spawn(async { matrix_client_platform::system_accent() }).await;
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
                cx.spawn(async move |cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(3))
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
