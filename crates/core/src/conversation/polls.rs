//! Polls (#181): creating, voting on and ending MSC3381 polls through the
//! SDK timeline, which aggregates responses and the end event. The frontend
//! owns all wording; the fallback text sent for other clients is protocol.
use super::{Conversation, ConversationError};
use matrix_sdk::ruma::{
    OwnedEventId, UserId,
    events::{
        AnyMessageLikeEventContent,
        poll::{
            start::PollKind,
            unstable_end::UnstablePollEndEventContent,
            unstable_response::UnstablePollResponseEventContent,
            unstable_start::{
                NewUnstablePollStartEventContent, UnstablePollAnswer, UnstablePollAnswers,
                UnstablePollStartContentBlock, UnstablePollStartEventContent,
            },
        },
    },
};
use matrix_sdk_ui::timeline::PollState;

/// Answers a poll may offer (MSC3381).
pub const MIN_ANSWERS: usize = 2;
pub const MAX_ANSWERS: usize = 20;

/// A poll as the timeline shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Poll {
    pub question: String,
    /// Votes are visible before the poll ends.
    pub disclosed: bool,
    pub max_selections: u64,
    pub answers: Vec<PollAnswer>,
    /// People who voted.
    pub voters: usize,
    pub ended: bool,
    pub edited: bool,
    /// Sent by this account, so it may end it.
    pub mine: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PollAnswer {
    pub id: String,
    pub text: String,
    pub votes: usize,
    /// This account's current choice.
    pub chosen: bool,
    /// Ended, and no answer has more votes.
    pub winner: bool,
}

/// A poll to create.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PollDraft {
    pub question: String,
    pub answers: Vec<String>,
    pub disclosed: bool,
}

impl PollDraft {
    /// Trimmed question and non-empty answers, or `InvalidPoll`.
    pub fn validate(&self) -> Result<(String, Vec<String>), ConversationError> {
        let question = self.question.trim().to_owned();
        let answers: Vec<String> = self
            .answers
            .iter()
            .map(|answer| answer.trim().to_owned())
            .filter(|answer| !answer.is_empty())
            .collect();
        if question.is_empty() || !(MIN_ANSWERS..=MAX_ANSWERS).contains(&answers.len()) {
            return Err(ConversationError::InvalidPoll);
        }
        Ok((question, answers))
    }
}

/// The SDK's aggregated state for one poll, seen by `own`.
pub(crate) fn project(state: &PollState, sender: &UserId, own: &UserId) -> Poll {
    let results = state.results();
    let ended = results.end_time.is_some();
    let count = |id: &str| results.votes.get(id).map_or(0, Vec::len);
    let most = results
        .answers
        .iter()
        .map(|answer| count(&answer.id))
        .max()
        .unwrap_or(0);
    let mut voters: Vec<&String> = results.votes.values().flatten().collect();
    voters.sort();
    voters.dedup();
    Poll {
        question: results.question.clone(),
        disclosed: results.kind == PollKind::Disclosed,
        max_selections: results.max_selections.max(1),
        answers: results
            .answers
            .iter()
            .map(|answer| {
                let votes = count(&answer.id);
                PollAnswer {
                    id: answer.id.clone(),
                    text: answer.text.clone(),
                    votes,
                    chosen: results
                        .votes
                        .get(&answer.id)
                        .is_some_and(|users| users.iter().any(|user| user == own.as_str())),
                    winner: ended && most > 0 && votes == most,
                }
            })
            .collect(),
        voters: voters.len(),
        ended,
        edited: results.has_been_edited,
        mine: sender == own,
    }
}

/// Plain-text fallback for clients without polls: the question, then the
/// numbered answers.
fn fallback(question: &str, answers: &[String]) -> String {
    let mut text = question.to_owned();
    for (index, answer) in answers.iter().enumerate() {
        text.push_str(&format!("\n{}. {answer}", index + 1));
    }
    text
}

