//! Polls (#181): the poll card in the timeline, voting, ending, and the
//! Create Poll dialog opened from the composer's ＋ menu.
use super::*;
use crate::backend::TimelineAction;
use crate::ui::spaces::ManageKind;
use gpui_kit::component::input::{Input, InputState};
use twayblade_core::conversation::{MAX_ANSWERS, MIN_ANSWERS, Poll, PollDraft};

fn votes(n: usize) -> String {
    match n {
        1 => "1 vote".to_owned(),
        n => format!("{n} votes"),
    }
}

/// Sample poll rows for the offline preview and tests: one you voted on,
/// one ended.
pub(crate) fn sample_polls(messages: &mut [crate::backend::Message], own: &str) {
    use twayblade_core::conversation::PollAnswer;
    let answer = |id: &str, text: &str, votes: usize, chosen: bool, winner: bool| PollAnswer {
        id: id.into(),
        text: text.into(),
        votes,
        chosen,
        winner,
    };
    let len = messages.len();
    if len < 2 {
        return;
    }
    let open = &mut messages[len - 2];
    open.kind = MessageKind::Poll;
    open.body = "Where should the offsite be?".into();
    open.rich = None;
    open.attachment = None;
    open.sender = own.to_owned();
    open.poll = Some(Poll {
        question: "Where should the offsite be?".into(),
        disclosed: true,
        max_selections: 1,
        answers: vec![
            answer("a", "Lisbon", 5, true, false),
            answer("b", "Kraków", 3, false, false),
            answer("c", "Online", 1, false, false),
        ],
        voters: 9,
        ended: false,
        edited: false,
        mine: true,
    });
    let ended = &mut messages[len - 1];
    ended.kind = MessageKind::Poll;
    ended.body = "Release name".into();
    ended.rich = None;
    ended.attachment = None;
    ended.poll = Some(Poll {
        question: "Release name".into(),
        disclosed: false,
        max_selections: 1,
        answers: vec![
            answer("a", "Twayblade 1.0", 6, false, true),
            answer("b", "Orchid", 2, false, false),
        ],
        voters: 8,
        ended: true,
        edited: false,
        mine: false,
    });
}

/// Whether counts show: after the end, or while open for a disclosed poll
/// once you have voted (so seeing results cannot sway your vote).
pub(crate) fn results_visible(poll: &Poll) -> bool {
    poll.ended || (poll.disclosed && poll.answers.iter().any(|a| a.chosen))
}

/// The line under the answers.
pub(crate) fn poll_status(poll: &Poll) -> String {
    let voted = poll.answers.iter().any(|a| a.chosen);
    if poll.ended {
        format!("Final results · {}", votes(poll.voters))
    } else if !poll.disclosed {
        format!("{} · Results show when the poll ends", votes(poll.voters))
    } else if voted {
        votes(poll.voters)
    } else {
        format!("{} · Vote to see the results", votes(poll.voters))
    }
}

/// What a screen reader announces for a poll row.
pub(crate) fn accessible_poll(poll: &Poll) -> String {
    let show = results_visible(poll);
    let answers = poll
        .answers
        .iter()
        .map(|answer| {
            let mut text = answer.text.clone();
            if show {
                text.push_str(&format!(", {}", votes(answer.votes)));
            }
            if answer.chosen {
                text.push_str(", your vote");
            }
            text
        })
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "Poll, {}{}. {answers}. {}",
        poll.question,
        if poll.ended { ", ended" } else { "" },
        poll_status(poll)
    )
}

