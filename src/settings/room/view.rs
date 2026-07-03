use cosmic::Element;
use cosmic::iced::Alignment;
use cosmic::widget::{
    Column, Row, button, radio, settings, slider, text, text_input, tooltip, tooltip::Position,
};
use matrix_sdk::ruma::RoomId;
use matrix_sdk::ruma::events::room::history_visibility::HistoryVisibility;

use super::message::Message;
use super::state::State;

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

                let is_me = self.current_user_id.as_deref() == Some(user_id_str);

                let mut user_col = Column::new().spacing(5);

                let current_level = match &self.pending_power_level {
                    Some((uid, l)) if uid == user_id_str => *l,
                    _ => *level,
                };

                let user_row = Row::new()
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .push(text::body(user_id_str).size(14))
                    .push(text::body(current_level.to_string()).size(14))
                    .wrap();

                let level_slider = slider(0..=100, current_level.clamp(0, 100) as i32, move |l| {
                    Message::PendingPowerLevel(user_id_str.to_string(), l as i64)
                })
                .on_release(Message::CommitPowerLevel(user_id_str.to_string()));

                user_col = user_col
                    .push(user_row)
                    .push(Row::new().spacing(10).push(level_slider).wrap());

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
        } else {
            println!("No members view")
        }

        col = col.push(self.view_invite());

        if let Some(actions_view) = self.view_membership_actions() {
            col = col.push(actions_view);
        }

        col.into()
    }
}
