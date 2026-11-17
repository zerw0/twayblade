use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Bounds, Pixels, Size, TestAppContext, WindowBounds, WindowOptions, px, size,
};

use crate::app::MatrixApp;

/// Opens your profile through the title-bar account menu.
fn open_own_profile(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle) {
    use gpui_kit::test::{TestEventExt, TestQueryExt};
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("my-profile", cx);
    })
    .expect("open account menu");
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let point = window.find_by_label("Your Profile").bounds().center();
        window.pointer_move(point, None, cx);
        window.pointer_down(point, gpui_kit::MouseButton::Left, cx);
        window.pointer_up(point, gpui_kit::MouseButton::Left, cx);
    })
    .expect("choose Your Profile");
    cx.run_until_parked();
}

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
    open_own_profile(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
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
    preview_window_sized(cx, size(px(1120.), px(760.)))
}

fn preview_window_sized(
    cx: &mut TestAppContext,
    window_size: Size<Pixels>,
) -> (gpui_kit::AnyWindowHandle, gpui_kit::Entity<MatrixApp>) {
    cx.update(gpui_kit::init);
    cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    window_size,
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
    })
    .expect("open details");
    open_own_profile(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("copy-profile-id").visible());
    })
    .expect("open details then profile");
    cx.update(|cx| {
        let app = app.read(cx);
        assert!(app.profile.is_some() && app.details_open);
    });
    // Escape closes the profile first, revealing details, then details.
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| app.dismiss_panel(window, cx))
    })
    .expect("dismiss the profile");
    cx.update(|cx| assert!(app.read(cx).profile.is_none() && app.read(cx).details_open));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("close-details", cx);
    })
    .expect("close details");
    cx.update(|cx| assert!(!app.read(cx).details_open));
}

#[gpui_kit::test]
fn sign_in_offers_password_and_discovered_oauth_then_changes_server(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.snapshot = crate::backend::Snapshot::default();
            app.options = Some(crate::backend::LoginOptions {
                homeserver: "https://twayblade.example.org/".into(),
                password: true,
                browser: true,
                sso: false,
                providers: Vec::new(),
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
        window.click("info-tab-Members", cx);
        window.render_frame(cx);
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
fn short_timeline_rests_against_the_composer(cx: &mut TestAppContext) {
    // Tall enough that the whole sample timeline fits with room to spare.
    let (window, app) = preview_window_sized(cx, size(px(1120.), px(1600.)));
    let (first, last) = cx.update(|cx| {
        let messages = &app.read(cx).snapshot.messages;
        (
            messages.first().unwrap().id.clone(),
            messages.last().unwrap().id.clone(),
        )
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        // The header sits above the timeline and the composer below it.
        let above = window.find("header-members").bounds();
        let below = window.find("send").bounds();
        let mut row = |id: &str| {
            window
                .within("messages")
                .find(gpui_kit::SharedString::from(format!("message-row-{id}")))
                .bounds()
        };
        let (first, last) = (row(&first), row(&last));
        let room_above = first.top() - above.bottom();
        let room_below = below.top() - last.bottom();
        assert!(
            room_above > room_below + px(100.),
            "Spare room belongs above the first message: {room_above:?} above, \
             {room_below:?} below"
        );
    })
    .expect("short timeline");
}

#[gpui_kit::test]
fn pagination_keeps_existing_group_and_reading_anchor(cx: &mut TestAppContext) {
    // Short enough that the sample timeline scrolls: a timeline that fits
    // rests against the composer and has no reading anchor to keep.
    let (window, app) = preview_window_sized(cx, size(px(1120.), px(420.)));
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

#[gpui_kit::test]
fn oauth_waiting_sample_cancels_without_network(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    let (cancel, receiver) = tokio::sync::watch::channel(false);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.snapshot = crate::backend::Snapshot::default();
            app.options = Some(crate::backend::LoginOptions {
                homeserver: "https://matrix.example.org/".into(),
                password: false,
                browser: true,
                sso: false,
                providers: Vec::new(),
            });
            app.busy = true;
            app.oauth_cancel = Some(cancel);
            app.status = "Sample data · Waiting for browser sign-in".into();
            cx.notify();
        })
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("cancel-browser-sign-in").visible());
        assert!(window.find_all("browser-sign-in").is_empty());
        assert!(window.find_all("sign-in").is_empty());
        window.click("cancel-browser-sign-in", cx);
    })
    .expect("cancel browser sign-in");
    assert!(*receiver.borrow());
    cx.update(|cx| {
        assert!(!app.read(cx).busy);
        assert!(app.read(cx).status.contains("cancelled"));
        assert!(app.read(cx).oauth_cancel.is_none());
    });
}

#[gpui_kit::test]
fn legacy_sso_provider_buttons_show_discovered_names(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.snapshot = crate::backend::Snapshot::default();
            app.options = Some(crate::backend::LoginOptions {
                homeserver: "https://matrix.example.org/".into(),
                password: false,
                browser: false,
                sso: true,
                providers: vec![twayblade_core::IdentityProvider {
                    id: "local".into(),
                    name: "Local Identity".into(),
                    icon_png: None,
                }],
            });
            cx.notify();
        })
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("sso-local").visible());
        assert!(window.find_all("sign-in").is_empty());
        assert!(window.find_all("browser-sign-in").is_empty());
        window.click("sso-local", cx);
    })
    .expect("provider controls");
    cx.update(|cx| assert!(!app.read(cx).busy, "sample action cannot start networking"));
}

#[gpui_kit::test]
fn hard_logout_requires_confirmation_before_local_reset(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.snapshot.session_state = twayblade_core::auth::SessionState::HardLoggedOut;
            cx.notify();
        })
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("sign-in-again").visible());
        assert!(window.find_all("confirm-session-reset").is_empty());
        assert!(window.find_all("composer-panel").is_empty());
        window.click("sign-in-again", cx);
        assert!(window.find("confirm-session-reset").visible());
        window.click("cancel-session-reset", cx);
    })
    .expect("cancel local cleanup");
    cx.update(|cx| {
        assert!(!app.read(cx).confirm_session_reset);
        assert!(!app.read(cx).snapshot.user_id.is_empty());
    });
    cx.update_window(window, |_, window, cx| {
        window.click("sign-in-again", cx);
        window.click("confirm-session-reset", cx);
    })
    .expect("confirm sample cleanup");
    cx.update(|cx| assert!(app.read(cx).snapshot.user_id.is_empty()));
}

#[gpui_kit::test]
fn soft_logout_retains_account_and_has_no_destructive_control(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.snapshot.session_state = twayblade_core::auth::SessionState::SoftLoggedOut;
            cx.notify();
        })
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("reauthenticate").visible());
        assert!(window.find_all("confirm-session-reset").is_empty());
        assert!(window.find_all("sign-in-again").is_empty());
        assert!(window.find_all("composer-panel").is_empty());
    })
    .expect("soft logout controls");
    cx.update(|cx| assert!(!app.read(cx).snapshot.user_id.is_empty()));
}

#[gpui_kit::test]
fn recovery_key_can_be_written_down_or_copied_and_is_dropped(cx: &mut TestAppContext) {
    let (_main_window, app) = preview_window(cx);
    let show_key = |cx: &mut TestAppContext| {
        cx.update(|cx| {
            app.update(cx, |app, cx| {
                app.security_open = true;
                app.security_key = Some(zeroize::Zeroizing::new("Sample recovery key".into()));
                cx.notify();
            })
        })
    };
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.open_settings(crate::ui::settings::Section::Security, cx)
        })
    });
    let window = cx.update(|cx| app.read(cx).settings.as_ref().unwrap().0);
    // Writing the key down is valid: confirmation must not require the clipboard.
    show_key(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("copy-recovery-key").visible());
        window.click("confirm-recovery-key", cx);
    })
    .expect("confirm without copying");
    cx.update(|cx| assert!(app.read(cx).security_key.is_none()));
    show_key(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("copy-recovery-key", cx);
        window.click("confirm-recovery-key", cx);
    })
    .expect("copy then confirm");
    cx.update(|cx| assert!(app.read(cx).security_key.is_none()));
}

#[gpui_kit::test]
fn sas_sample_has_explicit_match_mismatch_and_cancel_controls(cx: &mut TestAppContext) {
    let (_main_window, app) = preview_window(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.security_open = true;
            app.snapshot.security.ready = true;
            app.snapshot.security.verification = Some(twayblade_core::security::VerificationView {
                flow: "sample".into(),
                device: "Sample device".into(),
                incoming: false,
                phase: twayblade_core::security::VerificationPhase::Emojis(vec![
                    (
                        "🐶".into(),
                        "Dog".into()
                    );
                    7
                ]),
            });
            cx.notify();
        })
    });
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.open_settings(crate::ui::settings::Section::Security, cx)
        })
    });
    let window = cx.update(|cx| app.read(cx).settings.as_ref().unwrap().0);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("confirm-verification").visible());
        assert!(window.find("mismatch-verification").visible());
        assert!(window.find("cancel-verification").visible());
        window.click("mismatch-verification", cx);
    })
    .expect("SAS mismatch");
    cx.update(|cx| {
        assert!(
            app.read(cx)
                .snapshot
                .security
                .verification
                .as_ref()
                .is_some_and(
                    |flow| flow.phase == twayblade_core::security::VerificationPhase::Cancelled
                )
        )
    });
}

#[gpui_kit::test]
fn recovery_input_is_cleared_when_leaving_the_screen(cx: &mut TestAppContext) {
    let (_main_window, app) = preview_window(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.security_open = true;
            app.security_restore = true;
            cx.notify();
        })
    });
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.open_settings(crate::ui::settings::Section::Security, cx)
        })
    });
    let window = cx.update(|cx| app.read(cx).settings.as_ref().unwrap().0);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("cancel-recovery", cx);
    })
    .expect("leave recovery entry");
    cx.update(|cx| {
        assert!(!app.read(cx).security_restore);
        assert!(app.read(cx).security_input.read(cx).value().is_empty());
    });
}

#[gpui_kit::test]
fn polls_show_results_after_voting_and_create_poll_needs_two_answers(cx: &mut TestAppContext) {
    use gpui_kit::test::TestQueryExt;
    let (window, app) = preview_window(cx);
    let (open, ended) = cx.update(|cx| {
        app.update(cx, |app, cx| {
            let own = app.snapshot.user_id.clone();
            crate::ui::polls::sample_polls(&mut app.snapshot.messages, &own);
            let len = app.snapshot.messages.len();
            cx.notify();
            (
                app.snapshot.messages[len - 2].id.clone(),
                app.snapshot.messages[len - 1].id.clone(),
            )
        })
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        // Your own open poll offers End Poll; the ended one takes no votes.
        assert!(
            window
                .find(gpui_kit::SharedString::from(format!("poll-end-{open}")))
                .visible()
        );
        assert!(
            window
                .try_find(gpui_kit::SharedString::from(format!("poll-end-{ended}")))
                .is_none()
        );
        // Ended: plain rows with the final counts, no vote buttons.
        assert!(window.find_by_label("Twayblade 1.0, 6 votes").visible());
        assert!(window.find_by_label("Lisbon, 5 votes, your vote").visible());
        // Create Poll from the composer's ＋ menu.
        app.update(cx, |app, cx| app.open_create_poll(window, cx));
        window.render_frame(cx);
        assert_eq!(window.find("poll-create").disabled(), Some(true));
        window.click("poll-add-answer", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.update(|cx| {
        let page = app.read(cx).management.as_ref().expect("Create Poll");
        assert!(matches!(
            page.kind,
            crate::ui::spaces::ManageKind::CreatePoll(_)
        ));
        assert_eq!(page.poll_answers.len(), 3);
        assert!(page.poll_disclosed);
    });
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            let page = app.management.as_ref().unwrap();
            let question = page.name.clone();
            let answers = page.poll_answers.clone();
            question.update(cx, |s, cx| s.set_value("Lunch?", window, cx));
            answers[0].update(cx, |s, cx| s.set_value("Pizza", window, cx));
            answers[1].update(cx, |s, cx| s.set_value("Soup", window, cx));
        });
        window.render_frame(cx);
        // The empty third answer can go; two must stay.
        window.click(("poll-remove-answer", 2usize), cx);
        window.render_frame(cx);
        assert!(window.try_find(("poll-remove-answer", 0usize)).is_none());
    })
    .unwrap();
    cx.update(|cx| {
        let app = app.read(cx);
        let page = app.management.as_ref().unwrap();
        assert_eq!(page.poll_answers.len(), 2);
        assert_eq!(page.poll_answers[1].read(cx).value().as_str(), "Soup");
    });
}

#[gpui_kit::test]
fn sticker_picker_lists_packs_and_stickers_render_without_a_bubble(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    let id = cx.update(|cx| {
        app.update(cx, |app, cx| {
            let message = app.snapshot.messages.last_mut().expect("sample message");
            message.kind = crate::backend::MessageKind::Sticker;
            message.body = "Waving cat".into();
            message.rich = None;
            message.attachment = Some(twayblade_core::conversation::Attachment {
                filename: "Waving cat".into(),
                ..Default::default()
            });
            cx.notify();
            message.id.clone()
        })
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find(gpui_kit::SharedString::from(format!("message-row-{id}")))
                .visible()
        );
        app.update(cx, |app, cx| app.open_sticker_picker(window, cx));
        window.render_frame(cx);
        // Packs by name; the offline preview never sends.
        assert_eq!(window.find("sticker-user-wave").disabled(), Some(true));
        assert!(window.find("sticker-!design/-lgtm").visible());
    })
    .unwrap();
    cx.update(|cx| {
        let app = app.read(cx);
        let page = app.management.as_ref().expect("picker");
        assert!(matches!(
            page.kind,
            crate::ui::spaces::ManageKind::Stickers(_)
        ));
        assert_eq!(page.sticker_packs.as_ref().map(Vec::len), Some(2));
    });
}

#[gpui_kit::test]
fn voice_messages_show_a_play_button_and_their_length(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    let id = cx.update(|cx| {
        app.update(cx, |app, cx| {
            let message = app.snapshot.messages.last_mut().expect("sample message");
            message.kind = crate::backend::MessageKind::Voice;
            message.rich = None;
            message.voice = Some(twayblade_core::conversation::Voice {
                duration_ms: 75_000,
                waveform: vec![0.2, 0.9, 0.4],
            });
            cx.notify();
            message.id.clone()
        })
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let play = window.find(gpui_kit::SharedString::from(format!("voice-play-{id}")));
        assert!(play.visible());
        // The offline preview has no audio to play.
        assert_eq!(play.disabled(), Some(true));
        // Nothing records until the menu item is chosen.
        assert!(window.try_find("voice-recording").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn attachment_controls_require_an_explicit_action_and_samples_cannot_upload(
    cx: &mut TestAppContext,
) {
    let (window, app) = preview_window(cx);
    let id = cx.update(|cx| {
        app.update(cx, |app, cx| {
            let message = app.snapshot.messages.last_mut().expect("sample message");
            message.kind = crate::backend::MessageKind::File;
            message.delivery = crate::backend::DeliveryState::Remote;
            message.attachment = Some(twayblade_core::conversation::Attachment {
                filename: "report.pdf".into(),
                size: Some(2048),
                preview_png: None,
            });
            let id = message.id.clone();
            cx.notify();
            id
        })
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find(gpui_kit::SharedString::from(format!("save-{id}")))
                .visible()
        );
        assert!(
            window
                .find(gpui_kit::SharedString::from(format!("open-{id}")))
                .visible()
        );
        assert!(window.find("attach-file").visible());
        window.click(gpui_kit::SharedString::from(format!("open-{id}")), cx);
    })
    .expect("attachment controls");
    cx.update(|cx| assert!(app.read(cx).attachments.drafts.is_empty()));
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            let message = app.snapshot.messages.last_mut().unwrap();
            message.delivery = crate::backend::DeliveryState::Queued;
            message.upload_percent = Some(42);
            cx.notify();
        })
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find(gpui_kit::SharedString::from(format!("cancel-upload-{id}")))
                .visible()
        );
        assert!(
            window
                .find_all(gpui_kit::SharedString::from(format!("save-{id}")))
                .is_empty()
        );
    })
    .expect("upload controls");
}

