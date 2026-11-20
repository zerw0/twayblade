//! Who has read a message: the "Seen by" line and "Read" labels open a list
//! of the readers.
use super::*;
use gpui_kit::component::popover::Popover;

impl MatrixApp {
    /// A reader's display name; before the member list loads, the user ID's
    /// local part.
    pub(crate) fn reader_name(&self, user: &str) -> String {
        self.snapshot
            .members
            .iter()
            .find(|member| member.user_id == user)
            .map(|member| member.display_name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| {
                user.trim_start_matches('@')
                    .split(':')
                    .next()
                    .unwrap_or(user)
                    .to_owned()
            })
    }

    /// `label` as a button that lists `readers` in a popover. `key` keeps the
    /// popover's identity apart from other read lines.
    pub(crate) fn read_by_popover(
        &self,
        key: &str,
        readers: &[String],
        label: impl IntoElement,
        accessible: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let weak = cx.entity().downgrade();
        let readers = readers.to_vec();
        let trigger = Button::new(SharedString::from(format!("read-by-{key}")))
            .ghost()
            .compact()
            .h(px(18.))
            .px(px(4.))
            .rounded(px(4.))
            .text_size(px(11.))
            .text_color(cx.theme().muted_foreground)
            .accessibility_label(accessible)
            .child(label);
        Popover::new(SharedString::from(format!("read-by-list-{key}")))
            .anchor(Anchor::BottomLeft)
            .trigger(trigger)
            .content(move |_, _, cx| {
                let rows = weak
                    .upgrade()
                    .map(|app| app.update(cx, |this, cx| this.reader_rows(&readers, cx)))
                    .unwrap_or_default();
                v_flex()
                    .w(px(240.))
                    .p(px(6.))
                    .gap(px(2.))
                    .child(
                        div()
                            .px(px(6.))
                            .pt(px(2.))
                            .pb(px(4.))
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("Seen by {}", readers.len())),
                    )
                    .child(
                        v_flex()
                            .id("read-by-rows")
                            .max_h(px(280.))
                            .overflow_y_scroll()
                            .children(rows),
                    )
            })
            .into_any_element()
    }

    /// A delivery label under your latest message; "Read" lists the readers.
    pub(crate) fn delivery_label(
        &self,
        message: &crate::backend::Message,
        label: &'static str,
        icon: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let content = h_flex()
            .gap(px(3.))
            .text_size(px(10.5))
            .when(icon && label == "Read", |line| {
                line.child(Icon::new(IconName::CheckCheck).size(px(12.)))
            })
            .child(label);
        // The "Seen by" line above the composer already covers the room's
        // latest message; saying "Read" under it too would be twice.
        if label == "Read" && self.seen_by_covers(&message.id) {
            return div().into_any_element();
        }
        if label != "Read" || message.read_by.is_empty() {
            return content
                .text_color(cx.theme().muted_foreground)
                .into_any_element();
        }
        let count = message.read_by.len();
        self.read_by_popover(
            &message.id,
            &message.read_by,
            content,
            format!(
                "Read by {count} {}. Show who",
                if count == 1 { "person" } else { "people" }
            ),
            cx,
        )
    }

    /// Whether the composer's "Seen by" line shows message `id`'s readers: it
    /// describes the latest sent message, while nobody is typing.
    fn seen_by_covers(&self, id: &str) -> bool {
        self.snapshot.timeline_ready
            && model::typing_text(&self.snapshot).is_empty()
            && self
                .snapshot
                .messages
                .iter()
                .rev()
                .find(|message| message.event_id.is_some())
                .is_some_and(|latest| latest.id == id && !latest.read_by.is_empty())
    }

    /// One row per reader: avatar and name; clicking opens their profile.
    fn reader_rows(&self, readers: &[String], cx: &mut Context<Self>) -> Vec<AnyElement> {
        readers
            .iter()
            .map(|user| {
                let name = self.reader_name(user);
                let (id, open_name) = (user.clone(), name.clone());
                Button::new(SharedString::from(format!("reader-{user}")))
                    .ghost()
                    .w_full()
                    .h(px(28.))
                    .px(px(6.))
                    .justify_start()
                    .accessibility_label(format!("Open profile of {name}"))
                    .child(
                        h_flex()
                            .w_full()
                            .min_w_0()
                            .gap(px(8.))
                            .items_center()
                            .child(self.avatar_image(
                                self.member_avatar(user),
                                user,
                                &name,
                                20.,
                                10.,
                                cx,
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(12.5))
                                    .child(name),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_profile(id.clone(), open_name.clone(), window, cx)
                    }))
                    .into_any_element()
            })
            .collect()
    }
}
