#![recursion_limit = "512"]
extern crate gpui_kit as gpui;

mod app;
mod backend;
mod model;
#[cfg(all(test, feature = "capture"))]
mod tests;
mod theme;
mod workspace;

use app::{CloseProfile, FindConversation, MatrixApp, Quit, ToggleDetails};
use gpui_kit::component::Theme;
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
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            Theme::sync_system_appearance(None, cx);
            // Base type anchors Kit's rem scale; window constraints below are OS pixels.
            theme::apply(cx);
            cx.bind_keys([
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
            cx.on_action(|_: &Quit, cx| cx.quit());
            use gpui_kit::component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
            cx.set_menus([
                Menu {
                    name: "Matrix Client".into(),
                    items: vec![MenuItem::action("Quit Matrix Client", Quit)],
                    disabled: false,
                },
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
                Menu {
                    name: "View".into(),
                    items: vec![MenuItem::action("Room Details", ToggleDetails)],
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
                titlebar: Some(TitlebarOptions {
                    title: Some("Matrix Client".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            let home = args.iter().any(|arg| arg == "--preview-home");
            let large = args.iter().any(|arg| arg == "--preview-large");
            let profile = args.iter().any(|arg| arg == "--preview-profile");
            let (_, view) = gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| {
                    let mut view = MatrixApp::new(preview, home, large, window, cx);
                    if args.iter().any(|arg| arg == "--preview-signin") {
                        view.snapshot = backend::Snapshot::default();
                        view.groups.clear();
                        view.status = "Offline sign-in preview".into();
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
            #[cfg(feature = "capture")]
            if let Some(path) = capture {
                let weak = view.downgrade();
                cx.spawn(async move |cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(3))
                        .await;
                    let result = weak.update_in(cx, |_, window, _| window.render_to_image());
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
            let _ = view;
            cx.activate(true);
        });
}
