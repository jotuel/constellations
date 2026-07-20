use crate::matrix::MatrixEngine;
use cosmic::{Action, Task};
use matrix_sdk::encryption::verification::{SasState, VerificationRequestState};
use std::sync::Arc;

use super::message::Message;
use super::state::{CrossSigningInfo, DeviceInfo, State, Threepid, VerificationUIState};

impl State {
    pub fn update(
        &mut self,
        message: Message,
        matrix: &Option<MatrixEngine>,
    ) -> Task<Action<crate::Message>> {
        match message {
            Message::LoadProfile => {
                if let Some(matrix) = matrix {
                    self.is_loading = true;
                    self.error = None;
                    self.is_loading_avatar = true;
                    let matrix_name = matrix.clone();
                    let t_name = Task::perform(
                        async move {
                            matrix_name
                                .client()
                                .await
                                .account()
                                .get_display_name()
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| {
                            Action::from(crate::Message::UserSettings(Message::ProfileLoaded(res)))
                        },
                    );

                    let matrix_avatar = matrix.clone();
                    let t_avatar = Task::perform(
                        async move {
                            matrix_avatar
                                .client()
                                .await
                                .account()
                                .get_avatar_url()
                                .await
                                .map(|u| u.map(|uri| uri.to_string()))
                                .map_err(|e| e.to_string())
                        },
                        |res| {
                            Action::from(crate::Message::UserSettings(Message::AvatarUrlLoaded(
                                res,
                            )))
                        },
                    );

                    let t_devices = Task::perform(async move {}, |_| {
                        Action::from(crate::Message::UserSettings(Message::LoadDevices))
                    });

                    let t_3pids = Task::perform(async move {}, |_| {
                        Action::from(crate::Message::UserSettings(Message::Load3PIDs))
                    });

                    let matrix_dm = matrix.clone();
                    let t_dm = Task::perform(
                        async move {
                            let client = matrix_dm.client().await;
                            let ns = client.notification_settings().await;
                            ns.get_default_room_notification_mode(
                                matrix_sdk::notification_settings::IsEncrypted::Yes,
                                matrix_sdk::notification_settings::IsOneToOne::Yes,
                            )
                            .await
                        },
                        |mode| {
                            Action::from(crate::Message::UserSettings(
                                Message::GlobalNotificationModeLoaded(true, mode),
                            ))
                        },
                    );

                    let matrix_group = matrix.clone();
                    let t_group = Task::perform(
                        async move {
                            let client = matrix_group.client().await;
                            let ns = client.notification_settings().await;
                            ns.get_default_room_notification_mode(
                                matrix_sdk::notification_settings::IsEncrypted::Yes,
                                matrix_sdk::notification_settings::IsOneToOne::No,
                            )
                            .await
                        },
                        |mode| {
                            Action::from(crate::Message::UserSettings(
                                Message::GlobalNotificationModeLoaded(false, mode),
                            ))
                        },
                    );

                    let t_ignored = Task::perform(async move {}, |_| {
                        Action::from(crate::Message::UserSettings(Message::LoadIgnoredUsers))
                    });

                    let t_cross_signing = Task::perform(async move {}, |_| {
                        Action::from(crate::Message::UserSettings(
                            Message::LoadCrossSigningStatus,
                        ))
                    });

                    let t_keywords = Task::done(Action::from(crate::Message::UserSettings(
                        Message::LoadKeywords,
                    )));

                    return Task::batch(vec![
                        t_name,
                        t_avatar,
                        t_devices,
                        t_3pids,
                        t_dm,
                        t_group,
                        t_cross_signing,
                        t_keywords,
                        t_ignored,
                    ]);
                }
                Task::none()
            }
            Message::IgnoreUserById(user_id) => {
                if let Some(matrix) = matrix {
                    let matrix = matrix.clone();
                    self.is_loading_ignored_users = true;
                    return Task::perform(
                        async move {
                            matrix
                                .ignore_user(&user_id)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| Action::from(crate::Message::UserSettings(Message::UserIgnored(res))),
                    );
                }
                Task::none()
            }
            Message::UnignoreUserById(user_id) => {
                self.update(Message::UnignoreUser(user_id), matrix)
            }
            Message::LoadIgnoredUsers => {
                if let Some(matrix) = matrix {
                    self.is_loading_ignored_users = true;
                    let matrix = matrix.clone();
                    return Task::perform(
                        async move { matrix.ignored_users().await.map_err(|e| e.to_string()) },
                        |res| {
                            Action::from(crate::Message::UserSettings(Message::IgnoredUsersLoaded(
                                res,
                            )))
                        },
                    );
                }
                Task::none()
            }
            Message::IgnoredUsersLoaded(res) => {
                self.is_loading_ignored_users = false;
                match res {
                    Ok(users) => {
                        self.ignored_users = users;
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to load ignored users: {}", e));
                    }
                }
                Task::none()
            }
            Message::NewIgnoreUserIdChanged(user_id) => {
                self.new_ignore_user_id = user_id;
                Task::none()
            }
            Message::IgnoreUser => {
                if let Some(matrix) = matrix
                    && !self.new_ignore_user_id.is_empty()
                {
                    let matrix = matrix.clone();
                    let user_id_str = self.new_ignore_user_id.clone();
                    self.is_loading_ignored_users = true;
                    return Task::perform(
                        async move {
                            let user_id = matrix_sdk::ruma::UserId::parse(&user_id_str)
                                .map_err(|e| e.to_string())?;
                            matrix
                                .ignore_user(&user_id)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| Action::from(crate::Message::UserSettings(Message::UserIgnored(res))),
                    );
                }
                Task::none()
            }
            Message::UserIgnored(res) => {
                self.is_loading_ignored_users = false;
                match res {
                    Ok(_) => {
                        self.new_ignore_user_id.clear();
                        return self.update(Message::LoadIgnoredUsers, matrix);
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to ignore user: {}", e));
                    }
                }
                Task::none()
            }
            Message::UnignoreUser(user_id) => {
                if let Some(matrix) = matrix {
                    let matrix = matrix.clone();
                    let user_id_clone = user_id.clone();
                    self.is_loading_ignored_users = true;
                    return Task::perform(
                        async move {
                            matrix
                                .unignore_user(&user_id_clone)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        move |res| {
                            Action::from(crate::Message::UserSettings(Message::UserUnignored(
                                user_id, res,
                            )))
                        },
                    );
                }
                Task::none()
            }
            Message::UserUnignored(_, res) => {
                self.is_loading_ignored_users = false;
                match res {
                    Ok(_) => {
                        return self.update(Message::LoadIgnoredUsers, matrix);
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to unignore user: {}", e));
                    }
                }
                Task::none()
            }

            Message::ProfileLoaded(res) => {
                self.is_loading = false;
                match res {
                    Ok(name) => {
                        let name = name.unwrap_or_default();
                        self.display_name = name.clone();
                        self.original_display_name = name;
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to load profile name: {}", e));
                    }
                }
                Task::none()
            }
            Message::AvatarUrlLoaded(res) => {
                self.is_loading_avatar = false;
                match res {
                    Ok(Some(url)) => {
                        self.avatar_url = Some(url.clone());
                        if let Some(matrix) = matrix {
                            let matrix = matrix.clone();
                            return Task::perform(
                                async move {
                                    let uri = matrix_sdk::ruma::OwnedMxcUri::from(url.as_str());
                                    let source =
                                        matrix_sdk::ruma::events::room::MediaSource::Plain(uri);
                                    matrix.fetch_media(source).await.map_err(|e| e.to_string())
                                },
                                |res| {
                                    Action::from(crate::Message::UserSettings(
                                        Message::AvatarMediaFetched(res),
                                    ))
                                },
                            );
                        }
                    }
                    Ok(None) => {
                        self.avatar_url = None;
                        self.avatar_handle = None;
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to load avatar URL: {}", e));
                    }
                }
                Task::none()
            }
            Message::AvatarMediaFetched(res) => {
                match res {
                    Ok(data) => {
                        self.avatar_handle =
                            Some(cosmic::iced::widget::image::Handle::from_bytes(data));
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to fetch avatar media: {}", e));
                    }
                }
                Task::none()
            }
            Message::SelectAvatar => Task::perform(
                async move {
                    rfd::AsyncFileDialog::new()
                        .add_filter("Image", &["png", "jpg", "jpeg", "webp"])
                        .pick_file()
                        .await
                        .map(|f| f.path().to_path_buf())
                },
                |res| {
                    Action::from(crate::Message::UserSettings(Message::AvatarFileSelected(
                        res,
                    )))
                },
            ),
            Message::AvatarFileSelected(path_opt) => {
                if let (Some(path), Some(matrix)) = (path_opt, matrix) {
                    self.is_uploading_avatar = true;
                    self.error = None;
                    let matrix = matrix.clone();

                    return Task::perform(
                        async move {
                            let data = tokio::fs::read(&path).await.map_err(|e| e.to_string())?;
                            let mime_type = mime_guess::from_path(&path).first_or_octet_stream();
                            matrix
                                .client()
                                .await
                                .account()
                                .upload_avatar(&mime_type, data)
                                .await
                                .map(|uri| uri.to_string())
                                .map_err(|e| e.to_string())
                        },
                        |res| {
                            Action::from(crate::Message::UserSettings(Message::AvatarUploaded(res)))
                        },
                    );
                }
                Task::none()
            }
            Message::AvatarUploaded(res) => {
                self.is_uploading_avatar = false;
                match res {
                    Ok(uri) => {
                        return self.update(Message::AvatarUrlLoaded(Ok(Some(uri))), matrix);
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to upload avatar: {}", e));
                    }
                }
                Task::none()
            }
            Message::DisplayNameChanged(name) => {
                self.display_name = name;
                Task::none()
            }
            Message::SaveProfile => {
                if let Some(matrix) = matrix
                    && self.display_name != self.original_display_name
                {
                    self.is_saving = true;
                    self.error = None;
                    let matrix = matrix.clone();
                    let new_name = self.display_name.clone();
                    return Task::perform(
                        async move {
                            let name_opt = if new_name.is_empty() {
                                None
                            } else {
                                Some(new_name.as_str())
                            };
                            matrix
                                .client()
                                .await
                                .account()
                                .set_display_name(name_opt)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| {
                            Action::from(crate::Message::UserSettings(Message::ProfileSaved(res)))
                        },
                    );
                }
                Task::none()
            }
            Message::ProfileSaved(res) => {
                self.is_saving = false;
                match res {
                    Ok(_) => {
                        self.original_display_name = self.display_name.clone();
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to save profile: {}", e));
                    }
                }
                Task::none()
            }
            Message::GlobalNotificationModeLoaded(is_dm, mode) => {
                if is_dm {
                    self.global_notification_mode_dm = Some(mode);
                } else {
                    self.global_notification_mode_group = Some(mode);
                }
                Task::none()
            }
            Message::GlobalNotificationModeChanged(is_dm, mode) => {
                if is_dm {
                    self.global_notification_mode_dm = Some(mode);
                } else {
                    self.global_notification_mode_group = Some(mode);
                }
                if let Some(matrix) = matrix {
                    let matrix = matrix.clone();
                    self.is_loading_global_notifications = true;
                    return Task::perform(
                        async move {
                            let client = matrix.client().await;
                            let ns = client.notification_settings().await;

                            // Set for both encrypted and unencrypted
                            ns.set_default_room_notification_mode(
                                matrix_sdk::notification_settings::IsEncrypted::Yes,
                                if is_dm {
                                    matrix_sdk::notification_settings::IsOneToOne::Yes
                                } else {
                                    matrix_sdk::notification_settings::IsOneToOne::No
                                },
                                mode,
                            )
                            .await
                            .map_err(|e| e.to_string())?;

                            ns.set_default_room_notification_mode(
                                matrix_sdk::notification_settings::IsEncrypted::No,
                                if is_dm {
                                    matrix_sdk::notification_settings::IsOneToOne::Yes
                                } else {
                                    matrix_sdk::notification_settings::IsOneToOne::No
                                },
                                mode,
                            )
                            .await
                            .map_err(|e| e.to_string())
                        },
                        |res| {
                            Action::from(crate::Message::UserSettings(
                                Message::GlobalNotificationModeSet(res),
                            ))
                        },
                    );
                }
                Task::none()
            }
            Message::GlobalNotificationModeSet(res) => {
                self.is_loading_global_notifications = false;
                if let Err(e) = res {
                    self.error = Some(format!("Failed to set notification mode: {}", e));
                }
                Task::none()
            }
            Message::DismissError => {
                self.error = None;
                Task::none()
            }
            Message::CurrentPasswordChanged(pw) => {
                self.current_password = pw;
                Task::none()
            }
            Message::NewPasswordChanged(pw) => {
                self.new_password = pw;
                Task::none()
            }
            Message::ConfirmNewPasswordChanged(pw) => {
                self.confirm_new_password = pw;
                Task::none()
            }
            Message::ChangePassword => {
                if let Some(matrix) = matrix {
                    if self.new_password != self.confirm_new_password {
                        self.error = Some(crate::fl!("new-passwords-do-not-match"));
                        return Task::none();
                    }
                    if self.new_password.is_empty() || self.current_password.is_empty() {
                        self.error = Some(crate::fl!("passwords-cannot-be-empty"));
                        return Task::none();
                    }

                    self.is_changing_password = true;
                    self.error = None;
                    self.password_success = None;

                    let matrix = matrix.clone();
                    let current_password = self.current_password.clone();
                    let new_password = self.new_password.clone();

                    return Task::perform(
                        async move {
                            let user_id = matrix
                                .client()
                                .await
                                .user_id()
                                .map(|u| u.to_string())
                                .unwrap_or_default();
                            let identifier =
                                matrix_sdk::ruma::api::client::uiaa::UserIdentifier::Matrix(
                                    matrix_sdk::ruma::api::client::uiaa::MatrixUserIdentifier::new(
                                        user_id,
                                    ),
                                );
                            let password_auth = matrix_sdk::ruma::api::client::uiaa::Password::new(
                                identifier,
                                current_password,
                            );
                            let auth_data = matrix_sdk::ruma::api::client::uiaa::AuthData::Password(
                                password_auth,
                            );

                            matrix
                                .client()
                                .await
                                .account()
                                .change_password(&new_password, Some(auth_data))
                                .await
                                .map(|_| ())
                                .map_err(|e| e.to_string())
                        },
                        |res| {
                            Action::from(crate::Message::UserSettings(Message::PasswordChanged(
                                res,
                            )))
                        },
                    );
                }
                Task::none()
            }
            Message::PasswordChanged(res) => {
                self.is_changing_password = false;
                match res {
                    Ok(_) => {
                        self.password_success = Some("Password changed successfully".to_string());
                        self.current_password.clear();
                        self.new_password.clear();
                        self.confirm_new_password.clear();
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to change password: {}", e));
                    }
                }
                Task::none()
            }
            Message::DismissPasswordSuccess => {
                self.password_success = None;
                Task::none()
            }
            Message::LoadDevices => {
                if let Some(matrix) = matrix {
                    self.is_loading_devices = true;
                    self.error = None;
                    let matrix = matrix.clone();
                    return Task::perform(
                        async move {
                            let client = matrix.client().await;
                            let user_id = client.user_id().ok_or("No user ID")?;
                            let current_device_id = client.device_id().ok_or("No device ID")?;
                            let user_devices = client
                                .encryption()
                                .get_user_devices(user_id)
                                .await
                                .map_err(|e| e.to_string())?;

                            let devices: Vec<_> = user_devices.devices().map(|device| {
                                DeviceInfo {
                                    device_id: Arc::from(device.device_id().as_str()),
                                    display_name: device.display_name().map(|n| n.to_string()),
                                    is_verified: if device.device_id() == current_device_id {
                                        device.is_cross_signed_by_owner()
                                    } else {
                                        device.is_verified()
                                    },
                                    is_current: device.device_id() == current_device_id,
                                    is_renaming: false,
                                    edit_name: String::new(),
                                    is_deleting: false,
                                }
                            }).collect();
                            Ok(devices)
                        },
                        |res| {
                            Action::from(crate::Message::UserSettings(Message::DevicesLoaded(res)))
                        },
                    );
                }
                Task::none()
            }
            Message::DevicesLoaded(res) => {
                self.is_loading_devices = false;
                match res {
                    Ok(devices) => {
                        self.devices = devices;
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to load devices: {}", e));
                    }
                }
                Task::none()
            }
            Message::VerifyDevice(device_id) => {
                if let Some(matrix) = matrix {
                    self.error = None;
                    self.verification_ui_state = VerificationUIState::WaitingForOtherDevice;
                    let matrix = matrix.clone();
                    let device_id_clone = device_id.clone();
                    return Task::perform(
                        async move {
                            let client = matrix.client().await;
                            let user_id = client.user_id().ok_or("No user ID")?;
                            let device_id_typed =
                                matrix_sdk::ruma::OwnedDeviceId::from(device_id_clone.as_ref());
                            let device = client
                                .encryption()
                                .get_device(user_id, &device_id_typed)
                                .await
                                .map_err(|e| e.to_string())?
                                .ok_or("Device not found")?;

                            let request = device
                                .request_verification()
                                .await
                                .map_err(|e| e.to_string())?;
                            Ok(request)
                        },
                        |res| {
                            Action::from(crate::Message::UserSettings(
                                Message::VerificationRequested(res),
                            ))
                        },
                    );
                }
                Task::none()
            }
            Message::VerificationRequested(res) => {
                match res {
                    Ok(request) => {
                        self.active_verification_request = Some(request.clone());
                        return Task::run(request.changes(), |state| {
                            Action::from(crate::Message::UserSettings(
                                Message::VerificationRequestStateChanged(state),
                            ))
                        });
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to request verification: {}", e));
                        self.verification_ui_state = VerificationUIState::None;
                    }
                }
                Task::none()
            }
            Message::VerificationRequestStateChanged(state) => {
                match state {
                    VerificationRequestState::Ready { .. } => {
                        if let Some(request) = &self.active_verification_request {
                            let req = request.clone();
                            return Task::perform(
                                async move { req.start_sas().await.map_err(|e| e.to_string()) },
                                |res| {
                                    Action::from(crate::Message::UserSettings(Message::SasStarted(
                                        res,
                                    )))
                                },
                            );
                        }
                    }
                    VerificationRequestState::Done => {
                        self.verification_ui_state = VerificationUIState::Done;
                        self.active_verification_request = None;
                        self.active_sas = None;
                        return self.update(Message::LoadDevices, matrix);
                    }
                    VerificationRequestState::Cancelled(_) => {
                        self.verification_ui_state = VerificationUIState::Cancelled;
                        self.active_verification_request = None;
                        self.active_sas = None;
                    }
                    _ => {}
                }
                Task::none()
            }
            Message::SasStarted(res) => {
                match res {
                    Ok(Some(sas)) => {
                        self.active_sas = Some(sas.clone());
                        return Task::run(sas.changes(), |state| {
                            Action::from(crate::Message::UserSettings(Message::SasStateChanged(
                                state,
                            )))
                        });
                    }
                    Ok(None) => {
                        self.error =
                            Some("Other device does not support SAS verification.".to_string());
                        self.verification_ui_state = VerificationUIState::Cancelled;
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to start SAS: {}", e));
                        self.verification_ui_state = VerificationUIState::Cancelled;
                    }
                }
                Task::none()
            }
            Message::SasStateChanged(state) => {
                match state {
                    SasState::KeysExchanged {
                        emojis: Some(emojis),
                        ..
                    } => {
                        let emoji_list = emojis
                            .emojis
                            .iter()
                            .map(|e| (e.symbol.to_string(), e.description.to_string()))
                            .collect();
                        self.verification_ui_state = VerificationUIState::ShowingEmojis(emoji_list);
                    }
                    SasState::Done { .. } => {
                        self.verification_ui_state = VerificationUIState::Done;
                        self.active_sas = None;
                        self.active_verification_request = None;
                        return self.update(Message::LoadDevices, matrix);
                    }
                    SasState::Cancelled { .. } => {
                        self.verification_ui_state = VerificationUIState::Cancelled;
                        self.active_sas = None;
                        self.active_verification_request = None;
                    }
                    _ => {}
                }
                Task::none()
            }
            Message::ConfirmEmojis => {
                if let Some(sas) = &self.active_sas {
                    let sas = sas.clone();
                    return Task::perform(
                        async move { sas.confirm().await.map_err(|e| e.to_string()) },
                        |res| {
                            Action::from(crate::Message::UserSettings(Message::EmojisConfirmed(
                                res,
                            )))
                        },
                    );
                }
                Task::none()
            }
            Message::EmojisConfirmed(res) => {
                if let Err(e) = res {
                    self.error = Some(format!("Failed to confirm emojis: {}", e));
                }
                Task::none()
            }
            Message::CancelVerification => {
                let mut task = Task::none();
                if let Some(sas) = &self.active_sas {
                    let sas = sas.clone();
                    task = Task::perform(
                        async move {
                            let _ = sas.cancel().await;
                        },
                        |_| Action::from(crate::Message::NoOp),
                    );
                } else if let Some(req) = &self.active_verification_request {
                    let req = req.clone();
                    task = Task::perform(
                        async move {
                            let _ = req.cancel().await;
                        },
                        |_| Action::from(crate::Message::NoOp),
                    );
                }
                self.verification_ui_state = VerificationUIState::Cancelled;
                self.active_sas = None;
                self.active_verification_request = None;
                task
            }
            Message::DeviceDeletePasswordChanged(pw) => {
                self.device_delete_password = pw;
                Task::none()
            }
            Message::StartRenameDevice(ref device_id) => {
                if let Some(device) = self.devices.iter_mut().find(|d| d.device_id == *device_id) {
                    device.is_renaming = true;
                    device.edit_name = device.display_name.clone().unwrap_or_default();
                }
                Task::none()
            }
            Message::CancelRenameDevice(ref device_id) => {
                if let Some(device) = self.devices.iter_mut().find(|d| d.device_id == *device_id) {
                    device.is_renaming = false;
                }
                Task::none()
            }
            Message::EditDeviceNameChanged(ref device_id, new_name) => {
                if let Some(device) = self.devices.iter_mut().find(|d| d.device_id == *device_id) {
                    device.edit_name = new_name;
                }
                Task::none()
            }
            Message::SaveDeviceName(ref device_id) => {
                if let Some(matrix) = matrix
                    && let Some(device) =
                        self.devices.iter_mut().find(|d| d.device_id == *device_id)
                {
                    device.is_renaming = false;
                    let new_name = device.edit_name.clone();
                    let device_id_str = device_id.clone();
                    let device_id_for_closure = device_id_str.clone();
                    let matrix = matrix.clone();
                    return Task::perform(
                        async move {
                            let did = matrix_sdk::ruma::OwnedDeviceId::from(device_id_str.as_ref());
                            matrix
                                .client()
                                .await
                                .rename_device(&did, &new_name)
                                .await
                                .map(|_| ())
                                .map_err(|e| e.to_string())
                        },
                        move |res| {
                            Action::from(crate::Message::UserSettings(Message::DeviceRenamed(
                                device_id_for_closure,
                                res,
                            )))
                        },
                    );
                }
                Task::none()
            }
            Message::DeviceRenamed(ref device_id, res) => {
                match res {
                    Ok(_) => {
                        if let Some(device) =
                            self.devices.iter_mut().find(|d| d.device_id == *device_id)
                        {
                            device.display_name = Some(device.edit_name.clone());
                        }
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to rename device: {}", e));
                    }
                }
                Task::none()
            }
            Message::DeleteDevice(ref device_id) => {
                if let Some(matrix) = matrix
                    && let Some(device) =
                        self.devices.iter_mut().find(|d| d.device_id == *device_id)
                {
                    device.is_deleting = true;
                    let matrix = matrix.clone();
                    let device_id_str = device_id.clone();
                    let device_id_for_closure = device_id_str.clone();
                    let password = self.device_delete_password.clone();
                    return Task::perform(
                        async move {
                            let client = matrix.client().await;
                            let user_id = client.user_id().ok_or("No user ID")?.to_string();
                            let did = matrix_sdk::ruma::OwnedDeviceId::from(device_id_str.as_ref());

                            if let Err(e) = client
                                .delete_devices(std::slice::from_ref(&did), None)
                                .await
                            {
                                if let Some(info) = e.as_uiaa_response() {
                                    if password.is_empty() {
                                        return Err(
                                            "Password required to delete device".to_string()
                                        );
                                    }

                                    let identifier = matrix_sdk::ruma::api::client::uiaa::UserIdentifier::Matrix(matrix_sdk::ruma::api::client::uiaa::MatrixUserIdentifier::new(user_id));
                                    let mut password_auth =
                                        matrix_sdk::ruma::api::client::uiaa::Password::new(
                                            identifier, password,
                                        );
                                    password_auth.session = info.session.clone();

                                    client.delete_devices(&[did], Some(matrix_sdk::ruma::api::client::uiaa::AuthData::Password(password_auth))).await.map(|_| ()).map_err(|e| e.to_string())?;
                                    return Ok(());
                                }
                                return Err(e.to_string());
                            }
                            Ok(())
                        },
                        move |res| {
                            Action::from(crate::Message::UserSettings(Message::DeviceDeleted(
                                device_id_for_closure,
                                res,
                            )))
                        },
                    );
                }
                Task::none()
            }
            Message::DeviceDeleted(ref device_id, res) => {
                if let Some(device) = self.devices.iter_mut().find(|d| d.device_id == *device_id) {
                    device.is_deleting = false;
                }
                match res {
                    Ok(_) => {
                        self.devices.retain(|d| d.device_id != *device_id);
                        self.device_delete_password.clear();
                    }
                    Err(e) => {
                        self.error = Some(format!(
                            "Failed to delete device: {}. You might need to provide a password below.",
                            e
                        ));
                    }
                }
                Task::none()
            }
            Message::DeactivatePasswordChanged(pw) => {
                self.deactivate_password = pw;
                Task::none()
            }
            Message::DeactivateAccount => {
                if let Some(matrix) = matrix {
                    self.is_deactivating = true;
                    self.error = None;

                    let matrix = matrix.clone();
                    let password = self.deactivate_password.clone();

                    return Task::perform(
                        async move {
                            let client = matrix.client().await;
                            let user_id = client.user_id().ok_or("No user ID")?.to_string();

                            if let Err(e) = client.account().deactivate(None, None, false).await {
                                if let Some(info) = e.as_uiaa_response() {
                                    if password.is_empty() {
                                        return Err(
                                            "Password required to deactivate account".to_string()
                                        );
                                    }

                                    let identifier =
                                        matrix_sdk::ruma::api::client::uiaa::UserIdentifier::Matrix(
                                            matrix_sdk::ruma::api::client::uiaa::MatrixUserIdentifier::new(user_id)
                                        );
                                    let mut password_auth =
                                        matrix_sdk::ruma::api::client::uiaa::Password::new(
                                            identifier, password,
                                        );
                                    password_auth.session = info.session.clone();

                                    client
                                        .account()
                                        .deactivate(
                                            None,
                                            Some(matrix_sdk::ruma::api::client::uiaa::AuthData::Password(
                                                password_auth,
                                            )),
                                            false,
                                        )
                                        .await
                                        .map_err(|e| e.to_string())?;
                                    return Ok(());
                                }
                                return Err(e.to_string());
                            }
                            Ok(())
                        },
                        |res| {
                            Action::from(crate::Message::UserSettings(Message::AccountDeactivated(
                                res,
                            )))
                        },
                    );
                }
                Task::none()
            }
            Message::AccountDeactivated(res) => {
                self.is_deactivating = false;
                match res {
                    Ok(_) => {
                        // On success, we should log out and close settings
                        Task::batch(vec![
                            Task::done(Action::from(crate::Message::Logout)),
                            Task::done(Action::from(crate::Message::CloseSettings)),
                        ])
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to deactivate account: {}", e));
                        Task::none()
                    }
                }
            }
            Message::LoadCrossSigningStatus => {
                self.is_loading_cross_signing = true;
                if let Some(matrix) = matrix {
                    let matrix = matrix.clone();
                    return Task::perform(
                        async move {
                            let client = matrix.client().await;
                            let encryption = client.encryption();
                            let status_opt = encryption.cross_signing_status().await;

                            if let Some(status) = status_opt {
                                return Ok(Some(CrossSigningInfo {
                                    status,
                                    master_key: None,
                                    self_signing_key: None,
                                    user_signing_key: None,
                                }));
                            }
                            Ok(None)
                        },
                        |res| {
                            Action::from(crate::Message::UserSettings(
                                Message::CrossSigningStatusLoaded(res),
                            ))
                        },
                    );
                }
                Task::none()
            }
            Message::CrossSigningStatusLoaded(res) => {
                self.is_loading_cross_signing = false;
                match res {
                    Ok(info) => {
                        self.cross_signing_info = info;
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to load cross-signing status: {}", e));
                    }
                }
                Task::none()
            }
            Message::BootstrapCrossSigning => {
                self.is_bootstrapping = true;
                if let Some(matrix) = matrix {
                    let matrix = matrix.clone();
                    let password = self.device_delete_password.clone();
                    return Task::perform(
                        async move {
                            let client = matrix.client().await;
                            let user_id = client.user_id().ok_or("No user ID")?.to_string();

                            if let Err(e) = client.encryption().bootstrap_cross_signing(None).await
                            {
                                if let Some(info) = e.as_uiaa_response() {
                                    #[derive(serde::Deserialize)]
                                    struct UiaaParams {
                                        #[serde(rename = "m.oauth")]
                                        oauth: Option<OAuthParams>,
                                    }

                                    #[derive(serde::Deserialize)]
                                    struct OAuthParams {
                                        url: String,
                                    }

                                    if let Some(params_box) = &info.params
                                        && let Ok(params) =
                                            serde_json::from_str::<UiaaParams>(params_box.get())
                                        && let Some(oauth) = params.oauth
                                    {
                                        if let Err(err) = open::that(&oauth.url) {
                                            return Err(format!(
                                                "Failed to open authentication link: {err}. Please open: {}",
                                                oauth.url
                                            ));
                                        }
                                        return Err("Authentication required in web browser. Please complete the cross-signing reset in the browser, then try bootstrapping again.".to_string());
                                    }

                                    if password.is_empty() {
                                        return Err(
                                            "Password required for bootstrapping (use the delete-device password field)".to_string()
                                        );
                                    }

                                    let identifier = matrix_sdk::ruma::api::client::uiaa::UserIdentifier::Matrix(matrix_sdk::ruma::api::client::uiaa::MatrixUserIdentifier::new(user_id));
                                    let mut password_auth =
                                        matrix_sdk::ruma::api::client::uiaa::Password::new(
                                            identifier, password,
                                        );
                                    password_auth.session = info.session.clone();

                                    client
                                        .encryption()
                                        .bootstrap_cross_signing(Some(
                                            matrix_sdk::ruma::api::client::uiaa::AuthData::Password(
                                                password_auth,
                                            ),
                                        ))
                                        .await
                                        .map_err(|e| e.to_string())?;
                                    return Ok(());
                                }
                                return Err(e.to_string());
                            }
                            Ok(())
                        },
                        |res| {
                            Action::from(crate::Message::UserSettings(
                                Message::CrossSigningBootstrapped(res),
                            ))
                        },
                    );
                }
                Task::none()
            }
            Message::CrossSigningBootstrapped(res) => {
                self.is_bootstrapping = false;
                match res {
                    Ok(_) => {
                        return self.update(Message::LoadCrossSigningStatus, matrix);
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to bootstrap cross-signing: {}", e));
                    }
                }
                Task::none()
            }
            Message::ToggleMediaPreviewsDisplayPolicy(enabled) => {
                self.media_previews_display_policy = enabled;
                if let Some(matrix) = matrix {
                    let matrix = matrix.clone();
                    return Task::perform(
                        async move {
                            matrix
                                .set_media_previews_display_policy(enabled)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |_| Action::from(crate::Message::AppSettingChanged),
                    );
                }
                Task::done(Action::from(crate::Message::AppSettingChanged))
            }
            Message::ToggleInviteAvatarsDisplayPolicy(enabled) => {
                self.invite_avatars_display_policy = enabled;
                if let Some(matrix) = matrix {
                    let matrix = matrix.clone();
                    return Task::perform(
                        async move {
                            matrix
                                .set_invite_avatars_display_policy(enabled)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |_| Action::from(crate::Message::AppSettingChanged),
                    );
                }
                Task::done(Action::from(crate::Message::AppSettingChanged))
            }
            Message::Load3PIDs => {
                if let Some(matrix) = matrix {
                    self.is_loading_3pids = true;
                    let matrix = matrix.clone();
                    return Task::perform(
                        async move {
                            let client = matrix.client().await;
                            let resp = client
                                .account()
                                .get_3pids()
                                .await
                                .map_err(|e| e.to_string())?;
                            let threepids = resp
                                .threepids
                                .into_iter()
                                .map(|t| Threepid {
                                    address: t.address,
                                    medium: t.medium,
                                })
                                .collect();
                            Ok(threepids)
                        },
                        |res| {
                            Action::from(crate::Message::UserSettings(Message::ThreepidsLoaded(
                                res,
                            )))
                        },
                    );
                }
                Task::none()
            }
            Message::ThreepidsLoaded(res) => {
                self.is_loading_3pids = false;
                match res {
                    Ok(threepids) => {
                        self.threepids = threepids;
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to load 3PIDs: {}", e));
                    }
                }
                Task::none()
            }
            Message::New3PIDEmailChanged(email) => {
                self.new_3pid_email = email;
                Task::none()
            }
            Message::New3PIDMsisdnChanged(msisdn) => {
                self.new_3pid_msisdn = msisdn;
                Task::none()
            }
            Message::New3PIDCountryCodeChanged(cc) => {
                self.new_3pid_country_code = cc;
                Task::none()
            }
            Message::Add3PIDPasswordChanged(pw) => {
                self.add_3pid_password = pw;
                Task::none()
            }
            Message::Request3PIDEmailToken => {
                if let Some(matrix) = matrix {
                    self.is_requesting_3pid_token = true;
                    self.error = None;
                    let matrix = matrix.clone();
                    let email = self.new_3pid_email.clone();
                    let client_secret = matrix_sdk::ruma::ClientSecret::new();
                    self.adding_3pid_client_secret = Some(client_secret.to_string());

                    return Task::perform(
                        async move {
                            let client = matrix.client().await;
                            let resp = client
                                .account()
                                .request_3pid_email_token(
                                    &client_secret,
                                    &email,
                                    matrix_sdk::ruma::uint!(1),
                                )
                                .await
                                .map_err(|e| e.to_string())?;
                            Ok(resp.sid.to_string())
                        },
                        |res| {
                            Action::from(crate::Message::UserSettings(
                                Message::ThreepidTokenRequested(res),
                            ))
                        },
                    );
                }
                Task::none()
            }
            Message::Request3PIDMsisdnToken => {
                if let Some(matrix) = matrix {
                    self.is_requesting_3pid_token = true;
                    self.error = None;
                    let matrix = matrix.clone();
                    let msisdn = self.new_3pid_msisdn.clone();
                    let country = self.new_3pid_country_code.clone();
                    let client_secret = matrix_sdk::ruma::ClientSecret::new();
                    self.adding_3pid_client_secret = Some(client_secret.to_string());

                    return Task::perform(
                        async move {
                            let client = matrix.client().await;
                            let resp = client
                                .account()
                                .request_3pid_msisdn_token(
                                    &client_secret,
                                    &country,
                                    &msisdn,
                                    matrix_sdk::ruma::uint!(1),
                                )
                                .await
                                .map_err(|e| e.to_string())?;
                            Ok(resp.sid.to_string())
                        },
                        |res| {
                            Action::from(crate::Message::UserSettings(
                                Message::ThreepidTokenRequested(res),
                            ))
                        },
                    );
                }
                Task::none()
            }
            Message::ThreepidTokenRequested(res) => {
                self.is_requesting_3pid_token = false;
                match res {
                    Ok(sid) => {
                        self.adding_3pid_sid = Some(sid);
                        self.success_message = Some("Verification code sent. Please confirm the link/code and then provide your password to add it here.".to_string());
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to request verification token: {}", e));
                        self.adding_3pid_client_secret = None;
                    }
                }
                Task::none()
            }
            Message::Add3PID => {
                if let (Some(matrix), Some(sid), Some(secret)) = (
                    matrix,
                    &self.adding_3pid_sid,
                    &self.adding_3pid_client_secret,
                ) {
                    let matrix = matrix.clone();
                    let sid = sid.clone();
                    let secret = secret.clone();
                    let password = self.add_3pid_password.clone();

                    return Task::perform(
                        async move {
                            let client = matrix.client().await;
                            let user_id = client.user_id().ok_or("No user ID")?.to_string();
                            let sid_typed = matrix_sdk::ruma::SessionId::parse(sid)
                                .map_err(|e| e.to_string())?;
                            let secret_typed = matrix_sdk::ruma::ClientSecret::parse(secret)
                                .map_err(|e| e.to_string())?;

                            let res = client
                                .account()
                                .add_3pid(&secret_typed, &sid_typed, None)
                                .await;

                            match res {
                                Ok(_) => Ok(()),
                                Err(e) => {
                                    if let Some(info) = e.as_uiaa_response() {
                                        if password.is_empty() {
                                            return Err("Password required to add 3PID".to_string());
                                        }

                                        let identifier = matrix_sdk::ruma::api::client::uiaa::UserIdentifier::Matrix(matrix_sdk::ruma::api::client::uiaa::MatrixUserIdentifier::new(user_id));
                                        let mut password_auth =
                                            matrix_sdk::ruma::api::client::uiaa::Password::new(
                                                identifier, password,
                                            );
                                        password_auth.session = info.session.clone();

                                        client.account().add_3pid(&secret_typed, &sid_typed, Some(matrix_sdk::ruma::api::client::uiaa::AuthData::Password(password_auth))).await.map(|_| ()).map_err(|e| e.to_string())
                                    } else {
                                        Err(e.to_string())
                                    }
                                }
                            }
                        },
                        |res| {
                            Action::from(crate::Message::UserSettings(Message::ThreepidAdded(res)))
                        },
                    );
                }
                Task::none()
            }
            Message::ThreepidAdded(res) => {
                match res {
                    Ok(_) => {
                        self.new_3pid_email.clear();
                        self.new_3pid_msisdn.clear();
                        self.add_3pid_password.clear();
                        self.adding_3pid_sid = None;
                        self.adding_3pid_client_secret = None;
                        return self.update(Message::Load3PIDs, matrix);
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to add 3PID: {}", e));
                    }
                }
                Task::none()
            }
            Message::DismissSuccessMessage => {
                self.success_message = None;
                Task::none()
            }
            Message::Delete3PID(address, medium) => {
                if let Some(matrix) = matrix {
                    let matrix = matrix.clone();
                    let addr = address.clone();
                    return Task::perform(
                        async move {
                            let client = matrix.client().await;
                            client
                                .account()
                                .delete_3pid(&addr, medium, None)
                                .await
                                .map(|_| ())
                                .map_err(|e| e.to_string())
                        },
                        move |res| {
                            Action::from(crate::Message::UserSettings(Message::ThreepidDeleted(
                                address.clone(),
                                res,
                            )))
                        },
                    );
                }
                Task::none()
            }
            Message::ThreepidDeleted(address, res) => {
                match res {
                    Ok(_) => {
                        self.threepids.retain(|t| t.address != address);
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to delete 3PID: {}", e));
                    }
                }
                Task::none()
            }
            Message::LoadKeywords => {
                if let Some(matrix) = matrix {
                    self.is_loading_keywords = true;
                    let matrix = matrix.clone();
                    return Task::perform(
                        async move {
                            let client = matrix.client().await;
                            let ns = client.notification_settings().await;
                            ns.enabled_keywords().await.into_iter().collect()
                        },
                        |res| {
                            Action::from(crate::Message::UserSettings(Message::KeywordsLoaded(res)))
                        },
                    );
                }
                Task::none()
            }
            Message::KeywordsLoaded(res) => {
                self.is_loading_keywords = false;
                self.keywords = res;
                Task::none()
            }
            Message::NewKeywordChanged(keyword) => {
                self.new_keyword = keyword;
                Task::none()
            }
            Message::AddKeyword => {
                if let Some(matrix) = matrix
                    && !self.new_keyword.is_empty()
                {
                    self.is_loading_keywords = true;
                    let matrix = matrix.clone();
                    let keyword = self.new_keyword.clone();
                    return Task::perform(
                        async move {
                            let client = matrix.client().await;
                            let ns = client.notification_settings().await;
                            ns.add_keyword(keyword).await.map_err(|e| e.to_string())
                        },
                        |res| {
                            Action::from(crate::Message::UserSettings(Message::KeywordAdded(res)))
                        },
                    );
                }
                Task::none()
            }
            Message::KeywordAdded(res) => {
                match res {
                    Ok(_) => {
                        self.new_keyword.clear();
                        return self.update(Message::LoadKeywords, matrix);
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to add keyword: {}", e));
                    }
                }
                Task::none()
            }
            Message::RemoveKeyword(keyword) => {
                if let Some(matrix) = matrix {
                    self.is_loading_keywords = true;
                    let matrix = matrix.clone();
                    return Task::perform(
                        async move {
                            let client = matrix.client().await;
                            let ns = client.notification_settings().await;
                            ns.remove_keyword(&keyword).await.map_err(|e| e.to_string())
                        },
                        |res| {
                            Action::from(crate::Message::UserSettings(Message::KeywordRemoved(res)))
                        },
                    );
                }
                Task::none()
            }
            Message::KeywordRemoved(res) => {
                match res {
                    Ok(_) => {
                        return self.update(Message::LoadKeywords, matrix);
                    }
                    Err(e) => {
                        self.is_loading_keywords = false;
                        self.error = Some(format!("Failed to remove keyword: {}", e));
                    }
                }
                Task::none()
            }
        }
    }
}
