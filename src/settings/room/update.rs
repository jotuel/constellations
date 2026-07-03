use crate::matrix::MatrixEngine;
use cosmic::{Action, Task};
use matrix_sdk::room::power_levels::RoomPowerLevelChanges;
use matrix_sdk::ruma::RoomId;
use matrix_sdk::ruma::events::room::MediaSource;

use super::message::{Message, PowerLevelInfo, RoomInfo};

impl super::state::State {
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
            Message::PendingPowerLevel(user_id, level) => {
                self.pending_power_level = Some((user_id, level));
                Task::none()
            }
            Message::CommitPowerLevel(user_id) => {
                if self.updating_power_level_for.as_deref() == Some(user_id.as_str()) {
                    return Task::none();
                }
                if let Some((draft_user, draft_level)) = self.pending_power_level.take()
                    && draft_user == user_id
                    && let Some(matrix) = matrix
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
                                .update_user_power_level(
                                    &room_id_clone,
                                    &user_id_for_task,
                                    draft_level,
                                )
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
                self.pending_power_level = None;
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
}
