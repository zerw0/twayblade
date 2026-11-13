//! Sign-in and session recovery screens.
use super::*;

const SERVER_HINT: &str = "The part after the colon in your Matrix ID, such as matrix.org.";

/// Text a screen reader reads as a heading. GPUI exposes only elements with an
/// id and a role, so visible text needs both to be read at all.
fn heading(id: &'static str, text: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .role(Role::Heading)
        .aria_label(text)
        .font_weight(FontWeight::SEMIBOLD)
        .child(text)
}
/// Plain text exposed to screen readers; the caller adds the visible child.
fn text_label(id: &'static str, text: impl Into<SharedString>) -> Stateful<Div> {
    div().id(id).role(Role::Label).aria_label(text)
}
/// Status text that screen readers announce when it changes.
fn status_text(id: &'static str, text: impl Into<SharedString>) -> Stateful<Div> {
    div().id(id).role(Role::Status).aria_label(text)
}

impl MatrixApp {
    pub(crate) fn field_label(&self, text: &'static str) -> Div {
        div()
            .text_size(px(12.))
            .font_weight(FontWeight::SEMIBOLD)
            .child(text)
    }

    pub(crate) fn session_recovery_view(&self, cx: &mut Context<Self>) -> AnyElement {
        use twayblade_core::auth::SessionState;
        if self.snapshot.session_state == SessionState::SoftLoggedOut && self.options.is_some() {
            return self.sign_in_view(cx);
        }
        let soft = self.snapshot.session_state == SessionState::SoftLoggedOut;
        let detail = if soft {
            "Re-authenticate to continue with this device. Your local keys and history will be kept."
        } else if self.confirm_session_reset {
            "Delete this device’s local keys and cached history? Recovery or another verified device may be needed to read encrypted messages after signing in again."
        } else {
            "This session was revoked. Sign in again to continue."
        };
        let mut form = v_flex()
            .w(px(360.))
            .gap(px(16.))
            .child(heading("recovery-title", "You were signed out").text_size(px(22.)))
            .child(
                text_label("recovery-detail", detail)
                    .text_color(cx.theme().muted_foreground)
                    .child(detail),
            );
        if soft {
            form =
                form.child(
                    Button::new("reauthenticate")
                        .primary()
                        .label("Re-authenticate")
                        .disabled(self.busy || self.preview)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.prepare_reauthentication(window, cx)
                        })),
                );
        } else if self.confirm_session_reset {
            form = form
                .child(
                    Button::new("confirm-session-reset")
                        .label("Delete local data and continue")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.discard_revoked_session(window, cx)
                        })),
                )
                .child(
                    Button::new("cancel-session-reset")
                        .label("Cancel")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.confirm_session_reset = false;
                            cx.notify();
                        })),
                );
        } else {
            form = form.child(
                Button::new("sign-in-again")
                    .primary()
                    .label("Sign in again")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.confirm_session_reset = true;
                        cx.notify();
                    })),
            );
        }
        if self.preview {
            form = form.child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child("Sample data"),
            );
        }
        if !self.status.is_empty() {
            form = form.child(
                status_text("recovery-status", self.status.clone())
                    .text_color(cx.theme().muted_foreground)
                    .child(self.status.clone()),
            );
        }
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .child(form)
            .into_any_element()
    }

    pub(crate) fn sign_in_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let subtitle = match &self.options {
            None => "Enter the server that hosts your Matrix account.".to_owned(),
            Some(options) => format!("Choose how to sign in to {}.", options.homeserver),
        };
        let mut form = v_flex().w(px(360.)).gap(px(14.)).child(
            v_flex()
                .items_center()
                .gap(px(10.))
                .pb(px(8.))
                .child(
                    div()
                        .size(px(56.))
                        .rounded(px(14.))
                        .bg(cx.theme().primary)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            Icon::new(IconName::MessagesSquare)
                                .size(px(28.))
                                .text_color(cx.theme().primary_foreground),
                        ),
                )
                .child(
                    heading(
                        "sign-in-title",
                        if self.snapshot.session_state
                            == twayblade_core::auth::SessionState::SoftLoggedOut
                        {
                            "You were signed out"
                        } else {
                            "Sign in"
                        },
                    )
                    .text_size(px(22.)),
                )
                .child(
                    text_label("sign-in-subtitle", subtitle.clone())
                        .text_color(muted)
                        .text_center()
                        .child(subtitle),
                ),
        );
        match &self.options {
            None => {
                form = form
                    .child(
                        v_flex()
                            .gap(px(6.))
                            .child(self.field_label("Server"))
                            .child(
                                Input::new(&self.homeserver)
                                    .aria_label("Server")
                                    .disabled(self.busy),
                            )
                            .child(
                                text_label("server-hint", SERVER_HINT)
                                    .text_size(px(11.5))
                                    .text_color(muted)
                                    .child(SERVER_HINT),
                            ),
                    )
                    .child(
                        Button::new("discover")
                            .primary()
                            .w_full()
                            .label("Continue")
                            .disabled(self.busy || self.backend.is_none())
                            .on_click(cx.listener(|this, _, window, cx| this.discover(window, cx))),
                    );
            }
            Some(options) => {
                form = form.child(
                    h_flex()
                        .h(px(32.))
                        .pl(px(10.))
                        .pr(px(4.))
                        .gap(px(8.))
                        .rounded(px(8.))
                        .bg(cx.theme().secondary)
                        .child(Icon::new(IconName::Lock).small().text_color(muted))
                        .child(
                            text_label("chosen-server", format!("Server: {}", options.homeserver))
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(options.homeserver.clone()),
                        )
                        .child(
                            Button::new("change-server")
                                .ghost()
                                .xsmall()
                                .label("Change")
                                .accessibility_label("Change server")
                                .disabled(
                                    self.busy
                                        || self.snapshot.session_state
                                            != twayblade_core::auth::SessionState::Active,
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.options = None;
                                    this.password
                                        .update(cx, |state, cx| state.set_value("", window, cx));
                                    cx.notify();
                                })),
                        ),
                );
                if options.browser && self.oauth_cancel.is_none() {
                    form = form.child(
                        Button::new("browser-sign-in")
                            .primary()
                            .w_full()
                            .label("Continue in browser")
                            .disabled(self.busy || self.preview)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.browser_sign_in(window, cx)),
                            ),
                    );
                }
                if options.sso && self.oauth_cancel.is_none() {
                    if options.providers.is_empty() {
                        form = form.child(
                            Button::new("sso-sign-in")
                                .w_full()
                                .label("Continue with SSO")
                                .disabled(self.busy || self.preview)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.sso_sign_in(None, window, cx)
                                })),
                        );
                    }
                    for provider in &options.providers {
                        let id = provider.id.clone();
                        let mut button =
                            Button::new(SharedString::from(format!("sso-{}", provider.id)))
                                .w_full()
                                .label(format!("Continue with {}", provider.name))
                                .disabled(self.busy || self.preview)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.sso_sign_in(Some(id.clone()), window, cx)
                                }));
                        if let Some(icon) = provider
                            .icon_png
                            .as_ref()
                            .and_then(|bytes| crate::ui::attachments::render_small_png(bytes, cx))
                        {
                            button = button.child(img(icon).size(px(20.)));
                        }
                        form = form.child(button);
                    }
                }
                if options.password {
                    let ready = !self.busy
                        && !self.username.read(cx).value().trim().is_empty()
                        && !self.password.read(cx).value().is_empty();
                    let sign_in = Button::new("sign-in")
                        .w_full()
                        .label("Sign in")
                        .disabled(!ready)
                        .on_click(cx.listener(|this, _, window, cx| this.sign_in(window, cx)));
                    form = form
                        .child(
                            v_flex()
                                .gap(px(6.))
                                .child(self.field_label("Username"))
                                .child(Input::new(&self.username).aria_label("Username").disabled(
                                    self.busy
                                        || self.snapshot.session_state
                                            != twayblade_core::auth::SessionState::Active,
                                )),
                        )
                        .child(
                            v_flex()
                                .gap(px(6.))
                                .child(self.field_label("Password"))
                                .child(
                                    Input::new(&self.password)
                                        .aria_label("Password")
                                        .disabled(self.busy),
                                ),
                        )
                        .child(sign_in.primary());
                }
            }
        }
        if self.oauth_cancel.is_some() {
            form = form.child(
                Button::new("cancel-browser-sign-in")
                    .w_full()
                    .label("Cancel")
                    .on_click(cx.listener(|this, _, _, cx| this.cancel_browser_sign_in(cx))),
            );
        }
        let status = if self.busy && self.status.is_empty() {
            "Working…".to_owned()
        } else {
            self.status.clone()
        };
        if !status.is_empty() {
            form = form.child(
                status_text("sign-in-status", status.clone())
                    .text_size(px(12.))
                    .text_color(muted)
                    .text_center()
                    .child(status),
            );
        }
        let storage = format!(
            "Twayblade keeps your session securely in the {}.",
            secret_store_name()
        );
        form = form.child(
            text_label("session-storage", storage.clone())
                .pt(px(8.))
                .text_size(px(11.5))
                .text_color(muted)
                .text_center()
                .child(storage),
        );
        v_flex()
            .id("sign-in")
            .size_full()
            .items_center()
            .pt(px(72.))
            .px_6()
            .overflow_y_scroll()
            .child(form)
            .into_any_element()
    }
}
