use crate::matrix::MatrixEngine;
use cosmic::iced::Alignment;
use cosmic::widget::{
    Column, Row, button, radio, settings, text, text_input, tooltip, tooltip::Position,
};
use cosmic::{Action, Element, Task};
use matrix_sdk::room::power_levels::RoomPowerLevelChanges;
use matrix_sdk::ruma::RoomId;
use matrix_sdk::ruma::events::room::MediaSource;
use matrix_sdk::ruma::events::room::history_visibility::HistoryVisibility;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub struct State {
    pub room_id: Option<Arc<str>>,
    pub name: String,
    pub original_name: String,
    pub topic: String,
    pub original_topic: String,
    pub is_loading: bool,
    pub is_saving: bool,
    pub error: Option<String>,
    pub avatar_url: Option<String>,
    pub avatar_handle: Option<cosmic::iced::widget::image::Handle>,
    pub is_uploading_avatar: bool,
    pub is_loading_avatar: bool,
    pub membership: Option<matrix_sdk::RoomState>,
    pub power_levels: Option<(i64, HashMap<matrix_sdk::ruma::OwnedUserId, i64>)>,
    pub is_loading_power_levels: bool,
    pub updating_power_level_for: Option<String>,
    pub ban_level: i64,
    pub original_ban_level: i64,
    pub invite_level: i64,
    pub original_invite_level: i64,
    pub kick_level: i64,
    pub original_kick_level: i64,
    pub redact_level: i64,
    pub original_redact_level: i64,
    pub events_default_level: i64,
    pub original_events_default_level: i64,
    pub room_name_level: i64,
    pub original_room_name_level: i64,
    pub room_topic_level: i64,
    pub original_room_topic_level: i64,
    pub room_avatar_level: i64,
    pub original_room_avatar_level: i64,
    pub invite_level_str: String,
    pub kick_level_str: String,
    pub ban_level_str: String,
    pub redact_level_str: String,
    pub events_default_level_str: String,
    pub room_name_level_str: String,
    pub room_topic_level_str: String,
    pub room_avatar_level_str: String,
    pub invite_user_id: String,
    pub kick_user_id: String,
    pub ban_user_id: String,
    pub action_reason: String,
    pub current_user_id: Option<String>,
    pub my_power_level: i64,
    pub member_filter: String,
    pub notification_mode: Option<matrix_sdk::notification_settings::RoomNotificationMode>,
    pub is_loading_notifications: bool,
    pub join_rule: Option<matrix_sdk::ruma::events::room::join_rules::JoinRule>,
    pub history_visibility: Option<HistoryVisibility>,
    pub restricted_space_id: String,
    pub ignored_users: Vec<matrix_sdk::ruma::OwnedUserId>,
    pub is_encrypted: bool,
    pub canonical_alias: String,
    pub original_canonical_alias: String,
    pub alt_aliases: Vec<String>,
    pub original_alt_aliases: Vec<String>,
    pub new_alt_alias_input: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    LoadRoom(std::sync::Arc<str>),
    RoomLoaded(Box<Result<RoomInfo, String>>),
    NameChanged(String),
    TopicChanged(String),
    SaveRoom,
    RoomSaved(Result<(), String>),
    DismissError,
    AvatarMediaFetched(Result<Vec<u8>, String>),
    SelectAvatar,
    AvatarFileSelected(Option<std::path::PathBuf>),
    AvatarUploaded(Result<(), String>),
    LeaveRoom,
    RoomLeft(Result<(), String>),
    ForgetRoom,
    RoomForgotten(Result<(), String>),
    LoadPowerLevels,
    PowerLevelsLoaded(Result<PowerLevelInfo, String>),
    UpdatePowerLevel(String, i64),
    PowerLevelUpdated(String, Result<(), String>),
    BanLevelChanged(String),
    InviteLevelChanged(String),
    KickLevelChanged(String),
    RedactLevelChanged(String),
    EventsDefaultLevelChanged(String),
    RoomNameLevelChanged(String),
    RoomTopicLevelChanged(String),
    RoomAvatarLevelChanged(String),
    InviteUser,
    UserInvited(Result<(), String>),
    KickUser(String),
    UserKicked(String, Result<(), String>),
    BanUser(String),
    UserBanned(String, Result<(), String>),
    InviteUserIdChanged(String),
    ActionReasonChanged(String),
    MemberFilterChanged(String),
    NotificationModeChanged(matrix_sdk::notification_settings::RoomNotificationMode),
    NotificationModeSet(Result<(), String>),
    JoinRuleChanged(matrix_sdk::ruma::events::room::join_rules::JoinRule),
    HistoryVisibilityChanged(HistoryVisibility),
    RestrictedSpaceIdChanged(String),
    IgnoreUser(matrix_sdk::ruma::OwnedUserId),
    UnignoreUser(matrix_sdk::ruma::OwnedUserId),
    EnableEncryption,
    EncryptionEnabled(Result<(), String>),
    CanonicalAliasChanged(String),
    AltAliasAdded,
    AltAliasRemoved(String),
    NewAltAliasInputChanged(String),
}

#[derive(Debug, Clone)]
pub struct PowerLevelInfo {
    pub default_level: i64,
    pub users: HashMap<matrix_sdk::ruma::OwnedUserId, i64>,
    pub my_level: i64,
}

#[derive(Debug, Clone)]
pub struct RoomInfo {
    pub name: String,
    pub topic: String,
    pub avatar_url: Option<String>,
    pub membership: matrix_sdk::RoomState,
    pub ban_level: i64,
    pub invite_level: i64,
    pub kick_level: i64,
    pub redact_level: i64,
    pub events_default_level: i64,
    pub room_name_level: i64,
    pub room_topic_level: i64,
    pub room_avatar_level: i64,
    pub current_user_id: Option<String>,
    pub notification_mode: Option<matrix_sdk::notification_settings::RoomNotificationMode>,
    pub join_rule: Option<matrix_sdk::ruma::events::room::join_rules::JoinRule>,
    pub history_visibility: Option<HistoryVisibility>,
    pub ignored_users: Vec<matrix_sdk::ruma::OwnedUserId>,
    pub is_encrypted: bool,
    pub canonical_alias: Option<String>,
    pub alt_aliases: Vec<String>,
}

/// Lightweight `Copy` choice used as the radio value for join rule selection.
///
/// `JoinRule` itself is not `Copy` (it carries a `Restricted` payload), so it
/// cannot be used directly as a radio value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JoinRuleChoice {
    Public,
    Invite,
    Knock,
    Restricted,
}

/// Lightweight `Copy` choice used as the radio value for history visibility.
///
/// `HistoryVisibility` is not `Copy`, so it cannot be a radio value directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoryVisibilityChoice {
    Shared,
    Invited,
    Joined,
}

