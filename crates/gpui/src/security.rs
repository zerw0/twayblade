//! Security UI wording and interactions. Secrets never enter backend snapshots.
use crate::theme::scaled_px as px;
use crate::{
    app::MatrixApp,
    backend::{SecurityAction, SecurityOutcome},
};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{
    input::{Input, InputState},
    *,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use matrix_client_core::security::{VerificationPhase, VerificationView};

impl MatrixApp {
    pub(super) fn perform_security(
        &mut self,
        action: SecurityAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        if self.preview {
            match action {
                SecurityAction::Confirm => {
                    if let Some(flow) = &mut self.snapshot.security.verification {
                        flow.phase = VerificationPhase::Confirmed;
                    }
                }
                SecurityAction::Cancel(_) => {
                    if let Some(flow) = &mut self.snapshot.security.verification {
                        flow.phase = VerificationPhase::Cancelled;
                    }
                }
                _ => {}
            }
            cx.notify();
            return;
        }
        self.busy = true;
        self.status.clear();
        let expected = self.snapshot.account_instance;
        self.job(
            window,
            cx,
            move |backend| backend.security_action(expected, action),
            move |this, result, _, cx| {
                if this.snapshot.account_instance != expected {
                    return;
                }
                this.busy = false;
                match result {
                    Ok(SecurityOutcome::BrowserUrl(url)) => {
                        cx.open_url(&url);
                        this.security_device = None;
                        this.status = "Complete device sign-out in your browser, then refresh the device list.".into();
                    }
                    Ok(SecurityOutcome::Done) => {
                        this.security_restore = false;
                        this.security_device = None;
                        this.security_reset_confirmation = false;
                    }
                    Ok(SecurityOutcome::RecoveryKey(key)) => {
                        this.security_key = Some(key);
                        this.security_key_copied = false;
                        this.security_reset_confirmation = false;
                        this.security_open = true;
                    }
                    Err(error) => this.status = error.to_string(),
                }
                cx.notify();
            },
        );
    }

    fn take_security_input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> zeroize::Zeroizing<String> {
        let value = zeroize::Zeroizing::new(self.security_input.read(cx).value().to_string());
        // Replace the entity as well as clearing its visible value/undo lifetime.
        self.security_input
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.security_input = cx.new(|cx| InputState::new(window, cx).masked(true));
        value
    }

    pub(super) fn show_security_notice(&self) -> bool {
        let security = &self.snapshot.security;
        let incoming = security
            .verification
            .as_ref()
            .is_some_and(|flow| flow.incoming && flow.phase == VerificationPhase::Requested);
        // An incoming request is never hidden by an earlier dismissal.
        security.ready
            && (incoming
                || (!self.security_notice_dismissed
                    && (!security.verified || !security.recovery_enabled)))
    }

    /// A quiet one-line strip under the toolbar, like Mail's account notices.
    pub(super) fn security_notice(&self, cx: &mut Context<Self>) -> AnyElement {
        let security = &self.snapshot.security;
        let incoming = security
            .verification
            .as_ref()
            .is_some_and(|flow| flow.incoming && flow.phase == VerificationPhase::Requested);
        let text = if incoming {
            "Another device wants to verify with you."
        } else if !security.verified {
            "Verify this device to read your encrypted history."
        } else {
            "Set up secure backup to keep access to encrypted messages."
        };
        h_flex()
            .h(px(32.))
            .flex_none()
            .pl(px(14.))
            .pr(px(6.))
            .gap(px(8.))
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().secondary.opacity(0.5))
            .text_size(px(12.))
            .child(
                Icon::new(gpui_kit::assets::IconName::ShieldCheck)
                    .xsmall()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(div().flex_1().min_w_0().truncate().child(text))
            .child(
                Button::new("review-security")
                    .xsmall()
                    .label(if incoming { "Review…" } else { "Set Up…" })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.security_open = true;
                        cx.notify();
                    })),
            )
            .when(!incoming, |strip| {
                strip.child(
                    Button::new("dismiss-security-notice")
                        .ghost()
                        .xsmall()
                        .icon(gpui_kit::assets::IconName::X)
                        .accessibility_label("Dismiss")
                        .tooltip("Dismiss until next launch")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.security_notice_dismissed = true;
                            cx.notify();
                        })),
                )
            })
            .into_any_element()
    }

    pub(super) fn security_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let security = &self.snapshot.security;
        let muted = cx.theme().muted_foreground;
        let mut body = v_flex().id("security-content").w_full().gap(px(12.));
        if self.preview {
            body = body.child(
                div()
                    .text_color(muted)
                    .child("Sample data · Offline security preview"),
            );
        }
        if let Some(key) = &self.security_key {
            body = body.child(div().font_weight(FontWeight::SEMIBOLD).child("Save your recovery key"))
                .child(div().text_color(muted).child("This key is shown once. Keep it somewhere safe; anyone with it can recover your encrypted conversations."))
                .child(div().id("recovery-key").p(px(16.)).rounded(px(8.)).bg(cx.theme().secondary).child(key.as_str().to_owned()))
                .child(Button::new("copy-recovery-key").label("Copy recovery key").on_click(cx.listener(|this, _, _, cx| {
                    if let Some(key) = &this.security_key { cx.write_to_clipboard(ClipboardItem::new_string(key.as_str().to_owned())); this.security_key_copied = true; cx.notify(); }
                })))
                .child(Button::new("confirm-recovery-key").primary().label("I saved my recovery key").on_click(cx.listener(|this, _, _, cx| {
                    if let Some(key) = &this.security_key
                        && cx.read_from_clipboard().and_then(|item| item.text()).is_some_and(|text| text == key.as_str()) { cx.write_to_clipboard(ClipboardItem::new_string(String::new())); }
                    this.security_key = None; this.security_key_copied = false; cx.notify();
                })));
        } else if self.security_restore {
            body = body.child(div().font_weight(FontWeight::SEMIBOLD).child("Recover encrypted conversations"))
                .child(div().text_color(muted).child("Enter your recovery key or recovery passphrase to verify this device and restore backed-up room keys."))
                .child(div().child("Recovery key or passphrase"))
                .child(Input::new(&self.security_input).disabled(self.busy))
                .child(Button::new("restore-recovery").primary().label("Recover").disabled(self.busy || self.preview).on_click(cx.listener(|this, _, window, cx| {
                    let key = this.take_security_input(window, cx); this.perform_security(SecurityAction::Recover(key), window, cx);
                })))
                .child(Button::new("cancel-recovery").label("Back").disabled(self.busy).on_click(cx.listener(|this, _, window, cx| {
                    drop(this.take_security_input(window, cx)); this.security_restore = false; cx.notify();
                })));
        } else if let Some(id) = &self.security_device {
            if security.oauth_management {
                let id = id.clone();
                body = body
                    .child(div().child("Your server manages device sign-out in the browser."))
                    .child(
                        Button::new("browser-device-signout")
                            .label("Continue in browser")
                            .disabled(self.busy || self.preview)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.perform_security(
                                    SecurityAction::SignOutBrowser(id.clone()),
                                    window,
                                    cx,
                                )
                            })),
                    )
                    .child(
                        Button::new("cancel-device-signout")
                            .label("Cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.security_device = None;
                                cx.notify();
                            })),
                    );
                return div()
                    .id("security-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .flex()
                    .justify_center()
                    .child(body)
                    .into_any_element();
            }
            let id = id.clone();
            body = body
                .child(div().child("Sign out this other device?"))
                .child(
                    div()
                        .text_color(muted)
                        .child("Enter your account password to confirm."),
                )
                .child(Input::new(&self.security_input).disabled(self.busy))
                .child(
                    Button::new("confirm-device-signout")
                        .label("Sign out device")
                        .disabled(self.busy || self.preview)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            let password = this.take_security_input(window, cx);
                            this.perform_security(
                                SecurityAction::SignOut(id.clone(), password),
                                window,
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new("cancel-device-signout")
                        .label("Cancel")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, window, cx| {
                            drop(this.take_security_input(window, cx));
                            this.security_device = None;
                            cx.notify();
                        })),
                );
        } else {
            if !security.ready {
                body = body.child(div().child(if security.failed {
                    "Security information could not be loaded. Check your connection."
                } else {
                    "Checking encryption and backup…"
                }));
            }
            if let Some(flow) = &security.verification {
                body = body.child(self.verification_view(flow, cx));
            }
            if security.ready {
                body = body.child(div().font_weight(FontWeight::SEMIBOLD).child(
                    if security.verified {
                        "This device is verified"
                    } else {
                        "Verify this device"
                    },
                ));
                if !security.verified {
                    body = body.child(div().text_color(muted).child(
                        "Compare seven emojis with another device, or use your recovery key.",
                    ));
                    for device in security.devices.iter().filter(|device| !device.current) {
                        let id = device.id.clone();
                        body = body.child(
                            Button::new(SharedString::from(format!("verify-device-{id}")))
                                .label(format!(
                                    "Verify with {}",
                                    device.name.as_deref().unwrap_or(&device.id)
                                ))
                                .disabled(self.busy || self.preview)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.perform_security(
                                        SecurityAction::Request(id.clone()),
                                        window,
                                        cx,
                                    )
                                })),
                        );
                    }
                }
                if (!security.has_identity || security.verified) && !security.backup_exists {
                    body = body.child(
                        Button::new("setup-secure-backup")
                            .primary()
                            .label("Set up secure backup")
                            .disabled(self.busy || self.preview)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.perform_security(SecurityAction::Setup, window, cx)
                            })),
                    );
                }
                if security.backup_exists {
                    body = body.child(
                        Button::new("enter-recovery-key")
                            .label("Enter recovery key or passphrase")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.security_restore = true;
                                cx.notify();
                            })),
                    );
                }
                if security.recovery_enabled {
                    if self.security_reset_confirmation {
                        body = body.child(div().text_color(muted).child("Reset recovery? Your previous recovery key will stop working. Save the new key before leaving this screen."))
                            .child(Button::new("confirm-reset-recovery").label("Reset recovery key").disabled(self.busy || self.preview).on_click(cx.listener(|this, _, window, cx| this.perform_security(SecurityAction::Reset, window, cx))))
                            .child(Button::new("cancel-reset-recovery").label("Cancel").on_click(cx.listener(|this, _, _, cx| { this.security_reset_confirmation = false; cx.notify(); })));
                    } else {
                        body = body.child(
                            Button::new("change-recovery")
                                .label("Change or reset recovery")
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.security_reset_confirmation = true;
                                    cx.notify();
                                })),
                        );
                    }
                }
                body = body.child(
                    div()
                        .pt(px(8.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Your devices"),
                );
                body = body.child(
                    Button::new("refresh-devices")
                        .label("Refresh devices")
                        .disabled(self.busy || self.preview)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.perform_security(SecurityAction::Refresh, window, cx)
                        })),
                );
                for device in &security.devices {
                    let id = device.id.clone();
                    let mut row = h_flex().gap(px(12.)).child(
                        v_flex()
                            .flex_1()
                            .child(device.name.clone().unwrap_or_else(|| device.id.clone()))
                            .child(div().text_color(muted).child(format!(
                                "{}{}",
                                if device.current {
                                    "This device · "
                                } else {
                                    ""
                                },
                                if device.verified {
                                    "Verified"
                                } else {
                                    "Unverified"
                                }
                            ))),
                    );
                    if !device.current && (security.password_devices || security.oauth_management) {
                        row = row.child(
                            Button::new(SharedString::from(format!("sign-out-device-{id}")))
                                .small()
                                .label("Sign out")
                                .disabled(self.busy || self.preview)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.security_device = Some(id.clone());
                                    cx.notify();
                                })),
                        );
                    }
                    body = body.child(row);
                }
            }
        }
        if self.busy {
            body = body.child(div().text_color(muted).child("Working securely…"));
        }
        if !self.status.is_empty() {
            body = body.child(div().text_color(muted).child(self.status.clone()));
        }
        body.into_any_element()
    }

    fn verification_view(&self, flow: &VerificationView, cx: &mut Context<Self>) -> AnyElement {
        let mut body = v_flex()
            .id("verification-flow")
            .gap(px(12.))
            .p(px(16.))
            .rounded(px(8.))
            .bg(cx.theme().secondary);
        match &flow.phase {
            VerificationPhase::Requested => {
                body = body
                    .child(div().child("Another device wants to verify with you."))
                    .child(
                        Button::new("accept-verification")
                            .primary()
                            .label("Accept verification")
                            .disabled(self.busy || self.preview)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.perform_security(SecurityAction::Accept, window, cx)
                            })),
                    );
            }
            VerificationPhase::Waiting => {
                body = body.child(div().child("Waiting for the other device…"));
            }
            VerificationPhase::Emojis(emojis) => {
                body =
                    body.child(div().child("Do these seven emojis match on both devices?"))
                        .child(h_flex().gap(px(12.)).children(emojis.iter().map(
                            |(symbol, name)| {
                                v_flex()
                                    .items_center()
                                    .gap(px(4.))
                                    .child(div().text_size(px(28.)).child(symbol.clone()))
                                    .child(div().text_size(px(11.)).child(name.clone()))
                            },
                        )))
                        .child(
                            Button::new("confirm-verification")
                                .primary()
                                .label("They match")
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.perform_security(SecurityAction::Confirm, window, cx)
                                })),
                        )
                        .child(
                            Button::new("mismatch-verification")
                                .label("They don’t match")
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.perform_security(SecurityAction::Cancel(true), window, cx)
                                })),
                        );
            }
            VerificationPhase::Confirmed => {
                body = body.child(div().child("Confirmed here. Waiting for the other device…"));
            }
            VerificationPhase::Done => {
                body = body.child(div().child(
                    "Device verified. Encrypted conversations update as keys become available.",
                ));
            }
            VerificationPhase::Cancelled => {
                body = body.child(div().child("Verification cancelled. You can start again."));
            }
        }
        if !matches!(
            flow.phase,
            VerificationPhase::Done | VerificationPhase::Cancelled
        ) {
            body = body.child(
                Button::new("cancel-verification")
                    .label("Cancel verification")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.perform_security(SecurityAction::Cancel(false), window, cx)
                    })),
            );
        }
        body.into_any_element()
    }
}