#[gpui_kit::test]
fn emoji_picker_inserts_at_unicode_cursor_and_enter_never_sends(cx: &mut TestAppContext) {
    use gpui_kit::test::TestEventExt;
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.within("composer-panel").click("input-state", cx);
        window.input("A🦀 B", cx);
        window.key_down("left", false, cx);
        window.click("emoji-open", cx);
        window.within("emoji-search-panel").click("input-state", cx);
        window.input("crab", cx);
        window.key_down("enter", false, cx);
        assert_eq!(
            window
                .within("composer-panel")
                .find_by_label("Message")
                .value(),
            Some("A🦀 🦀B")
        );
        assert!(window.find_all("emoji-close").is_empty());
    })
    .expect("unicode cursor insertion");
    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(
            app.emoji_preferences.recents.first().map(String::as_str),
            Some("🦀")
        );
        assert!(app.sending.is_empty());
    });
}

#[gpui_kit::test]
fn emoji_cell_click_reaches_the_button_under_its_glyph(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("emoji-open", cx);
        window.within("emoji-search-panel").click("input-state", cx);
        window.input("crab", cx);
        // The glyph is drawn by a sibling above the button; it must not block the click.
        window.click("emoji-0", cx);
        assert_eq!(
            window
                .within("composer-panel")
                .find_by_label("Message")
                .value(),
            Some("🦀")
        );
    })
    .expect("click an emoji cell");
    cx.update(|cx| {
        assert_eq!(
            app.read(cx)
                .emoji_preferences
                .recents
                .first()
                .map(String::as_str),
            Some("🦀")
        );
    });
}

#[gpui_kit::test]
fn typing_at_lists_members_and_return_mentions_one(cx: &mut TestAppContext) {
    use gpui_kit::test::TestEventExt;
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.within("composer-panel").click("input-state", cx);
        window.input("thanks @sa", cx);
    })
    .expect("type a mention");
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("mention-list").visible());
        window.key_down("enter", false, cx);
        assert_eq!(
            window
                .within("composer-panel")
                .find_by_label("Message")
                .value(),
            Some("thanks @Sam ")
        );
    })
    .expect("pick with Return");
    cx.update(|cx| {
        let app = app.read(cx);
        assert!(app.mention_completion.is_none());
        let room = app.snapshot.selected_room.clone();
        let picked = app.mentions.get(&room).cloned().unwrap_or_default();
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].user_id, "@sam:preview.invalid");
        assert_eq!(picked[0].label, "Sam");
    });
}

#[gpui_kit::test]
fn shortcode_skin_tone_and_reaction_share_the_picker(cx: &mut TestAppContext) {
    use gpui_kit::test::TestEventExt;
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.within("composer-panel").click("input-state", cx);
        window.input("hello :thumbsup", cx);
    })
    .expect("shortcode input");
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("emoji-picker").visible());
        window.key_down("enter", false, cx);
        assert_eq!(
            window
                .within("composer-panel")
                .find_by_label("Message")
                .value(),
            Some("hello 👍")
        );
    })
    .expect("shortcode replacement");
    let id = cx.update(|cx| app.read(cx).snapshot.messages.last().unwrap().id.clone());
    cx.update_window(window, |_, window, cx| {
        window.click(
            gpui_kit::SharedString::from(format!("add-reaction-{id}")),
            cx,
        );
        window.within("emoji-search-panel").click("input-state", cx);
        window.input("crab", cx);
        window.click("emoji-0", cx);
        assert_eq!(
            window
                .within("composer-panel")
                .find_by_label("Message")
                .value(),
            Some("hello 👍")
        );
    })
    .expect("shared reaction picker");
    cx.update(|cx| {
        let app = app.read(cx);
        assert!(app.emoji_picker.is_none());
        assert!(app.pending_actions.is_empty());
        assert_eq!(
            app.emoji_preferences.recents.first().map(String::as_str),
            Some("🦀")
        );
    });
}

#[gpui_kit::test]
fn emoji_with_skin_tones_opens_a_tone_popover_and_remembers_the_tone(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("emoji-open", cx);
        window.render_frame(cx);
        assert!(window.find("emoji-group-0").visible());
        window.within("emoji-search-panel").click("input-state", cx);
        window.input("thumbs up", cx);
        window.render_frame(cx);
        window.click("emoji-0", cx);
        window.render_frame(cx);
        assert!(window.find("emoji-tones").visible());
        window.click("emoji-tone-5", cx);
        assert_eq!(
            window
                .within("composer-panel")
                .find_by_label("Message")
                .value(),
            Some("👍🏿")
        );
    })
    .expect("tone popover");
    cx.update(|cx| {
        let app = app.read(cx);
        assert!(app.emoji_picker.is_none());
        assert_eq!(app.emoji_preferences.tone, 5);
    });
}

#[gpui_kit::test]
fn member_actions_follow_power_levels_and_confirm_destructive_steps(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("details", cx);
        window.click("info-tab-Members", cx);
        window.render_frame(cx);
        // Alex outranks you: no moderation actions, only Message/Verify/Ignore.
        window.click("member-@alex:preview.invalid", cx);
        window.render_frame(cx);
        assert!(window.find("member-message").visible());
        assert!(window.find("member-verify").visible());
        assert!(window.find("member-ignore").visible());
        assert!(window.try_find("member-kick").is_none());
    })
    .expect("member actions for an admin");
    cx.update_window(window, |_, window, cx| {
        window.click("close-profile", cx);
        window.render_frame(cx);
        window.click("member-@sam:preview.invalid", cx);
        window.render_frame(cx);
        assert!(window.find("member-mute").visible());
        assert!(window.find("member-ban").visible());
        window.click("member-kick", cx);
    })
    .expect("moderation actions");
    cx.update(|cx| {
        let confirm = app.read(cx).member_confirm.clone();
        assert_eq!(
            confirm,
            Some((
                "@sam:preview.invalid".into(),
                twayblade_core::room_state::MemberAction::Kick
            ))
        );
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("member-kick", cx);
    })
    .expect("confirm kick");
    cx.update(|cx| assert!(app.read(cx).member_confirm.is_none()));
}

#[gpui_kit::test]
fn message_search_can_be_limited_to_one_sender(cx: &mut TestAppContext) {
    use gpui_kit::test::{TestEventExt, TestQueryExt};
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.within("title-search").click("input-state", cx);
        window.input("rust", cx);
        window.render_frame(cx);
        window.key_down("enter", false, cx);
    })
    .expect("open message search");
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("search-from").visible());
        window.click("search-from", cx);
        window.render_frame(cx);
        assert!(window.find_by_label("Anyone").visible());
    })
    .expect("sender menu");
    cx.update(|cx| assert!(app.read(cx).search_sender.is_none()));
}

#[gpui_kit::test]
fn title_search_offers_message_search_and_opens_the_panel(cx: &mut TestAppContext) {
    use gpui_kit::test::TestEventExt;
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.within("title-search").click("input-state", cx);
        window.input("rust", cx);
        window.render_frame(cx);
        // Typing filters conversations and offers a message search row.
        assert!(window.find("search-messages").visible());
        window.key_down("enter", false, cx);
    })
    .expect("type a query");
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("close-search").visible());
        window.click("close-search", cx);
        window.render_frame(cx);
    })
    .expect("message search panel");
    cx.update(|cx| assert!(!app.read(cx).search_open));
}

#[gpui_kit::test]
fn search_panel_shows_recent_conversations_and_opens_a_match(cx: &mut TestAppContext) {
    use gpui_kit::test::TestEventExt;
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.within("title-search").click("input-state", cx);
        window.render_frame(cx);
        // An empty field shows recent conversations and where else to look.
        assert!(window.find("spotlight").visible());
        assert!(window.find("spotlight-recent").visible());
        window.input("crit", cx);
        window.render_frame(cx);
        assert!(window.find("search-messages").visible());
        window.key_down("enter", false, cx);
    })
    .expect("search for a conversation");
    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(app.navigation.selected_room.as_deref(), Some("critique"));
        assert!(app.search.read(cx).value().is_empty());
        assert!(!app.spotlight_open);
    });
}

#[gpui_kit::test]
fn system_accent_updates_primary_buttons_and_selection_in_both_appearances(
    cx: &mut TestAppContext,
) {
    use gpui_kit::component::{ActiveTheme, Theme, ThemeMode};
    cx.update(gpui_kit::init);
    cx.update(|cx| {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            Theme::change(mode, None, cx);
            crate::theme::set_system_accent(Some([0.2, 0.8, 0.4]), cx);
            crate::theme::apply(cx);
            let primary: gpui_kit::Rgba = cx.theme().primary.into();
            let button: gpui_kit::Rgba = cx.theme().button_primary.into();
            assert!((primary.r - 0.2).abs() < 0.001);
            assert!((primary.g - 0.8).abs() < 0.001);
            assert!((primary.b - 0.4).abs() < 0.001);
            assert_eq!(primary, button);
            let selection: gpui_kit::Rgba = cx.theme().accent.into();
            assert!(selection.g > selection.r);
            assert!(!crate::theme::set_system_accent(Some([0.2, 0.8, 0.4]), cx));
        }
    });
}

#[gpui_kit::test]
fn consecutive_message_groups_keep_six_pixel_spacing_after_content_updates(
    cx: &mut TestAppContext,
) {
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            let mut snapshot = app.snapshot.clone();
            snapshot.revision += 1;
            snapshot.messages.truncate(3);
            for (ix, message) in snapshot.messages.iter_mut().enumerate() {
                message.sender = if ix == 1 {
                    snapshot.user_id.clone()
                } else {
                    "@alex:preview.invalid".into()
                };
                message.kind = crate::backend::MessageKind::Text;
                message.reactions.clear();
                message.thread_replies = 0;
                message.new_messages = false;
                message.delivery = crate::backend::DeliveryState::Remote;
            }
            app.apply(snapshot, window, cx);
        });
        window.render_frame(cx);
        for changed in [false, true] {
            if changed {
                app.update(cx, |app, cx| {
                    let mut snapshot = app.snapshot.clone();
                    snapshot.revision += 1;
                    snapshot.messages[1].body = "A longer message. ".repeat(50);
                    snapshot.messages[1].reply = None;
                    app.apply(snapshot, window, cx);
                });
            }
            window.render_frame(cx);
            window.render_frame(cx);
            for (previous, next) in [
                ("group-sample-0", "group-sample-1"),
                ("group-sample-1", "group-sample-2"),
            ] {
                let previous = window.find(previous).bounds();
                let next = window.find(next).bounds();
                // Group bounds include the next group's 6px top padding.
                let gap = next.top() - previous.bottom() + px(6.);
                assert!(
                    (gap - px(6.)).abs() < px(0.5),
                    "gap: {gap:?}, changed: {changed}"
                );
            }
        }
    })
    .expect("consecutive group layout");
}

#[gpui_kit::test]
fn unencrypted_warning_wraps_inside_details_panel(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.snapshot
                .rooms
                .iter_mut()
                .find(|room| room.id == "general")
                .unwrap()
                .encrypted = false;
            app.details_open = true;
            cx.notify();
        })
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let warning = window.find("encryption-warning").bounds();
        let text = window.find("encryption-warning-text").bounds();
        assert!(warning.size.width <= px(280.));
        assert!(text.left() >= warning.left() && text.right() <= warning.right());
        assert!(text.size.height > px(24.), "warning should wrap: {text:?}");
    })
    .expect("warning layout");
}

#[gpui_kit::test]
fn composer_icons_have_balanced_targets_and_centres(cx: &mut TestAppContext) {
    let (window, _) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.within("composer-panel").click("input-state", cx);
        window.input("Sample draft", cx);
        window.render_frame(cx);
        let attach = window.find("attach-file").bounds();
        // The composer's toolbar row: square 26pt tool buttons, a wider send
        // button of the same height, all on one centre line.
        for id in ["attach-file", "emoji-open", "send", "open-settings"] {
            let bounds = window.find(id).bounds();
            if id == "send" {
                assert_eq!(bounds.size.height, px(26.), "{id}");
            } else {
                assert_eq!(bounds.size, size(px(26.), px(26.)), "{id}");
            }
            if matches!(id, "emoji-open" | "send") {
                assert!(
                    (bounds.center().y - attach.center().y).abs() < px(0.5),
                    "{id}"
                );
            }
        }
        assert!(window.find("typing-notice").bounds().size.height >= px(13.));
    })
    .expect("composer icon layout");
}

#[gpui_kit::test]
fn thread_summary_opens_panel_preserves_draft_and_yields_to_details(cx: &mut TestAppContext) {
    use gpui_kit::test::TestEventExt;
    let (window, app) = preview_window(cx);
    cx.update(|cx| {
        cx.bind_keys([gpui_kit::KeyBinding::new(
            "escape",
            crate::app::CloseProfile,
            Some("Twayblade"),
        )])
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("thread-sample-0", cx);
        assert!(window.find("close-thread").visible());
        window
            .within("thread-composer-panel")
            .find_by_label("Reply in thread");
        window
            .within("thread-composer-panel")
            .click("input-state", cx);
        window.input("Thread draft 🦀", cx);
        assert!(window.find("thread-send").disabled() == Some(true));
        window.click("close-thread", cx);
        window.click("thread-sample-0", cx);
        assert_eq!(
            window
                .within("thread-composer-panel")
                .find_by_label("Reply in thread")
                .value(),
            Some("Thread draft 🦀")
        );
        window.click("details", cx);
        assert!(window.find("copy-room-id").visible());
        window.click("thread-sample-0", cx);
        window.key_down("escape", false, cx);
    })
    .expect("thread controls and draft");
    cx.update(|cx| assert!(app.read(cx).thread.is_none()));
}

#[gpui_kit::test]
fn thread_drafts_are_independent_and_stale_thread_updates_are_rejected(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| app.open_thread("sample-0".into(), window, cx));
        window.render_frame(cx);
        window
            .within("thread-composer-panel")
            .click("input-state", cx);
        window.input("First draft", cx);
        app.update(cx, |app, cx| app.open_thread("sample-3".into(), window, cx));
        window.render_frame(cx);
        window
            .within("thread-composer-panel")
            .click("input-state", cx);
        window.input("Second draft", cx);
        let stale = app.update(cx, |app, _| {
            let t = app.thread.as_ref().unwrap();
            crate::backend::ThreadSnapshot {
                room: t.room.clone(),
                root: t.root.event_id.clone().unwrap(),
                request: t.request,
                messages: Vec::new(),
                status: twayblade_core::conversation::TimelineStatus::Failed,
                ready: false,
                unread: 99,
            }
        });
        app.update(cx, |app, cx| {
            app.open_thread("sample-0".into(), window, cx);
            app.update_thread(Some(&stale), cx);
            let t = app.thread.as_ref().unwrap();
            assert!(t.ready && t.unread == 0 && !t.messages.is_empty());
        });
        window.render_frame(cx);
        assert_eq!(
            window
                .within("thread-composer-panel")
                .find_by_label("Reply in thread")
                .value(),
            Some("First draft")
        );
    })
    .expect("thread lifetime and independent drafts");
}