impl MatrixApp {
    /// The composer's ＋ menu: more kinds of message (#181).
    pub(crate) fn composer_more_menu(&self, editable: bool, cx: &mut Context<Self>) -> AnyElement {
        use gpui_kit::component::menu::DropdownMenu;
        let weak = cx.entity().downgrade();
        Button::new("composer-more")
            .ghost()
            .size(px(26.))
            .icon(IconName::CirclePlus)
            .accessibility_label("More message types")
            .tooltip("More")
            .disabled(!editable || self.preview)
            .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, _| {
                let poll = weak.clone();
                let sticker = weak.clone();
                menu.item(
                    PopupMenuItem::new("Sticker")
                        .icon(IconName::Sticker)
                        .on_click(move |_, window, cx| {
                            let _ =
                                sticker.update(cx, |this, cx| this.open_sticker_picker(window, cx));
                        }),
                )
                .item(
                    PopupMenuItem::new("Poll")
                        .icon(IconName::ChartBar)
                        .on_click(move |_, window, cx| {
                            let _ = poll.update(cx, |this, cx| this.open_create_poll(window, cx));
                        }),
                )
            })
            .into_any_element()
    }

    pub(crate) fn poll_card(
        &self,
        message: &crate::backend::Message,
        poll: &Poll,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let show = results_visible(poll);
        let total: usize = poll.answers.iter().map(|a| a.votes).sum();
        let can_vote = !poll.ended && !disabled && message.event_id.is_some();
        let mut answers = v_flex().w_full().gap(px(6.));
        for answer in &poll.answers {
            let id = message.id.clone();
            let answer_id = answer.id.clone();
            let share = if show && total > 0 {
                answer.votes as f32 / total as f32
            } else {
                0.
            };
            let emphasis = answer.chosen || answer.winner;
            let mark = Icon::new(if answer.chosen {
                IconName::CircleCheck
            } else {
                IconName::Circle
            })
            .size(px(15.))
            .text_color(if answer.chosen {
                cx.theme().primary
            } else {
                muted
            });
            let row = v_flex()
                .w_full()
                .gap(px(5.))
                .child(
                    h_flex()
                        .w_full()
                        .gap(px(8.))
                        .child(mark)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_left()
                                .when(emphasis, |text| text.font_weight(FontWeight::SEMIBOLD))
                                .child(answer.text.clone()),
                        )
                        .when(show, |line| {
                            line.child(
                                div()
                                    .flex_none()
                                    .text_size(px(12.))
                                    .text_color(muted)
                                    .child(votes(answer.votes)),
                            )
                        }),
                )
                .child(
                    // Bar track; filled with the share once results show.
                    div()
                        .w_full()
                        .h(px(4.))
                        .rounded(px(2.))
                        .bg(cx.theme().border)
                        .child(div().h_full().rounded(px(2.)).w(relative(share)).bg(
                            if poll.ended && !answer.winner {
                                muted
                            } else {
                                cx.theme().primary
                            },
                        )),
                );
            let label = format!(
                "{}{}{}",
                answer.text,
                if show {
                    format!(", {}", votes(answer.votes))
                } else {
                    String::new()
                },
                if answer.chosen { ", your vote" } else { "" }
            );
            let element_id = SharedString::from(format!("poll-{}-{}", message.id, answer.id));
            // Only a poll you can vote on has buttons; otherwise the answers
            // are plain rows, so results stay fully legible.
            answers = answers.child(if can_vote {
                Button::new(element_id)
                    .ghost()
                    .w_full()
                    .h_auto()
                    .px(px(8.))
                    .py(px(6.))
                    .rounded(px(8.))
                    .justify_start()
                    .accessibility_label(label)
                    .selected(answer.chosen)
                    .child(row)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.timeline_action(
                            id.clone(),
                            TimelineAction::Vote {
                                message: id.clone(),
                                answer: answer_id.clone(),
                            },
                            window,
                            cx,
                        )
                    }))
                    .into_any_element()
            } else {
                div()
                    .id(element_id)
                    .role(Role::ListItem)
                    .aria_label(label)
                    .w_full()
                    .px(px(8.))
                    .py(px(6.))
                    .child(row)
                    .test_support()
                    .into_any_element()
            });
        }
        let end_id = message.id.clone();
        v_flex()
            .id(SharedString::from(format!("poll-{}", message.id)))
            .w(px(360.))
            .max_w_full()
            .gap(px(10.))
            .p(px(12.))
            .rounded(px(12.))
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                h_flex()
                    .gap(px(8.))
                    .items_start()
                    .child(
                        Icon::new(IconName::ChartBar)
                            .size(px(15.))
                            .text_color(muted)
                            .mt(px(2.)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(poll.question.clone()),
                    ),
            )
            .child(answers)
            .child(
                h_flex()
                    .w_full()
                    .gap(px(8.))
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(muted)
                            .child(poll_status(poll)),
                    )
                    .when(poll.mine && !poll.ended, |footer| {
                        footer.child(
                            Button::new(SharedString::from(format!("poll-end-{}", message.id)))
                                .ghost()
                                .xsmall()
                                .label("End Poll")
                                .disabled(disabled || message.event_id.is_none())
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.confirm_end_poll(end_id.clone(), window, cx)
                                })),
                        )
                    }),
            )
            .test_support()
            .into_any_element()
    }

    /// Ending cannot be undone, so it asks first.
    fn confirm_end_poll(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let answer = window.prompt(
            PromptLevel::Warning,
            "End this poll?",
            Some("No one can vote after it ends, and everyone sees the final results."),
            &["End Poll", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |view, cx| {
            if answer.await == Ok(0) {
                let _ = view.update_in(cx, |this, window, cx| {
                    this.timeline_action(
                        id.clone(),
                        TimelineAction::EndPoll { message: id },
                        window,
                        cx,
                    )
                });
            }
        })
        .detach();
    }

    /// Opens Create Poll for the selected room.
    pub(crate) fn open_create_poll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(room) = self.navigation.selected_room.clone() else {
            return;
        };
        self.open_management(ManageKind::CreatePoll(room), window, cx);
        if let Some(page) = &mut self.management {
            page.poll_answers = (0..MIN_ANSWERS)
                .map(|index| {
                    cx.new(|cx| {
                        InputState::new(window, cx).placeholder(format!("Answer {}", index + 1))
                    })
                })
                .collect();
            page.poll_disclosed = true;
        }
        cx.notify();
    }

    fn add_poll_answer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(page) = &mut self.management
            && page.poll_answers.len() < MAX_ANSWERS
        {
            let number = page.poll_answers.len() + 1;
            let input =
                cx.new(|cx| InputState::new(window, cx).placeholder(format!("Answer {number}")));
            input.update(cx, |state, cx| state.focus(window, cx));
            page.poll_answers.push(input);
        }
        cx.notify();
    }

    fn create_poll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(page) = &mut self.management else {
            return;
        };
        let ManageKind::CreatePoll(room) = page.kind.clone() else {
            return;
        };
        let draft = PollDraft {
            question: page.name.read(cx).value().to_string(),
            answers: page
                .poll_answers
                .iter()
                .map(|input| input.read(cx).value().to_string())
                .collect(),
            disclosed: page.poll_disclosed,
        };
        if draft.validate().is_err() || self.preview {
            return;
        }
        page.busy = true;
        page.status.clear();
        let request = page.request;
        let account = self.snapshot.account_instance;
        self.job(
            window,
            cx,
            move |backend| backend.timeline_action(room, account, TimelineAction::SendPoll(draft)),
            move |this, result, window, cx| {
                let Some(page) = this.management.as_mut().filter(|p| p.request == request) else {
                    return;
                };
                page.busy = false;
                match result {
                    Ok(()) => this.close_management_page(window, cx),
                    Err(error) => page.status = error.to_string(),
                }
                cx.notify();
            },
        );
        cx.notify();
    }

    /// The Create Poll dialog's fields.
    pub(crate) fn poll_dialog(
        &self,
        page: &crate::ui::spaces::Management,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let filled = page
            .poll_answers
            .iter()
            .filter(|input| !input.read(cx).value().trim().is_empty())
            .count();
        let ready = !page.name.read(cx).value().trim().is_empty() && filled >= MIN_ANSWERS;
        let removable = page.poll_answers.len() > MIN_ANSWERS;
        let mut answers = v_flex().w_full().gap(px(6.));
        for (index, input) in page.poll_answers.iter().enumerate() {
            answers = answers.child(
                h_flex()
                    .w_full()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex_1()
                            .child(Input::new(input).aria_label(format!("Answer {}", index + 1))),
                    )
                    .when(removable, |row| {
                        row.child(
                            Button::new(("poll-remove-answer", index))
                                .ghost()
                                .size(px(26.))
                                .icon(IconName::X)
                                .accessibility_label(format!("Remove answer {}", index + 1))
                                .tooltip("Remove Answer")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(page) = &mut this.management
                                        && page.poll_answers.len() > MIN_ANSWERS
                                        && index < page.poll_answers.len()
                                    {
                                        page.poll_answers.remove(index);
                                    }
                                    cx.notify();
                                })),
                        )
                    }),
            );
        }
        v_flex()
            .w_full()
            .gap(px(10.))
            .child("Question")
            .child(
                div()
                    .w_full()
                    .child(Input::new(&page.name).aria_label("Poll question")),
            )
            .child("Answers")
            .child(answers)
            .when(page.poll_answers.len() < MAX_ANSWERS, |body| {
                body.child(
                    Button::new("poll-add-answer")
                        .ghost()
                        .small()
                        .self_start()
                        .icon(IconName::Plus)
                        .label("Add Answer")
                        .on_click(
                            cx.listener(|this, _, window, cx| this.add_poll_answer(window, cx)),
                        ),
                )
            })
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .gap(px(12.))
                    .py(px(4.))
                    .child(
                        v_flex().child("Show results before the poll ends").child(
                            div()
                                .text_size(px(11.))
                                .text_color(muted)
                                .child("People see the votes once they have voted."),
                        ),
                    )
                    .child(
                        switch::Switch::new("poll-disclosed")
                            .accessibility_label("Show results before the poll ends")
                            .checked(page.poll_disclosed)
                            .color(cx.theme().primary)
                            .on_change(cx.listener(|this, checked: &bool, _, cx| {
                                if let Some(page) = &mut this.management {
                                    page.poll_disclosed = *checked;
                                }
                                cx.notify();
                            })),
                    ),
            )
            .child(
                Button::new("poll-create")
                    .primary()
                    .self_end()
                    .label("Create Poll")
                    .disabled(self.preview || page.busy || !ready)
                    .on_click(cx.listener(|this, _, window, cx| this.create_poll(window, cx))),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{accessible_poll, poll_status, results_visible};
    use twayblade_core::conversation::{Poll, PollAnswer};

    fn poll(disclosed: bool, ended: bool, chosen: bool) -> Poll {
        Poll {
            question: "Lunch?".into(),
            disclosed,
            max_selections: 1,
            answers: vec![
                PollAnswer {
                    id: "a".into(),
                    text: "Pizza".into(),
                    votes: 2,
                    chosen,
                    winner: ended,
                },
                PollAnswer {
                    id: "b".into(),
                    text: "Soup".into(),
                    votes: 1,
                    ..Default::default()
                },
            ],
            voters: 3,
            ended,
            ..Default::default()
        }
    }

    #[test]
    fn results_show_after_voting_on_open_polls_and_when_ended() {
        assert!(!results_visible(&poll(true, false, false)));
        assert!(results_visible(&poll(true, false, true)));
        assert!(!results_visible(&poll(false, false, true)));
        assert!(results_visible(&poll(false, true, false)));
        assert_eq!(
            poll_status(&poll(true, false, false)),
            "3 votes · Vote to see the results"
        );
        assert_eq!(
            poll_status(&poll(false, true, false)),
            "Final results · 3 votes"
        );
    }

    #[test]
    fn screen_readers_hear_counts_only_when_they_show() {
        let open = accessible_poll(&poll(true, false, false));
        assert!(open.starts_with("Poll, Lunch?. Pizza; Soup."));
        let voted = accessible_poll(&poll(true, false, true));
        assert!(voted.contains("Pizza, 2 votes, your vote; Soup, 1 vote"));
    }
}