impl State {
    pub fn update(
        &mut self,
        message: Message,
        matrix: &Option<MatrixEngine>,
    ) -> Task<Action<crate::Message>> {
        match message {
            Message::LoadRoom(room_id) => {
                if let Some(matrix) = matrix {
                    self.room_id = Some(room_id.clone());
                    self.is_loading = true;
                    self.error = None;

                    let engine = matrix.clone();
                    Task::perform(
                        async move {
                            let room_id_parsed =
                                RoomId::parse(&room_id).map_err(|e| e.to_string())?;
                            let client = engine.client().await;
                            let room = client
                                .get_room(&room_id_parsed)
                                .ok_or_else(|| "Room not found".to_string())?;

                            let pl = room.power_levels().await.map_err(|e| e.to_string())?;
                            let current_user_id = client.user_id().map(|id| id.to_string());
                            let notification_settings = client.notification_settings().await;
                            let notification_mode = notification_settings
                                .get_user_defined_room_notification_mode(&room_id_parsed)
                                .await;

                            let join_rule = room
                                .get_state_event_static::<matrix_sdk::ruma::events::room::join_rules::RoomJoinRulesEventContent>()
                                .await
                                .ok()
                                .flatten()
                                .and_then(|e| e.deserialize().ok())
                                .and_then(|e| match e {
                                    matrix_sdk_base::deserialized_responses::SyncOrStrippedState::Sync(
                                        matrix_sdk::ruma::events::SyncStateEvent::Original(ev),
                                    ) => Some(ev.content.join_rule),
                                    matrix_sdk_base::deserialized_responses::SyncOrStrippedState::Stripped(
                                        ev,
                                    ) => Some(ev.content.join_rule),
                                    _ => None,
                                });

                            let history_visibility = room
                                .get_state_event_static::<matrix_sdk::ruma::events::room::history_visibility::RoomHistoryVisibilityEventContent>()
                                .await
                                .ok()
                                .flatten()
                                .and_then(|e| e.deserialize().ok())
                                .and_then(|e| match e {
                                    matrix_sdk_base::deserialized_responses::SyncOrStrippedState::Sync(
                                        matrix_sdk::ruma::events::SyncStateEvent::Original(ev),
                                    ) => Some(ev.content.history_visibility),
                                    matrix_sdk_base::deserialized_responses::SyncOrStrippedState::Stripped(
                                        ev,
                                    ) => Some(ev.content.history_visibility),
                                    _ => None,
                                });

                            let ignored_users = engine.ignored_users().await.unwrap_or_default();
                            let is_encrypted = room.encryption_settings().is_some();
                            let (canonical_alias, alt_aliases) = room
                                .get_state_event_static::<matrix_sdk::ruma::events::room::canonical_alias::RoomCanonicalAliasEventContent>()
                                .await
                                .ok()
                                .flatten()
                                .and_then(|e| e.deserialize().ok())
                                .and_then(|e| match e {
                                    matrix_sdk_base::deserialized_responses::SyncOrStrippedState::Sync(
                                        matrix_sdk::ruma::events::SyncStateEvent::Original(ev),
                                    ) => Some((
                                        ev.content.alias.map(|a| a.to_string()),
                                        ev.content.alt_aliases.into_iter().map(|a| a.to_string()).collect(),
                                    )),
                                    matrix_sdk_base::deserialized_responses::SyncOrStrippedState::Stripped(
                                        ev,
                                    ) => Some((
                                        ev.content.alias.map(|a| a.to_string()),
                                        ev.content.alt_aliases.into_iter().map(|a| a.to_string()).collect(),
                                    )),
                                    _ => None,
                                })
                                .unwrap_or((None, Vec::new()));

                            let mut avatar_url = room.avatar_url().map(|u| u.to_string());
                            if (room.joined_members_count() == 2
                                || room.active_members_count() == 2)
                                && let Some(my_user_id) = client.user_id()
                                && let Ok(members) = room
                                    .members_no_sync(matrix_sdk::RoomMemberships::ACTIVE)
                                    .await
                            {
                                let other_member =
                                    members.iter().find(|m| m.user_id() != my_user_id);
                                if let Some(other_member) = other_member
                                    && let Some(other_avatar) = other_member.avatar_url()
                                {
                                    avatar_url = Some(other_avatar.to_string());
                                }
                            }

                            Ok(RoomInfo {
                                name: room.name().unwrap_or_default(),
                                topic: room.topic().unwrap_or_default(),
                                avatar_url,
                                membership: room.state(),
                                ban_level: pl.ban.into(),
                                invite_level: pl.invite.into(),
                                kick_level: pl.kick.into(),
                                redact_level: pl.redact.into(),
                                events_default_level: pl.events_default.into(),
                                room_name_level: pl
                                    .events
                                    .get(&matrix_sdk::ruma::events::TimelineEventType::RoomName)
                                    .map(|l| (*l).into())
                                    .unwrap_or(pl.state_default.into()),
                                room_topic_level: pl
                                    .events
                                    .get(&matrix_sdk::ruma::events::TimelineEventType::RoomTopic)
                                    .map(|l| (*l).into())
                                    .unwrap_or(pl.state_default.into()),
                                room_avatar_level: pl
                                    .events
                                    .get(&matrix_sdk::ruma::events::TimelineEventType::RoomAvatar)
                                    .map(|l| (*l).into())
                                    .unwrap_or(pl.state_default.into()),
                                current_user_id,
                                notification_mode,
                                join_rule,
                                history_visibility,
                                ignored_users,
                                is_encrypted,
                                canonical_alias,
                                alt_aliases,
                            })
                        },
                        |res| {
                            Action::from(crate::Message::RoomSettings(Message::RoomLoaded(
                                Box::new(res),
                            )))
                        },
                    )
                } else {
                    Task::none()
                }
            }
            Message::RoomLoaded(res) => {
                self.is_loading = false;
                match *res {
                    Ok(info) => {
                        self.name = info.name.clone();
                        self.original_name = info.name;
                        self.topic = info.topic.clone();
                        self.original_topic = info.topic;
                        self.avatar_url = info.avatar_url;
                        self.membership = Some(info.membership);
                        self.kick_level = info.kick_level;
                        self.original_kick_level = info.kick_level;
                        self.kick_level_str = info.kick_level.to_string();
                        self.redact_level = info.redact_level;
                        self.original_redact_level = info.redact_level;
                        self.redact_level_str = info.redact_level.to_string();
                        self.ban_level = info.ban_level;
                        self.original_ban_level = info.ban_level;
                        self.ban_level_str = info.ban_level.to_string();
                        self.invite_level = info.invite_level;
                        self.original_invite_level = info.invite_level;
                        self.invite_level_str = info.invite_level.to_string();
                        self.events_default_level = info.events_default_level;
                        self.original_events_default_level = info.events_default_level;
                        self.events_default_level_str = info.events_default_level.to_string();
                        self.room_name_level = info.room_name_level;
                        self.original_room_name_level = info.room_name_level;
                        self.room_name_level_str = info.room_name_level.to_string();
                        self.room_topic_level = info.room_topic_level;
                        self.original_room_topic_level = info.room_topic_level;
                        self.room_topic_level_str = info.room_topic_level.to_string();
                        self.room_avatar_level = info.room_avatar_level;
                        self.original_room_avatar_level = info.room_avatar_level;
                        self.room_avatar_level_str = info.room_avatar_level.to_string();
                        self.current_user_id = info.current_user_id;
                        self.notification_mode = info.notification_mode;
                        self.join_rule = info.join_rule.clone();
                        self.history_visibility = info.history_visibility;
                        self.restricted_space_id = match &info.join_rule {
                            Some(matrix_sdk::ruma::events::room::join_rules::JoinRule::Restricted(r)) => {
                                r.allow.iter().find_map(|a| match a {
                                    matrix_sdk::ruma::events::room::join_rules::AllowRule::RoomMembership(m) => Some(m.room_id.to_string()),
                                    _ => None,
                                }).unwrap_or_default()
                            }
                            _ => String::new(),
                        };
                        self.ignored_users = info.ignored_users;
                        self.is_encrypted = info.is_encrypted;
                        self.canonical_alias = info.canonical_alias.clone().unwrap_or_default();
                        self.original_canonical_alias = info.canonical_alias.unwrap_or_default();
                        self.alt_aliases = info.alt_aliases.clone();
                        self.original_alt_aliases = info.alt_aliases;
                        self.new_alt_alias_input = String::new();
                        self.error = None;

                        let mut tasks = Vec::new();

                        if let Some(url) = &self.avatar_url
                            && let Some(matrix) = matrix
                        {
                            let engine = matrix.clone();
                            let mxc = url.clone();
                            self.is_loading_avatar = true;
                            tasks.push(Task::perform(
                                async move {
                                    let mxc_uri = <&matrix_sdk::ruma::MxcUri>::from(mxc.as_str());
                                    let source = MediaSource::Plain(mxc_uri.to_owned());
                                    engine.fetch_media(source).await.map_err(|e| e.to_string())
                                },
                                |res| {
                                    Action::from(crate::Message::RoomSettings(
                                        Message::AvatarMediaFetched(res),
                                    ))
                                },
                            ));
                        }

                        tasks.push(Task::done(Action::from(crate::Message::RoomSettings(
                            Message::LoadPowerLevels,
                        ))));
                        return Task::batch(tasks);
                    }
                    Err(e) => {
                        self.error = Some(e);
                    }
                }
                Task::none()
            }
            Message::HistoryVisibilityChanged(history_visibility) => {
                if let Some(matrix) = matrix
                    && let Some(room_id) = &self.room_id
                {
                    let engine = matrix.clone();
                    let room_id_clone = room_id.clone();
                    let room_id_clone_reload = room_id.clone();
                    return Task::perform(
                        async move {
                            engine
                                .set_room_history_visibility(&room_id_clone, history_visibility)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        move |res| {
                            Action::from(crate::Message::RoomSettings(match res {
                                Ok(_) => {
                                    // Reload room data to reflect changes
                                    Message::LoadRoom(room_id_clone_reload.clone())
                                }
                                Err(e) => Message::RoomSaved(Err(e)),
                            }))
                        },
                    );
                }
                Task::none()
            }
            Message::EventsDefaultLevelChanged(l) => {
                self.events_default_level_str = l.clone();
                if let Ok(l) = l.parse() {
                    self.events_default_level = l;
                }
                Task::none()
            }
            Message::RoomNameLevelChanged(l) => {
                self.room_name_level_str = l.clone();
                if let Ok(l) = l.parse() {
                    self.room_name_level = l;
                }
                Task::none()
            }
            Message::RoomTopicLevelChanged(l) => {
                self.room_topic_level_str = l.clone();
                if let Ok(l) = l.parse() {
                    self.room_topic_level = l;
                }
                Task::none()
            }
            Message::RoomAvatarLevelChanged(l) => {
                self.room_avatar_level_str = l.clone();
                if let Ok(l) = l.parse() {
                    self.room_avatar_level = l;
                }
                Task::none()
            }
            Message::LoadPowerLevels => {
                if let Some(matrix) = matrix
                    && let Some(room_id) = &self.room_id
                {
                    self.is_loading_power_levels = true;
                    let engine = matrix.clone();
                    let room_id_clone = room_id.clone();
                    return Task::perform(
                        async move {
                            let (default, users) = engine
                                .get_room_power_levels(&room_id_clone)
                                .await
                                .map_err(|e| e.to_string())?;
                            let client = engine.client().await;
                            let user_id = client.user_id().ok_or("No user ID")?;
                            let room = client
                                .get_room(
                                    &RoomId::parse(&room_id_clone).map_err(|e| e.to_string())?,
                                )
                                .ok_or("Room not found")?;
                            let my_level = match room.get_user_power_level(user_id).await {
                                    Ok(matrix_sdk::ruma::events::room::power_levels::UserPowerLevel::Int(l)) => l.into(),
                                    Ok(matrix_sdk::ruma::events::room::power_levels::UserPowerLevel::Infinite) => 100, // Room creators are basically 100+
                                    Ok(_) => 100, // Handle future versions gracefully
                                    Err(_) => default,
                                };
                            Ok(PowerLevelInfo {
                                default_level: default,
                                users,
                                my_level,
                            })
                        },
                        |res| {
                            Action::from(crate::Message::RoomSettings(Message::PowerLevelsLoaded(
                                res,
                            )))
                        },
                    );
                }
                Task::none()
            }
            Message::PowerLevelsLoaded(res) => {
                self.is_loading_power_levels = false;
                match res {
                    Ok(info) => {
                        self.power_levels = Some((info.default_level, info.users));
                        self.my_power_level = info.my_level;
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to load power levels: {}", e));
                    }
                }
                Task::none()
            }
            Message::InviteUser => {
                if let Some(matrix) = matrix
                    && let Some(room_id) = &self.room_id
                {
                    let engine = matrix.clone();
                    let room_id_clone = room_id.clone();
                    let user_id_clone = self.invite_user_id.clone();
                    return Task::perform(
                        async move {
                            engine
                                .invite_user(&room_id_clone, &user_id_clone)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| Action::from(crate::Message::RoomSettings(Message::UserInvited(res))),
                    );
                }
                Task::none()
            }
            Message::UserInvited(res) => {
                match res {
                    Ok(_) => {
                        self.invite_user_id = String::new();
                        self.error = None;
                        return Task::done(Action::from(crate::Message::RoomSettings(
                            Message::LoadPowerLevels,
                        )));
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to invite user: {}", e));
                    }
                }
                Task::none()
            }
            Message::KickUser(user_id) => {
                if let Some(matrix) = matrix
                    && let Some(room_id) = &self.room_id
                {
                    let engine = matrix.clone();
                    let room_id_clone = room_id.clone();
                    let user_id_clone = user_id.clone();
                    let user_id_for_task = user_id.clone();
                    let reason = if self.action_reason.is_empty() {
                        None
                    } else {
                        Some(self.action_reason.clone())
                    };
                    return Task::perform(
                        async move {
                            engine
                                .kick_user(&room_id_clone, &user_id_for_task, reason)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        move |res| {
                            Action::from(crate::Message::RoomSettings(Message::UserKicked(
                                user_id_clone,
                                res,
                            )))
                        },
                    );
                }
                Task::none()
            }
            Message::JoinRuleChanged(join_rule) => {
                self.join_rule = Some(join_rule.clone());
                if let Some(matrix) = matrix
                    && let Some(room_id) = &self.room_id
                {
                    let engine = matrix.clone();
                    let room_id_clone = room_id.clone();
                    let room_id_clone_reload = room_id.clone();
                    return Task::perform(
                        async move {
                            engine
                                .set_room_join_rule(&room_id_clone, join_rule)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        move |res| {
                            Action::from(crate::Message::RoomSettings(match res {
                                Ok(_) => {
                                    // Reload room data to reflect changes
                                    Message::LoadRoom(room_id_clone_reload.clone())
                                }
                                Err(e) => Message::RoomSaved(Err(e)),
                            }))
                        },
                    );
                }
                Task::none()
            }
            Message::UserKicked(user_id, res) => {
                match res {
                    Ok(_) => {
                        self.action_reason = String::new();
                        self.error = None;
                        return Task::done(Action::from(crate::Message::RoomSettings(
                            Message::LoadPowerLevels,
                        )));
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to kick {}: {}", user_id, e));
                    }
                }
                Task::none()
            }
            Message::BanUser(user_id) => {
                if let Some(matrix) = matrix
                    && let Some(room_id) = &self.room_id
                {
                    let engine = matrix.clone();
                    let room_id_clone = room_id.clone();
                    let user_id_clone = user_id.clone();
                    let user_id_for_task = user_id.clone();
                    let reason = if self.action_reason.is_empty() {
                        None
                    } else {
                        Some(self.action_reason.clone())
                    };
                    return Task::perform(
                        async move {
                            engine
                                .ban_user(&room_id_clone, &user_id_for_task, reason)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        move |res| {
                            Action::from(crate::Message::RoomSettings(Message::UserBanned(
                                user_id_clone,
                                res,
                            )))
                        },
                    );
                }
                Task::none()
            }
            Message::UserBanned(user_id, res) => {
                match res {
                    Ok(_) => {
                        self.action_reason = String::new();
                        self.error = None;
                        return Task::done(Action::from(crate::Message::RoomSettings(
                            Message::LoadPowerLevels,
                        )));
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to ban {}: {}", user_id, e));
                    }
                }
                Task::none()
            }
            Message::InviteUserIdChanged(id) => {
                self.invite_user_id = id;
                Task::none()
            }
            Message::ActionReasonChanged(r) => {
                self.action_reason = r;
                Task::none()
            }
            Message::MemberFilterChanged(f) => {
                self.member_filter = f;
                Task::none()
            }
            Message::UpdatePowerLevel(user_id, level) => {
                if let Some(matrix) = matrix
                    && let Some(room_id) = &self.room_id
                {
                    self.updating_power_level_for = Some(user_id.clone());
                    let engine = matrix.clone();
                    let room_id_clone = room_id.clone();
                    let user_id_clone = user_id.clone();
                    let user_id_for_task = user_id.clone();
                    return Task::perform(
                        async move {
                            engine
                                .update_user_power_level(&room_id_clone, &user_id_for_task, level)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        move |res| {
                            Action::from(crate::Message::RoomSettings(Message::PowerLevelUpdated(
                                user_id_clone,
                                res,
                            )))
                        },
                    );
                }
                Task::none()
            }
            Message::PowerLevelUpdated(user_id, res) => {
                self.updating_power_level_for = None;
                match res {
                    Ok(_) => {
                        self.invite_user_id = String::new();
                        return Task::done(Action::from(crate::Message::RoomSettings(
                            Message::LoadPowerLevels,
                        )));
                    }
                    Err(e) => {
                        self.error = Some(format!(
                            "Failed to update power level for {}: {}",
                            user_id, e
                        ));
                    }
                }
                Task::none()
            }
            Message::BanLevelChanged(l) => {
                self.ban_level_str = l.clone();
                if let Ok(l) = l.parse() {
                    self.ban_level = l;
                }
                Task::none()
            }
            Message::InviteLevelChanged(l) => {
                self.invite_level_str = l.clone();
                if let Ok(l) = l.parse() {
                    self.invite_level = l;
                }
                Task::none()
            }
            Message::KickLevelChanged(l) => {
                self.kick_level_str = l.clone();
                if let Ok(l) = l.parse() {
                    self.kick_level = l;
                }
                Task::none()
            }
            Message::RedactLevelChanged(l) => {
                self.redact_level_str = l.clone();
                if let Ok(l) = l.parse() {
                    self.redact_level = l;
                }
                Task::none()
            }
            Message::AvatarMediaFetched(res) => {
                self.is_loading_avatar = false;
                match res {
                    Ok(data) => {
                        self.avatar_handle =
                            Some(cosmic::iced::widget::image::Handle::from_bytes(data));
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to fetch avatar: {}", e));
                    }
                }
                Task::none()
            }
            Message::NameChanged(name) => {
                self.name = name;
                Task::none()
            }
            Message::TopicChanged(topic) => {
                self.topic = topic;
                Task::none()
            }
            Message::SaveRoom => {
                if let Some(matrix) = matrix {
                    if let Some(room_id) = &self.room_id {
                        self.is_saving = true;
                        self.error = None;

                        let engine = matrix.clone();
                        let new_name = self.name.clone();
                        let new_topic = self.topic.clone();
                        let room_id_clone = room_id.clone();
                        let original_name = self.original_name.clone();
                        let original_topic = self.original_topic.clone();
                        let original_ban = self.original_ban_level;
                        let original_invite = self.original_invite_level;
                        let original_kick = self.original_kick_level;
                        let original_redact = self.original_redact_level;
                        let original_events_default = self.original_events_default_level;
                        let original_room_name = self.original_room_name_level;
                        let original_room_topic = self.original_room_topic_level;
                        let original_room_avatar = self.original_room_avatar_level;

                        let new_ban = self.ban_level;
                        let new_invite = self.invite_level;
                        let new_kick = self.kick_level;
                        let new_redact = self.redact_level;
                        let original_canonical = self.original_canonical_alias.clone();
                        let new_canonical = if self.canonical_alias.is_empty() {
                            None
                        } else {
                            Some(self.canonical_alias.clone())
                        };
                        let original_alt = self.original_alt_aliases.clone();
                        let new_alt = self.alt_aliases.clone();
                        let new_events_default = self.events_default_level;
                        let new_room_name = self.room_name_level;
                        let new_room_topic = self.room_topic_level;
                        let new_room_avatar = self.room_avatar_level;

                        Task::perform(
                            async move {
                                if new_name != original_name {
                                    engine
                                        .set_room_name(&room_id_clone, new_name)
                                        .await
                                        .map_err(|e| e.to_string())?;
                                }
                                if new_topic != original_topic {
                                    engine
                                        .set_room_topic(&room_id_clone, new_topic)
                                        .await
                                        .map_err(|e| e.to_string())?;
                                }
                                if new_ban != original_ban
                                    || new_invite != original_invite
                                    || new_kick != original_kick
                                    || new_redact != original_redact
                                    || new_events_default != original_events_default
                                    || new_room_name != original_room_name
                                    || new_room_topic != original_room_topic
                                    || new_room_avatar != original_room_avatar
                                {
                                    engine
                                        .update_room_power_level_settings(
                                            &room_id_clone,
                                            RoomPowerLevelChanges {
                                                ban: if new_ban != original_ban {
                                                    Some(new_ban)
                                                } else {
                                                    None
                                                },
                                                invite: if new_invite != original_invite {
                                                    Some(new_invite)
                                                } else {
                                                    None
                                                },
                                                kick: if new_kick != original_kick {
                                                    Some(new_kick)
                                                } else {
                                                    None
                                                },
                                                redact: if new_redact != original_redact {
                                                    Some(new_redact)
                                                } else {
                                                    None
                                                },
                                                events_default: if new_events_default
                                                    != original_events_default
                                                {
                                                    Some(new_events_default)
                                                } else {
                                                    None
                                                },
                                                room_name: if new_room_name != original_room_name {
                                                    Some(new_room_name)
                                                } else {
                                                    None
                                                },
                                                room_topic: if new_room_topic != original_room_topic
                                                {
                                                    Some(new_room_topic)
                                                } else {
                                                    None
                                                },
                                                room_avatar: if new_room_avatar
                                                    != original_room_avatar
                                                {
                                                    Some(new_room_avatar)
                                                } else {
                                                    None
                                                },
                                                state_default: None,
                                                users_default: None,
                                                space_child: None,
                                                beacon: None,
                                                beacon_info: None,
                                            },
                                        )
                                        .await
                                        .map_err(|e| e.to_string())?;
                                }

                                if new_canonical.as_deref() != Some(&original_canonical)
                                    || new_alt != original_alt
                                {
                                    engine
                                        .update_room_aliases(&room_id_clone, new_canonical, new_alt)
                                        .await
                                        .map_err(|e| e.to_string())?;
                                }

                                Ok(())
                            },
                            |res| {
                                Action::from(crate::Message::RoomSettings(Message::RoomSaved(res)))
                            },
                        )
                    } else {
                        Task::none()
                    }
                } else {
                    Task::none()
                }
            }
            Message::RoomSaved(res) => {
                self.is_saving = false;
                match res {
                    Ok(_) => {
                        self.original_name = self.name.clone();
                        self.original_topic = self.topic.clone();
                        self.original_ban_level = self.ban_level;
                        self.original_invite_level = self.invite_level;
                        self.original_kick_level = self.kick_level;
                        self.original_redact_level = self.redact_level;
                        self.original_canonical_alias = self.canonical_alias.clone();
                        self.original_alt_aliases = self.alt_aliases.clone();
                        self.original_events_default_level = self.events_default_level;
                        self.original_room_name_level = self.room_name_level;
                        self.original_room_topic_level = self.room_topic_level;
                        self.original_room_avatar_level = self.room_avatar_level;
                        self.error = None;
                    }
                    Err(e) => {
                        self.error = Some(e);
                    }
                }
                Task::none()
            }
            Message::SelectAvatar => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif"])
                        .set_title("Select Room Avatar")
                        .pick_file()
                        .await
                        .map(|handle| handle.path().to_owned())
                },
                |res| {
                    Action::from(crate::Message::RoomSettings(Message::AvatarFileSelected(
                        res,
                    )))
                },
            ),
            Message::AvatarFileSelected(path_opt) => {
                if let Some(path) = path_opt
                    && let Some(matrix) = matrix
                {
                    self.is_uploading_avatar = true;
                    let engine = matrix.clone();
                    let room_id = self.room_id.clone().unwrap_or_default();

                    return Task::perform(
                        async move {
                            let data = std::fs::read(&path).map_err(|e| e.to_string())?;
                            let mime = mime_guess::from_path(&path)
                                .first_raw()
                                .unwrap_or("image/jpeg");
                            engine
                                .upload_room_avatar(&room_id, data, mime)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| {
                            Action::from(crate::Message::RoomSettings(Message::AvatarUploaded(res)))
                        },
                    );
                }
                Task::none()
            }
            Message::AvatarUploaded(res) => {
                self.is_uploading_avatar = false;
                match res {
                    Ok(_) => {
                        // Reload room data to get new avatar URL
                        if let Some(room_id) = &self.room_id {
                            return self.update(Message::LoadRoom(room_id.clone()), matrix);
                        }
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to upload avatar: {}", e));
                    }
                }
                Task::none()
            }
            Message::LeaveRoom => {
                if let Some(matrix) = matrix
                    && let Some(room_id) = &self.room_id
                {
                    self.is_saving = true;
                    let engine = matrix.clone();
                    let room_id_clone = room_id.clone();
                    return Task::perform(
                        async move {
                            engine
                                .leave_room(&room_id_clone)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| Action::from(crate::Message::RoomSettings(Message::RoomLeft(res))),
                    );
                }
                Task::none()
            }
            Message::RoomLeft(res) => {
                self.is_saving = false;
                match res {
                    Ok(_) => {
                        // Reload to update membership state UI
                        if let Some(room_id) = &self.room_id {
                            return self.update(Message::LoadRoom(room_id.clone()), matrix);
                        }
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to leave room: {}", e));
                    }
                }
                Task::none()
            }
            Message::ForgetRoom => {
                if let Some(matrix) = matrix
                    && let Some(room_id) = &self.room_id
                {
                    self.is_saving = true;
                    let engine = matrix.clone();
                    let room_id_clone = room_id.clone();
                    return Task::perform(
                        async move {
                            engine
                                .forget_room(&room_id_clone)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| {
                            Action::from(crate::Message::RoomSettings(Message::RoomForgotten(res)))
                        },
                    );
                }
                Task::none()
            }
            Message::RoomForgotten(res) => {
                self.is_saving = false;
                match res {
                    Ok(_) => {
                        // Close settings panel as the room is gone
                        return Task::done(Action::from(crate::Message::CloseSettings));
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to forget room: {}", e));
                    }
                }
                Task::none()
            }
            Message::NotificationModeChanged(mode) => {
                if let Some(matrix) = matrix
                    && let Some(room_id) = &self.room_id
                {
                    self.is_loading_notifications = true;
                    self.notification_mode = Some(mode);
                    let engine = matrix.clone();
                    let room_id_clone = room_id.clone();
                    return Task::perform(
                        async move {
                            let client = engine.client().await;
                            let ns = client.notification_settings().await;
                            let rid = RoomId::parse(&room_id_clone).map_err(|e| e.to_string())?;
                            ns.set_room_notification_mode(&rid, mode)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| {
                            Action::from(crate::Message::RoomSettings(
                                Message::NotificationModeSet(res),
                            ))
                        },
                    );
                }
                Task::none()
            }
            Message::NotificationModeSet(res) => {
                self.is_loading_notifications = false;
                if let Err(e) = res {
                    self.error = Some(e);
                }
                Task::none()
            }
            Message::DismissError => {
                self.error = None;
                Task::none()
            }
            Message::RestrictedSpaceIdChanged(id) => {
                self.restricted_space_id = id;
                Task::none()
            }
            Message::IgnoreUser(user_id) => {
                if let Some(matrix) = matrix
                    && let Some(room_id) = &self.room_id
                {
                    let engine = matrix.clone();
                    let room_id_clone_reload = room_id.clone();
                    return Task::perform(
                        async move {
                            engine
                                .ignore_user(&user_id)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        move |res| {
                            Action::from(crate::Message::RoomSettings(match res {
                                Ok(_) => Message::LoadRoom(room_id_clone_reload.clone()),
                                Err(e) => Message::RoomSaved(Err(e)),
                            }))
                        },
                    );
                }
                Task::none()
            }
            Message::UnignoreUser(user_id) => {
                if let Some(matrix) = matrix
                    && let Some(room_id) = &self.room_id
                {
                    let engine = matrix.clone();
                    let room_id_clone_reload = room_id.clone();
                    return Task::perform(
                        async move {
                            engine
                                .unignore_user(&user_id)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        move |res| {
                            Action::from(crate::Message::RoomSettings(match res {
                                Ok(_) => Message::LoadRoom(room_id_clone_reload.clone()),
                                Err(e) => Message::RoomSaved(Err(e)),
                            }))
                        },
                    );
                }
                Task::none()
            }
            Message::EnableEncryption => {
                if let Some(matrix) = matrix
                    && let Some(room_id) = &self.room_id
                {
                    let engine = matrix.clone();
                    let room_id_clone = room_id.clone();
                    return Task::perform(
                        async move {
                            engine
                                .enable_encryption(&room_id_clone)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| {
                            Action::from(crate::Message::RoomSettings(Message::EncryptionEnabled(
                                res,
                            )))
                        },
                    );
                }
                Task::none()
            }
            Message::EncryptionEnabled(res) => {
                match res {
                    Ok(_) => {
                        if let Some(room_id) = &self.room_id {
                            return self.update(Message::LoadRoom(room_id.clone()), matrix);
                        }
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to enable encryption: {}", e));
                    }
                }
                Task::none()
            }
            Message::CanonicalAliasChanged(alias) => {
                self.canonical_alias = alias;
                Task::none()
            }
            Message::AltAliasAdded => {
                let alias = self.new_alt_alias_input.trim().to_string();
                if !alias.is_empty() && !self.alt_aliases.contains(&alias) {
                    self.alt_aliases.push(alias);
                }
                self.new_alt_alias_input = String::new();
                Task::none()
            }
            Message::AltAliasRemoved(alias) => {
                self.alt_aliases.retain(|a| a != &alias);
                Task::none()
            }
            Message::NewAltAliasInputChanged(input) => {
                self.new_alt_alias_input = input;
                Task::none()
            }
        }
    }

    fn view_notifications(&self) -> Element<'_, Message> {
        use matrix_sdk::notification_settings::RoomNotificationMode;
        let mut r = Row::new().spacing(10);

        for mode in [
            RoomNotificationMode::AllMessages,
            RoomNotificationMode::MentionsAndKeywordsOnly,
            RoomNotificationMode::Mute,
        ] {
            let label = match mode {
                RoomNotificationMode::AllMessages => crate::fl!("notification-mode-all"),
                RoomNotificationMode::MentionsAndKeywordsOnly => {
                    crate::fl!("notification-mode-mentions")
                }
                RoomNotificationMode::Mute => crate::fl!("notification-mode-mute"),
            };

            r = r.push(radio(
                text::body(label),
                mode,
                self.notification_mode,
                move |_| Message::NotificationModeChanged(mode),
            ));
        }

        settings::section()
            .title(crate::fl!("notifications"))
            .add(settings::item_row(vec![r.wrap().into()]))
            .into()
    }

    fn view_error(&self) -> Option<Element<'_, Message>> {
        self.error.as_ref().map(|error| {
            settings::section()
                .add(settings::item(
                    error,
                    button::text(crate::fl!("dismiss")).on_press(Message::DismissError),
                ))
                .into()
        })
    }

    fn view_security(&self) -> Element<'_, Message> {
        let mut section = settings::section().title(crate::fl!("security"));

        let mut r = Row::new().spacing(10).align_y(Alignment::Center);

        if self.is_encrypted {
            r = r.push(button::suggested(crate::fl!("enabled")));
            section = section.add(settings::item(crate::fl!("e2e-encryption"), r));
        } else {
            r = r.push(
                button::destructive(crate::fl!("enable-encryption"))
                    .on_press(Message::EnableEncryption),
            );
            section = section
                .add(settings::item(crate::fl!("e2e-encryption"), r))
                .add(text::body(crate::fl!("encryption-warning")).size(12));
        }

        section.into()
    }

    fn view_profile(&self) -> Element<'_, Message> {
        let mut section = settings::section().title(crate::fl!("room-profile"));

        // Avatar Section
        let mut avatar_row = Row::new().spacing(20).align_y(Alignment::Center);
        if let Some(handle) = &self.avatar_handle {
            avatar_row = avatar_row.push(
                cosmic::widget::image(handle.clone())
                    .width(cosmic::iced::Length::Fixed(64.0))
                    .height(cosmic::iced::Length::Fixed(64.0)),
            );
        } else if self.is_loading_avatar {
            avatar_row = avatar_row.push(text::body(crate::fl!("loading")));
        } else {
            avatar_row = avatar_row.push(
                cosmic::widget::container(text::body(crate::fl!("no-avatar")))
                    .width(64)
                    .height(64)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center),
            );
        }

        let mut upload_btn = button::text(if self.is_uploading_avatar {
            crate::fl!("uploading")
        } else {
            crate::fl!("change-avatar")
        });
        if !self.is_uploading_avatar {
            upload_btn = upload_btn.on_press(Message::SelectAvatar);
        }
        avatar_row = avatar_row.push(upload_btn);
        section = section.add(avatar_row);

        section = section
            .add(settings::item(
                crate::fl!("room-name-label"),
                text_input(crate::fl!("room-name-label"), &self.name)
                    .on_input(Message::NameChanged),
            ))
            .add(settings::item(
                crate::fl!("room-topic-label"),
                text_input(crate::fl!("room-topic-label"), &self.topic)
                    .on_input(Message::TopicChanged),
            ));

        if let Some(id) = &self.room_id {
            section = section.add(settings::item(
                crate::fl!("room-id-label"),
                text_input("", id.as_ref()),
            ));
        }

        section.into()
    }

    fn view_aliases(&self) -> Element<'_, Message> {
        let mut section = settings::section().title(crate::fl!("room-aliases"));

        // Canonical Alias
        section = section.add(settings::item(
            crate::fl!("canonical-alias-label"),
            text_input("#alias:example.com", &self.canonical_alias)
                .on_input(Message::CanonicalAliasChanged),
        ));

        // Alternative Aliases
        section = section.add(text::body(crate::fl!("alternative-aliases")).size(12));
        for alias in &self.alt_aliases {
            section = section.add(settings::item(
                alias.as_str(),
                button::destructive(crate::fl!("remove"))
                    .on_press(Message::AltAliasRemoved(alias.clone())),
            ));
        }

        // Add Alternative Alias
        let is_empty = self.new_alt_alias_input.trim().is_empty();
        let mut add_btn = button::text(crate::fl!("add"));
        if !is_empty {
            add_btn = add_btn.on_press(Message::AltAliasAdded);
        }

        let add_widget: Element<'_, Message> = if is_empty {
            tooltip(
                add_btn,
                text::body(crate::fl!("enter-alias-to-add")),
                Position::Top,
            )
            .into()
        } else {
            add_btn.into()
        };

        let add_alias_layout = Column::new()
            .spacing(5)
            .push(text::body(crate::fl!("add-alternative-alias")).size(12))
            .push(
                Row::new()
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .push(
                        text_input("#new-alias:example.com", &self.new_alt_alias_input)
                            .on_input(Message::NewAltAliasInputChanged)
                            .on_submit(|_| Message::AltAliasAdded),
                    )
                    .push(add_widget),
            );

        section = section.add(settings::item_row(vec![add_alias_layout.into()]));

        section.into()
    }

    fn view_permissions(&self) -> Element<'_, Message> {
        use matrix_sdk::ruma::events::room::join_rules::{AllowRule, JoinRule, Restricted};

        let mut perm_col = Column::new().spacing(10);

        let is_restricted = matches!(self.join_rule, Some(JoinRule::Restricted(_)));

        let parsed_restricted_space_id = RoomId::parse(&self.restricted_space_id).ok();

        perm_col = perm_col.push(text::body(crate::fl!("join-rule")).width(180));
        let mut join_rule_row = Row::new().spacing(10).align_y(Alignment::Center);

        let selected_rule = match &self.join_rule {
            Some(JoinRule::Public) => Some(JoinRuleChoice::Public),
            Some(JoinRule::Invite) => Some(JoinRuleChoice::Invite),
            Some(JoinRule::Knock) => Some(JoinRuleChoice::Knock),
            Some(JoinRule::Restricted(_)) => Some(JoinRuleChoice::Restricted),
            _ => None,
        };

        for choice in [
            JoinRuleChoice::Public,
            JoinRuleChoice::Invite,
            JoinRuleChoice::Knock,
            JoinRuleChoice::Restricted,
        ] {
            let label = match choice {
                JoinRuleChoice::Public => crate::fl!("join-rule-public"),
                JoinRuleChoice::Invite => crate::fl!("join-rule-invite"),
                JoinRuleChoice::Knock => crate::fl!("join-rule-knock"),
                JoinRuleChoice::Restricted => crate::fl!("join-rule-restricted"),
            };

            let msg = match choice {
                JoinRuleChoice::Public => Message::JoinRuleChanged(JoinRule::Public),
                JoinRuleChoice::Invite => Message::JoinRuleChanged(JoinRule::Invite),
                JoinRuleChoice::Knock => Message::JoinRuleChanged(JoinRule::Knock),
                JoinRuleChoice::Restricted => {
                    if let Some(space_id) = &parsed_restricted_space_id {
                        let restricted =
                            Restricted::new(vec![AllowRule::room_membership(space_id.clone())]);
                        Message::JoinRuleChanged(JoinRule::Restricted(restricted))
                    } else {
                        Message::JoinRuleChanged(JoinRule::Restricted(Restricted::default()))
                    }
                }
            };

            join_rule_row =
                join_rule_row.push(radio(text::body(label), choice, selected_rule, move |_| {
                    msg
                }));
        }

        perm_col = perm_col.push(join_rule_row.wrap());

        perm_col = perm_col.push(text::body(crate::fl!("history-visibility")).width(180));
        let mut history_visibility_row = Row::new().spacing(10).align_y(Alignment::Center);

        let selected_visibility = match &self.history_visibility {
            Some(HistoryVisibility::Shared) => Some(HistoryVisibilityChoice::Shared),
            Some(HistoryVisibility::Invited) => Some(HistoryVisibilityChoice::Invited),
            Some(HistoryVisibility::Joined) => Some(HistoryVisibilityChoice::Joined),
            _ => None,
        };

        for choice in [
            HistoryVisibilityChoice::Shared,
            HistoryVisibilityChoice::Invited,
            HistoryVisibilityChoice::Joined,
        ] {
            let (label, visibility) = match choice {
                HistoryVisibilityChoice::Shared => (
                    crate::fl!("history-visibility-shared"),
                    HistoryVisibility::Shared,
                ),
                HistoryVisibilityChoice::Invited => (
                    crate::fl!("history-visibility-invited"),
                    HistoryVisibility::Invited,
                ),
                HistoryVisibilityChoice::Joined => (
                    crate::fl!("history-visibility-joined"),
                    HistoryVisibility::Joined,
                ),
            };

            history_visibility_row = history_visibility_row.push(radio(
                text::body(label),
                choice,
                selected_visibility,
                move |_| Message::HistoryVisibilityChanged(visibility),
            ));
        }

        perm_col = perm_col.push(history_visibility_row.wrap());
        if is_restricted || !self.restricted_space_id.is_empty() {
            let mut restricted_row = Row::new().spacing(10).align_y(Alignment::Center);
            restricted_row = restricted_row.push(text::body(crate::fl!("space-id")).width(100));
            restricted_row = restricted_row.push(
                text_input::text_input("!space_id:example.com", &self.restricted_space_id)
                    .on_input(Message::RestrictedSpaceIdChanged),
            );

            if let Some(space_id) = parsed_restricted_space_id {
                let current_restricted_match =
                    if let Some(JoinRule::Restricted(r)) = &self.join_rule {
                        r.allow.iter().any(|a| match a {
                            AllowRule::RoomMembership(m) => m.room_id == space_id,
                            _ => false,
                        })
                    } else {
                        false
                    };

                if !current_restricted_match {
                    restricted_row =
                        restricted_row.push(button::text(crate::fl!("apply")).on_press(
                            Message::JoinRuleChanged(JoinRule::Restricted(Restricted::default())),
                        ));
                }
            }

            perm_col = perm_col.push(restricted_row.wrap());
        }

        perm_col = perm_col.push(
            Row::new()
                .spacing(10)
                .align_y(Alignment::Center)
                .push(text::body(crate::fl!("invite-level")).width(100))
                .push(
                    text_input::text_input("50", &self.invite_level_str)
                        .on_input(Message::InviteLevelChanged),
                )
                .wrap(),
        );
        perm_col = perm_col.push(
            Row::new()
                .spacing(10)
                .align_y(Alignment::Center)
                .push(text::body(crate::fl!("kick-level")).width(100))
                .push(
                    text_input::text_input("50", &self.kick_level_str)
                        .on_input(Message::KickLevelChanged),
                )
                .wrap(),
        );
        perm_col = perm_col.push(
            Row::new()
                .spacing(10)
                .align_y(Alignment::Center)
                .push(text::body(crate::fl!("ban-level")).width(100))
                .push(
                    text_input::text_input("50", &self.ban_level_str)
                        .on_input(Message::BanLevelChanged),
                )
                .wrap(),
        );
        perm_col = perm_col.push(
            Row::new()
                .spacing(10)
                .align_y(Alignment::Center)
                .push(text::body(crate::fl!("redact-level")).width(100))
                .push(
                    text_input::text_input("50", &self.redact_level_str)
                        .on_input(Message::RedactLevelChanged),
                )
                .wrap(),
        );
        perm_col = perm_col.push(
            Row::new()
                .spacing(10)
                .align_y(Alignment::Center)
                .push(text::body(crate::fl!("send-messages-level")).width(100))
                .push(
                    text_input::text_input("0", &self.events_default_level_str)
                        .on_input(Message::EventsDefaultLevelChanged),
                )
                .wrap(),
        );
        perm_col = perm_col.push(
            Row::new()
                .spacing(10)
                .align_y(Alignment::Center)
                .push(text::body(crate::fl!("change-name-level")).width(100))
                .push(
                    text_input::text_input("50", &self.room_name_level_str)
                        .on_input(Message::RoomNameLevelChanged),
                )
                .wrap(),
        );
        perm_col = perm_col.push(
            Row::new()
                .spacing(10)
                .align_y(Alignment::Center)
                .push(text::body(crate::fl!("change-topic-level")).width(100))
                .push(
                    text_input::text_input("50", &self.room_topic_level_str)
                        .on_input(Message::RoomTopicLevelChanged),
                )
                .wrap(),
        );
        perm_col = perm_col.push(
            Row::new()
                .spacing(10)
                .align_y(Alignment::Center)
                .push(text::body(crate::fl!("change-avatar-level")).width(100))
                .push(
                    text_input::text_input("50", &self.room_avatar_level_str)
                        .on_input(Message::RoomAvatarLevelChanged),
                )
                .wrap(),
        );
        settings::section()
            .title(crate::fl!("permissions"))
            .add(settings::item_row(vec![perm_col.into()]))
            .into()
    }

    fn view_save_button(&self) -> Option<Element<'_, Message>> {
        let mut save_btn = button::text(if self.is_saving {
            crate::fl!("saving")
        } else {
            crate::fl!("save-changes")
        });

        let has_changes = self.name != self.original_name
            || self.topic != self.original_topic
            || self.ban_level != self.original_ban_level
            || self.invite_level != self.original_invite_level
            || self.kick_level != self.original_kick_level
            || self.redact_level != self.original_redact_level
            || self.canonical_alias != self.original_canonical_alias
            || self.alt_aliases != self.original_alt_aliases
            || self.events_default_level != self.original_events_default_level
            || self.room_name_level != self.original_room_name_level
            || self.room_topic_level != self.original_room_topic_level
            || self.room_avatar_level != self.original_room_avatar_level;

        if has_changes && !self.is_saving {
            save_btn = save_btn.on_press(Message::SaveRoom);
        }

        let widget: Element<'_, Message> = if !has_changes {
            tooltip(
                save_btn,
                text::body(crate::fl!("make-changes-to-save")),
                Position::Top,
            )
            .into()
        } else {
            save_btn.into()
        };

        Some(
            settings::section()
                .add(settings::item_row(vec![widget]))
                .into(),
        )
    }

    #[rust_analyzer::skip]
    fn view_manage_members(&self) -> Option<Element<'_, Message>> {
        if let Some((default_level, users)) = &self.power_levels {
            let mut section = settings::section().title(crate::fl!("manage-members"));

            section = section.add(settings::item(
                crate::fl!("filter-members"),
                text_input(
                    crate::fl!("filter-members-placeholder"),
                    &self.member_filter,
                )
                .on_input(Message::MemberFilterChanged),
            ));

            section = section
                .add(text::body(crate::fl!("default-level", level = default_level)).size(12));

            section = section.add(settings::item(
                crate::fl!("reason-for-action"),
                text_input(crate::fl!("reason-placeholder"), &self.action_reason)
                    .on_input(Message::ActionReasonChanged),
            ));

            let filter_is_ascii = self.member_filter.is_ascii();
            let filter_lower_fallback =
                (!filter_is_ascii).then(|| self.member_filter.to_lowercase());

            for (user_id, level) in users {
                let user_id_str = user_id.as_str();
                if !self.member_filter.is_empty() {
                    let matches = crate::contains_ignore_ascii_case(
                        user_id_str,
                        &self.member_filter,
                        filter_lower_fallback.as_deref(),
                    );

                    if !matches {
                        continue;
                    }
                }

                let is_updating = self.updating_power_level_for.as_deref() == Some(user_id_str);
                let is_me = self.current_user_id.as_deref() == Some(user_id_str);

                let mut user_col = Column::new().spacing(5);

                let user_row = Row::new()
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .push(text::body(user_id_str).size(14))
                    .push(text::body(level.to_string()).size(14))
                    .wrap();

                let mut level_row = Row::new().spacing(5);
                for l in [0, 50, 100] {
                    let mut btn = button::text(match l {
                        0 => crate::fl!("default"),
                        50 => crate::fl!("mod"),
                        100 => crate::fl!("admin"),
                        _ => "??".to_string(),
                    });
                    if !is_updating && *level != l {
                        btn = btn.on_press(Message::UpdatePowerLevel(user_id_str.to_string(), l));
                    }
                    level_row = level_row.push(btn);
                }

                user_col = user_col.push(user_row).push(level_row.wrap());

                if !is_me {
                    let mut action_row = Row::new().spacing(5);
                    if self.my_power_level >= self.kick_level {
                        action_row = action_row.push(
                            button::destructive(crate::fl!("kick"))
                                .on_press(Message::KickUser(user_id_str.to_string())),
                        );
                    }
                    if self.my_power_level >= self.ban_level {
                        action_row = action_row.push(
                            button::destructive(crate::fl!("ban"))
                                .on_press(Message::BanUser(user_id_str.to_string())),
                        );
                    }

                    let is_ignored = self.ignored_users.contains(user_id);
                    if is_ignored {
                        action_row = action_row.push(
                            button::text(crate::fl!("unignore"))
                                .on_press(Message::UnignoreUser(user_id.clone())),
                        );
                    } else {
                        action_row = action_row.push(
                            button::destructive(crate::fl!("ignore"))
                                .on_press(Message::IgnoreUser(user_id.clone())),
                        );
                    }

                    user_col = user_col.push(action_row.wrap());
                }

                section = section.add(user_col);
            }
            Some(section.into())
        } else {
            None
        }
    }

