//! Server notification rules. Local device delivery preferences belong to the UI.
mod delivery;
use crate::auth::Account;
pub use delivery::{
    ActivationState, NotificationContext, NotificationItem, NotificationPreview, should_notify,
};
use matrix_sdk::notification_settings::RoomNotificationMode;
use matrix_sdk::{notification_settings::NotificationSettings, ruma::RoomId};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DefaultLevel {
    All,
    #[default]
    Mentions,
}
impl From<DefaultLevel> for RoomNotificationMode {
    fn from(level: DefaultLevel) -> Self {
        match level {
            DefaultLevel::All => Self::AllMessages,
            DefaultLevel::Mentions => Self::MentionsAndKeywordsOnly,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RoomMode {
    #[default]
    Default,
    All,
    Mentions,
    Mute,
}
impl RoomMode {
    fn from_sdk(mode: Option<RoomNotificationMode>) -> Self {
        match mode {
            None => Self::Default,
            Some(RoomNotificationMode::AllMessages) => Self::All,
            Some(RoomNotificationMode::MentionsAndKeywordsOnly) => Self::Mentions,
            Some(RoomNotificationMode::Mute) => Self::Mute,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoomClass {
    pub encrypted: bool,
    pub direct: bool,
}
impl RoomClass {
    pub const ALL: [Self; 4] = [
        Self {
            encrypted: false,
            direct: true,
        },
        Self {
            encrypted: true,
            direct: true,
        },
        Self {
            encrypted: false,
            direct: false,
        },
        Self {
            encrypted: true,
            direct: false,
        },
    ];
    pub fn index(self) -> usize {
        usize::from(!self.direct) * 2 + usize::from(self.encrypted)
    }
}
#[derive(Clone, Default, PartialEq, Eq)]
pub struct RulesSnapshot {
    pub ready: bool,
    pub defaults: [DefaultLevel; 4],
    pub keywords: Vec<String>,
    pub rooms: HashMap<String, RoomMode>,
}
impl RulesSnapshot {
    pub fn room_mode(&self, room: &str) -> RoomMode {
        self.rooms.get(room).copied().unwrap_or_default()
    }
}
#[derive(Clone)]
pub struct NotificationRules(NotificationSettings, matrix_sdk::Client);
#[derive(Debug, thiserror::Error)]
pub enum RulesError {
    #[error("notification_rules_unavailable")]
    Unavailable,
    #[error("notification_rule_update_failed")]
    Update,
    #[error("invalid_room_id")]
    InvalidRoom,
    #[error("empty_keyword")]
    EmptyKeyword,
}
#[derive(Clone)]
pub enum RuleChange {
    Default(RoomClass, DefaultLevel),
    AddKeyword(String),
    RemoveKeyword(String),
    Room(String, RoomMode),
}
impl Account {
    pub async fn notification_rules(&self) -> Result<NotificationRules, RulesError> {
        self.client
            .account()
            .push_rules()
            .await
            .map_err(|_| RulesError::Unavailable)?;
        Ok(NotificationRules(
            self.client.notification_settings().await,
            self.client.clone(),
        ))
    }
}
impl NotificationRules {
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<()> {
        self.0.subscribe_to_changes()
    }
    pub async fn snapshot(&self) -> RulesSnapshot {
        let mut snapshot = RulesSnapshot {
            ready: true,
            ..Default::default()
        };
        for class in RoomClass::ALL {
            snapshot.defaults[class.index()] = match self
                .0
                .get_default_room_notification_mode(class.encrypted.into(), class.direct.into())
                .await
            {
                RoomNotificationMode::AllMessages => DefaultLevel::All,
                _ => DefaultLevel::Mentions,
            };
        }
        snapshot.keywords = self.0.enabled_keywords().await.into_iter().collect();
        for id in self.0.get_rooms_with_user_defined_rules(Some(true)).await {
            if let Ok(room) = RoomId::parse(&id) {
                snapshot.rooms.insert(
                    id,
                    RoomMode::from_sdk(self.0.get_user_defined_room_notification_mode(&room).await),
                );
            }
        }
        snapshot
    }
    pub async fn change(&self, requested: RuleChange) -> Result<(), RulesError> {
        let mut changes = self.subscribe();
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            for _ in 0..4 {
                // Each edit starts from synced SDK account data. A multi-command
                // edit can race an older intermediate echo, so an idempotent
                // retry must use a fresh SDK view rather than the stale one.
                let operation =
                    NotificationRules(self.1.notification_settings().await, self.1.clone());
                let result = operation.apply(requested.clone()).await;
                if matches!(
                    result,
                    Err(RulesError::EmptyKeyword | RulesError::InvalidRoom)
                ) {
                    return result;
                }
                loop {
                    let persisted =
                        NotificationRules(self.1.notification_settings().await, self.1.clone())
                            .snapshot()
                            .await;
                    if requested.matches(&persisted) && requested.matches(&self.snapshot().await) {
                        return Ok(());
                    }
                    let changed = if result.is_err() {
                        tokio::time::timeout(std::time::Duration::from_secs(2), changes.recv())
                            .await
                            .map_err(|_| RulesError::Update)?
                    } else {
                        changes.recv().await
                    };
                    match changed {
                        Ok(()) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                        Err(_) => return Err(RulesError::Unavailable),
                    }
                    if result.is_err() {
                        break;
                    }
                }
            }
            Err(RulesError::Update)
        })
        .await
        .map_err(|_| RulesError::Update)?
    }

    async fn apply(&self, change: RuleChange) -> Result<(), RulesError> {
        let result = match change {
            RuleChange::Default(class, level) => {
                self.0
                    .set_default_room_notification_mode(
                        class.encrypted.into(),
                        class.direct.into(),
                        level.into(),
                    )
                    .await
            }
            RuleChange::AddKeyword(word) => {
                if word.trim().is_empty() {
                    return Err(RulesError::EmptyKeyword);
                }
                self.0.add_keyword(word.trim().to_owned()).await
            }
            RuleChange::RemoveKeyword(word) => self.0.remove_keyword(&word).await,
            RuleChange::Room(id, mode) => {
                let id = RoomId::parse(id).map_err(|_| RulesError::InvalidRoom)?;
                match mode {
                    RoomMode::Default => self.0.delete_user_defined_room_rules(&id).await,
                    mode => {
                        self.0
                            .set_room_notification_mode(
                                &id,
                                match mode {
                                    RoomMode::All => RoomNotificationMode::AllMessages,
                                    RoomMode::Mentions => {
                                        RoomNotificationMode::MentionsAndKeywordsOnly
                                    }
                                    _ => RoomNotificationMode::Mute,
                                },
                            )
                            .await
                    }
                }
            }
        };
        result.map_err(|_| RulesError::Update)
    }
}

impl RuleChange {
    fn matches(&self, snapshot: &RulesSnapshot) -> bool {
        match self {
            Self::Default(class, level) => snapshot.defaults[class.index()] == *level,
            Self::AddKeyword(word) => snapshot.keywords.iter().any(|entry| entry == word.trim()),
            Self::RemoveKeyword(word) => !snapshot.keywords.contains(word),
            Self::Room(room, mode) => snapshot.room_mode(room) == *mode,
        }
    }
}
