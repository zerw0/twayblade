//! Trailing panels: Info (About, Members, Files), search, profile and member actions.
use super::*;

impl MatrixApp {
    pub(crate) fn panel(&self, cx: &App) -> Stateful<Div> {
        v_flex()
            .id("details-panel")
            .w(px(DETAILS_WIDTH))
            .max_w_full()
            .h_full()
            .flex_none()
            .bg(cx.theme().background)
            .border_l_1()
            .border_color(cx.theme().border)
            .overflow_y_scroll()
    }

    pub(crate) fn panel_header(
        &self,
        title: &'static str,
        close: &'static str,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.toolbar("panel-toolbar", cx)
            .px(px(14.))
            .justify_between()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(
                Button::new(close)
                    .ghost()
                    .small()
                    .icon(IconName::X)
                    .accessibility_label("Close panel")
                    .tooltip("Close · Escape")
                    .on_click(cx.listener(|this, _, _, cx| this.dismiss_panel(cx))),
            )
    }

    pub(crate) fn member_row(&self, member: &Member, cx: &Context<Self>) -> AnyElement {
        let id = member.user_id.clone();
        let name = member.display_name.clone();
        let role = match member.role {
            MemberRole::Admin => "Admin",
            MemberRole::Moderator => "Moderator",
            MemberRole::Member => "",
        };
        // One line per member, like Contacts; the Matrix ID is in the tooltip.
        Button::new(SharedString::from(format!("member-{id}")))
            .ghost()
            .w_full()
            .h(px(MEMBER_ROW_HEIGHT))
            .px(px(6.))
            .rounded(px(6.))
            .accessibility_label(format!("Open profile of {name} {role}"))
            .tooltip(id.clone())
            .child(
                h_flex()
                    .w_full()
                    .gap(px(8.))
                    .child(
                        self.with_presence(
                            self.avatar_image(member.avatar.as_deref(), &id, &name, 22., 11., cx)
                                .into_any_element(),
                            member.presence.as_ref(),
                            22.,
                            cx,
                        ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_left()
                            .text_size(px(13.))
                            .child(name.clone()),
                    )
                    .when(!role.is_empty(), |row| {
                        row.child(
                            div()
                                .text_size(px(11.))
                                .text_color(cx.theme().muted_foreground)
                                .child(role),
                        )
                    }),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_profile(id.clone(), name.clone(), window, cx)
            }))
            .into_any_element()
    }

    pub(crate) fn section_label(&self, label: String, cx: &App) -> Div {
        div()
            .px(px(6.))
            .pb(px(4.))
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(cx.theme().muted_foreground)
            .child(label)
    }

    /// Message search results, in the trailing panel.
    pub(crate) fn search_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let palette = theme::palette(cx);
        let search = &self.snapshot.search;
        let in_room = !self.search_all_rooms;
        let room_name = self
            .snapshot
            .rooms
            .iter()
            .find(|room| room.id == self.snapshot.selected_room)
            .map(|room| room.name.clone());
        let scope = h_flex()
            .p(px(2.))
            .gap(px(2.))
            .rounded(px(7.))
            .bg(cx.theme().secondary)
            .children(
                [(true, "All Rooms"), (false, "This Room")].map(|(all, label)| {
                    let selected = self.search_all_rooms == all;
                    Button::new(SharedString::from(format!(
                        "search-scope-{}",
                        if all { "all" } else { "room" }
                    )))
                    .ghost()
                    .xsmall()
                    .flex_1()
                    .h(px(22.))
                    .rounded(px(5.))
                    .label(label)
                    .disabled(!all && room_name.is_none())
                    .selected(selected)
                    .when(selected, |button| button.bg(palette.pill).shadow_xs())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.search_all_rooms = all;
                        this.run_message_search(window, cx);
                    }))
                }),
            );
        let files = Button::new("search-has-file")
            .ghost()
            .xsmall()
            .icon(IconName::Paperclip)
            .label("Has Files")
            .selected(self.search_has_file)
            .on_click(cx.listener(|this, _, window, cx| {
                this.search_has_file = !this.search_has_file;
                this.run_message_search(window, cx);
            }));
        let mut list = v_flex().px(px(8.)).py(px(6.)).gap(px(2.));
        if self.preview {
            list = list.child(
                div()
                    .p(px(8.))
                    .text_color(muted)
                    .child("Search needs a connected account."),
            );
        }
        if search.limited {
            list =
                list.child(
                    h_flex()
                        .id("search-limited")
                        .items_start()
                        .gap(px(8.))
                        .mx(px(4.))
                        .mb(px(4.))
                        .px(px(8.))
                        .py(px(6.))
                        .rounded(px(6.))
                        .bg(cx.theme().secondary)
                        .text_size(px(11.5))
                        .text_color(muted)
                        .child(
                            Icon::new(IconName::Lock)
                                .size(px(12.))
                                .flex_none()
                                .mt(px(2.)),
                        )
                        .child(
                            div().flex_1().min_w_0().whitespace_normal().child(
                                "Encrypted rooms only include messages already on this device.",
                            ),
                        )
                        .test_support(),
                );
        }
        for result in &search.results {
            let room = self
                .snapshot
                .rooms
                .iter()
                .find(|room| room.id == result.room_id);
            let sender_name = if result.sender_name == result.sender {
                // Cached encrypted results carry only the ID; prefer a known name.
                self.snapshot
                    .members
                    .iter()
                    .find(|member| member.user_id == result.sender)
                    .map(|member| member.display_name.clone())
                    .unwrap_or_else(|| result.sender.clone())
            } else {
                result.sender_name.clone()
            };
            let highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = result
                .match_ranges
                .iter()
                .filter(|range| result.snippet.get((*range).clone()).is_some())
                .map(|range| {
                    (
                        range.clone(),
                        HighlightStyle {
                            font_weight: Some(FontWeight::SEMIBOLD),
                            background_color: Some(palette.selection),
                            ..Default::default()
                        },
                    )
                })
                .collect();
            let room_id = result.room_id.clone();
            let event_id = result.event_id.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!(
                        "search-result-{}",
                        result.event_id
                    )))
                    .w_full()
                    .px(px(8.))
                    .py(px(6.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|style| style.bg(cx.theme().secondary.opacity(0.6)))
                    .child(
                        h_flex()
                            .gap(px(8.))
                            .items_start()
                            .child(self.avatar_image(
                                self.member_avatar(&result.sender),
                                &result.sender,
                                &sender_name,
                                22.,
                                11.,
                                cx,
                            ))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap(px(2.))
                                    .child(
                                        h_flex()
                                            .gap(px(6.))
                                            .text_size(px(11.5))
                                            .child(
                                                div()
                                                    .truncate()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(sender_name),
                                            )
                                            .when(!in_room, |line| {
                                                line.children(room.map(|room| {
                                                    div().truncate().text_color(muted).child(
                                                        if room.direct {
                                                            room.name.clone()
                                                        } else {
                                                            format!("# {}", room.name)
                                                        },
                                                    )
                                                }))
                                            })
                                            .child(div().flex_1())
                                            .child(div().flex_none().text_color(muted).child(
                                                format!(
                                                    "{} {}",
                                                    day_label(result.timestamp_ms),
                                                    time_label(result.timestamp_ms)
                                                ),
                                            )),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.5))
                                            .whitespace_normal()
                                            .line_clamp(3)
                                            .child(
                                                StyledText::new(result.snippet.clone())
                                                    .with_highlights(highlights),
                                            ),
                                    ),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_event(room_id.clone(), event_id.clone(), window, cx)
                    })),
            );
        }
        if search.loading {
            list = list.children((0..4).map(|_| {
                div()
                    .mx(px(8.))
                    .my(px(4.))
                    .h(px(44.))
                    .rounded(px(6.))
                    .bg(cx.theme().secondary.opacity(0.6))
            }));
        } else if search.error.is_some() {
            list = list.child(
                v_flex()
                    .p(px(8.))
                    .gap(px(6.))
                    .text_color(muted)
                    .child("Search failed. Check your connection and try again.")
                    .child(
                        Button::new("search-retry")
                            .small()
                            .label("Try Again")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.run_message_search(window, cx)
                            })),
                    ),
            );
        } else if search.query.is_some() && search.results.is_empty() && search.exhausted {
            list = list.child(
                div()
                    .p(px(8.))
                    .text_color(muted)
                    .child("No messages match. Try other words or search all rooms."),
            );
        } else if search.query.is_some() && !search.exhausted {
            list = list.child(
                Button::new("search-more")
                    .ghost()
                    .small()
                    .w_full()
                    .label("Load More Results")
                    .on_click(cx.listener(|this, _, window, cx| this.search_more(window, cx))),
            );
        }
        self.panel(cx)
            .child(self.panel_header("Search", "close-search", cx))
            .child(
                v_flex()
                    .px(px(12.))
                    .pt(px(10.))
                    .pb(px(6.))
                    .gap(px(8.))
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(scope)
                    .child(h_flex().child(files)),
            )
            .child(
                div()
                    .id("search-results")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(list),
            )
            .into_any_element()
    }

    pub(crate) fn details_view(&self, cx: &mut Context<Self>) -> AnyElement {
        use crate::app::InfoTab;
        let palette = theme::palette(cx);
        let Some(room) = self
            .snapshot
            .rooms
            .iter()
            .find(|room| self.navigation.selected_room.as_ref() == Some(&room.id))
            .cloned()
        else {
            return div().into_any_element();
        };
        let tab = self.info_tab;
        let tabs = h_flex()
            .p(px(2.))
            .gap(px(2.))
            .rounded(px(7.))
            .bg(cx.theme().secondary)
            .children(
                [
                    (InfoTab::About, "About"),
                    (InfoTab::Members, "Members"),
                    (InfoTab::Files, "Files"),
                ]
                .map(|(value, label)| {
                    Button::new(SharedString::from(format!("info-tab-{label}")))
                        .ghost()
                        .xsmall()
                        .flex_1()
                        .h(px(22.))
                        .rounded(px(5.))
                        .label(label)
                        .selected(value == tab)
                        .when(value == tab, |button| button.bg(palette.pill).shadow_xs())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.info_tab = value;
                            cx.notify();
                        }))
                }),
            );
        // The conversation header already names the room; the panel stays "Info".
        let header = self.panel_header("Info", "close-details", cx);
        let body = match tab {
            InfoTab::About => self.info_about(&room, cx),
            InfoTab::Members => self.info_members(cx),
            InfoTab::Files => self.info_files(cx),
        };
        self.panel(cx)
            .child(header)
            .child(div().px(px(12.)).pt(px(10.)).child(tabs))
            .child(body)
            .into_any_element()
    }

    pub(crate) fn info_about(&self, room: &Room, cx: &mut Context<Self>) -> AnyElement {
        use twayblade_core::notifications::{RoomMode, RuleChange};
        let palette = theme::palette(cx);
        let muted = cx.theme().muted_foreground;
        let mode = self.snapshot.notifications.room_mode(&room.id);
        let muted_room = mode == RoomMode::Mute;
        let tile = |id: &'static str, icon: IconName, label: &'static str, active: bool| {
            Button::new(id)
                .ghost()
                .h(px(54.))
                .flex_1()
                .rounded(px(8.))
                .bg(cx.theme().secondary)
                .accessibility_label(label)
                .child(
                    v_flex()
                        .items_center()
                        .gap(px(4.))
                        .text_size(px(11.))
                        .child(Icon::new(icon).size(px(15.)).text_color(if active {
                            cx.theme().link
                        } else {
                            muted
                        }))
                        .child(label),
                )
        };
        let favourite_room = room.id.clone();
        let favourite = room.favourite;
        let mute_room = room.id.clone();
        let link_room = room.id.clone();
        let copy_id = room.id.clone();
        let files = self
            .snapshot
            .messages
            .iter()
            .filter(|message| message.attachment.is_some())
            .count();
        let info_row = |label: &'static str, value: String| {
            h_flex()
                .h(px(32.))
                .px(px(8.))
                .gap(px(10.))
                .child(div().flex_1().child(label))
                .child(div().text_size(px(12.)).text_color(muted).child(value))
        };
        v_flex()
            .id("info-about")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(
                v_flex()
                    .px(px(16.))
                    .pt(px(14.))
                    .pb(px(12.))
                    .gap(px(4.))
                    .child(match room.topic.clone() {
                        Some(topic) => div().whitespace_normal().child(topic),
                        None => div()
                            .text_color(muted)
                            .child("No topic. Room admins can add one in room settings."),
                    }),
            )
            .child(
                h_flex()
                    .px(px(12.))
                    .pb(px(14.))
                    .gap(px(6.))
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        tile(
                            "info-favourite",
                            IconName::Star,
                            if favourite { "Favourited" } else { "Favourite" },
                            favourite,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.set_favourite(favourite_room.clone(), !favourite, window, cx)
                            },
                        )),
                    )
                    .child(
                        tile(
                            "info-mute",
                            if muted_room {
                                IconName::BellOff
                            } else {
                                IconName::Bell
                            },
                            if muted_room { "Unmute" } else { "Mute" },
                            muted_room,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.change_notification_rule(
                                    RuleChange::Room(
                                        mute_room.clone(),
                                        if muted_room {
                                            RoomMode::Default
                                        } else {
                                            RoomMode::Mute
                                        },
                                    ),
                                    window,
                                    cx,
                                )
                            },
                        )),
                    )
                    .child(
                        tile("info-copy-link", IconName::Link, "Copy link", false).on_click(
                            move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(format!(
                                    "https://matrix.to/#/{link_room}"
                                )))
                            },
                        ),
                    ),
            )
            .child(
                v_flex()
                    .px(px(8.))
                    .py(px(6.))
                    .child(info_row(
                        "Encryption",
                        if room.encrypted { "On" } else { "Off" }.into(),
                    ))
                    .when(!room.encrypted, |rows| {
                        rows.child(
                            h_flex()
                                .id("encryption-warning")
                                .items_start()
                                .gap(px(8.))
                                .mx(px(8.))
                                .my(px(4.))
                                .px(px(8.))
                                .py(px(6.))
                                .rounded(px(6.))
                                .bg(palette.warning_background)
                                .text_color(palette.warning)
                                .text_size(px(12.))
                                .child(Icon::new(IconName::TriangleAlert).xsmall().flex_none())
                                .child(
                                    div()
                                        .id("encryption-warning-text")
                                        .flex_1()
                                        .min_w_0()
                                        .whitespace_normal()
                                        .child(
                                            "Not encrypted. Your server can read these messages.",
                                        )
                                        .test_support(),
                                )
                                .test_support(),
                        )
                    })
                    .child(info_row("Pinned messages", room.pinned.to_string()))
                    .child(info_row("Files", files.to_string()))
                    .child(
                        div()
                            .px(px(-2.))
                            .child(self.room_notification_controls(&room.id, cx)),
                    ),
            )
            .child(
                h_flex()
                    .mt_auto()
                    .px(px(16.))
                    .py(px(12.))
                    .gap(px(6.))
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(11.))
                            .text_color(muted)
                            .child(room.id.clone()),
                    )
                    .child(
                        Button::new("copy-room-id")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Copy)
                            .accessibility_label("Copy room ID")
                            .tooltip("Copy Room ID")
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copy_id.clone()))
                            }),
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn info_members(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let body = match self.snapshot.members_state {
            MembersState::Loading => div()
                .p(px(14.))
                .text_color(muted)
                .child("Loading members…")
                .into_any_element(),
            MembersState::Failed => div()
                .p(px(14.))
                .text_color(muted)
                .child("Members could not be loaded.")
                .into_any_element(),
            MembersState::Ready => {
                let members = self.snapshot.members.len();
                uniform_list(
                    "members",
                    members,
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .filter_map(|ix| this.snapshot.members.get(ix))
                            .map(|member| this.member_row(member, cx))
                            .collect::<Vec<_>>()
                    }),
                )
                .flex_1()
                .w_full()
                .into_any_element()
            }
        };
        v_flex()
            .flex_1()
            .min_h_0()
            .px(px(8.))
            .pt(px(10.))
            .child(self.section_label(format!("Members · {}", self.snapshot.members.len()), cx))
            .child(body)
            .into_any_element()
    }

    /// Info › Files: the room's paginated gallery (Images grid, Files list).
    /// Offline previews have no server, so they list the loaded timeline.
    pub(crate) fn info_files(&self, cx: &mut Context<Self>) -> AnyElement {
        use twayblade_core::media::MediaKind;
        if self.preview || self.backend.is_none() {
            return self.info_files_loaded(cx);
        }
        let palette = theme::palette(cx);
        let muted = cx.theme().muted_foreground;
        let media = &self.snapshot.media;
        let current = media.room == self.snapshot.selected_room;
        let kind = media.kind;
        let kinds = h_flex().px(px(12.)).pt(px(10.)).gap(px(6.)).children(
            [(MediaKind::Images, "Images"), (MediaKind::Files, "Files")].map(|(value, label)| {
                Button::new(SharedString::from(format!("gallery-kind-{label}")))
                    .ghost()
                    .xsmall()
                    .rounded(px(6.))
                    .label(label)
                    .selected(value == kind)
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.load_media(value, window, cx)),
                    )
            }),
        );
        let items: &[twayblade_core::media::MediaItem] = if current { &media.items } else { &[] };
        let mut body = v_flex().px(px(12.)).py(px(10.)).gap(px(8.));
        if kind == MediaKind::Images {
            // Thumbnails come from already-loaded timeline previews; others show
            // a plain tile until gallery thumbnails are fetched.
            let mut grid = h_flex().flex_wrap().gap(px(4.));
            for item in items {
                let preview = self
                    .snapshot
                    .messages
                    .iter()
                    .find(|message| message.event_id.as_ref() == Some(&item.event_id))
                    .and_then(|message| message.attachment.as_ref())
                    .and_then(|attachment| attachment.preview_png.as_ref())
                    .and_then(|png| crate::ui::attachments::render_png(png, cx));
                let event = item.event_id.clone();
                grid = grid.child(
                    div()
                        .id(SharedString::from(format!(
                            "gallery-image-{}",
                            item.event_id
                        )))
                        .size(px(84.))
                        .rounded(px(6.))
                        .overflow_hidden()
                        .bg(cx.theme().secondary)
                        .border_1()
                        .border_color(cx.theme().border)
                        .cursor_pointer()
                        .hover(|style| style.border_color(cx.theme().muted_foreground))
                        .tooltip({
                            let label = format!(
                                "{} · {} · {}",
                                item.filename,
                                item.sender_name,
                                day_label(item.timestamp_ms)
                            );
                            move |window, cx| {
                                gpui_kit::component::tooltip::Tooltip::new(label.clone())
                                    .build(window, cx)
                            }
                        })
                        .child(match preview {
                            Some(image) => img(image)
                                .size_full()
                                .object_fit(ObjectFit::Cover)
                                .into_any_element(),
                            None => v_flex()
                                .size_full()
                                .items_center()
                                .justify_center()
                                .gap(px(4.))
                                .p(px(6.))
                                .text_color(muted)
                                .child(Icon::new(IconName::Image).size(px(18.)))
                                .child(
                                    div()
                                        .w_full()
                                        .truncate()
                                        .text_center()
                                        .text_size(px(10.))
                                        .child(item.filename.clone()),
                                )
                                .into_any_element(),
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_media_image(&event, window, cx)
                        })),
                );
            }
            body = body.child(grid);
        } else {
            for item in items {
                let extension = item
                    .filename
                    .rsplit('.')
                    .next()
                    .filter(|ext| ext.len() <= 4 && *ext != item.filename)
                    .unwrap_or("FILE")
                    .to_uppercase();
                let event = item.event_id.clone();
                let message = self
                    .snapshot
                    .messages
                    .iter()
                    .find(|message| message.event_id.as_ref() == Some(&item.event_id))
                    .map(|message| message.id.clone());
                let name = item.filename.clone();
                body = body.child(
                    Button::new(SharedString::from(format!("gallery-file-{event}")))
                        .ghost()
                        .w_full()
                        .h(px(44.))
                        .px(px(6.))
                        .accessibility_label(format!(
                            "{}, from {}",
                            item.filename, item.sender_name
                        ))
                        .tooltip(if message.is_some() {
                            "Save…"
                        } else {
                            "Show in conversation"
                        })
                        .child(
                            h_flex()
                                .w_full()
                                .gap(px(10.))
                                .child(
                                    div()
                                        .size(px(30.))
                                        .flex_none()
                                        .rounded(px(6.))
                                        .bg(theme::avatar_color(&item.filename))
                                        .text_color(palette.on_avatar)
                                        .text_size(px(9.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(extension),
                                )
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_w_0()
                                        .child(
                                            div()
                                                .truncate()
                                                .text_left()
                                                .text_size(px(12.5))
                                                .child(item.filename.clone()),
                                        )
                                        .child(
                                            div()
                                                .truncate()
                                                .text_left()
                                                .text_size(px(11.))
                                                .text_color(muted)
                                                .child(format!(
                                                    "{} · {} · {}",
                                                    item.sender_name,
                                                    day_label(item.timestamp_ms),
                                                    model::size_label(item.size)
                                                )),
                                        ),
                                ),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| match &message {
                            // Loaded: save directly. Older: open it in context to save there.
                            Some(id) => {
                                this.save_attachment(id.clone(), name.clone(), false, window, cx)
                            }
                            None => {
                                let room = this.snapshot.selected_room.clone();
                                this.open_event(room, event.clone(), window, cx)
                            }
                        })),
                );
            }
        }
        if current && media.loading {
            body = body.child(h_flex().gap(px(4.)).flex_wrap().children((0..6).map(|_| {
                div()
                    .when(kind == MediaKind::Images, |tile| tile.size(px(84.)))
                    .when(kind == MediaKind::Files, |row| row.w_full().h(px(36.)))
                    .rounded(px(6.))
                    .bg(cx.theme().secondary.opacity(0.6))
            })));
        } else if current && media.error.is_some() {
            body = body.child(
                v_flex()
                    .gap(px(6.))
                    .text_color(muted)
                    .child("The room's files could not be loaded. Check your connection.")
                    .child(
                        Button::new("gallery-retry")
                            .small()
                            .label("Try Again")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.load_media(kind, window, cx)
                            })),
                    ),
            );
        } else if current && items.is_empty() && media.exhausted {
            body = body.child(div().text_color(muted).child(if kind == MediaKind::Images {
                "No images have been shared in this room."
            } else {
                "No files have been shared in this room."
            }));
        } else if current && !media.exhausted {
            body = body.child(
                Button::new("gallery-more")
                    .ghost()
                    .small()
                    .w_full()
                    .label("Load Older")
                    .on_click(cx.listener(|this, _, window, cx| this.load_more_media(window, cx))),
            );
        }
        v_flex()
            .flex_1()
            .min_h_0()
            .child(kinds)
            .child(
                div()
                    .id("info-gallery")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(body),
            )
            .into_any_element()
    }

    pub(crate) fn info_files_loaded(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let files: Vec<_> = self
            .snapshot
            .messages
            .iter()
            .rev()
            .filter_map(|message| message.attachment.as_ref().map(|file| (message, file)))
            .collect();
        let mut list = v_flex()
            .id("info-files")
            .flex_1()
            .min_h_0()
            .px(px(8.))
            .py(px(10.))
            .gap(px(1.))
            .overflow_y_scroll();
        if files.is_empty() {
            list = list.child(div().p(px(6.)).text_color(muted).whitespace_normal().child(
                "No files in the loaded history. Scroll up in the conversation to load more.",
            ));
        }
        for (message, file) in files {
            let extension = file
                .filename
                .rsplit('.')
                .next()
                .filter(|ext| ext.len() <= 4 && *ext != file.filename)
                .unwrap_or("FILE")
                .to_uppercase();
            let id = message.id.clone();
            let name = file.filename.clone();
            list = list.child(
                Button::new(SharedString::from(format!("info-file-{}", message.id)))
                    .ghost()
                    .w_full()
                    .h(px(44.))
                    .px(px(8.))
                    .accessibility_label(format!("Save {}", file.filename))
                    .tooltip("Save…")
                    .child(
                        h_flex()
                            .w_full()
                            .gap(px(10.))
                            .child(
                                div()
                                    .size(px(30.))
                                    .flex_none()
                                    .rounded(px(6.))
                                    .bg(theme::avatar_color(&file.filename))
                                    .text_color(theme::palette(cx).on_avatar)
                                    .text_size(px(9.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(extension),
                            )
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .truncate()
                                            .text_left()
                                            .text_size(px(12.5))
                                            .child(file.filename.clone()),
                                    )
                                    .child(
                                        div()
                                            .truncate()
                                            .text_left()
                                            .text_size(px(11.))
                                            .text_color(muted)
                                            .child(format!(
                                                "{} · {}",
                                                message.display_name,
                                                time_label(message.timestamp_ms)
                                            )),
                                    ),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.save_attachment(id.clone(), name.clone(), false, window, cx)
                    })),
            );
        }
        list.into_any_element()
    }

    /// Message, Verify and Ignore for anyone; Mute, Kick and Ban when your power
    /// level allows. Kick, Ban and Ignore need a second click to confirm.
    pub(crate) fn member_actions(&self, user: &str, cx: &mut Context<Self>) -> AnyElement {
        use twayblade_core::room_state::MemberAction;
        let member = self
            .snapshot
            .members
            .iter()
            .find(|member| member.user_id == user)
            .cloned()
            .unwrap_or_default();
        let palette = theme::palette(cx);
        let action = |id: String, icon: IconName, label: String, danger: bool| {
            Button::new(SharedString::from(id))
                .ghost()
                .w_full()
                .h(px(32.))
                .px(px(10.))
                .justify_start()
                .accessibility_label(label.clone())
                .child(
                    h_flex()
                        .gap(px(10.))
                        .when(danger, |row| row.text_color(palette.error))
                        .child(Icon::new(icon).size(px(14.)))
                        .child(label),
                )
        };
        let confirming =
            |kind: MemberAction| self.member_confirm.as_ref() == Some(&(user.to_owned(), kind));
        let user_owned = user.to_owned();
        let mut list = v_flex()
            .w_full()
            .px(px(8.))
            .py(px(6.))
            .gap(px(1.))
            .border_t_1()
            .border_color(cx.theme().border);
        let target = user_owned.clone();
        list =
            list.child(
                action(
                    "member-message".into(),
                    IconName::MessageCircle,
                    "Message".into(),
                    false,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.message_user(target.clone(), window, cx)
                })),
            );
        let target = user_owned.clone();
        list =
            list.child(
                action(
                    "member-verify".into(),
                    IconName::ShieldCheck,
                    "Verify…".into(),
                    false,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.verify_user(target.clone(), window, cx)
                })),
            );
        if member.can_mute {
            let target = user_owned.clone();
            let mute = !member.muted;
            list = list.child(
                action(
                    "member-mute".into(),
                    if mute {
                        IconName::MicOff
                    } else {
                        IconName::Mic
                    },
                    if mute {
                        "Mute in This Room".into()
                    } else {
                        "Unmute in This Room".into()
                    },
                    false,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.member_action(target.clone(), MemberAction::Mute(mute), window, cx)
                })),
            );
        }
        let target = user_owned.clone();
        list = list.child(
            action(
                "member-ignore".into(),
                IconName::EyeOff,
                if confirming(MemberAction::Ignore) {
                    "Click again to ignore everywhere".into()
                } else {
                    "Ignore…".into()
                },
                true,
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.member_action(target.clone(), MemberAction::Ignore, window, cx)
            })),
        );
        for (kind, id, icon, label, confirm) in [
            (
                MemberAction::Kick,
                "member-kick",
                IconName::UserX,
                "Remove from Room…",
                "Click again to remove",
            ),
            (
                MemberAction::Ban,
                "member-ban",
                IconName::Ban,
                "Ban…",
                "Click again to ban",
            ),
        ] {
            let allowed = match kind {
                MemberAction::Kick => member.can_kick,
                _ => member.can_ban,
            };
            if !allowed {
                continue;
            }
            let target = user_owned.clone();
            list = list.child(
                action(
                    id.into(),
                    icon,
                    if confirming(kind) {
                        confirm.into()
                    } else {
                        label.into()
                    },
                    true,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.member_action(target.clone(), kind, window, cx)
                })),
            );
        }
        list.into_any_element()
    }

    pub(crate) fn profile_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(page) = &self.profile else {
            return div().into_any_element();
        };
        let palette = theme::palette(cx);
        let muted = cx.theme().muted_foreground;
        let profile_id = page
            .profile
            .as_ref()
            .map(|profile| profile.user_id.clone())
            .unwrap_or_else(|| page.id.clone());
        let name = page
            .profile
            .as_ref()
            .map(|profile| profile.display_name.clone())
            .unwrap_or_else(|| page.name.clone());
        let mut body = v_flex()
            .items_center()
            .gap(px(6.))
            .px(px(16.))
            .py(px(20.))
            .child(self.avatar_image(
                self.member_avatar(&profile_id),
                &profile_id,
                &name,
                64.,
                32.,
                cx,
            ))
            .child(
                div()
                    .text_size(px(17.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_center()
                    .child(name.clone()),
            )
            .child(
                TextView::markdown("profile-user-id", model::plain_text(&profile_id))
                    .selectable(true),
            )
            .child(
                Button::new("copy-profile-id")
                    .ghost()
                    .small()
                    .icon(IconName::Copy)
                    .label("Copy Matrix ID")
                    .on_click({
                        let id = page.id.clone();
                        move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(id.clone()))
                    }),
            );
        if page.loading {
            body = body.child(div().text_color(muted).child("Loading profile…"));
        }
        if let Some(info) = &page.profile {
            if let Some(room_name) = &info.room_display_name
                && room_name != &info.display_name
            {
                body = body.child(
                    div()
                        .text_color(muted)
                        .child(format!("Name in this room: {room_name}")),
                );
            }
            if !info.server_profile_available {
                body = body.child(
                    div()
                        .text_color(muted)
                        .child("Showing the saved conversation profile."),
                );
            }
        }
        if !page.error.is_empty() {
            let id = page.id.clone();
            let name = page.name.clone();
            body = body
                .child(div().text_color(palette.error).child(page.error.clone()))
                .child(
                    Button::new("retry-profile")
                        .small()
                        .label("Try again")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_profile(id.clone(), name.clone(), window, cx)
                        })),
                );
        }
        if self.preview {
            body = body.child(div().text_color(muted).child("Offline sample profile"));
        }
        let own = profile_id == self.snapshot.user_id;
        self.panel(cx)
            .child(self.panel_header("Profile", "close-profile", cx))
            .child(body)
            .when(!own, |panel| {
                panel.child(self.member_actions(&profile_id, cx))
            })
            .into_any_element()
    }
}
