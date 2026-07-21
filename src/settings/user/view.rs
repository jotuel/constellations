use cosmic::Element;
use cosmic::iced::Alignment;
use cosmic::widget::{Column, Row, button, icon::Named, settings, text, text_input};
use std::sync::Arc;

use super::message::Message;
use super::state::{State, VerificationUIState};
use crate::utils::widget::{disabled_or_tooltip, tooltip_button};

impl State {
    fn view_privacy<'a>(&'a self) -> Element<'a, Message> {
        settings::section()
            .title(crate::fl!("privacy-and-preferences"))
            .add(settings::item(
                crate::fl!("display-media-previews"),
                cosmic::widget::toggler(self.media_previews_display_policy)
                    .on_toggle(Message::ToggleMediaPreviewsDisplayPolicy),
            ))
            .add(settings::item(
                crate::fl!("display-invite-avatars"),
                cosmic::widget::toggler(self.invite_avatars_display_policy)
                    .on_toggle(Message::ToggleInviteAvatarsDisplayPolicy),
            ))
            .into()
    }

    fn view_notifications<'a>(&'a self) -> Element<'a, Message> {
        use matrix_sdk::notification_settings::RoomNotificationMode;

        let modes = [
            RoomNotificationMode::AllMessages,
            RoomNotificationMode::MentionsAndKeywordsOnly,
            RoomNotificationMode::Mute,
        ];

        let build_mode_row = |current_mode: Option<RoomNotificationMode>, is_dm: bool| {
            let mut r = Row::new().spacing(10);
            for mode in modes {
                let label = match mode {
                    RoomNotificationMode::AllMessages => {
                        crate::fl!("notification-mode-all-messages")
                    }
                    RoomNotificationMode::MentionsAndKeywordsOnly => {
                        crate::fl!("notification-mode-mentions-only")
                    }
                    RoomNotificationMode::Mute => crate::fl!("notification-mode-muted"),
                };

                let mut btn = if current_mode == Some(mode) {
                    button::suggested(label)
                } else {
                    button::text(label)
                };

                if current_mode != Some(mode) && !self.is_loading_global_notifications {
                    btn = btn.on_press(Message::GlobalNotificationModeChanged(is_dm, mode));
                }
                r = r.push(btn);
            }
            r.wrap()
        };

        settings::section()
            .title(crate::fl!("default-notification-settings"))
            .add(settings::item(
                crate::fl!("direct-messages"),
                build_mode_row(self.global_notification_mode_dm, true),
            ))
            .add(settings::item(
                crate::fl!("group-chats"),
                build_mode_row(self.global_notification_mode_group, false),
            ))
            .into()
    }

    fn view_keywords<'a>(&'a self) -> Element<'a, Message> {
        let mut section = settings::section()
            .title(crate::fl!("keyword-notifications"))
            .header(text::body(crate::fl!("keyword-notifications-description")));

        if self.is_loading_keywords {
            section = section.add(text::body(crate::fl!("loading-keywords")));
        } else {
            for keyword in &self.keywords {
                section = section.add(settings::item(
                    keyword.as_str(),
                    tooltip_button(
                        button::custom(cosmic::widget::icon::from_name("user-trash-symbolic"))
                            .class(cosmic::theme::Button::Destructive)
                            .on_press(Message::RemoveKeyword(keyword.clone())),
                        crate::fl!("remove-keyword"),
                    ),
                ));
            }

            let is_empty = self.new_keyword.trim().is_empty();
            let btn_widget: Element<'_, Message> = disabled_or_tooltip(
                button::text(crate::fl!("add")),
                !is_empty,
                Message::AddKeyword,
                crate::fl!("enter-keyword-to-add"),
            );

            let add_keyword_layout = Column::new()
                .spacing(5)
                .push(text::body(crate::fl!("add-keyword-title")).size(12))
                .push(
                    Row::new()
                        .spacing(10)
                        .align_y(Alignment::Center)
                        .push(
                            text_input(crate::fl!("new-keyword-placeholder"), &self.new_keyword)
                                .on_input(Message::NewKeywordChanged)
                                .on_submit(|_| Message::AddKeyword),
                        )
                        .push(btn_widget),
                );

            section = section.add(settings::item_row(vec![add_keyword_layout.into()]));
        }

        section.into()
    }

    fn view_profile<'a>(&'a self) -> Element<'a, Message> {
        if self.is_loading || self.is_loading_avatar {
            return text::body(crate::fl!("loading-profile")).into();
        }

        let mut avatar_col = Column::new().spacing(10).align_x(Alignment::Center);

        if let Some(handle) = &self.avatar_handle {
            avatar_col =
                avatar_col.push(cosmic::widget::image(handle.clone()).width(128).height(128));
        } else {
            avatar_col = avatar_col.push(
                cosmic::widget::container(text::body(crate::fl!("no-avatar")).size(16))
                    .width(128)
                    .height(128)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center),
            );
        }

        let mut avatar_btn = button::text(if self.is_uploading_avatar {
            crate::fl!("uploading")
        } else {
            crate::fl!("change-avatar")
        });

        if !self.is_uploading_avatar {
            avatar_btn = avatar_btn.on_press(Message::SelectAvatar);
        }

        avatar_col = avatar_col.push(avatar_btn);

        let mut save_btn = button::text(if self.is_saving {
            crate::fl!("saving")
        } else {
            crate::fl!("save-changes")
        });
        let has_changes = self.display_name != self.original_display_name;

        if has_changes && !self.is_saving {
            save_btn = save_btn.on_press(Message::SaveProfile);
        }

        let save_widget: Element<'_, Message> = if !has_changes && !self.is_saving {
            tooltip_button(save_btn, crate::fl!("make-changes-to-save"))
        } else {
            save_btn.into()
        };

        settings::section()
            .title(crate::fl!("profile"))
            .add(avatar_col)
            .add(settings::item(
                crate::fl!("display-name"),
                text_input(crate::fl!("display-name"), &self.display_name)
                    .on_input(Message::DisplayNameChanged),
            ))
            .add(settings::item_row(vec![save_widget]))
            .into()
    }

    fn view_password_change<'a>(&'a self) -> Element<'a, Message> {
        let mut section = settings::section().title(crate::fl!("change-password"));

        section = section
            .add(settings::item(
                crate::fl!("current-password"),
                text_input(crate::fl!("current-password"), &self.current_password)
                    .password()
                    .on_input(Message::CurrentPasswordChanged),
            ))
            .add(settings::item(
                crate::fl!("new-password"),
                text_input(crate::fl!("new-password"), &self.new_password)
                    .password()
                    .on_input(Message::NewPasswordChanged),
            ))
            .add(settings::item(
                crate::fl!("confirm-password"),
                text_input(crate::fl!("confirm-password"), &self.confirm_new_password)
                    .password()
                    .on_input(Message::ConfirmNewPasswordChanged),
            ));

        let is_empty = self.current_password.is_empty()
            || self.new_password.is_empty()
            || self.confirm_new_password.is_empty();

        let passwords_match = self.new_password == self.confirm_new_password;

        let mut pw_btn = button::text(if self.is_changing_password {
            crate::fl!("changing")
        } else {
            crate::fl!("change-password")
        });

        if !self.is_changing_password && !is_empty && passwords_match {
            pw_btn = pw_btn.on_press(Message::ChangePassword);
        }

        let pw_btn_widget: Element<'_, Message> = if !self.is_changing_password {
            if is_empty {
                tooltip_button(pw_btn, crate::fl!("fill-all-fields-to-change-password"))
            } else if !passwords_match {
                tooltip_button(pw_btn, crate::fl!("new-passwords-do-not-match"))
            } else {
                pw_btn.into()
            }
        } else {
            pw_btn.into()
        };

        section = section.add(settings::item_row(vec![pw_btn_widget]));

        if let Some(success) = &self.password_success {
            section = section.add(settings::item(
                success.as_str(),
                button::text(crate::fl!("dismiss")).on_press(Message::DismissPasswordSuccess),
            ));
        }

        section.into()
    }

    fn view_devices<'a>(&'a self) -> Element<'a, Message> {
        let mut section = settings::section().title(crate::fl!("devices-and-sessions"));

        if self.is_loading_devices {
            section = section.add(text::body(crate::fl!("loading-devices")));
        } else {
            let is_current_verified = self
                .devices
                .iter()
                .find(|d| d.is_current)
                .map(|d| d.is_verified)
                .unwrap_or(false);

            for device in &self.devices {
                let name = device
                    .display_name
                    .clone()
                    .unwrap_or_else(|| crate::fl!("unknown-device"));

                let mut action_row = Row::new().spacing(10).align_y(Alignment::Center);

                if device.is_verified {
                    action_row =
                        action_row.push(text::body(crate::fl!("verified-device")).size(14));
                    if !device.is_current && !is_current_verified {
                        action_row = action_row.push(
                            button::text(crate::fl!("verify"))
                                .on_press(Message::VerifyDevice(device.device_id.clone())),
                        );
                    }
                } else {
                    action_row =
                        action_row.push(text::body(crate::fl!("unverified-device")).size(14));
                    if !device.is_current {
                        action_row = action_row.push(
                            button::text(crate::fl!("verify"))
                                .on_press(Message::VerifyDevice(device.device_id.clone())),
                        );
                    }
                }

                let mut del_btn = button::destructive(if device.is_deleting {
                    crate::fl!("deleting")
                } else {
                    crate::fl!("delete")
                });
                if !device.is_deleting {
                    del_btn = del_btn.on_press(Message::DeleteDevice(device.device_id.clone()));
                }
                action_row = action_row.push(tooltip_button(del_btn, crate::fl!("delete-device")));

                let mut title_row = Row::new().spacing(10).align_y(Alignment::Center);
                if device.is_renaming {
                    title_row = title_row
                        .push(
                            text_input(crate::fl!("new-device-name"), &device.edit_name)
                                .on_input({
                                    let id = Arc::clone(&device.device_id);
                                    move |v| Message::EditDeviceNameChanged(id.clone(), v)
                                })
                                .on_submit(|_| Message::SaveDeviceName(device.device_id.clone())),
                        )
                        .push(
                            button::text(crate::fl!("save"))
                                .on_press(Message::SaveDeviceName(device.device_id.clone())),
                        )
                        .push(
                            button::text(crate::fl!("cancel"))
                                .on_press(Message::CancelRenameDevice(device.device_id.clone())),
                        );
                } else {
                    title_row = title_row
                        .push(text::body(name).size(14))
                        .push(text::body(format!("({})", device.device_id.as_ref())).size(12))
                        .push(tooltip_button(
                            button::icon(Named::new("document-edit-symbolic"))
                                .on_press(Message::StartRenameDevice(device.device_id.clone())),
                            crate::fl!("rename-device"),
                        ));
                }

                if device.is_current {
                    title_row = title_row.push(
                        cosmic::widget::container(
                            text::body(crate::fl!("current-device")).size(12),
                        )
                        .padding(2),
                    );
                }

                let device_layout = Column::new().spacing(6).push(title_row).push(action_row);

                section = section.add(settings::item_row(vec![device_layout.into()]));
            }

            section = section.add(settings::item(
                crate::fl!("password-to-delete-devices"),
                text_input(crate::fl!("password"), &self.device_delete_password)
                    .password()
                    .on_input(Message::DeviceDeletePasswordChanged),
            ));
        }

        section.into()
    }

    fn view_deactivate_account<'a>(&'a self) -> Element<'a, Message> {
        let deactivate_btn = button::destructive(if self.is_deactivating {
            crate::fl!("deactivating")
        } else {
            crate::fl!("deactivate-account")
        });

        let is_pass_empty = self.deactivate_password.is_empty();

        let deactivate_widget: Element<'_, Message> = disabled_or_tooltip(
            deactivate_btn,
            !is_pass_empty,
            Message::DeactivateAccount,
            crate::fl!("enter-password-to-deactivate"),
        );

        settings::section()
            .title(crate::fl!("deactivate-account"))
            .add(text::body(crate::fl!("deactivate-warning")))
            .add(settings::item(
                crate::fl!("password"),
                text_input(crate::fl!("confirm-password"), &self.deactivate_password)
                    .password()
                    .on_input(Message::DeactivatePasswordChanged),
            ))
            .add(settings::item(crate::fl!("deactivate"), deactivate_widget))
            .into()
    }

    fn view_cross_signing<'a>(&'a self) -> Element<'a, Message> {
        let mut section = settings::section().title(crate::fl!("cross-signing"));

        if self.is_loading_cross_signing {
            section = section.add(text::body(crate::fl!("loading-cross-signing-status")));
        } else if let Some(info) = &self.cross_signing_info {
            let status = &info.status;

            let build_key_row = |label: &str, has_key: bool, key_val: Option<&String>| {
                let mut c = Column::new().spacing(5);
                c = c.push(
                    Row::new()
                        .spacing(10)
                        .push(text::body(label.to_string()))
                        .push(text::body(if has_key {
                            crate::fl!("key-present")
                        } else {
                            crate::fl!("key-missing")
                        })),
                );
                if let Some(key) = key_val {
                    c = c.push(text::body(crate::fl!("public-key", key = key)).size(10));
                }
                c
            };

            section = section
                .add(settings::item(
                    crate::fl!("master-key"),
                    build_key_row("", status.has_master, info.master_key.as_ref()),
                ))
                .add(settings::item(
                    crate::fl!("self-signing-key"),
                    build_key_row("", status.has_self_signing, info.self_signing_key.as_ref()),
                ))
                .add(settings::item(
                    crate::fl!("user-signing-key"),
                    build_key_row("", status.has_user_signing, info.user_signing_key.as_ref()),
                ));

            if !status.is_complete() {
                let mut btn = button::text(if self.is_bootstrapping {
                    crate::fl!("bootstrapping")
                } else {
                    crate::fl!("bootstrap-cross-signing")
                });
                if !self.is_bootstrapping {
                    btn = btn.on_press(Message::BootstrapCrossSigning);
                }
                section = section.add(settings::item_row(vec![btn.into()]));
            }
        } else {
            let mut btn = button::text(if self.is_bootstrapping {
                crate::fl!("bootstrapping")
            } else {
                crate::fl!("setup-cross-signing")
            });
            if !self.is_bootstrapping {
                btn = btn.on_press(Message::BootstrapCrossSigning);
            }
            section = section.add(settings::item_row(vec![btn.into()]));
        }

        section.into()
    }

    fn view_3pids<'a>(&'a self) -> Element<'a, Message> {
        let mut section = settings::section().title(crate::fl!("emails-and-phone-numbers"));

        if self.is_loading_3pids {
            section = section.add(text::body(crate::fl!("loading-linked-accounts")));
        } else {
            for t in &self.threepids {
                section = section.add(settings::item(
                    t.address.as_str(),
                    button::destructive(crate::fl!("remove"))
                        .on_press(Message::Delete3PID(t.address.clone(), t.medium.clone())),
                ));
            }

            let is_email_empty = self.new_3pid_email.trim().is_empty();
            let email_widget: Element<'_, Message> = disabled_or_tooltip(
                button::text(crate::fl!("send-verification")),
                !is_email_empty,
                Message::Request3PIDEmailToken,
                crate::fl!("enter-email-to-link"),
            );

            let link_email_layout = Column::new()
                .spacing(5)
                .push(text::body(crate::fl!("link-email")).size(12))
                .push(
                    Row::new()
                        .spacing(10)
                        .align_y(Alignment::Center)
                        .push(
                            text_input("email@example.com", &self.new_3pid_email)
                                .on_input(Message::New3PIDEmailChanged),
                        )
                        .push(email_widget),
                );

            section = section.add(settings::item_row(vec![link_email_layout.into()]));

            if let Some(sid) = &self.adding_3pid_sid {
                section = section.add(settings::item(
                    crate::fl!("verification-session"),
                    text::body(sid.as_str()),
                ));

                section = section.add(settings::item(
                    crate::fl!("confirm-with-password"),
                    text_input(crate::fl!("password"), &self.add_3pid_password)
                        .password()
                        .on_input(Message::Add3PIDPasswordChanged),
                ));

                let is_pass_empty = self.add_3pid_password.is_empty();
                let complete_widget: Element<'_, Message> = disabled_or_tooltip(
                    button::suggested(crate::fl!("add-account")),
                    !is_pass_empty,
                    Message::Add3PID,
                    crate::fl!("enter-password-to-confirm"),
                );

                section = section.add(settings::item(crate::fl!("complete"), complete_widget));
            }

            let is_phone_empty = self.new_3pid_msisdn.trim().is_empty();
            let phone_widget: Element<'_, Message> = disabled_or_tooltip(
                button::text(crate::fl!("send-sms")),
                !is_phone_empty,
                Message::Request3PIDMsisdnToken,
                crate::fl!("enter-phone-to-link"),
            );

            let link_phone_layout = Column::new()
                .spacing(5)
                .push(text::body(crate::fl!("link-phone")).size(12))
                .push(
                    Row::new()
                        .spacing(10)
                        .align_y(Alignment::Center)
                        .push(
                            text_input("+1", &self.new_3pid_country_code)
                                .width(50)
                                .on_input(Message::New3PIDCountryCodeChanged),
                        )
                        .push(
                            text_input(crate::fl!("phone-number"), &self.new_3pid_msisdn)
                                .on_input(Message::New3PIDMsisdnChanged),
                        )
                        .push(phone_widget),
                );

            section = section.add(settings::item_row(vec![link_phone_layout.into()]));
        }

        section.into()
    }

    fn view_ignored_users<'a>(&'a self) -> Element<'a, Message> {
        let mut section = settings::section().title(crate::fl!("ignored-users"));

        if self.is_loading_ignored_users {
            section = section.add(text::body(crate::fl!("loading-ignored-users")));
        } else {
            if self.ignored_users.is_empty() {
                section = section.add(text::body(crate::fl!("no-ignored-users")));
            } else {
                for user_id in &self.ignored_users {
                    section = section.add(settings::item(
                        user_id.as_str(),
                        button::text(crate::fl!("unignore"))
                            .on_press(Message::UnignoreUser(user_id.clone())),
                    ));
                }
            }

            let is_user_empty = self.new_ignore_user_id.trim().is_empty();
            let ignore_widget: Element<'_, Message> = disabled_or_tooltip(
                button::destructive(crate::fl!("ignore")),
                !is_user_empty,
                Message::IgnoreUser,
                crate::fl!("enter-user-id-to-ignore"),
            );

            let ignore_layout = Column::new()
                .spacing(5)
                .push(text::body(crate::fl!("ignore")).size(12))
                .push(
                    Row::new()
                        .spacing(10)
                        .align_y(Alignment::Center)
                        .push(
                            text_input("@user:example.com", &self.new_ignore_user_id)
                                .on_input(Message::NewIgnoreUserIdChanged)
                                .on_submit(|_| Message::IgnoreUser),
                        )
                        .push(ignore_widget),
                );

            section = section.add(settings::item_row(vec![ignore_layout.into()]));
        }

        section.into()
    }

    fn view_verification<'a>(&'a self) -> Element<'a, Message> {
        if self.verification_ui_state == VerificationUIState::None {
            return Column::new().into();
        }

        let mut section = settings::section().title(crate::fl!("verification"));
        match &self.verification_ui_state {
            VerificationUIState::WaitingForOtherDevice => {
                section = section.add(text::body(crate::fl!("waiting-for-other-device")));
            }
            VerificationUIState::ShowingEmojis(emojis) => {
                let mut emoji_row = Row::new().spacing(20);
                for (symbol, desc) in emojis {
                    emoji_row = emoji_row.push(
                        Column::new()
                            .spacing(5)
                            .align_x(Alignment::Center)
                            .push(text::body(symbol).size(32))
                            .push(text::body(desc).size(10)),
                    );
                }
                section = section.add(emoji_row.wrap());
                section = section.add(text::body(crate::fl!("do-emojis-match")));
                section = section.add(
                    Row::new()
                        .spacing(10)
                        .push(
                            button::suggested(crate::fl!("match")).on_press(Message::ConfirmEmojis),
                        )
                        .push(
                            button::destructive(crate::fl!("cancel"))
                                .on_press(Message::CancelVerification),
                        )
                        .wrap(),
                );
            }
            VerificationUIState::Done => {
                section = section.add(text::body(crate::fl!("verification-successful")));
                section = section
                    .add(button::text(crate::fl!("done")).on_press(Message::CancelVerification));
            }
            VerificationUIState::Cancelled => {
                section = section.add(text::body(crate::fl!("verification-cancelled")));
                section = section
                    .add(button::text(crate::fl!("dismiss")).on_press(Message::CancelVerification));
            }
            _ => {}
        }
        section.into()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut col = settings::view_column(vec![
            self.view_profile(),
            self.view_notifications(),
            self.view_privacy(),
            self.view_keywords(),
            self.view_password_change(),
            self.view_ignored_users(),
            self.view_devices(),
            self.view_verification(),
            self.view_cross_signing(),
            self.view_3pids(),
            self.view_deactivate_account(),
        ]);

        if let Some(err) = &self.error {
            col = col.push(settings::section().add(settings::item(
                err.as_str(),
                button::text(crate::fl!("dismiss")).on_press(Message::DismissError),
            )));
        }

        if let Some(msg) = &self.success_message {
            col = col.push(settings::section().add(settings::item(
                msg.as_str(),
                button::text(crate::fl!("dismiss")).on_press(Message::DismissSuccessMessage),
            )));
        }

        col.into()
    }
}