#[gpui_kit::test]
fn space_context_menu_opens_directory_with_access_states(cx: &mut TestAppContext) {
    use gpui_kit::test::{TestEventExt, TestQueryExt};
    let (window, _) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let point = window.find("space-design").bounds().center();
        window.pointer_move(point, None, cx);
        window.pointer_down(point, gpui_kit::MouseButton::Right, cx);
        window.pointer_up(point, gpui_kit::MouseButton::Right, cx);
    })
    .expect("open deferred context menu");
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let point = window.find_by_label("Browse rooms").bounds().center();
        window.pointer_move(point, None, cx);
        window.pointer_down(point, gpui_kit::MouseButton::Left, cx);
        window.pointer_up(point, gpui_kit::MouseButton::Left, cx);
        assert!(window.find("join-sample-research").disabled() == Some(true));
        assert!(window.find("join-sample-restricted").disabled() == Some(true));
        assert!(window.try_find("join-sample-private").is_none());
        assert!(window.find("browse-sample-nested").visible());
    })
    .expect("space context menu and directory");
}

#[gpui_kit::test]
fn space_leave_defaults_to_space_only_and_creation_respects_parent_power(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.open_management(
                crate::ui::spaces::ManageKind::Leave("design".into()),
                window,
                cx,
            )
        });
        window.render_frame(cx);
        app.update(cx, |app, _| {
            assert_eq!(
                app.management.as_ref().unwrap().selected,
                std::collections::HashSet::from(["design".into()])
            )
        });
        window.click("leave-choice-general", cx);
        app.update(cx, |app, _| {
            assert!(
                app.management
                    .as_ref()
                    .unwrap()
                    .selected
                    .contains("general")
            )
        });
        window.click("close-management", cx);
        app.update(cx, |app, _| app.snapshot.spaces[0].can_add_rooms = false);
        window.click("new-room", cx);
        assert!(window.try_find("parent-design").is_none());
        assert!(window.find("room-encryption").checked() == Some(true));
        assert!(window.find("create-room-submit").disabled() == Some(true));
        window.click("room-encryption", cx);
        assert!(window.find("room-encryption").checked() == Some(false));
    })
    .expect("leave choice and permission-aware creation");
}

#[gpui_kit::test]
fn room_dm_and_space_invites_have_explicit_accept_and_decline(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.snapshot.invites = (0..3)
                .map(|ix| twayblade_core::space_actions::Invite {
                    id: format!("invite-{ix}"),
                    name: format!("Sample invitation {ix}"),
                    direct: ix == 1,
                    space: ix == 2,
                })
                .collect();
            cx.notify();
        })
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        for ix in 0..3 {
            assert!(
                window
                    .find(gpui_kit::SharedString::from(format!("accept-invite-{ix}")))
                    .disabled()
                    == Some(true)
            );
            assert!(
                window
                    .find(gpui_kit::SharedString::from(format!("decline-invite-{ix}")))
                    .disabled()
                    == Some(true)
            );
        }
    })
    .expect("offline invite actions");
}

#[gpui_kit::test]
fn viewer_keyboard_navigation_zoom_clipboard_and_focus_restore(cx: &mut TestAppContext) {
    use gpui_kit::test::TestEventExt;
    let (window, app) = preview_window(cx);
    cx.update(|cx| cx.bind_keys(crate::shortcuts::bindings()));
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.preview_viewer(window, cx);
            app.close_viewer(window, cx);
        });
        window.render_frame(cx);
        window.key_down("space", false, cx);
        assert!(window.find("viewer-copy").visible());
        let width = window.find("viewer-image").bounds().size.width;
        window.key_down(
            if cfg!(target_os = "macos") {
                "cmd-="
            } else {
                "ctrl-="
            },
            false,
            cx,
        );
        assert!(window.find("viewer-image").bounds().size.width > width);
        window.key_down(
            if cfg!(target_os = "macos") {
                "cmd-0"
            } else {
                "ctrl-0"
            },
            false,
            cx,
        );
        assert_eq!(window.find("viewer-image").bounds().size.width, width);
        window.key_down("right", false, cx);
        window.key_down("left", false, cx);
        app.update(cx, |app, _| {
            assert_eq!(app.viewer.as_ref().unwrap().index, 0)
        });
        window.key_down(
            if cfg!(target_os = "macos") {
                "cmd-c"
            } else {
                "ctrl-c"
            },
            false,
            cx,
        );
        assert!(
            cx.read_from_clipboard()
                .unwrap()
                .entries
                .iter()
                .any(|e| matches!(e, gpui_kit::ClipboardEntry::Image(_)))
        );
        let mut focused = Vec::new();
        for _ in 0..18 {
            window.key_down("tab", false, cx);
            app.update(cx, |app, cx| {
                assert!(
                    app.viewer
                        .as_ref()
                        .unwrap()
                        .focus
                        .contains_focused(window, cx),
                    "Tab escaped viewer"
                )
            });
            if let Some(focus) = window.focused(cx)
                && !focused.contains(&focus)
            {
                focused.push(focus);
            }
        }
        assert!(
            focused.len() >= 8,
            "viewer actions must be reachable with Tab"
        );
        window.key_down("right", false, cx);
        window.key_down("right", false, cx);
        window.key_down(
            if cfg!(target_os = "macos") {
                "cmd-shift-l"
            } else {
                "ctrl-shift-l"
            },
            false,
            cx,
        );
    })
    .expect("viewer keyboard navigation");
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("image-sample-image-2").visible());
        assert!(window.find("image-sample-image-2").focused() == Some(true));
        window.key_down("space", false, cx);
        window.key_down("escape", false, cx);
        app.update(cx, |app, _| assert!(app.viewer.is_none()));
    })
    .expect("viewer returns focus to revealed image");
}

#[gpui_kit::test]
fn viewer_and_dialogs_are_named_for_screen_readers(cx: &mut TestAppContext) {
    use gpui_kit::test::TestQueryExt;
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| app.preview_viewer(window, cx));
        window.render_frame(cx);
    })
    .expect("open the viewer");
    let total = cx.update(|cx| app.read(cx).viewer.as_ref().expect("viewer").entries.len());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        // The overlay says which image of how many; the position is a status.
        assert!(
            window
                .find_by_label(&format!("Image viewer, Image 1 of {total}"))
                .visible()
        );
        assert!(
            window
                .find_by_label(&format!("Image 1 of {total}"))
                .visible()
        );
        window.click("viewer-close", cx);
    })
    .expect("viewer labels");
    // The create-room sheet is a dialog with its heading as its name.
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.open_management(crate::ui::spaces::ManageKind::Create, window, cx)
        });
        window.render_frame(cx);
        assert!(window.find_by_label("Create Channel").visible());
    })
    .expect("dialog label");
    // A side panel is a named region.
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.management = None;
            app.search_open = true;
            cx.notify();
        });
        window.render_frame(cx);
        assert!(window.find_by_label("Message search").visible());
    })
    .expect("panel label");
}

#[gpui_kit::test]
fn viewer_save_and_open_shortcuts_require_a_destination(cx: &mut TestAppContext) {
    use gpui_kit::test::TestEventExt;
    let (window, app) = preview_window(cx);
    cx.update(|cx| cx.bind_keys(crate::shortcuts::bindings()));
    let path = std::env::temp_dir().join(format!(
        "matrix-viewer-ui-{}.png",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| app.preview_viewer(window, cx));
        window.render_frame(cx);
        window.key_down(
            if cfg!(target_os = "macos") {
                "cmd-s"
            } else {
                "ctrl-s"
            },
            false,
            cx,
        );
    })
    .expect("save shortcut");
    assert!(cx.did_prompt_for_new_path() && !path.exists());
    cx.simulate_new_path_selection(|_| Some(path.clone()));
    cx.run_until_parked();
    assert!(
        path.exists(),
        "explicit save should write chosen destination"
    );
    std::fs::remove_file(&path).unwrap();
    cx.update_window(window, |_, window, cx| {
        window.key_down(
            if cfg!(target_os = "macos") {
                "cmd-o"
            } else {
                "ctrl-o"
            },
            false,
            cx,
        )
    })
    .expect("open shortcut");
    assert!(
        cx.did_prompt_for_new_path(),
        "opening another app must first ask to save"
    );
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    assert!(!path.exists(), "cancelling Open must not save anything");
}

#[gpui_kit::test]
fn viewer_pinch_double_click_pan_outside_click_and_stale_loads(cx: &mut TestAppContext) {
    use gpui_kit::test::{ClickOptions, TestEventExt};
    use gpui_kit::{InputEvent, MouseButton, point};
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| app.preview_viewer(window, cx));
        window.render_frame(cx);
        let centre = window.find("viewer-image").bounds().center();
        window.pointer_move(centre, None, cx);
        window.dispatch_event(
            gpui_kit::PinchEvent {
                position: centre,
                delta: 1.,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        app.update(cx, |app, _| assert!(app.viewer.as_ref().unwrap().zoom > 1.));
        window.click_with_options("viewer-image", ClickOptions::new().with_count(2), cx);
        app.update(cx, |app, _| {
            assert_eq!(app.viewer.as_ref().unwrap().zoom, 1.)
        });
        app.update(cx, |app, cx| app.zoom_viewer(16., window, cx));
        window.render_frame(cx);
        let centre = window.find("viewer-canvas").bounds().center();
        window.pointer_down(centre, MouseButton::Left, cx);
        window.pointer_move(
            centre + point(px(80.), px(20.)),
            Some(MouseButton::Left),
            cx,
        );
        window.pointer_up(centre + point(px(80.), px(20.)), MouseButton::Left, cx);
        app.update(cx, |app, cx| {
            assert!(app.viewer.as_ref().unwrap().pan.x > px(0.));
            let old = app.viewer.as_ref().unwrap().request;
            let cancel = app.viewer.as_ref().unwrap().cancel.clone();
            app.step_image(1, window, cx);
            assert!(*cancel.borrow());
            app.finish_viewer(
                old,
                app.snapshot.account_instance,
                Err(twayblade_core::conversation::ImageError::Download),
                cx,
            );
            assert!(matches!(
                app.viewer.as_ref().unwrap().load,
                crate::ui::viewer::ImageLoad::Ready { .. }
            ));
        });
        window.pointer_down(point(px(10.), px(80.)), MouseButton::Left, cx);
        window.pointer_up(point(px(10.), px(80.)), MouseButton::Left, cx);
        app.update(cx, |app, _| assert!(app.viewer.is_none()));
    })
    .expect("viewer gestures and request lifetime");
}

#[gpui_kit::test]
fn settings_sections_share_one_window_and_preserve_conversation(cx: &mut TestAppContext) {
    let (main_window, app) = preview_window(cx);
    cx.update_window(main_window, |_, window, cx| {
        window.render_frame(cx);
        window.within("composer-panel").click("input-state", cx);
        window.input("Retained Settings draft", cx);
        window.click("open-settings", cx);
        window.render_frame(cx);
    })
    .unwrap();
    let handle = cx.update(|cx| app.read(cx).settings.as_ref().unwrap().0);
    for section in crate::ui::settings::Section::VISIBLE {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click(
                gpui_kit::SharedString::from(format!("settings-{}", section.label())),
                cx,
            );
            window.render_frame(cx);
            // Reset exists only where there are local preferences to reset.
            let resettable = matches!(
                section,
                crate::ui::settings::Section::Appearance | crate::ui::settings::Section::Chat
            );
            assert_eq!(
                !window.find_all("reset-settings-section").is_empty(),
                resettable
            );
        })
        .unwrap();
    }
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.open_settings(crate::ui::settings::Section::Appearance, cx)
        });
        let main = app.read(cx);
        assert_eq!(main.settings.as_ref().unwrap().0, handle);
        assert_eq!(main.snapshot.selected_room, "general");
        assert_eq!(
            main.composer.read(cx).value().as_str(),
            "Retained Settings draft"
        );
    });
    cx.update_window(main_window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .within("composer-panel")
                .find("input-state")
                .visible()
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn live_scheme_controls_preserve_room_draft_timeline_and_thread(cx: &mut TestAppContext) {
    use gpui_kit::AppContext;
    let (main_window, app) = preview_window(cx);
    cx.update_window(main_window, |_, window, cx| {
        window.render_frame(cx);
        window.within("composer-panel").click("input-state", cx);
        window.input("Scheme draft", cx);
        app.update(cx, |app, cx| {
            app.open_thread("sample-0".into(), window, cx);
            app.open_settings(crate::ui::settings::Section::Appearance, cx);
        });
    })
    .unwrap();
    let (handle, ids, scroller) = cx.update(|cx| {
        let main = app.read(cx);
        (
            main.settings.as_ref().unwrap().0,
            main.snapshot
                .messages
                .iter()
                .map(|m| m.id.clone())
                .collect::<Vec<_>>(),
            main.scroller.entity_id(),
        )
    });
    for scheme in crate::preferences::Scheme::ALL {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click(
                gpui_kit::SharedString::from(format!("scheme-{}", scheme.name())),
                cx,
            );
        })
        .unwrap();
        cx.update(|cx| {
            let main = app.read(cx);
            assert_eq!(main.preferences.appearance.scheme, scheme);
            assert_eq!(main.snapshot.selected_room, "general");
            assert_eq!(main.composer.read(cx).value().as_str(), "Scheme draft");
            assert!(main.thread.is_some());
            assert_eq!(main.scroller.entity_id(), scroller);
            assert_eq!(
                main.snapshot
                    .messages
                    .iter()
                    .map(|m| m.id.clone())
                    .collect::<Vec<_>>(),
                ids
            );
        });
    }
}

#[gpui_kit::test]
fn theme_editor_preset_loading_and_custom_palette(cx: &mut TestAppContext) {
    use gpui_kit::AppContext;
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.open_settings(crate::ui::settings::Section::Appearance, cx);
        });
        window.render_frame(cx);
    })
    .unwrap();
    let handle = cx.update(|cx| app.read(cx).settings.as_ref().unwrap().0);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("preset-catppuccin", cx);
    })
    .unwrap();
    cx.update(|cx| {
        let main = app.read(cx);
        assert_eq!(
            main.preferences.appearance.scheme,
            crate::preferences::Scheme::Custom
        );
        assert!(main.preferences.appearance.custom_theme.is_some());
        let custom = main.preferences.appearance.custom_theme.unwrap();
        let [bg, ..] = crate::theme::surfaces(crate::preferences::Scheme::Catppuccin, false);
        assert_eq!(custom.background, bg);
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("reset-custom-theme", cx);
    })
    .unwrap();
    cx.update(|cx| {
        let main = app.read(cx);
        assert_eq!(
            main.preferences.appearance.scheme,
            crate::preferences::Scheme::Custom
        );
        let custom = main.preferences.appearance.custom_theme.unwrap();
        assert_eq!(custom, crate::preferences::CustomTheme::default());
    });
}

#[gpui_kit::test]
fn scale_changes_real_layout_and_missing_fonts_fall_back(cx: &mut TestAppContext) {
    use gpui_kit::AppContext;
    let (window, app) = preview_window(cx);
    let before = cx
        .update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.find("room-general").bounds().size.height
        })
        .unwrap();
    for percent in [85, 150, 100] {
        cx.update_window(window, |_, window, cx| {
            app.update(cx, |app, cx| {
                app.preferences.appearance.scale = percent;
                app.preferences.appearance.interface_font = Some("Missing Matrix Test Font".into());
                app.appearance_changed(window, cx);
            });
            window.render_frame(cx);
            let height = window.find("room-general").bounds().size.height;
            assert!((f32::from(height) - f32::from(before) * f32::from(percent) / 100.).abs() < 1.);
            assert!(
                window
                    .within("composer-panel")
                    .find("input-state")
                    .visible()
            );
            assert!(window.find("open-settings").visible());
        })
        .unwrap();
    }
    cx.update(|_| {
        assert_eq!(
            crate::theme::font_or_default(Some("missing"), &["Installed".into()], "System")
                .as_ref(),
            "System"
        );
        assert_eq!(
            crate::theme::font_or_default(Some("Installed"), &["Installed".into()], "System")
                .as_ref(),
            "Installed"
        );
    });
}

