use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Bounds, TestAppContext, WindowBounds, WindowOptions, px, size};

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
    cx.update(|cx| assert!(app.read(cx).attachment_drafts.is_empty()));
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
    cx.update(|cx| cx.bind_keys(crate::ui::viewer::bindings()));
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
fn viewer_save_and_open_shortcuts_require_a_destination(cx: &mut TestAppContext) {
    use gpui_kit::test::TestEventExt;
    let (window, app) = preview_window(cx);
    cx.update(|cx| cx.bind_keys(crate::ui::viewer::bindings()));
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
        assert_eq!(main.preferences.appearance.scheme, crate::preferences::Scheme::Custom);
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
        assert_eq!(main.preferences.appearance.scheme, crate::preferences::Scheme::Custom);
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
            assert_eq!(main.scroller.read(cx).logical_scroll_top().item_ix, 0);
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
