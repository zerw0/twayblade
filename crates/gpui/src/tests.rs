use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Bounds, TestAppContext, WindowBounds, WindowOptions, px, size};

use crate::app::MatrixApp;

#[gpui_kit::test]
fn navigation_preserves_typed_drafts_through_real_controls(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (window, app) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1120.), px(760.)),
                    cx,
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| MatrixApp::new(true, false, false, window, cx)),
        )
        .expect("test window")
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("home", cx);
        window.click("room-notes", cx);
        window.within("composer-panel").click("input-state", cx);
        window.input("Draft with emoji 🦀 and accents José", cx);
        window.click("space-design", cx);
        window.click("room-general", cx);
        assert_eq!(
            window
                .within("composer-panel")
                .find_by_label("Message")
                .value(),
            Some("")
        );
        window.click("home", cx);
        window.click("room-notes", cx);
        assert_eq!(
            window
                .within("composer-panel")
                .find_by_label("Message")
                .value(),
            Some("Draft with emoji 🦀 and accents José")
        );
    })
    .expect("test controls");
    cx.update(|cx| {
        let app = app.read(cx);
        assert!(app.navigation.selected_space.is_none());
        assert_eq!(app.navigation.selected_room.as_deref(), Some("notes"));
        assert_eq!(
            app.navigation.drafts.get("notes").map(String::as_str),
            Some("Draft with emoji 🦀 and accents José")
        );
    });
}

#[gpui_kit::test]
fn own_profile_opens_and_closes_through_real_buttons(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (window, app) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1120.), px(760.)),
                    cx,
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| MatrixApp::new(true, false, false, window, cx)),
        )
        .expect("test window")
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("my-profile", cx);
        assert!(window.find("copy-profile-id").visible());
    })
    .expect("open profile");
    cx.update(|cx| {
        assert_eq!(
            app.read(cx).profile.as_ref().map(|page| page.id.as_str()),
            Some("@you:preview.invalid")
        )
    });
    cx.update_window(window, |_, window, cx| window.click("close-profile", cx))
        .expect("close profile");
    cx.update(|cx| assert!(app.read(cx).profile.is_none()));
}

fn preview_window(
    cx: &mut TestAppContext,
) -> (gpui_kit::AnyWindowHandle, gpui_kit::Entity<MatrixApp>) {
    cx.update(gpui_kit::init);
    cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1120.), px(760.)),
                    cx,
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| MatrixApp::new(true, false, false, window, cx)),
        )
        .expect("test window")
    })
}

#[gpui_kit::test]
fn details_panel_toggles_and_yields_to_profile(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("details", cx);
        assert!(window.find("copy-room-id").visible());
        window.click("my-profile", cx);
        assert!(window.find("copy-profile-id").visible());
    })
    .expect("open details then profile");
    cx.update(|cx| {
        let app = app.read(cx);
        assert!(app.profile.is_some() && app.details_open);
    });
    // Escape closes the profile first, revealing details, then details.
    cx.update(|cx| app.update(cx, |app, cx| app.dismiss_panel(cx)));
    cx.update(|cx| assert!(app.read(cx).profile.is_none() && app.read(cx).details_open));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("close-details", cx);
    })
    .expect("close details");
    cx.update(|cx| assert!(!app.read(cx).details_open));
}

#[gpui_kit::test]
fn sign_in_offers_browser_and_password_then_changes_server(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.snapshot = crate::backend::Snapshot::default();
            app.options = Some(crate::backend::LoginOptions {
                homeserver: "https://matrix-client.example.org/".into(),
                password: true,
                browser: true,
            });
            cx.notify();
        })
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("browser-sign-in").visible());
        assert!(window.find("sign-in").visible());
        window.click("change-server", cx);
    })
    .expect("sign-in controls");
    cx.update(|cx| assert!(app.read(cx).options.is_none()));
}