#[gpui_kit::test]
fn switching_all_styles_preserves_visible_ids_draft_anchor_thread_and_actions(
    cx: &mut TestAppContext,
) {
    use crate::preferences::ChatStyle;
    use gpui_kit::AppContext;
    cx.update(gpui_kit::init);
    let (window, app) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1600.), px(1200.)),
                    cx,
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| MatrixApp::new(true, false, false, window, cx)),
        )
        .unwrap()
    });
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            let mut snapshot = app.snapshot.clone();
            let mut deleted = snapshot.messages[0].clone();
            deleted.id = "sample-deleted".into();
            deleted.kind = crate::backend::MessageKind::Redacted;
            deleted.reactions.clear();
            deleted.thread_replies = 0;
            deleted.reply = None;
            deleted.body.clear();
            snapshot.messages.insert(2, deleted);
            snapshot.revision += 1;
            app.apply(snapshot, window, cx);
        });
        window.render_frame(cx);
        window.within("composer-panel").click("input-state", cx);
        window.input("Style draft 🦀", cx);
        app.update(cx, |app, cx| {
            app.open_thread("sample-0".into(), window, cx);
            app.open_settings(crate::ui::settings::Section::Chat, cx);
            app.scroller
                .update(cx, |state, cx| state.scroll_to_item(0, cx));
        });
        window.render_frame(cx);
    })
    .unwrap();
    let handle = cx.update(|cx| app.read(cx).settings.as_ref().unwrap().0);
    let ids = cx.update(|cx| {
        app.read(cx)
            .snapshot
            .messages
            .iter()
            .map(|m| m.id.clone())
            .collect::<Vec<_>>()
    });
    for style in [ChatStyle::Bubbles, ChatStyle::Irc, ChatStyle::Minimal] {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click(gpui_kit::SharedString::from(format!("style-{style:?}")), cx);
        })
        .unwrap();
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            for id in &ids {
                assert!(
                    window
                        .within("messages")
                        .find(gpui_kit::SharedString::from(format!("message-row-{id}")))
                        .visible(),
                    "Missing visible message {id} in {style:?}"
                );
            }
            assert!(window.within("messages").find("edited-sample-1").visible());
            for action in [
                "add-reaction-sample-0",
                "reply-thread-sample-0",
                "reaction-sample-0-👍",
                "thread-sample-0",
                "image-sample-4",
                "save-sample-4",
                "open-sample-4",
                "retry-sample-5",
                "cancel-sample-5",
            ] {
                window.within("messages").hover(action, cx);
                assert!(
                    window.within("messages").find(action).visible(),
                    "Missing action {action} in {style:?}"
                );
            }
            assert_eq!(
                window
                    .within("composer-panel")
                    .find_by_label("Message")
                    .value(),
                Some("Style draft 🦀")
            );
        })
        .unwrap();
        cx.update(|cx| {
            let main = app.read(cx);
            assert_eq!(main.snapshot.selected_room, "general");
            assert_eq!(
                main.snapshot
                    .messages
                    .iter()
                    .map(|m| m.id.clone())
                    .collect::<Vec<_>>(),
                ids
            );
            assert_eq!(main.thread.as_ref().unwrap().root.id, "sample-0");
        });
    }
}

#[gpui_kit::test]
fn leaving_security_section_discards_recovery_entry(cx: &mut TestAppContext) {
    let (_, app) = preview_window(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.open_settings(crate::ui::settings::Section::Security, cx)
        })
    });
    let settings = cx.update(|cx| app.read(cx).settings.as_ref().unwrap().0);
    cx.update_window(settings, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.security_restore = true;
            app.security_input.update(cx, |input, cx| {
                input.set_value("Sample recovery entry", window, cx)
            });
            cx.notify();
        });
        window.render_frame(cx);
        window.click("settings-Appearance", cx);
    })
    .unwrap();
    cx.update(|cx| {
        assert!(app.read(cx).security_input.read(cx).value().is_empty());
        assert!(!app.read(cx).security_restore);
    });
}

#[gpui_kit::test]
fn returning_to_the_system_scheme_follows_the_os_again(cx: &mut TestAppContext) {
    use crate::preferences::{Appearance, Scheme};
    use gpui_kit::component::ActiveTheme;
    cx.update(gpui_kit::init);
    cx.update(|cx| {
        let os_dark = matches!(
            cx.window_appearance(),
            gpui_kit::WindowAppearance::Dark | gpui_kit::WindowAppearance::VibrantDark
        );
        // Force the opposite of the OS, then go back to System.
        let mut appearance = Appearance {
            scheme: if os_dark { Scheme::Light } else { Scheme::Dark },
            ..Appearance::default()
        };
        crate::theme::set_appearance(appearance.clone(), cx);
        assert_eq!(cx.theme().is_dark(), !os_dark);
        appearance.scheme = Scheme::System;
        crate::theme::set_appearance(appearance, cx);
        assert_eq!(cx.theme().is_dark(), os_dark);
    });
}

#[gpui_kit::test]
fn notification_rules_controls_keywords_room_modes_and_context_menu(cx: &mut TestAppContext) {
    use gpui_kit::test::{TestEventExt, TestQueryExt};
    use twayblade_core::notifications::{DefaultLevel, RoomClass, RoomMode};
    let (main_window, app) = preview_window(cx);
    cx.update_window(main_window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.open_settings(crate::ui::settings::Section::Notifications, cx)
        });
        window.render_frame(cx);
    })
    .unwrap();
    let settings = cx.update(|cx| app.read(cx).settings.as_ref().unwrap().0);
    cx.update_window(settings, |_, window, cx| {
        window.render_frame(cx);
        window.click("notify-encrypted-dm-mentions", cx);
        assert!(
            !window
                .find_all("notify-default-off")
                .iter()
                .any(|node| node.visible())
        );
        window
            .within("notification-keyword-entry")
            .click("input-state", cx);
        window.input("native", cx);
        window.click("add-notification-keyword", cx);
        window.render_frame(cx);
        window.click("remove-keyword-2", cx);
    })
    .unwrap();
    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(
            app.snapshot.notifications.defaults[RoomClass {
                encrypted: true,
                direct: true
            }
            .index()],
            DefaultLevel::Mentions
        );
        assert!(
            !app.snapshot
                .notifications
                .keywords
                .iter()
                .any(|word| word == "native")
        );
    });
    cx.update_window(main_window, |_, window, cx| {
        window.click("details", cx);
        window.render_frame(cx);
        window.click("room-notifications-mute", cx);
        window.render_frame(cx);
        app.update(cx, |app, _| {
            assert_eq!(
                app.snapshot.notifications.room_mode("general"),
                RoomMode::Mute
            );
            let room = app
                .snapshot
                .rooms
                .iter()
                .find(|room| room.id == "general")
                .unwrap();
            assert!(room.muted && room.unread == 0);
        });
        window.right_click("room-general", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(main_window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find_by_label("Mentions and keywords").visible());
        let point = window.find_by_label("All messages").bounds().center();
        window.pointer_move(point, None, cx);
        window.pointer_down(point, gpui_kit::MouseButton::Left, cx);
        window.pointer_up(point, gpui_kit::MouseButton::Left, cx);
    })
    .unwrap();
    cx.update(|cx| {
        assert_eq!(
            app.read(cx).snapshot.notifications.room_mode("general"),
            RoomMode::All
        )
    });
}

#[gpui_kit::test]
fn notification_preview_click_opens_thread_and_device_controls_stay_local(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.notification_sample = true;
            cx.notify();
        });
        window.render_frame(cx);
        window.click("sample-notification-open", cx);
        app.update(cx, |app, cx| {
            assert!(!app.notification_sample);
            assert_eq!(
                app.thread.as_ref().unwrap().root.event_id.as_deref(),
                Some("$sample-0:preview.invalid")
            );
            app.activate_notification(
                twayblade_platform::notifications::Activation {
                    account: u128::MAX,
                    room: "research".into(),
                    thread: None,
                },
                window,
                cx,
            );
            assert_eq!(app.navigation.selected_room.as_deref(), Some("general"));
            app.open_settings(crate::ui::settings::Section::Notifications, cx);
        });
    })
    .unwrap();
    let settings = cx.update(|cx| app.read(cx).settings.as_ref().unwrap().0);
    cx.update_window(settings, |_, window, cx| {
        window.render_frame(cx);
        window.scroll(
            "settings-body",
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(-600.))),
            cx,
        );
        window.click("device-notifications", cx);
        window.click("notification-sound", cx);
        window.click("notification-preview", cx);
        window.click("notification-badge", cx);
    })
    .unwrap();
    cx.update(|cx| {
        let app = app.read(cx);
        let preferences = &app.preferences.notifications;
        assert!(
            preferences.enabled && !preferences.sound && !preferences.preview && !preferences.badge
        );
        assert!(app.backend.is_none());
    });
}

#[gpui_kit::test]
fn stacked_attachments_show_in_the_composer_and_can_be_removed(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.attachment_sample = true;
            cx.notify();
        })
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("attachment-tray").visible());
        assert!(window.find("attachment-sample-0").visible());
        assert!(window.find("attachment-sample-1").visible());
        window.click("attachment-remove-sample-0", cx);
    })
    .expect("attachment tray");
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find_all("attachment-tray").is_empty());
    })
    .expect("tray cleared");
}

#[gpui_kit::test]
fn emoji_button_toggles_the_picker_and_escape_closes_it(cx: &mut TestAppContext) {
    use gpui_kit::test::TestEventExt;
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("emoji-open", cx);
    })
    .expect("open picker");
    cx.update(|cx| assert!(app.read(cx).emoji_picker.is_some()));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("emoji-open", cx);
    })
    .expect("toggle picker closed");
    cx.update(|cx| assert!(app.read(cx).emoji_picker.is_none()));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("emoji-open", cx);
        window.render_frame(cx);
        window.key_down("escape", false, cx);
    })
    .expect("escape closes picker");
    cx.update(|cx| assert!(app.read(cx).emoji_picker.is_none()));
}

#[gpui_kit::test]
fn editing_loads_your_message_and_always_restores_the_draft(cx: &mut TestAppContext) {
    use gpui_kit::test::TestEventExt;
    let (window, app) = preview_window(cx);
    let ids = cx.update(|cx| {
        app.update(cx, |app, cx| {
            let user = app.snapshot.user_id.clone();
            let count = app.snapshot.messages.len();
            let mut ids = Vec::new();
            for (offset, body) in [(2, "first"), (1, "second")] {
                let message = &mut app.snapshot.messages[count - offset];
                message.sender = user.clone();
                message.kind = crate::backend::MessageKind::Text;
                message.attachment = None;
                message.body = body.into();
                message.editable = true;
                message.event_id = Some(format!("$edit-{body}"));
                ids.push(message.id.clone());
            }
            cx.notify();
            ids
        })
    });
    let composer_value = |window: &mut gpui_kit::Window| {
        window
            .within("composer-panel")
            .find_by_label("Message")
            .value()
            .map(|v| v.to_string())
    };
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.within("composer-panel").click("input-state", cx);
        window.key_down("up", false, cx);
        window.render_frame(cx);
        assert_eq!(composer_value(window).as_deref(), Some("second"));
        assert!(window.find("edit-bar").visible());
        // Down past your newest message returns to the (empty) draft.
        window.key_down("end", false, cx);
        window.key_down("down", false, cx);
        window.render_frame(cx);
        assert_eq!(composer_value(window).as_deref(), Some(""));
        assert!(window.find_all("edit-bar").is_empty());
        // Edit from the message actions keeps what you were typing.
        window.input("half-written reply", cx);
        window.click(gpui_kit::SharedString::from(format!("edit-{}", ids[0])), cx);
        window.render_frame(cx);
        assert_eq!(composer_value(window).as_deref(), Some("first"));
        window.key_down("escape", false, cx);
        window.render_frame(cx);
        assert_eq!(
            composer_value(window).as_deref(),
            Some("half-written reply")
        );
    })
    .expect("editing flow");
    cx.update(|cx| assert!(app.read(cx).edit_session.is_none()));

    // Emptying the message and pressing Return asks before deleting it.
    // Composer events reach the app after each window update, so every key
    // press gets its own update.
    let empty_and_return = |cx: &mut TestAppContext| {
        cx.update_window(window, |_, window, cx| {
            app.update(cx, |app, cx| {
                app.composer
                    .update(cx, |state, cx| state.set_value("", window, cx))
            });
            window.within("composer-panel").click("input-state", cx);
            window.key_down("enter", false, cx);
        })
        .expect("return");
        cx.run_until_parked();
    };
    let asking = |cx: &mut TestAppContext| {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            !window.find_all("edit-delete").is_empty()
        })
        .expect("render")
    };
    cx.update_window(window, |_, window, cx| {
        window.click(gpui_kit::SharedString::from(format!("edit-{}", ids[1])), cx);
        window.render_frame(cx);
    })
    .expect("edit");
    empty_and_return(cx);
    assert!(asking(cx), "asks before deleting");
    // Typing again keeps editing instead.
    cx.update_window(window, |_, window, cx| window.input("x", cx))
        .expect("type");
    cx.run_until_parked();
    assert!(!asking(cx), "typing withdraws the question");
    empty_and_return(cx);
    assert!(asking(cx));
    cx.update(|cx| assert!(app.read(cx).edit_session.is_some()));
    // A second Return deletes: in the preview that ends the edit.
    cx.update_window(window, |_, window, cx| window.key_down("enter", false, cx))
        .expect("delete");
    cx.run_until_parked();
    cx.update(|cx| assert!(app.read(cx).edit_session.is_none()));
}

#[gpui_kit::test]
fn the_message_menu_offers_actions_and_asks_before_deleting(cx: &mut TestAppContext) {
    use gpui_kit::test::{TestEventExt, TestQueryExt};
    let (window, app) = preview_window(cx);
    let id = cx.update(|cx| {
        app.update(cx, |app, cx| {
            let user = app.snapshot.user_id.clone();
            let message = app
                .snapshot
                .messages
                .iter_mut()
                .find(|m| m.kind == crate::backend::MessageKind::Text && m.attachment.is_none())
                .expect("a text message");
            message.sender = user;
            message.body = "menu target".into();
            message.editable = true;
            message.event_id = Some("$menu-target".into());
            let id = message.id.clone();
            cx.notify();
            id
        })
    });
    let open_menu = |cx: &mut TestAppContext| {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.right_click(
                gpui_kit::SharedString::from(format!("message-row-{id}")),
                cx,
            );
            window.render_frame(cx);
        })
        .expect("right click");
        cx.run_until_parked();
    };
    open_menu(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        for label in ["Add Reaction…", "Copy Text", "Copy Link", "Edit", "Delete…"] {
            assert!(window.find_by_label(label).visible(), "missing {label}");
        }
        let point = window.find_by_label("Delete…").bounds().center();
        window.pointer_move(point, None, cx);
        window.pointer_down(point, gpui_kit::MouseButton::Left, cx);
        window.pointer_up(point, gpui_kit::MouseButton::Left, cx);
    })
    .expect("choose delete");
    cx.run_until_parked();
    // Delete… asks first, exactly as an emptied edit does.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("edit-delete").visible(), "asks before deleting");
    })
    .expect("delete question");
    cx.update(|cx| {
        let app = app.read(cx);
        assert!(app.edit_session.as_ref().is_some_and(|s| s.confirm_delete));
        assert_eq!(app.composer.read(cx).value().to_string(), "");
    });
}

#[gpui_kit::test]
fn reply_names_the_message_and_can_be_cancelled(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    let id = cx.update(|cx| {
        app.read(cx)
            .snapshot
            .messages
            .iter()
            .find(|m| m.event_id.is_some() && m.sender != app.read(cx).snapshot.user_id)
            .map(|m| m.id.clone())
            .expect("someone else's message")
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click(gpui_kit::SharedString::from(format!("reply-to-{id}")), cx);
        window.render_frame(cx);
        assert!(window.find("reply-bar").visible(), "reply bar shown");
    })
    .expect("reply");
    cx.update(|cx| {
        let reply = app.read(cx).reply_to.as_ref().expect("replying");
        assert!(!reply.event_id.is_empty());
        assert!(!reply.sender.is_empty());
    });
    cx.update_window(window, |_, window, cx| {
        window.click("reply-cancel", cx);
        window.render_frame(cx);
        assert!(window.find_all("reply-bar").is_empty(), "reply cancelled");
    })
    .expect("cancel");
}

