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
fn sign_in_only_offers_implemented_password_then_changes_server(cx: &mut TestAppContext) {
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
        assert!(window.find_all("browser-sign-in").is_empty());
        assert!(window.find("sign-in").visible());
        window.click("change-server", cx);
    })
    .expect("sign-in controls");
    cx.update(|cx| assert!(app.read(cx).options.is_none()));
}

#[gpui_kit::test]
fn message_text_keeps_readable_width_with_and_without_details(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            for message in &mut app.snapshot.messages {
                message.reply = None;
                message.reactions.clear();
                message.thread_replies = 0;
                message.attachment = None;
                message.delivery = crate::backend::DeliveryState::Remote;
                message.kind = crate::backend::MessageKind::Text;
            }
            cx.notify();
        })
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        for details in [false, true] {
            if details {
                window.click("details", cx);
            }
            for id in ["group-sample-0", "group-sample-3"] {
                let bounds = window.find(id).bounds();
                assert!(bounds.size.width > px(100.), "Message collapsed: {id}");
                assert!(
                    bounds.size.height < px(300.),
                    "Message wrapped vertically: {id}"
                );
            }
        }
    })
    .expect("message layout");
}

#[gpui_kit::test]
fn authenticated_snapshot_clears_sign_in_status_but_sync_keeps_operation_errors(
    cx: &mut TestAppContext,
) {
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            let mut signed_in = app.snapshot.clone();
            signed_in.revision = 1;
            app.snapshot = crate::backend::Snapshot::default();
            app.groups.clear();
            app.status = "Sign in to your homeserver.".into();
            app.apply(signed_in.clone(), window, cx);
            assert!(app.status.is_empty());
            app.status = "Older messages could not be loaded.".into();
            signed_in.revision = 2;
            app.apply(signed_in, window, cx);
            assert_eq!(app.status, "Older messages could not be loaded.");
            app.apply(
                crate::backend::Snapshot {
                    revision: 3,
                    ..Default::default()
                },
                window,
                cx,
            );
            assert!(app.status.is_empty());
        });
    })
    .expect("authentication status transition");
}

#[gpui_kit::test]
fn offline_timeline_shows_sdk_metadata_samples_and_disables_network_actions(
    cx: &mut TestAppContext,
) {
    let (window, app) = preview_window(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.scroller.update(cx, |state, cx| {
                state.scroll_to_item(0, cx);
            })
        })
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("reply-sample-1").visible());
        assert!(window.find("thread-sample-0").visible());
        let reaction = window.find("reaction-sample-0-👍");
        assert!(reaction.checked() == Some(true) && reaction.disabled() == Some(true));
    })
    .expect("sample reply/reactions/thread");
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.scroller.update(cx, |state, cx| state.scroll_to_end(cx))
        })
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("retry-sample-5").visible());
        assert!(window.find("retry-sample-5").disabled() == Some(true));
        assert!(window.find("cancel-sample-5").visible());
    })
    .expect("sample failed send");
}

#[gpui_kit::test]
fn room_state_samples_show_typing_unread_and_member_profile_navigation(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("typing-notice").visible());
        assert!(window.find("new-messages-sample-3").visible());
        window.click("details", cx);
        assert!(window.find("member-@alex:preview.invalid").visible());
        window.click("member-@alex:preview.invalid", cx);
        assert!(window.find("copy-profile-id").visible());
    })
    .expect("room state controls");
    cx.update(|cx| {
        assert!(
            app.read(cx)
                .profile
                .as_ref()
                .is_some_and(|page| page.id == "@alex:preview.invalid")
        )
    });
}

#[gpui_kit::test]
fn latest_event_visibility_follows_actual_clipped_timeline_rows(cx: &mut TestAppContext) {
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
            |window, cx| cx.new(|cx| MatrixApp::new(true, false, true, window, cx)),
        )
        .expect("large test window")
    });
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.scroller.update(cx, |state, cx| {
                state.scroll_to_item(0, cx);
            })
        })
    });
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("top frame");
    cx.run_until_parked();
    cx.update(|cx| {
        assert!(
            app.read(cx).latest_visible.is_none(),
            "Offscreen latest event marked visible"
        )
    });
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.scroller.update(cx, |state, cx| state.scroll_to_end(cx))
        })
    });
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("latest frame");
    cx.run_until_parked();
    cx.update(|cx| {
        assert!(
            app.read(cx).latest_visible.is_some(),
            "Visible latest event was missed"
        )
    });
}

#[gpui_kit::test]
fn pagination_keeps_existing_group_and_reading_anchor(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.scroller
                .update(cx, |state, cx| state.scroll_to_item(1, cx));
        });
        window.render_frame(cx);
    })
    .expect("initial anchor");
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            let top = app.scroller.read(cx).logical_scroll_top();
            let anchor = app.snapshot.messages[app.groups[top.item_ix].start]
                .id
                .clone();
            let old_first = app.snapshot.messages[0].id.clone();
            let mut snapshot = app.snapshot.clone();
            let mut earlier = snapshot.messages[0].clone();
            earlier.id = "earlier-sample".into();
            earlier.timestamp_ms -= 1;
            snapshot.messages.insert(0, earlier);
            snapshot.revision += 1;
            app.apply(snapshot, window, cx);
            let after = app.scroller.read(cx).logical_scroll_top();
            assert_eq!(
                app.snapshot.messages[app.groups[after.item_ix].start].id,
                anchor
            );
            assert_eq!(after.offset_in_item, top.offset_in_item);
            assert!(
                app.groups
                    .iter()
                    .any(|g| app.snapshot.messages[g.start].id == old_first)
            );
            let mut snapshot = app.snapshot.clone();
            snapshot.revision += 1;
            app.apply(snapshot, window, cx);
            assert!(
                app.groups
                    .iter()
                    .any(|g| app.snapshot.messages[g.start].id == old_first)
            );
        });
        window.render_frame(cx);
    })
    .expect("prepend preserves reading position");
}

#[gpui_kit::test]
fn reaching_top_requests_history_once_until_leaving_top(cx: &mut TestAppContext) {
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
            |window, cx| cx.new(|cx| MatrixApp::new(true, false, true, window, cx)),
        )
        .expect("large test window")
    });

    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("initial frame");
    cx.run_until_parked();
    let before = cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.preview = false;
            app.backend = Some(crate::backend::Backend::with_lock(None).expect("isolated backend"));
            let before = app.pagination_request;
            app.scroller
                .update(cx, |state, cx| state.scroll_to_item(0, cx));
            before
        })
    });
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("top frame");
    cx.run_until_parked();
    cx.update(|cx| assert_eq!(app.read(cx).pagination_request, before + 1));
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("repeat frame");
    cx.run_until_parked();
    cx.update(|cx| assert_eq!(app.read(cx).pagination_request, before + 1));
}