    fn view_invite(&self) -> Element<'_, Message> {
        let mut section = settings::section().title(crate::fl!("invite"));

        section = section.add(settings::item(
            crate::fl!("user-id"),
            text_input("@user:example.com", &self.invite_user_id)
                .on_input(Message::InviteUserIdChanged),
        ));

        let is_empty = self.invite_user_id.trim().is_empty();

        let mut invite_row = Row::new().spacing(10);

        if self.my_power_level >= self.invite_level {
            let mut invite_btn = button::text(crate::fl!("invite"));
            if !is_empty {
                invite_btn = invite_btn.on_press(Message::InviteUser);
            }

            let invite_widget: Element<'_, Message> = if is_empty {
                tooltip(
                    invite_btn,
                    text::body(crate::fl!("enter-user-id-to-invite")),
                    Position::Top,
                )
                .into()
            } else {
                invite_btn.into()
            };
            invite_row = invite_row.push(invite_widget);
        }

        section.add(invite_row).into()
    }

    fn view_membership_actions(&self) -> Option<Element<'_, Message>> {
        if let Some(membership) = &self.membership {
            use matrix_sdk::RoomState;
            let mut section = settings::section().title(crate::fl!("actions"));

            match membership {
                RoomState::Joined => {
                    section = section.add(settings::item(
                        crate::fl!("leave-room"),
                        button::destructive(crate::fl!("leave")).on_press(Message::LeaveRoom),
                    ));
                }
                RoomState::Left | RoomState::Invited => {
                    section = section.add(settings::item(
                        crate::fl!("forget-room"),
                        button::destructive(crate::fl!("forget")).on_press(Message::ForgetRoom),
                    ));
                }
                _ => {}
            }
            Some(section.into())
        } else {
            None
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        if self.is_loading {
            return settings::view_column(vec![text::body(crate::fl!("loading-room-data")).into()])
                .into();
        }

        let mut col = settings::view_column(vec![
            self.view_profile(),
            self.view_security(),
            self.view_aliases(),
            self.view_notifications(),
            self.view_permissions(),
        ]);

        if let Some(error_view) = self.view_error() {
            col = col.push(error_view);
        }

        if let Some(save_btn) = self.view_save_button() {
            col = col.push(save_btn);
        }

        col.into()
    }

    pub fn view_manage(&self) -> Element<'_, Message> {
        let mut col = settings::view_column(Vec::new());

        if let Some(members_view) = self.view_manage_members() {
            col = col.push(members_view);
        } else if self.is_loading_power_levels {
            col = col.push(text::body(crate::fl!("loading-members")));
        }

        col = col.push(self.view_invite());

        if let Some(actions_view) = self.view_membership_actions() {
            col = col.push(actions_view);
        }

        col.into()
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_changed() {
        let mut state = State::default();
        let _ = state.update(Message::NameChanged("New Room Name".to_string()), &None);
        assert_eq!(state.name, "New Room Name");
    }

    #[test]
    fn test_topic_changed() {
        let mut state = State::default();
        let _ = state.update(Message::TopicChanged("New Topic".to_string()), &None);
        assert_eq!(state.topic, "New Topic");
    }

    #[test]
    fn test_load_room_no_matrix() {
        let mut state = State::default();
        let room_id: Arc<str> = Arc::from("!some_room:example.com");
        let _ = state.update(Message::LoadRoom(room_id.clone()), &None);

        // Without matrix engine, it shouldn't try to load
        assert!(!state.is_loading);
        assert_eq!(state.room_id, None);
    }

    #[test]
    fn test_dismiss_error() {
        let mut state = State {
            error: Some("An error occurred".to_string()),
            ..Default::default()
        };

        let _ = state.update(Message::DismissError, &None);
        assert_eq!(state.error, None);
    }

    #[test]
    fn test_invite_user_id_changed() {
        let mut state = State::default();
        let _ = state.update(
            Message::InviteUserIdChanged("@user:example.com".to_string()),
            &None,
        );
        assert_eq!(state.invite_user_id, "@user:example.com");
    }

    #[test]
    fn test_action_reason_changed() {
        let mut state = State::default();
        let _ = state.update(Message::ActionReasonChanged("Spam".to_string()), &None);
        assert_eq!(state.action_reason, "Spam");
    }

    #[test]
    fn test_member_filter_changed() {
        let mut state = State::default();
        let _ = state.update(Message::MemberFilterChanged("John".to_string()), &None);
        assert_eq!(state.member_filter, "John");
    }

    #[test]
    fn test_join_rule_changed() {
        use matrix_sdk::ruma::events::room::join_rules::JoinRule;
        let mut state = State {
            room_id: Some(Arc::from("!room:example.com")),
            ..Default::default()
        };
        // This won't actually call the engine since we pass None, but we can check if it returns a Task
        let _task = state.update(Message::JoinRuleChanged(JoinRule::Public), &None);
        // The task should be none since matrix engine is None
        // We can't easily inspect Task, but we can verify it compiles and runs.
    }

    #[test]
    fn test_history_visibility_changed() {
        use matrix_sdk::ruma::events::room::history_visibility::HistoryVisibility;
        let mut state = State {
            room_id: Some(Arc::from("!room:example.com")),
            ..Default::default()
        };
        // This won't actually call the engine since we pass None
        let _task = state.update(
            Message::HistoryVisibilityChanged(HistoryVisibility::Shared),
            &None,
        );
    }

    #[test]
    fn test_restricted_space_id_changed() {
        let mut state = State::default();
        let _ = state.update(
            Message::RestrictedSpaceIdChanged("!space:example.com".to_string()),
            &None,
        );
        assert_eq!(state.restricted_space_id, "!space:example.com");
    }

    #[test]
    fn test_join_rule_changed_knock() {
        use matrix_sdk::ruma::events::room::join_rules::JoinRule;
        let mut state = State {
            room_id: Some(Arc::from("!room:example.com")),
            ..Default::default()
        };
        let _ = state.update(Message::JoinRuleChanged(JoinRule::Knock), &None);
        assert_eq!(state.join_rule, Some(JoinRule::Knock));
    }

    #[test]
    fn test_aliases_changed() {
        let mut state = State::default();

        // Test canonical alias change
        let _ = state.update(
            Message::CanonicalAliasChanged("#new:example.com".to_string()),
            &None,
        );
        assert_eq!(state.canonical_alias, "#new:example.com");

        // Test alt alias input
        let _ = state.update(
            Message::NewAltAliasInputChanged("#alt1:example.com".to_string()),
            &None,
        );
        assert_eq!(state.new_alt_alias_input, "#alt1:example.com");

        // Test alt alias addition
        let _ = state.update(Message::AltAliasAdded, &None);
        assert_eq!(state.alt_aliases, vec!["#alt1:example.com".to_string()]);
        assert_eq!(state.new_alt_alias_input, "");

        // Test alt alias removal
        let _ = state.update(
            Message::AltAliasRemoved("#alt1:example.com".to_string()),
            &None,
        );
        assert!(state.alt_aliases.is_empty());
    }
}