#[gpui_kit::test]
fn the_composer_shows_who_has_seen_the_latest_message(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    let seen = |cx: &mut TestAppContext| {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            !window.find_all("seen-by").is_empty()
        })
        .expect("render")
    };
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.snapshot.typing.clear();
            let reader = app.snapshot.members[0].user_id.clone();
            let latest = app
                .snapshot
                .messages
                .iter_mut()
                .rev()
                .find(|m| m.event_id.is_some())
                .expect("a sent message");
            latest.read_by = vec![reader];
            cx.notify();
        })
    });
    assert!(seen(cx), "a reader of the latest message is shown");
    // Someone typing takes the line instead.
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.snapshot.typing = app
                .snapshot
                .members
                .iter()
                .take(1)
                .map(|m| twayblade_core::room_state::TypingUser {
                    user_id: m.user_id.clone(),
                    name: m.display_name.clone(),
                    avatar: None,
                })
                .collect();
            cx.notify();
        })
    });
    assert!(!seen(cx), "typing replaces the read line");
}

#[gpui_kit::test]
fn a_thread_reply_has_its_own_attachments(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.open_thread("sample-0".into(), window, cx);
            // A file still being added to the thread's reply.
            app.thread.as_mut().unwrap().attachments.pending = 1;
            cx.notify();
        });
        window.render_frame(cx);
        assert!(window.find("thread-attach-file").visible());
        assert!(window.find("thread-attachment-pending-0").visible());
        // The main composer stays empty: the file belongs to the thread.
        assert!(window.find_all("attachment-pending-0").is_empty());
    })
    .expect("thread attachments");
    cx.update(|cx| {
        let app = app.read(cx);
        assert!(app.attachments.is_empty());
        assert!(!app.attachments(true).unwrap().is_empty());
    });
}

#[gpui_kit::test]
fn image_size_setting_updates_preferences(cx: &mut TestAppContext) {
    use twayblade_core::conversation::PreviewSize;
    let (_, app) = preview_window(cx);
    cx.update(|cx| {
        app.update(cx, |app, _| {
            app.set_timeline_preview_size(PreviewSize::Large);
            assert_eq!(app.preferences.chat.preview_size, PreviewSize::Large);
            app.set_timeline_preview_size(PreviewSize::Default);
            assert_eq!(app.preferences.chat.preview_size, PreviewSize::Default);
        })
    });
}

/// Technical adapter acceptance: uses Kit's public byte-selection API, including
/// an emoji selection, then exercises Kit's existing undo binding.
#[gpui_kit::test]
fn formatting_adapter_uses_byte_selection_and_preserves_undo(cx: &mut TestAppContext) {
    use gpui_kit::test::TestEventExt;
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.within("composer-panel").click("input-state", cx);
        let composer = app.read(cx).composer.clone();
        composer.update(cx, |state, cx| {
            state.set_value("a👩🏽‍💻z", window, cx);
            let end = 1 + "👩🏽‍💻".len();
            state.set_selected_range(1..end, cx);
            crate::composer_format::apply(crate::composer_format::Format::Bold, state, window, cx);
            assert_eq!(state.value().as_str(), "a**👩🏽‍💻**z");
            assert_eq!(state.selected_range(), 3..end + 2);
        });
        window.key_down(
            if cfg!(target_os = "macos") {
                "cmd-z"
            } else {
                "ctrl-z"
            },
            false,
            cx,
        );
        assert_eq!(composer.read(cx).value().as_str(), "a👩🏽‍💻z");
        window.key_down(
            if cfg!(target_os = "macos") {
                "cmd-shift-z"
            } else {
                "ctrl-y"
            },
            false,
            cx,
        );
        assert_eq!(composer.read(cx).value().as_str(), "a**👩🏽‍💻**z");
    })
    .expect("formatting adapter");
}

/// ⌘B / ⌘I go through the real bindings to whichever composer has focus.
#[gpui_kit::test]
fn formatting_shortcuts_reach_main_and_thread_composers(cx: &mut TestAppContext) {
    use gpui_kit::test::TestEventExt;
    let (window, app) = preview_window(cx);
    cx.update(|cx| cx.bind_keys(crate::shortcuts::bindings()));
    let (bold, italic) = if cfg!(target_os = "macos") {
        ("cmd-b", "cmd-i")
    } else {
        ("ctrl-b", "ctrl-i")
    };
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.within("composer-panel").click("input-state", cx);
        window.input("hello", cx);
        let composer = app.read(cx).composer.clone();
        composer.update(cx, |state, cx| state.set_selected_range(0..5, cx));
        window.key_down(bold, false, cx);
        assert_eq!(composer.read(cx).value().as_str(), "**hello**");

        window.click("thread-sample-0", cx);
        window.render_frame(cx);
        window
            .within("thread-composer-panel")
            .click("input-state", cx);
        window.input("hi", cx);
        let thread = app.read(cx).thread.as_ref().unwrap().composer.clone();
        thread.update(cx, |state, cx| state.set_selected_range(0..2, cx));
        window.key_down(italic, false, cx);
        assert_eq!(thread.read(cx).value().as_str(), "_hi_");
        // ⌘I in a composer formats; it no longer opens room details.
        assert_eq!(composer.read(cx).value().as_str(), "**hello**");
    })
    .expect("formatting shortcuts");
}

/// Help › Keyboard Shortcuts (⌘/) opens Settings at the Keyboard section.
#[gpui_kit::test]
fn keyboard_shortcuts_open_the_keyboard_settings(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.within("composer-panel").click("input-state", cx);
        window.dispatch_action(Box::new(crate::shortcuts::ShowKeyboardShortcuts), cx);
    })
    .unwrap();
    cx.run_until_parked();
    let (handle, view) = cx.update(|cx| app.read(cx).settings.clone().unwrap());
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.update(|cx| {
        assert_eq!(
            view.upgrade().unwrap().read(cx).section,
            crate::ui::settings::Section::Keyboard
        )
    });
}

#[gpui_kit::test]
fn failed_room_load_offers_try_again_instead_of_loading_text(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.preview = false;
            app.backend = Some(crate::backend::Backend::with_lock(None).expect("isolated backend"));
            let room = app.navigation.selected_room.clone().expect("sample room");
            app.snapshot.selected_room = room.clone();
            app.snapshot.messages.clear();
            app.groups.clear();
            app.snapshot.timeline_ready = false;
            app.snapshot.timeline_phase = twayblade_core::conversation::TimelineStatus::Loading;
            assert!(app.room_load(&room) == crate::ui::conversation::RoomLoad::Loading);
            app.snapshot.timeline_phase = twayblade_core::conversation::TimelineStatus::Failed;
            assert!(app.room_load(&room) == crate::ui::conversation::RoomLoad::Failed);
            cx.notify();
        })
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("room-retry", cx);
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn call_focus_button_toggles_one_person_large(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update(|cx| app.update(cx, |app, _| app.preview_call("--preview-call-group")));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("call-focus-@alex:preview.invalid-PREVIEW", cx);
    })
    .expect("focus a participant");
    let key = crate::ui::call_layout::CallFocus::Person(
        "@alex:preview.invalid".to_owned(),
        "PREVIEW".to_owned(),
    );
    cx.update(|cx| assert_eq!(app.read(cx).call_focus.as_ref(), Some(&key)));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("call-focus-@alex:preview.invalid-PREVIEW", cx);
    })
    .expect("release focus");
    cx.update(|cx| assert!(app.read(cx).call_focus.is_none()));
}

#[gpui_kit::test]
fn shared_screen_can_be_focused_and_released(cx: &mut TestAppContext) {
    use crate::ui::call_layout::CallFocus;
    let (window, app) = preview_window(cx);
    cx.update(|cx| app.update(cx, |app, _| app.preview_call("--preview-call-screen")));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("call-screen-focus", cx);
    })
    .expect("focus the shared screen");
    cx.update(|cx| {
        assert_eq!(
            app.read(cx).call_focus,
            Some(CallFocus::Screen("preview-screen-alex".into()))
        )
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("call-screen-focus", cx);
    })
    .expect("release the shared screen");
    cx.update(|cx| assert!(app.read(cx).call_focus.is_none()));
}

#[gpui_kit::test]
fn follow_speaker_toggle_changes_the_preference(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update(|cx| app.update(cx, |app, _| app.preview_call("--preview-call-group")));
    cx.update(|cx| assert!(!app.read(cx).preferences.calls.follow_speaker));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("call-follow", cx);
    })
    .expect("turn follow on");
    cx.update(|cx| assert!(app.read(cx).preferences.calls.follow_speaker));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("call-follow", cx);
    })
    .expect("turn follow off");
    cx.update(|cx| assert!(!app.read(cx).preferences.calls.follow_speaker));
}

#[gpui_kit::test]
fn volume_menu_belongs_to_other_people_and_shows_local_playback(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update(|cx| app.update(cx, |app, _| app.preview_call("--preview-call-group")));
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("render");
    cx.update(|cx| {
        let app = app.read(cx);
        // A slider for each other person; none for you.
        assert_eq!(app.call_volume.len(), 4);
        assert!(
            !app.call_volume
                .contains_key(&format!("{}:PREVIEW", app.snapshot.user_id))
        );
    });
    cx.update_window(window, |_, window, cx| {
        window.click("call-more-@alex:preview.invalid:PREVIEW", cx);
    })
    .expect("open the menu");
    cx.update(|cx| {
        assert_eq!(
            app.read(cx).call_menu.as_deref(),
            Some("@alex:preview.invalid:PREVIEW")
        )
    });
    cx.update(|cx| app.update(cx, |app, _| app.preview_call("--preview-call-volume")));
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("render the volume preview");
    cx.update(|cx| {
        let app = app.read(cx);
        let call = app.snapshot.call.as_ref().expect("call");
        let playback = |user: &str| {
            call.participants
                .iter()
                .find(|p| p.user_id == user)
                .expect("participant")
                .playback
        };
        assert!(playback("@sam:preview.invalid").muted);
        assert!((playback("@alex:preview.invalid").volume() - 0.6).abs() < 1e-6);
    });
}

#[gpui_kit::test]
fn call_full_screen_shows_only_the_call(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update(|cx| app.update(cx, |app, _| app.preview_call("--preview-call-fullscreen")));
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("render");
    cx.update(|cx| assert!(app.read(cx).call_fills_window()));
    // The floating controls are there, and the chat chrome is not.
    cx.update_window(window, |_, window, cx| {
        window.click("call-leave", cx);
    })
    .expect("controls are shown");
}

#[gpui_kit::test]
fn offline_banner_shows_only_without_a_connection(cx: &mut TestAppContext) {
    use gpui_kit::test::TestQueryExt;
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find_all("offline-banner").is_empty());
    })
    .expect("online");
    cx.update(|cx| app.update(cx, |app, _| app.preview_offline()));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find_by_label("No internet connection. Showing saved messages.")
                .visible()
        );
    })
    .expect("offline");
}

#[gpui_kit::test]
fn screen_picker_lists_sources_and_opens_from_the_share_button(cx: &mut TestAppContext) {
    use twayblade_core::calls::SourceKind;
    let (window, app) = preview_window(cx);
    cx.update(|cx| app.update(cx, |app, _| app.preview_call("--preview-call-picker")));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-picker-cancel").visible());
        assert!(window.find("share-source-1").visible());
        // Both screens show stills.
        assert_eq!(app.read(cx).screen_stills.borrow().len(), 2);
        window.click("screen-picker-windows", cx);
    })
    .expect("switch to windows");
    cx.update(|cx| assert_eq!(app.read(cx).screen_picker, Some(SourceKind::Window)));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        // The Windows tab keeps only its one still (the other window has
        // none) and released the screens'.
        assert_eq!(app.read(cx).screen_stills.borrow().len(), 1);
        window.click("screen-picker-cancel", cx);
    })
    .expect("cancel");
    cx.update(|cx| assert!(app.read(cx).screen_picker.is_none()));
    // Where the OS has no picker, Share opens ours instead of failing.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find_all("screen-picker-cancel").is_empty());
        // A closed chooser holds no stills.
        assert!(app.read(cx).screen_stills.borrow().is_empty());
        window.click("call-screen-share", cx);
    })
    .expect("open from the share button");
    cx.update(|cx| {
        assert_eq!(app.read(cx).screen_picker, Some(SourceKind::Screen));
        assert!(app.read(cx).status.is_empty());
    });
}

#[test]
fn known_people_come_from_direct_conversations_most_recent_first() {
    use crate::ui::spaces::known_people;
    let dm = |id: &str, name: &str, partner: &str, recency| crate::backend::Room {
        id: id.into(),
        name: name.into(),
        direct: true,
        partner: Some(partner.into()),
        recency,
        ..Default::default()
    };
    let rooms = vec![
        dm("a", "Alex", "@alex:example.org", 1),
        dm("s", "Sam", "@sam:example.org", 5),
        // A second conversation with Sam does not list Sam twice.
        dm("s2", "Sam", "@sam:example.org", 3),
        crate::backend::Room {
            id: "g".into(),
            name: "General".into(),
            ..Default::default()
        },
    ];
    let names = |people: Vec<crate::ui::spaces::KnownPerson>| {
        people.into_iter().map(|p| p.name).collect::<Vec<_>>()
    };
    assert_eq!(names(known_people(&rooms, "", &[], 6)), ["Sam", "Alex"]);
    assert_eq!(names(known_people(&rooms, "ALEX", &[], 6)), ["Alex"]);
    assert_eq!(names(known_people(&rooms, "sam:example", &[], 6)), ["Sam"]);
    // Members of the room being invited to are left out.
    assert_eq!(
        names(known_people(&rooms, "", &["@sam:example.org".into()], 6)),
        ["Alex"]
    );
    assert_eq!(names(known_people(&rooms, "", &[], 1)), ["Sam"]);
}

#[gpui_kit::test]
fn explore_lists_public_rooms_and_switches_servers(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| app.open_explore(window, cx));
        window.render_frame(cx);
        assert!(window.find("explore-room-!matrix:example.org").visible());
        // Joined rooms offer Open; knock-only rooms Ask to Join.
        assert!(window.find("explore-join-!design:example.org").visible());
        window.click("explore-server-matrix.org", cx);
    })
    .expect("explore");
    cx.update(|cx| assert_eq!(app.read(cx).explore_server.as_deref(), Some("matrix.org")));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("explore-join-!design:example.org", cx);
    })
    .expect("open a joined room");
    cx.update(|cx| {
        let app = app.read(cx);
        assert!(app.management.is_none());
        assert_eq!(
            app.navigation.selected_room.as_deref(),
            Some("!design:example.org")
        );
    });
}

#[gpui_kit::test]
fn direct_message_dialog_reopens_a_known_conversation(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.open_management(crate::ui::spaces::ManageKind::Dm, window, cx)
        });
        window.render_frame(cx);
        window.click("suggest-@alex:example.org", cx);
    })
    .expect("pick a known person");
    cx.update(|cx| {
        let app = app.read(cx);
        assert!(app.management.is_none());
        assert_eq!(app.navigation.selected_room.as_deref(), Some("alex"));
    });
}

#[gpui_kit::test]
fn status_message_dialog_opens_on_the_current_status(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.snapshot.own_presence = Some(twayblade_core::presence::Presence {
                state: twayblade_core::presence::State::Online,
                status_msg: Some("At the conference".into()),
                last_active_ms: None,
            });
            app.open_management(crate::ui::spaces::ManageKind::StatusMessage, window, cx)
        });
        window.render_frame(cx);
        assert!(window.find("status-save").visible());
        // With a status set, it can also be cleared from here.
        assert!(window.find("status-clear").visible());
        let page = app.read(cx).management.as_ref().unwrap();
        assert_eq!(page.name.read(cx).value(), "At the conference");
    })
    .expect("status dialog");
}