impl Conversation {
    /// Sends a new single-choice poll.
    pub async fn send_poll(&self, draft: PollDraft) -> Result<(), ConversationError> {
        self.ensure_text_channel()?;
        let (question, answers) = draft.validate()?;
        let options: Vec<UnstablePollAnswer> = answers
            .iter()
            .enumerate()
            .map(|(index, text)| UnstablePollAnswer::new(format!("answer-{index}"), text))
            .collect();
        let options =
            UnstablePollAnswers::try_from(options).map_err(|_| ConversationError::InvalidPoll)?;
        let mut block = UnstablePollStartContentBlock::new(question.clone(), options);
        block.kind = if draft.disclosed {
            PollKind::Disclosed
        } else {
            PollKind::Undisclosed
        };
        let content =
            NewUnstablePollStartEventContent::plain_text(fallback(&question, &answers), block);
        self.timeline
            .send(AnyMessageLikeEventContent::UnstablePollStart(
                UnstablePollStartEventContent::New(content),
            ))
            .await
            .map(|_| ())
            .map_err(|_| ConversationError::Queue)
    }

    /// The poll's start event, for a timeline row that is a sent poll.
    async fn poll_event(&self, id: &str) -> Result<(OwnedEventId, Poll), ConversationError> {
        let own = self
            .timeline
            .room()
            .client()
            .user_id()
            .ok_or(ConversationError::Unavailable)?
            .to_owned();
        let items = self.timeline.items().await;
        let event = items
            .iter()
            .find(|item| item.unique_id().0 == id)
            .and_then(|item| item.as_event())
            .ok_or(ConversationError::Unavailable)?;
        let matrix_sdk_ui::timeline::TimelineItemContent::MsgLike(content) = event.content() else {
            return Err(ConversationError::Unavailable);
        };
        let matrix_sdk_ui::timeline::MsgLikeKind::Poll(state) = &content.kind else {
            return Err(ConversationError::Unavailable);
        };
        let event_id = event
            .event_id()
            .ok_or(ConversationError::Unavailable)?
            .to_owned();
        Ok((event_id, project(state, event.sender(), &own)))
    }

    /// Votes for `answers` (replacing an earlier vote). Ended polls refuse.
    pub async fn vote_poll(&self, id: &str, answers: Vec<String>) -> Result<(), ConversationError> {
        self.ensure_text_channel()?;
        let (event_id, poll) = self.poll_event(id).await?;
        if poll.ended
            || answers.is_empty()
            || answers.len() as u64 > poll.max_selections
            || !answers
                .iter()
                .all(|answer| poll.answers.iter().any(|known| &known.id == answer))
        {
            return Err(ConversationError::InvalidPoll);
        }
        self.timeline
            .send(AnyMessageLikeEventContent::UnstablePollResponse(
                UnstablePollResponseEventContent::new(answers, event_id),
            ))
            .await
            .map(|_| ())
            .map_err(|_| ConversationError::Queue)
    }

    /// Ends a poll this account started.
    pub async fn end_poll(&self, id: &str) -> Result<(), ConversationError> {
        self.ensure_text_channel()?;
        let (event_id, poll) = self.poll_event(id).await?;
        if poll.ended || !poll.mine {
            return Err(ConversationError::InvalidPoll);
        }
        self.timeline
            .send(AnyMessageLikeEventContent::UnstablePollEnd(
                UnstablePollEndEventContent::new("The poll has ended.", event_id),
            ))
            .await
            .map(|_| ())
            .map_err(|_| ConversationError::Queue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drafts_need_a_question_and_two_to_twenty_answers() {
        let mut draft = PollDraft {
            question: "  Lunch?  ".into(),
            answers: vec!["Pizza".into(), "  ".into(), " Soup ".into()],
            disclosed: false,
        };
        assert_eq!(
            draft.validate().unwrap(),
            (
                "Lunch?".to_owned(),
                vec!["Pizza".to_owned(), "Soup".to_owned()]
            )
        );
        draft.answers = vec!["Only one".into(), "".into()];
        assert!(matches!(
            draft.validate(),
            Err(ConversationError::InvalidPoll)
        ));
        draft.answers = (0..21).map(|n| n.to_string()).collect();
        assert!(matches!(
            draft.validate(),
            Err(ConversationError::InvalidPoll)
        ));
        draft.answers = vec!["A".into(), "B".into()];
        draft.question = " ".into();
        assert!(matches!(
            draft.validate(),
            Err(ConversationError::InvalidPoll)
        ));
    }

    #[test]
    fn fallback_lists_numbered_answers() {
        assert_eq!(
            fallback("Lunch?", &["Pizza".into(), "Soup".into()]),
            "Lunch?\n1. Pizza\n2. Soup"
        );
    }
}
