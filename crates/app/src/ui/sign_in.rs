//! Sign-in and session recovery screens.
use super::*;

/// Public servers offered as starting points; any other server can be typed.
const SUGGESTED_SERVERS: [(&str, &str); 4] = [
    ("matrix.org", "Run by the Matrix.org Foundation."),
    ("mozilla.org", "Matrix server hosted by Mozilla."),
    ("tchncs.de", "A general server for everyone."),
    ("knoten.chat", "A general server run by volunteers."),
];
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
            .items_start()
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
                        .icon(IconName::LogIn)
                        .label("Re-authenticate")
                        .disabled(self.busy || self.preview)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.prepare_reauthentication(window, cx)
                        })),
                );
        } else if self.confirm_session_reset {
            // Cancel, then the destructive action at the trailing edge.
            form = form.child(
                h_flex()
                    .w_full()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        Button::new("cancel-session-reset")
                            .label("Cancel")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirm_session_reset = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("confirm-session-reset")
                            .danger()
                            .icon(IconName::Trash)
                            .label("Delete local data and continue")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.discard_revoked_session(window, cx)
                            })),
                    ),
            );
        } else {
            form = form.child(
                Button::new("sign-in-again")
                    .primary()
                    .icon(IconName::LogIn)
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
        let form = self
            .sign_in_form(cx)
            .p(px(28.))
            .rounded(px(16.))
            .bg(cx.theme().secondary)
            .border_1()
            .border_color(cx.theme().border);
        v_flex()
            .id("sign-in")
            .size_full()
            .items_center()
            .pt(px(32.))
            .pb(px(24.))
            .px_6()
            .overflow_y_scroll()
            .child(form)
            .into_any_element()
    }

    /// Add Account…: the same flow in a sheet over the dimmed workspace. It
    /// closes only through Cancel or Escape, never by a stray outside click.
    pub(crate) fn add_account_sheet(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("add-account-backdrop")
            .test_support()
            .absolute()
            .inset_0()
            .occlude()
            .bg(gpui_kit::black().opacity(0.35))
            .flex()
            .items_start()
            .justify_center()
            .pt(px(56.))
            .child(crate::ui::motion::rise(
                div().child(
                    self.sign_in_form(cx)
                        .id("add-account-sheet")
                        .role(Role::Dialog)
                        .aria_label("Add Account")
                        .p(px(24.))
                        .rounded(px(12.))
                        .bg(cx.theme().background)
                        .border_1()
                        .border_color(cx.theme().border)
                        .shadow_lg()
                        .max_h(relative(0.86))
                        .overflow_y_scroll()
                        .focus_trap("add-account-dialog", &self.add_account_focus),
                ),
                "add-account-motion",
                self.snapshot.reduce_motion,
            ))
            .into_any_element()
    }

    /// The Cancel button that closes the Add Account sheet.
    fn cancel_add_button(&self, cx: &mut Context<Self>) -> Button {
        Button::new("cancel-add-account")
            .ghost()
            .label("Cancel")
            .disabled(self.busy)
            .on_click(cx.listener(|this, _, window, cx| this.cancel_add_account(window, cx)))
    }

    fn sign_in_form(&self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let adding = self.adding_account;
        let welcome = self.options.is_none() && !self.choosing_server && !adding;
        let signed_out = !adding
            && self.snapshot.session_state == twayblade_core::auth::SessionState::SoftLoggedOut;
        let (title, subtitle) = match &self.options {
            None if adding => (
                "Add an account",
                "Pick where the account lives, or enter another server.".to_owned(),
            ),
            Some(options) if adding => (
                "Add an account",
                format!("Choose how to sign in to {}.", options.homeserver),
            ),
            None if welcome && !signed_out => (
                "Twayblade",
                "Encrypted messages and calls over Matrix.".to_owned(),
            ),
            None => (
                if signed_out {
                    "You were signed out"
                } else {
                    "Choose your server"
                },
                "Pick where your account lives, or enter another server.".to_owned(),
            ),
            Some(options) => (
                if signed_out {
                    "You were signed out"
                } else {
                    "Sign in"
                },
                format!("Choose how to sign in to {}.", options.homeserver),
            ),
        };
        let mut form = v_flex().w(px(420.)).max_w_full().gap(px(14.)).child(
            v_flex()
                .items_center()
                .gap(px(if adding { 4. } else { 10. }))
                .pb(px(if adding { 2. } else { 8. }))
                // The sheet sits over the app, so it skips the brand mark and
                // uses a sheet-sized title.
                .when(!adding, |header| {
                    header.child(
                        div()
                            .size(px(56.))
                            .rounded_full()
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
                })
                .child(heading("sign-in-title", title).text_size(px(if adding {
                    16.
                } else {
                    22.
                })))
                .child(
                    text_label("sign-in-subtitle", subtitle.clone())
                        .text_color(muted)
                        .text_center()
                        .child(subtitle),
                ),
        );
        match &self.options {
            None if welcome && !signed_out => {
                form = form.child(
                    h_flex().justify_center().child(
                        Button::new("start-sign-in")
                            .primary()
                            .icon(IconName::LogIn)
                            .label("Sign in")
                            .disabled(self.backend.is_none() && !self.preview)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.choosing_server = true;
                                cx.notify();
                            })),
                    ),
                );
            }
            None => {
                let current = self.homeserver.read(cx).value().trim().to_lowercase();
                let mut list = v_flex()
                    .id("suggested-servers")
                    .role(Role::RadioGroup)
                    .aria_label("Suggested servers")
                    .gap(px(2.));
                for (host, about) in SUGGESTED_SERVERS {
                    let selected = current == host;
                    list =
                        list.child(
                            h_flex()
                                .id(SharedString::from(format!("server-{host}")))
                                .role(Role::RadioButton)
                                .aria_label(if selected {
                                    format!("{host}, selected")
                                } else {
                                    host.to_owned()
                                })
                                .gap(px(12.))
                                .px(px(10.))
                                .py(px(8.))
                                .rounded(px(10.))
                                .cursor_pointer()
                                .when(selected, |row| row.bg(cx.theme().background))
                                .hover(|row| row.bg(cx.theme().background))
                                .child(
                                    Icon::new(if selected {
                                        IconName::CircleCheck
                                    } else {
                                        IconName::Circle
                                    })
                                    .small()
                                    .text_color(if selected { cx.theme().primary } else { muted }),
                                )
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_w_0()
                                        .child(div().font_weight(FontWeight::SEMIBOLD).child(host))
                                        .child(
                                            div().text_size(px(12.)).text_color(muted).child(about),
                                        ),
                                )
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.homeserver
                                        .update(cx, |state, cx| state.set_value(host, window, cx));
                                    cx.notify();
                                })),
                        );
                }
                form =
                    form.child(list)
                        .child(
                            v_flex()
                                .gap(px(6.))
                                .child(self.field_label("Other server"))
                                .child(
                                    div().w_full().child(
                                        Input::new(&self.homeserver)
                                            .aria_label("Server")
                                            .disabled(self.busy),
                                    ),
                                )
                                .child(
                                    text_label("server-hint", SERVER_HINT)
                                        .text_size(px(11.5))
                                        .text_color(muted)
                                        .child(SERVER_HINT),
                                ),
                        )
                        // A form footer, as in macOS sheets: Back, then the default
                        // action at the trailing edge.
                        .child(
                            h_flex()
                                .justify_end()
                                .gap(px(8.))
                                .child(if adding {
                                    self.cancel_add_button(cx)
                                } else {
                                    Button::new("back-to-welcome")
                                        .ghost()
                                        .label("Back")
                                        .disabled(self.busy || signed_out)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.choosing_server = false;
                                            cx.notify();
                                        }))
                                })
                                .child(
                                    Button::new("discover")
                                        .primary()
                                        .icon(IconName::ArrowRight)
                                        .label("Continue")
                                        .disabled(self.busy || self.backend.is_none())
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.discover(window, cx)
                                        })),
                                ),
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
                                    this.choosing_server = true;
                                    this.password
                                        .update(cx, |state, cx| state.set_value("", window, cx));
                                    cx.notify();
                                })),
                        ),
                );
                // Sign-in choices stack centred at their own width.
                let mut choices = v_flex().items_center().gap(px(8.));
                if options.browser && self.oauth_cancel.is_none() {
                    choices = choices.child(
                        Button::new("browser-sign-in")
                            .primary()
                            .icon(IconName::ExternalLink)
                            .label("Continue in browser")
                            .disabled(self.busy || self.preview)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.browser_sign_in(window, cx)),
                            ),
                    );
                }
                if options.sso && self.oauth_cancel.is_none() {
                    if options.providers.is_empty() {
                        choices = choices.child(
                            Button::new("sso-sign-in")
                                .icon(IconName::ExternalLink)
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
                        choices = choices.child(button);
                    }
                }
                form = form.child(choices);
                if options.password {
                    let ready = !self.busy
                        && !self.username.read(cx).value().trim().is_empty()
                        && !self.password.read(cx).value().is_empty();
                    let sign_in = Button::new("sign-in")
                        .icon(IconName::LogIn)
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
                        .child(
                            h_flex()
                                .justify_end()
                                .gap(px(8.))
                                .when(adding && self.oauth_cancel.is_none(), |row| {
                                    row.child(self.cancel_add_button(cx))
                                })
                                .child(sign_in.primary()),
                        );
                } else if adding && self.oauth_cancel.is_none() {
                    form = form.child(h_flex().justify_end().child(self.cancel_add_button(cx)));
                }
            }
        }
        if self.oauth_cancel.is_some() {
            form = form.child(
                h_flex().justify_center().child(
                    Button::new("cancel-browser-sign-in")
                        .label("Cancel")
                        .on_click(cx.listener(|this, _, _, cx| this.cancel_browser_sign_in(cx))),
                ),
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
        form.child(
            text_label("session-storage", storage.clone())
                .pt(px(8.))
                .text_size(px(11.5))
                .text_color(muted)
                .text_center()
                .child(storage),
        )
    }
}