#[gpui_kit::test]
fn profile_picture_opens_full_size_and_closes(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.open_avatar_viewer("mxc://example.org/avatar".into(), "Alex".into(), window, cx)
        });
        window.render_frame(cx);
        // Offline there is no media service, so the picture reports failure
        // instead of loading forever.
        assert!(window.find("avatar-viewer").visible());
        assert!(matches!(
            app.read(cx).avatar_viewer.as_ref().map(|v| &v.load),
            Some(crate::ui::avatar_viewer::AvatarLoad::Failed)
        ));
        window.click("avatar-viewer-close", cx);
    })
    .expect("open and close");
    cx.update(|cx| assert!(app.read(cx).avatar_viewer.is_none()));
    // Only Matrix media URLs open.
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.open_avatar_viewer(
                "https://example.org/a.png".into(),
                "Alex".into(),
                window,
                cx,
            )
        });
    })
    .expect("refuse");
    cx.update(|cx| assert!(app.read(cx).avatar_viewer.is_none()));
}

#[gpui_kit::test]
fn invite_tile_opens_the_invite_dialog_for_the_selected_room(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("details", cx);
        window.render_frame(cx);
        window.click("info-invite", cx);
        window.render_frame(cx);
        assert!(window.find("invite-submit").visible());
    })
    .expect("open the invite dialog");
    cx.update(|cx| {
        let app = app.read(cx);
        let page = app.management.as_ref().expect("dialog");
        assert!(matches!(
            &page.kind,
            crate::ui::spaces::ManageKind::InviteMember(room) if *room == app.snapshot.selected_room
        ));
    });
}

#[gpui_kit::test]
fn leave_room_dialog_names_itself_and_waits_for_confirmation(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    let room = cx.update(|cx| app.read(cx).snapshot.selected_room.clone());
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.open_management(
                crate::ui::spaces::ManageKind::LeaveRoom(room.clone()),
                window,
                cx,
            )
        });
        window.render_frame(cx);
        // The confirm button exists but a preview never leaves a real room.
        assert!(window.find("leave-room-confirm").visible());
    })
    .expect("open the leave dialog");
    cx.update(|cx| {
        let page = app.read(cx).management.as_ref().expect("dialog");
        assert!(page.kind.is_dialog());
        assert_eq!(
            crate::app::MatrixApp::management_title(&page.kind),
            "Leave Room"
        );
    });
}

#[gpui_kit::test]
fn text_dialogs_focus_their_field_and_hand_focus_back_on_close(cx: &mut TestAppContext) {
    use gpui_kit::test::TestEventExt;
    let (window, app) = preview_window(cx);
    cx.update(|cx| cx.bind_keys(crate::shortcuts::bindings()));
    let composer_focused =
        |app: &gpui_kit::Entity<MatrixApp>, window: &gpui_kit::Window, cx: &gpui_kit::App| {
            let composer = app.read(cx).composer.clone();
            gpui::Focusable::focus_handle(composer.read(cx), cx).is_focused(window)
        };
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        for close in ["close-management", "escape"] {
            app.update(cx, |app, cx| {
                app.open_management(crate::ui::spaces::ManageKind::Dm, window, cx)
            });
            window.render_frame(cx);
            let (field, sheet) = {
                let page = app.read(cx).management.as_ref().expect("dialog");
                (page.name.clone(), page.focus.clone())
            };
            assert!(
                gpui::Focusable::focus_handle(field.read(cx), cx).is_focused(window),
                "the dialog's text field takes focus when it opens"
            );
            for _ in 0..6 {
                window.key_down("tab", false, cx);
                window.render_frame(cx);
                assert!(
                    sheet.contains_focused(window, cx),
                    "Tab stays inside the open dialog"
                );
            }
            if close == "escape" {
                window.key_down("escape", false, cx);
            } else {
                window.click(close, cx);
            }
            window.render_frame(cx);
            assert!(app.read(cx).management.is_none());
            assert!(
                composer_focused(&app, window, cx),
                "closing the dialog with {close} returns focus to the composer"
            );
        }
        // The same holds for a side panel closed with Escape.
        window.click("details", cx);
        window.render_frame(cx);
        assert!(app.read(cx).details_open);
        app.update(cx, |app, cx| app.focus.focus(window, cx));
        window.key_down("escape", false, cx);
        window.render_frame(cx);
        assert!(!app.read(cx).details_open);
        assert!(
            composer_focused(&app, window, cx),
            "closing a side panel with Escape returns focus to the composer"
        );
    })
    .expect("open and close the dialog");
}

#[gpui_kit::test]
fn archive_moves_a_room_out_of_its_section_and_lists_it_from_home(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        app.update(cx, |app, cx| {
            app.set_archived("announcements".into(), true, window, cx)
        });
        let space = app.read(cx).navigation.selected_space.clone();
        assert!(space.is_some(), "the sample opens inside a space");
        window.render_frame(cx);
        assert!(
            app.read(cx).collapsed_sections.contains("archive"),
            "the Archive starts collapsed"
        );
        // The avatar menu's Archive opens Home with the section expanded,
        // listing a room that lives in a space.
        app.update(cx, |app, cx| app.show_archive(window, cx));
        window.render_frame(cx);
        let row = app.update(cx, |app, cx| app.room_focus("announcements", cx));
        assert!(
            row.is_focused(window),
            "keyboard focus lands on the first archived room"
        );
        let app_state = app.read(cx);
        assert!(app_state.navigation.selected_space.is_none());
        assert!(!app_state.collapsed_sections.contains("archive"));
        assert!(
            app_state
                .navigation
                .room_visible("announcements", &app_state.snapshot),
            "Home shows archived rooms from every space"
        );
        // Favouriting restores it, as the SDK clears the other tag.
        app.update(cx, |app, cx| {
            app.set_favourite("announcements".into(), true, window, cx)
        });
        let room = app
            .read(cx)
            .snapshot
            .rooms
            .iter()
            .find(|room| room.id == "announcements")
            .cloned()
            .expect("room");
        assert!(room.favourite && !room.archived);
    })
    .expect("archive and restore");
}

#[gpui_kit::test]
fn invite_contact_copies_a_matrix_to_link(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        app.update(cx, |app, cx| {
            app.open_management(crate::ui::spaces::ManageKind::InviteContact, window, cx)
        });
        window.render_frame(cx);
        let user = app.read(cx).snapshot.user_id.clone();
        window.click("contact-copy", cx);
        window.render_frame(cx);
        assert_eq!(
            cx.read_from_clipboard().and_then(|item| item.text()),
            Some(format!("https://matrix.to/#/{user}"))
        );
        assert!(app.read(cx).management.as_ref().expect("dialog").copied);
    })
    .expect("copy the contact link");
}

fn two_accounts(app: &mut MatrixApp) {
    let me = app.snapshot.user_id.clone();
    app.snapshot.accounts = vec![
        crate::backend::AccountEntry {
            id: "one".into(),
            user_id: me,
            homeserver: "https://matrix.example.org/".into(),
            active: true,
            ..Default::default()
        },
        crate::backend::AccountEntry {
            id: "two".into(),
            user_id: "@sam:example.com".into(),
            homeserver: "https://example.com/".into(),
            active: false,
            ..Default::default()
        },
    ];
}

#[gpui_kit::test]
fn add_account_sheet_keeps_the_workspace_and_cancels_with_escape(cx: &mut TestAppContext) {
    use gpui_kit::test::TestEventExt;
    let (window, app) = preview_window(cx);
    cx.update(|cx| cx.bind_keys(crate::shortcuts::bindings()));
    let user = cx.update(|cx| app.read(cx).snapshot.user_id.clone());
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            two_accounts(app);
            app.begin_add_account(window, cx)
        });
        window.render_frame(cx);
        assert!(window.find("add-account-backdrop").visible());
        assert!(
            window.find("cancel-add-account").visible(),
            "the server step offers Cancel instead of Back"
        );
        assert!(window.try_find("back-to-welcome").is_none());
        // Escape closes the sheet; the open account is untouched.
        window.key_down("escape", false, cx);
        window.render_frame(cx);
        assert!(window.try_find("add-account-backdrop").is_none());
    })
    .expect("open and cancel the sheet");
    cx.update(|cx| {
        let app = app.read(cx);
        assert!(!app.adding_account);
        assert!(app.options.is_none());
        assert_eq!(app.snapshot.user_id, user);
    });
}

#[gpui_kit::test]
fn add_account_options_step_offers_cancel_next_to_sign_in(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.begin_add_account(window, cx);
            app.choosing_server = false;
            app.options = Some(crate::backend::LoginOptions {
                homeserver: "matrix.example.org".into(),
                password: true,
                browser: true,
                sso: false,
                providers: Vec::new(),
            });
        });
        window.render_frame(cx);
        assert!(window.find("browser-sign-in").visible());
        assert!(window.find("add-account-backdrop").visible());
        assert!(window.find("cancel-add-account").visible());
    })
    .expect("options step");
}

#[gpui_kit::test]
fn sign_out_dialog_names_the_account_and_mentions_other_accounts(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            two_accounts(app);
            app.open_management(crate::ui::spaces::ManageKind::SignOut, window, cx)
        });
        window.render_frame(cx);
        assert!(window.find("sign-out-confirm").visible());
        assert!(window.find("sign-out-cancel").visible());
    })
    .expect("open the sign-out dialog");
    cx.update(|cx| {
        let page = app.read(cx).management.as_ref().expect("dialog");
        assert!(page.kind.is_dialog());
    });
}

#[test]
fn account_menu_uses_the_local_part_unless_it_is_ambiguous() {
    use crate::backend::AccountEntry;
    let entry = |user: &str| AccountEntry {
        user_id: user.into(),
        ..Default::default()
    };
    let one = [entry("@alice:example.org")];
    assert_eq!(
        crate::ui::account_label("@alice:example.org", &one),
        "alice"
    );
    let clash = [entry("@alice:example.org"), entry("@alice:matrix.org")];
    assert_eq!(
        crate::ui::account_label("@alice:example.org", &clash),
        "@alice:example.org"
    );
}

#[gpui_kit::test]
fn direct_downloads_offer_a_checked_installer_and_package_installs_do_not(cx: &mut TestAppContext) {
    use crate::backend::UpdateDownload;
    use twayblade_platform::updates::{Available, Download, DownloadError, Phase, Snapshot};
    let (_main_window, app) = preview_window(cx);
    let available = |command: Option<&str>| Snapshot {
        phase: Phase::Available,
        available: Some(Available {
            version: "9.0.0".into(),
            prerelease: false,
            changelog: String::new(),
            release_url: "https://github.com/zerw0/twayblade/releases/tag/v9.0.0".into(),
            upgrade_command: command.map(str::to_owned),
            download: command.is_none().then(|| Download {
                file: "Twayblade-9.0.0-arm64.dmg".into(),
                size: 10,
                url: String::new(),
                sums_url: String::new(),
            }),
        }),
        ..Default::default()
    };
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.snapshot.updater = available(None);
            app.open_settings(crate::ui::settings::Section::General, cx)
        })
    });
    let window = cx.update(|cx| app.read(cx).settings.as_ref().unwrap().0);
    let set = |cx: &mut TestAppContext, state: UpdateDownload| {
        cx.update(|cx| {
            app.update(cx, |app, cx| {
                app.snapshot.update_download = state;
                cx.notify();
            })
        })
    };
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("update-download").visible());
        assert!(window.try_find("update-open").is_none());
    })
    .expect("download offered");
    set(
        cx,
        UpdateDownload::Downloading {
            version: "9.0.0".into(),
            received: 5,
            total: 10,
        },
    );
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("update-download-progress").visible());
        assert!(window.try_find("update-download").is_none());
    })
    .expect("downloading");
    set(
        cx,
        UpdateDownload::Ready {
            version: "9.0.0".into(),
            path: "Twayblade-9.0.0-arm64.dmg".into(),
        },
    );
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("update-open").visible());
    })
    .expect("ready to open");
    set(cx, UpdateDownload::Failed(DownloadError::ChecksumMismatch));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.find("update-download").visible(),
            "a failed check can be retried"
        );
        assert!(window.try_find("update-open").is_none());
    })
    .expect("mismatch");
    // A package-managed install shows its command and offers no download.
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.snapshot.updater = available(Some("brew upgrade --cask twayblade"));
            app.snapshot.update_download = UpdateDownload::Idle;
            cx.notify();
        })
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("update-command").visible());
        assert!(window.try_find("update-download").is_none());
    })
    .expect("package manager");
}

#[gpui_kit::test]
fn selecting_composer_text_shows_a_format_bar_that_keeps_focus(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        let composer = app.read(cx).composer.clone();
        composer.update(cx, |state, cx| {
            state.set_value("ship the notes", window, cx);
            state.set_selected_range(5..8, cx);
            state.focus(window, cx);
        });
        // The bar anchors to the laid-out selection, so it appears once the
        // composer has been laid out.
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(window.find("selection-format-bar").visible());
        window.click("selection-format-0", cx);
        window.render_frame(cx);
        let state = composer.read(cx);
        assert_eq!(state.value().as_ref(), "ship **the** notes");
        assert!(
            gpui::Focusable::focus_handle(state, cx).is_focused(window),
            "the composer keeps focus"
        );
        assert!(
            !state.selected_range().is_empty(),
            "the selection stays for another format"
        );
        // Collapsing the selection hides the bar.
        composer.update(cx, |state, cx| state.set_selected_range(0..0, cx));
        window.render_frame(cx);
        assert!(window.try_find("selection-format-bar").is_none());
    })
    .expect("format bar");
}

#[gpui_kit::test]
fn bell_opens_notifications_and_a_row_opens_its_room(cx: &mut TestAppContext) {
    use gpui_kit::test::TestEventExt;
    let (window, app) = preview_window(cx);
    cx.update(|cx| cx.bind_keys(crate::shortcuts::bindings()));
    let target = cx.update(|cx| {
        app.update(cx, |app, _| {
            let room = app
                .snapshot
                .rooms
                .iter_mut()
                .find(|room| !room.is_space && room.id != app.snapshot.selected_room)
                .expect("another room");
            room.mentions = 1;
            room.unread = 1;
            room.id.clone()
        })
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("notifications-popover").is_none());
        window.click("open-notifications", cx);
        window.render_frame(cx);
        assert!(window.find("notifications-popover").visible());
        // Escape closes it.
        window.key_down("escape", false, cx);
        window.render_frame(cx);
        assert!(window.try_find("notifications-popover").is_none());
    })
    .expect("open and close");
    let row = format!("activity-room-{target}");
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.notifications_closed_at = None;
            app.toggle_notifications(window, cx)
        });
        window.render_frame(cx);
        window.click(gpui_kit::SharedString::from(row.clone()), cx);
        window.render_frame(cx);
        assert!(window.try_find("notifications-popover").is_none());
    })
    .expect("open a room from the popover");
    cx.update(|cx| {
        let app = app.read(cx);
        assert!(!app.notifications_open);
        assert_eq!(
            app.navigation.selected_room.as_deref(),
            Some(target.as_str())
        );
    });
}

#[gpui_kit::test]
fn f6_moves_between_areas_and_each_timeline_is_one_tab_stop(cx: &mut TestAppContext) {
    use gpui_kit::test::TestEventExt;
    let (window, app) = preview_window(cx);
    cx.update(|cx| cx.bind_keys(crate::shortcuts::bindings()));
    cx.update_window(window, |_, window, cx| {
        let press = |key: &str, window: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
            window.key_down(key, false, cx);
            window.render_frame(cx);
        };
        window.render_frame(cx);
        let (room, ids) = {
            let app = app.read(cx);
            let ids: Vec<String> = app.snapshot.messages.iter().map(|m| m.id.clone()).collect();
            (
                app.navigation.selected_room.clone().expect("open room"),
                ids,
            )
        };
        let row = |id: &str, cx: &mut gpui_kit::App| {
            app.update(cx, |app, cx| app.message_focus(false, id, cx))
        };
        app.update(cx, |app, cx| app.focus.focus(window, cx));
        window.render_frame(cx);

        // F6 from the title bar: the open conversation's row in the sidebar.
        press("f6", window, cx);
        let room_row = app.update(cx, |app, cx| app.room_focus(&room, cx));
        assert!(room_row.is_focused(window), "F6 starts at the sidebar");
        // Then the latest message.
        press("f6", window, cx);
        let latest = ids.last().unwrap();
        assert!(
            row(latest, cx).is_focused(window),
            "F6 lands on the latest message"
        );
        // Arrows move through messages; Home goes to the first loaded one.
        press("up", window, cx);
        let previous = &ids[ids.len() - 2];
        assert!(
            row(previous, cx).is_focused(window),
            "↑ moves to the previous message"
        );
        press("home", window, cx);
        assert!(
            row(&ids[0], cx).is_focused(window),
            "Home moves to the first message"
        );
        // Then the message field, and back: the timeline remembers its message.
        press("f6", window, cx);
        let composer = app.read(cx).composer.clone();
        assert!(
            gpui::Focusable::focus_handle(composer.read(cx), cx).is_focused(window),
            "F6 reaches the message field"
        );
        press("shift-f6", window, cx);
        assert!(
            row(&ids[0], cx).is_focused(window),
            "⇧F6 returns to the message last moved to"
        );
        press("f6", window, cx);
        press("f6", window, cx);
        assert!(
            room_row.is_focused(window),
            "F6 wraps from the message field to the sidebar"
        );

        // Tab from the message: its own controls, then out of the timeline;
        // no other message's controls are on the way.
        press("shift-f6", window, cx);
        press("shift-f6", window, cx);
        assert!(row(&ids[0], cx).is_focused(window));
        let area = app.read(cx).timeline_area.clone();
        let mut controls = 0;
        let mut reached_thread = false;
        loop {
            press("tab", window, cx);
            if !area.contains_focused(window, cx) {
                break;
            }
            controls += 1;
            reached_thread |= window.find("thread-sample-0").focused() == Some(true);
            let other = app
                .read(cx)
                .image_focus
                .borrow()
                .iter()
                .find(|(key, handle)| handle.is_focused(window) && !key.ends_with("sample-0"))
                .map(|(key, _)| key.clone());
            assert_eq!(other, None, "Tab reached another message's control");
            assert!(
                controls < 10,
                "Tab must leave the timeline after one message"
            );
        }
        assert!(
            reached_thread,
            "the focused message's controls are reachable with Tab"
        );
    })
    .expect("area and message keyboard navigation");
}

#[gpui_kit::test]
fn opening_the_archive_scrolls_a_long_sidebar_to_it(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        app.update(cx, |app, cx| {
            // Enough rooms to push the Archive below the sidebar's bottom edge.
            let template = app
                .snapshot
                .rooms
                .iter()
                .find(|room| !room.is_space && !room.direct)
                .cloned()
                .expect("a sample room");
            for ix in 0..60 {
                let mut room = template.clone();
                room.id = format!("filler-{ix}");
                room.name = format!("Filler {ix}");
                room.favourite = false;
                app.snapshot.rooms.push(room);
            }
            app.set_archived("announcements".into(), true, window, cx);
            app.show_archive(window, cx);
        });
        window.render_frame(cx);
        window.render_frame(cx);
        let height = window.viewport_size().height;
        let row = window.find("room-announcements").bounds();
        assert!(
            row.top() >= gpui_kit::px(0.) && row.bottom() <= height,
            "the archived room is scrolled into view: {row:?} in a {height:?} window"
        );
    })
    .expect("archive scroll");
}

#[gpui_kit::test]
fn the_thread_panel_fits_its_region(cx: &mut TestAppContext) {
    // Narrow enough that the region's 32% cap is below the panel's 360 px.
    let (window, app) = preview_window_sized(cx, size(px(1000.), px(760.)));
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| app.open_thread("sample-0".into(), window, cx));
        window.render_frame(cx);
        let panel = window.find("close-thread").bounds();
        let region = window.find("panel-thread-region").bounds();
        // The region holds the panel at the width it allows (32% of the
        // window here), not a fraction of it and not more.
        assert!(
            region.size.width > px(300.),
            "thread panel squeezed: {region:?}"
        );
        assert!(
            panel.right() <= region.right(),
            "close button cut off: {panel:?} past {region:?}"
        );
        // The close button sits at the panel's trailing edge, so it ends
        // near the region's: a squeezed panel leaves it far from there.
        assert!(
            region.right() - panel.right() < px(40.),
            "panel narrower than its region: close at {panel:?}, region {region:?}"
        );
    })
    .expect("thread panel");
}

#[gpui_kit::test]
fn switching_back_to_an_account_reopens_its_last_room(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    let switch = |cx: &mut TestAppContext, user: Option<&str>| {
        cx.update_window(window, |_, window, cx| {
            app.update(cx, |app, cx| {
                let mut snapshot = app.snapshot.clone();
                if let Some(user) = user {
                    snapshot.user_id = user.into();
                }
                snapshot.account_instance = snapshot.account_instance.wrapping_add(1);
                snapshot.revision += 1;
                app.apply(snapshot, window, cx);
            });
        })
        .expect("switch account");
        cx.run_until_parked();
    };
    let own = cx.update(|cx| app.read(cx).snapshot.user_id.clone());
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| app.select_room("critique".into(), window, cx));
    })
    .expect("open a room");
    // Another account starts with nothing open: it never showed a room.
    switch(cx, Some("@other:preview.invalid"));
    assert_eq!(
        cx.update(|cx| app.read(cx).navigation.selected_room.clone()),
        None
    );
    // Back on the first account, its room opens again.
    switch(cx, Some(&own));
    assert_eq!(
        cx.update(|cx| app.read(cx).navigation.selected_room.clone()),
        Some("critique".into())
    );
}

#[gpui_kit::test]
fn a_click_for_another_account_opens_its_room_after_the_switch(cx: &mut TestAppContext) {
    use twayblade_platform::notifications::Activation;
    let (window, app) = preview_window(cx);
    let (own, open) = cx.update(|cx| {
        let app = app.read(cx);
        (app.snapshot.user_id.clone(), app.snapshot.account_instance)
    });
    let other = open.wrapping_add(7);
    let all_rooms = cx.update(|cx| app.read(cx).snapshot.rooms.clone());
    let accounts = move |active_other: bool| {
        vec![
            crate::backend::AccountEntry {
                id: "own".into(),
                active: !active_other,
                instance: open,
                ..Default::default()
            },
            crate::backend::AccountEntry {
                id: "other".into(),
                active: active_other,
                instance: other,
                ..Default::default()
            },
        ]
    };
    let land = |cx: &mut TestAppContext, user: &str, instance: u128, loaded: bool| {
        let user = user.to_owned();
        cx.update_window(window, |_, window, cx| {
            app.update(cx, |app, cx| {
                let mut snapshot = app.snapshot.clone();
                snapshot.user_id = user;
                snapshot.account_instance = instance;
                snapshot.accounts = accounts(instance == other);
                // Before the list loads: no rooms and no spaces yet.
                snapshot.rooms = if loaded {
                    all_rooms.clone()
                } else {
                    Vec::new()
                };
                snapshot.spaces_loaded = loaded;
                snapshot.revision += 1;
                app.apply(snapshot, window, cx);
            });
        })
        .expect("snapshot");
        cx.run_until_parked();
    };
    // Each account has shown a room before.
    land(cx, &own, open, true);
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| app.select_room("critique".into(), window, cx));
    })
    .unwrap();
    land(cx, "@other:preview.invalid", other, true);
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| app.select_room("research".into(), window, cx));
    })
    .unwrap();
    land(cx, &own, open, true);
    // A click for the other account's "announcements" switches to it.
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.activate_notification(
                Activation {
                    account: other,
                    room: "announcements".into(),
                    thread: None,
                },
                window,
                cx,
            )
        });
    })
    .unwrap();
    // The switch lands in steps: first no rooms, then the list, then more
    // updates, which must not close the room again.
    land(cx, "@other:preview.invalid", other, false);
    land(cx, "@other:preview.invalid", other, true);
    land(cx, "@other:preview.invalid", other, true);
    let (room, space) = cx.update(|cx| {
        let navigation = &app.read(cx).navigation;
        (
            navigation.selected_room.clone(),
            navigation.selected_space.clone(),
        )
    });
    assert_eq!(
        room,
        Some("announcements".into()),
        "the clicked room wins over the account's last room"
    );
    // "announcements" belongs to a space, so it opens there, not in Home.
    assert!(space.is_some(), "opened in its space");
}

#[gpui_kit::test]
fn a_notification_from_another_account_switches_and_then_opens_the_room(cx: &mut TestAppContext) {
    use twayblade_platform::notifications::Activation;
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let (room, open) = app.update(cx, |app, _| {
            let open = app.snapshot.account_instance;
            app.snapshot.accounts = vec![
                crate::backend::AccountEntry {
                    id: "open".into(),
                    active: true,
                    instance: open,
                    ..Default::default()
                },
                crate::backend::AccountEntry {
                    id: "other".into(),
                    instance: open.wrapping_add(1),
                    unread: 2,
                    ..Default::default()
                },
            ];
            let room = app
                .snapshot
                .rooms
                .iter()
                .find(|room| {
                    !room.is_space && Some(&room.id) != app.navigation.selected_room.as_ref()
                })
                .expect("another room")
                .id
                .clone();
            (room, open)
        });
        let target = Activation {
            account: open.wrapping_add(1),
            room: room.clone(),
            thread: None,
        };
        app.update(cx, |app, cx| {
            app.activate_notification(target.clone(), window, cx)
        });
        assert_eq!(
            app.read(cx)
                .pending_activation
                .as_ref()
                .map(|t| t.room.clone()),
            Some(room.clone()),
            "the click waits for the switch"
        );
        assert_ne!(app.read(cx).navigation.selected_room.as_ref(), Some(&room));
        // The switch lands: this window now shows the other account.
        app.update(cx, |app, cx| {
            app.busy = false;
            app.snapshot.account_instance = open.wrapping_add(1);
            app.resolve_pending_activation(window, cx);
        });
        assert_eq!(app.read(cx).navigation.selected_room.as_ref(), Some(&room));
        assert!(app.read(cx).pending_activation.is_none());

        // A click for an account that is not signed in does nothing.
        app.update(cx, |app, cx| {
            app.activate_notification(
                Activation {
                    account: 12345,
                    room: room.clone(),
                    thread: None,
                },
                window,
                cx,
            )
        });
        assert!(app.read(cx).pending_activation.is_none());
        // A switch that ends on another account forgets the click.
        app.update(cx, |app, cx| {
            app.pending_activation = Some(target.clone());
            app.snapshot.account_instance = open;
            app.busy = false;
            app.resolve_pending_activation(window, cx);
        });
        assert!(app.read(cx).pending_activation.is_none());
    })
    .expect("cross-account notification");
}

#[gpui_kit::test]
fn the_popover_lists_missed_calls_previews_and_marks_all_read(cx: &mut TestAppContext) {
    let (window, app) = preview_window(cx);
    let target = cx.update(|cx| {
        app.update(cx, |app, _| {
            let room = app
                .snapshot
                .rooms
                .iter_mut()
                .find(|room| !room.is_space && room.id != app.snapshot.selected_room)
                .expect("another room");
            room.unread = 2;
            let id = room.id.clone();
            app.snapshot.missed_calls = vec![crate::backend::MissedCall {
                room: id.clone(),
                room_name: "Sample".into(),
                caller: "@alex:example.org".into(),
                caller_name: "Alex".into(),
                video: false,
                at: 1,
            }];
            app.snapshot.activity_previews.insert(
                id.clone(),
                twayblade_core::activity::RoomPreview {
                    room: id.clone(),
                    sender: "@alex:example.org".into(),
                    sender_name: "Alex".into(),
                    own: false,
                    body: twayblade_core::activity::PreviewBody::Text("Sample text".into()),
                    timestamp_ms: 1,
                },
            );
            id
        })
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("open-notifications", cx);
        window.render_frame(cx);
        assert!(
            window
                .find(gpui_kit::SharedString::from(format!(
                    "activity-missed-{target}-1"
                )))
                .visible()
        );
        assert!(
            window
                .find(gpui_kit::SharedString::from(format!(
                    "activity-room-{target}"
                )))
                .visible()
        );
        window.click("notifications-mark-read", cx);
        window.render_frame(cx);
        // With nothing left the popover says so and offers no Mark All.
        assert!(
            window
                .try_find(gpui_kit::SharedString::from(format!(
                    "activity-room-{target}"
                )))
                .is_none()
        );
        assert!(window.try_find("notifications-mark-read").is_none());
    })
    .expect("mark all read");
    cx.update(|cx| {
        let app = app.read(cx);
        assert!(app.snapshot.missed_calls.is_empty());
        assert!(app.snapshot.rooms.iter().all(|room| room.unread == 0));
    });
}

#[gpui_kit::test]
fn an_available_update_shows_a_title_bar_notice_that_opens_settings(cx: &mut TestAppContext) {
    use twayblade_platform::updates::{Available, Phase, Snapshot};
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("update-available").is_none(),
            "no notice while up to date"
        );
        app.update(cx, |app, cx| {
            app.snapshot.updater = Snapshot {
                phase: Phase::Available,
                available: Some(Available {
                    version: "9.0.0".into(),
                    prerelease: false,
                    changelog: String::new(),
                    release_url: String::new(),
                    upgrade_command: None,
                    download: None,
                }),
                ..Default::default()
            };
            cx.notify();
        });
        window.render_frame(cx);
        window.click("update-available", cx);
    })
    .expect("update notice");
    cx.update(|cx| {
        let app = app.read(cx);
        assert!(app.settings.is_some(), "the notice opens Settings");
    });
}

fn mouse_navigate(
    window: &mut gpui_kit::Window,
    point: gpui_kit::Point<gpui_kit::Pixels>,
    direction: gpui_kit::NavigationDirection,
    cx: &mut gpui_kit::App,
) {
    use gpui_kit::test::TestEventExt;
    window.render_frame(cx);
    window.pointer_move(point, None, cx);
    window.pointer_down(point, gpui_kit::MouseButton::Navigate(direction), cx);
}

#[gpui_kit::test]
fn mouse_back_and_forward_navigate_across_spaces_and_skip_invalid_entries(cx: &mut TestAppContext) {
    use gpui_kit::{NavigationDirection, point};
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(!app.read(cx).can_navigate(true));
        assert!(!app.read(cx).can_navigate(false));

        // Design space: #general -> #critique -> Home -> #notes.
        window.click("room-critique", cx);
        window.click("home", cx);
        window.click("room-notes", cx);
        window.render_frame(cx);
        assert!(app.read(cx).can_navigate(true));

        // Mouse Back from Home #notes skips the empty Home transition and returns
        // straight to Design space #critique.
        mouse_navigate(
            window,
            point(px(500.), px(300.)),
            NavigationDirection::Back,
            cx,
        );
        window.render_frame(cx);
    })
    .expect("mouse back across spaces");
    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(app.navigation.selected_space.as_deref(), Some("design"));
        assert_eq!(app.navigation.selected_room.as_deref(), Some("critique"));
    });

    // A subsequent snapshot update must keep the restored space+room rather than wiping it.
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            let mut snapshot = app.snapshot.clone();
            snapshot.revision += 1;
            app.apply(snapshot, window, cx);
        });
        window.render_frame(cx);
    })
    .expect("snapshot reconcile keeps restored cross-space selection");
    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(app.navigation.selected_space.as_deref(), Some("design"));
        assert_eq!(app.navigation.selected_room.as_deref(), Some("critique"));
    });

    // Mouse Forward returns to Home #notes; Back returns to #critique, then #general.
    cx.update_window(window, |_, window, cx| {
        mouse_navigate(
            window,
            point(px(500.), px(300.)),
            NavigationDirection::Forward,
            cx,
        );
        window.render_frame(cx);
    })
    .expect("mouse forward across spaces");
    cx.update(|cx| {
        let app = app.read(cx);
        assert!(app.navigation.selected_space.is_none());
        assert_eq!(app.navigation.selected_room.as_deref(), Some("notes"));
    });

    cx.update_window(window, |_, window, cx| {
        // The compact title bar has no Back button; ⌘[ goes back.
        app.update(cx, |app, cx| app.focus.focus(window, cx));
        window.dispatch_action(Box::new(crate::app::NavigateBack), cx);
        window.render_frame(cx);
    })
    .expect("keyboard back to critique");
    cx.update(|cx| {
        assert_eq!(
            app.read(cx).navigation.selected_room.as_deref(),
            Some("critique")
        );
    });

    // Remove #general from the workspace; nav-back must reflect there is no valid back entry.
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.snapshot.rooms.retain(|room| room.id != "general");
            cx.notify();
        });
        window.render_frame(cx);
        assert!(!app.read(cx).can_navigate(true));
        app.update(cx, |app, cx| app.focus.focus(window, cx));
        window.render_frame(cx);
        window.dispatch_action(Box::new(crate::app::NavigateForward), cx);
        window.render_frame(cx);
    })
    .expect("invalid entry disables nav-back and action navigates forward");
    cx.update(|cx| {
        assert_eq!(
            app.read(cx).navigation.selected_room.as_deref(),
            Some("notes")
        );
    });
}

#[gpui_kit::test]
fn back_and_forward_navigate_pages_panels_and_prune_on_close(cx: &mut TestAppContext) {
    use gpui_kit::{NavigationDirection, point};
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        // Visit #critique -> open thread -> mouse Back closes thread -> Forward reopens it.
        window.click("room-critique", cx);
        app.update(cx, |app, cx| {
            app.open_thread_event("$critique-root:preview.invalid".into(), window, cx);
        });
        window.render_frame(cx);
        assert!(window.find("close-thread").visible());

        mouse_navigate(
            window,
            point(px(500.), px(300.)),
            NavigationDirection::Back,
            cx,
        );
        window.render_frame(cx);
        assert!(window.try_find("close-thread").is_none());

        mouse_navigate(
            window,
            point(px(500.), px(300.)),
            NavigationDirection::Forward,
            cx,
        );
        window.render_frame(cx);
        assert!(window.find("close-thread").visible());

        // Clicking X explicitly dismisses the thread and prunes it from history_back so
        // the next Back returns to #general rather than reopening the thread.
        window.click("close-thread", cx);
        window.render_frame(cx);
        assert!(window.try_find("close-thread").is_none());
        mouse_navigate(
            window,
            point(px(500.), px(300.)),
            NavigationDirection::Back,
            cx,
        );
        window.render_frame(cx);
    })
    .expect("thread history and prune");
    cx.update(|cx| {
        assert_eq!(
            app.read(cx).navigation.selected_room.as_deref(),
            Some("general")
        );
    });

    // Search panel -> open search result in another room -> Back returns to search in #general -> Back closes search.
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.search
                .update(cx, |s, cx| s.set_value("matrix", window, cx));
            app.run_message_search(window, cx);
        });
        window.render_frame(cx);
        assert!(window.find("close-search").visible());

        app.update(cx, |app, cx| {
            app.open_event("research".into(), "$hit:preview.invalid".into(), window, cx);
        });
        window.render_frame(cx);
        assert_eq!(
            app.read(cx).navigation.selected_room.as_deref(),
            Some("research")
        );
        assert!(window.find("close-search").visible());

        mouse_navigate(
            window,
            point(px(500.), px(300.)),
            NavigationDirection::Back,
            cx,
        );
        window.render_frame(cx);
        assert_eq!(
            app.read(cx).navigation.selected_room.as_deref(),
            Some("general")
        );
        assert!(window.find("close-search").visible());

        mouse_navigate(
            window,
            point(px(500.), px(300.)),
            NavigationDirection::Back,
            cx,
        );
        window.render_frame(cx);
        assert!(window.try_find("close-search").is_none());
    })
    .expect("search panel history across rooms");

    // Profile -> Directory -> Nested Subspace Directory -> Back -> X prune.
    cx.update_window(window, |_, window, cx| {
        window.click("details", cx);
        window.click("info-tab-Members", cx);
        window.render_frame(cx);
        window.click("member-@alex:preview.invalid", cx);
        window.render_frame(cx);
        assert!(window.find("copy-profile-id").visible());

        mouse_navigate(
            window,
            point(px(500.), px(300.)),
            NavigationDirection::Back,
            cx,
        );
        window.render_frame(cx);
        assert!(window.try_find("copy-profile-id").is_none());

        app.update(cx, |app, cx| {
            app.open_management(
                crate::ui::spaces::ManageKind::Directory("design".into()),
                window,
                cx,
            );
        });
        window.render_frame(cx);
        window.click("browse-sample-nested", cx);
        window.render_frame(cx);
    })
    .expect("open nested directory");
    cx.update(|cx| {
        assert!(matches!(
            app.read(cx).management.as_ref().map(|m| &m.kind),
            Some(crate::ui::spaces::ManageKind::Directory(id)) if id == "sample-nested"
        ));
    });

    cx.update_window(window, |_, window, cx| {
        mouse_navigate(
            window,
            point(px(500.), px(300.)),
            NavigationDirection::Back,
            cx,
        );
        window.render_frame(cx);
    })
    .expect("back from nested directory");
    cx.update(|cx| {
        assert!(matches!(
            app.read(cx).management.as_ref().map(|m| &m.kind),
            Some(crate::ui::spaces::ManageKind::Directory(id)) if id == "design"
        ));
    });

    cx.update_window(window, |_, window, cx| {
        window.click("close-management", cx);
        window.render_frame(cx);
        assert!(!app.read(cx).can_navigate(true));
    })
    .expect("close-management prunes directory chain");
}

#[gpui_kit::test]
fn mouse_back_closes_viewer_and_modal_dialogs_before_workspace_history(cx: &mut TestAppContext) {
    use gpui_kit::{NavigationDirection, point};
    let (window, app) = preview_window(cx);
    let initial_messages = cx.update(|cx| app.read(cx).snapshot.messages.clone());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("room-critique", cx);
        app.update(cx, |app, cx| {
            app.snapshot.messages = initial_messages;
            app.preview_viewer(window, cx);
        });
        window.render_frame(cx);
        let centre = window.find("viewer-canvas").bounds().center();

        // Forward inside viewer is ignored; Back closes viewer without changing rooms.
        mouse_navigate(window, centre, NavigationDirection::Forward, cx);
        mouse_navigate(window, centre, NavigationDirection::Back, cx);
        window.render_frame(cx);
    })
    .expect("mouse back closes image viewer");
    cx.update(|cx| {
        let app = app.read(cx);
        assert!(app.viewer.is_none());
        assert_eq!(app.navigation.selected_room.as_deref(), Some("critique"));
    });

    // Open a modal dialog (New Room) over #critique; mouse Back closes only the dialog,
    // then another mouse Back navigates from #critique to #general.
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.open_management(crate::ui::spaces::ManageKind::Create, window, cx);
        });
        window.render_frame(cx);
        let dialog_centre = window.find("create-room-submit").bounds().center();
        mouse_navigate(window, dialog_centre, NavigationDirection::Forward, cx);
        mouse_navigate(window, dialog_centre, NavigationDirection::Back, cx);
        window.render_frame(cx);
    })
    .expect("mouse back closes modal dialog");
    cx.update(|cx| {
        let app = app.read(cx);
        assert!(app.management.is_none());
        assert_eq!(app.navigation.selected_room.as_deref(), Some("critique"));
    });

    // Opening the notifications popover or Add Account sheet intercepts mouse Back before
    // workspace room history is touched.
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.notifications_open = true;
            cx.notify();
        });
        window.render_frame(cx);
        mouse_navigate(
            window,
            point(px(500.), px(300.)),
            NavigationDirection::Back,
            cx,
        );
        window.render_frame(cx);
    })
    .expect("mouse back closes notifications popover");
    cx.update(|cx| {
        let app = app.read(cx);
        assert!(!app.notifications_open);
        assert_eq!(app.navigation.selected_room.as_deref(), Some("critique"));
    });

    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.begin_add_account(window, cx);
            app.options = Some(crate::backend::LoginOptions {
                homeserver: "https://twayblade.example.org/".into(),
                password: true,
                browser: true,
                sso: false,
                providers: Vec::new(),
            });
            cx.notify();
        });
        window.render_frame(cx);
        assert!(window.find("browser-sign-in").visible());
        mouse_navigate(
            window,
            point(px(500.), px(300.)),
            NavigationDirection::Back,
            cx,
        );
        window.render_frame(cx);
        assert!(window.find("discover").visible());
        mouse_navigate(
            window,
            point(px(500.), px(300.)),
            NavigationDirection::Back,
            cx,
        );
        window.render_frame(cx);
        assert!(window.try_find("add-account-backdrop").is_none());
    })
    .expect("mouse back steps back and closes add-account sheet");
    cx.update(|cx| {
        let app = app.read(cx);
        assert!(!app.adding_account);
        assert_eq!(app.navigation.selected_room.as_deref(), Some("critique"));
    });

    cx.update_window(window, |_, window, cx| {
        mouse_navigate(
            window,
            point(px(500.), px(300.)),
            NavigationDirection::Back,
            cx,
        );
        window.render_frame(cx);
    })
    .expect("subsequent mouse back returns to general");
    cx.update(|cx| {
        assert_eq!(
            app.read(cx).navigation.selected_room.as_deref(),
            Some("general")
        );
    });
}

#[gpui_kit::test]
fn reply_quote_draft_survives_room_back_and_forward_without_colliding_with_escape(
    cx: &mut TestAppContext,
) {
    use gpui_kit::{NavigationDirection, point};
    let (window, app) = preview_window(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let msg = app
            .read(cx)
            .snapshot
            .messages
            .iter()
            .find(|m| m.event_id.is_some())
            .expect("message with event_id")
            .id
            .clone();
        app.update(cx, |app, cx| {
            app.start_reply(msg, window, cx);
        });
        window.render_frame(cx);
        assert!(window.find("reply-bar").visible());

        // Switching to #critique hides #general's reply bar; mouse Back restores #general with it intact.
        window.click("room-critique", cx);
        window.render_frame(cx);
        assert!(window.try_find("reply-bar").is_none());

        mouse_navigate(
            window,
            point(px(500.), px(300.)),
            NavigationDirection::Back,
            cx,
        );
        window.render_frame(cx);
        assert_eq!(
            app.read(cx).navigation.selected_room.as_deref(),
            Some("general")
        );
        assert!(window.find("reply-bar").visible());

        // Forward to #critique still keeps the quote scoped to #general, and Back returns cleanly.
        mouse_navigate(
            window,
            point(px(500.), px(300.)),
            NavigationDirection::Forward,
            cx,
        );
        window.render_frame(cx);
        assert!(window.try_find("reply-bar").is_none());
        mouse_navigate(
            window,
            point(px(500.), px(300.)),
            NavigationDirection::Back,
            cx,
        );
        window.render_frame(cx);
        assert!(window.find("reply-bar").visible());
    })
    .expect("reply quote preserved across back/forward navigation");
}

#[gpui_kit::test]
fn mouse_back_and_forward_handle_sign_in_and_session_reset(cx: &mut TestAppContext) {
    use gpui_kit::{NavigationDirection, point};
    let (window, app) = preview_window(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.snapshot = crate::backend::Snapshot::default();
            app.choosing_server = true;
            app.options = Some(crate::backend::LoginOptions {
                homeserver: "https://twayblade.example.org/".into(),
                password: true,
                browser: true,
                sso: false,
                providers: Vec::new(),
            });
            cx.notify();
        });
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("sign-in").visible());
        mouse_navigate(
            window,
            point(px(400.), px(300.)),
            NavigationDirection::Back,
            cx,
        );
        window.render_frame(cx);
        assert!(window.find("discover").visible());

        mouse_navigate(
            window,
            point(px(400.), px(300.)),
            NavigationDirection::Back,
            cx,
        );
        window.render_frame(cx);
        assert!(window.find("start-sign-in").visible());

        mouse_navigate(
            window,
            point(px(400.), px(300.)),
            NavigationDirection::Forward,
            cx,
        );
        window.render_frame(cx);
        assert!(window.find("discover").visible());

        mouse_navigate(
            window,
            point(px(400.), px(300.)),
            NavigationDirection::Forward,
            cx,
        );
        window.render_frame(cx);
        assert!(window.find("sign-in").visible());
    })
    .expect("sign-in step back and forward");

    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.snapshot.session_state = twayblade_core::auth::SessionState::HardLoggedOut;
            app.confirm_session_reset = true;
            cx.notify();
        });
        window.render_frame(cx);
        assert!(window.find("confirm-session-reset").visible());
        mouse_navigate(
            window,
            point(px(400.), px(300.)),
            NavigationDirection::Back,
            cx,
        );
        window.render_frame(cx);
        assert!(window.find("sign-in-again").visible());
    })
    .expect("mouse back cancels hard logout reset confirmation");
}

#[gpui_kit::test]
fn settings_window_mouse_back_and_forward_switch_sections_and_respect_security_guard(
    cx: &mut TestAppContext,
) {
    use crate::ui::settings::Section;
    use gpui_kit::{NavigationDirection, point};
    let (main_window, app) = preview_window(cx);
    cx.update_window(main_window, |_, window, cx| {
        window.render_frame(cx);
        app.update(cx, |app, cx| app.open_settings(Section::Appearance, cx));
        window.render_frame(cx);
    })
    .unwrap();
    let settings = cx.update(|cx| app.read(cx).settings.as_ref().unwrap().0);
    let view = cx.update(|cx| app.read(cx).settings.as_ref().unwrap().1.upgrade().unwrap());

    cx.update_window(settings, |_, window, cx| {
        window.render_frame(cx);
        window.click("settings-Chat", cx);
        window.click("settings-Notifications", cx);
        window.click("settings-Security", cx);
        window.render_frame(cx);
        assert_eq!(view.read(cx).section, Section::Security);

        // Lock navigation while recovery key is unacknowledged.
        app.update(cx, |app, cx| {
            app.security_key = Some(zeroize::Zeroizing::new("ABCD 1234".into()));
            cx.notify();
        });
        mouse_navigate(
            window,
            point(px(400.), px(300.)),
            NavigationDirection::Back,
            cx,
        );
        assert_eq!(view.read(cx).section, Section::Security);

        // Once acknowledged, mouse Back walks Security -> Notifications -> Chat -> Appearance.
        app.update(cx, |app, cx| {
            app.security_key = None;
            cx.notify();
        });
        mouse_navigate(
            window,
            point(px(400.), px(300.)),
            NavigationDirection::Back,
            cx,
        );
        assert_eq!(view.read(cx).section, Section::Notifications);
        mouse_navigate(
            window,
            point(px(400.), px(300.)),
            NavigationDirection::Back,
            cx,
        );
        assert_eq!(view.read(cx).section, Section::Chat);
        mouse_navigate(
            window,
            point(px(400.), px(300.)),
            NavigationDirection::Back,
            cx,
        );
        assert_eq!(view.read(cx).section, Section::Appearance);

        // Forward walks back to Chat.
        mouse_navigate(
            window,
            point(px(400.), px(300.)),
            NavigationDirection::Forward,
            cx,
        );
        assert_eq!(view.read(cx).section, Section::Chat);
    })
    .expect("settings mouse back/forward");
}
