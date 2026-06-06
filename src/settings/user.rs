use crate::matrix::MatrixEngine;
use cosmic::iced::Alignment;
use cosmic::widget::{
    Column, Row, button, icon::Named, settings, text, text_input, tooltip, tooltip::Position,
};
use cosmic::{Action, Element, Task};
use matrix_sdk::encryption::CrossSigningStatus;
use matrix_sdk::encryption::verification::{
    SasState, SasVerification, VerificationRequest, VerificationRequestState,
};
use matrix_sdk::ruma::OwnedUserId;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct CrossSigningInfo {
    pub status: CrossSigningStatus,
    pub master_key: Option<String>,
    pub self_signing_key: Option<String>,
    pub user_signing_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub device_id: Arc<str>,
    pub display_name: Option<String>,
    pub is_verified: bool,
    pub is_current: bool,
    pub is_renaming: bool,
    pub edit_name: String,
    pub is_deleting: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Threepid {
    pub address: String,
    pub medium: matrix_sdk::ruma::thirdparty::Medium,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum VerificationUIState {
    #[default]
    None,
    WaitingForOtherDevice,
    ShowingEmojis(Vec<(String, String)>),
    Done,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct State {
    pub display_name: String,
    pub original_display_name: String,
    pub is_loading: bool,
    pub is_saving: bool,
    pub error: Option<String>,
    pub avatar_url: Option<String>,
    pub avatar_handle: Option<cosmic::iced::widget::image::Handle>,
    pub is_uploading_avatar: bool,
    pub is_loading_avatar: bool,
    pub current_password: String,
    pub new_password: String,
    pub confirm_new_password: String,
    pub is_changing_password: bool,
    pub password_success: Option<String>,
    pub success_message: Option<String>,
    pub devices: Vec<DeviceInfo>,
    pub is_loading_devices: bool,
    pub active_verification_request: Option<VerificationRequest>,
    pub active_sas: Option<SasVerification>,
    pub verification_ui_state: VerificationUIState,
    pub device_delete_password: String,
    pub global_notification_mode_dm:
        Option<matrix_sdk::notification_settings::RoomNotificationMode>,
    pub global_notification_mode_group:
        Option<matrix_sdk::notification_settings::RoomNotificationMode>,
    pub is_loading_global_notifications: bool,
    pub deactivate_password: String,
    pub is_deactivating: bool,
    pub cross_signing_info: Option<CrossSigningInfo>,
    pub is_loading_cross_signing: bool,
    pub is_bootstrapping: bool,
    pub media_previews_display_policy: bool,
    pub invite_avatars_display_policy: bool,
    pub threepids: Vec<Threepid>,
    pub is_loading_3pids: bool,
    pub new_3pid_email: String,
    pub new_3pid_msisdn: String,
    pub new_3pid_country_code: String,
    pub is_requesting_3pid_token: bool,
    pub adding_3pid_sid: Option<String>,
    pub adding_3pid_client_secret: Option<String>,
    pub add_3pid_password: String,
    pub keywords: Vec<String>,
    pub new_keyword: String,
    pub is_loading_keywords: bool,
    pub ignored_users: Vec<OwnedUserId>,
    pub is_loading_ignored_users: bool,
    pub new_ignore_user_id: String,
}

impl Default for State {
    fn default() -> Self {
        Self {
            display_name: String::new(),
            original_display_name: String::new(),
            is_loading: false,
            is_saving: false,
            error: None,
            avatar_url: None,
            avatar_handle: None,
            is_uploading_avatar: false,
            is_loading_avatar: false,
            current_password: String::new(),
            new_password: String::new(),
            confirm_new_password: String::new(),
            is_changing_password: false,
            password_success: None,
            success_message: None,
            devices: Vec::new(),
            is_loading_devices: false,
            active_verification_request: None,
            active_sas: None,
            verification_ui_state: VerificationUIState::default(),
            device_delete_password: String::new(),
            global_notification_mode_dm: None,
            global_notification_mode_group: None,
            is_loading_global_notifications: false,
            deactivate_password: String::new(),
            is_deactivating: false,
            cross_signing_info: None,
            is_loading_cross_signing: false,
            is_bootstrapping: false,
            media_previews_display_policy: true,
            invite_avatars_display_policy: true,
            threepids: Vec::new(),
            is_loading_3pids: false,
            new_3pid_email: String::new(),
            new_3pid_msisdn: String::new(),
            new_3pid_country_code: String::new(),
            is_requesting_3pid_token: false,
            adding_3pid_sid: None,
            adding_3pid_client_secret: None,
            add_3pid_password: String::new(),
            keywords: Vec::new(),
            new_keyword: String::new(),
            is_loading_keywords: false,
            ignored_users: Vec::new(),
            is_loading_ignored_users: false,
            new_ignore_user_id: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    LoadProfile,
    LoadIgnoredUsers,
    IgnoredUsersLoaded(Result<Vec<OwnedUserId>, String>),
    NewIgnoreUserIdChanged(String),
    IgnoreUser,
    UserIgnored(Result<(), String>),
    UnignoreUser(OwnedUserId),
    UserUnignored(OwnedUserId, Result<(), String>),
    ProfileLoaded(Result<Option<String>, String>),
    AvatarUrlLoaded(Result<Option<String>, String>),
    AvatarMediaFetched(Result<Vec<u8>, String>),
    SelectAvatar,
    AvatarFileSelected(Option<std::path::PathBuf>),
    AvatarUploaded(Result<String, String>),
    DisplayNameChanged(String),
    SaveProfile,
    ProfileSaved(Result<(), String>),
    DismissError,
    CurrentPasswordChanged(String),
    NewPasswordChanged(String),
    ConfirmNewPasswordChanged(String),
    ChangePassword,
    PasswordChanged(Result<(), String>),
    DismissPasswordSuccess,
    LoadDevices,
    DevicesLoaded(Result<Vec<DeviceInfo>, String>),
    VerifyDevice(Arc<str>),
    VerificationRequested(Result<VerificationRequest, String>),
    VerificationRequestStateChanged(VerificationRequestState),
    SasStarted(Result<Option<SasVerification>, String>),
    SasStateChanged(SasState),
    ConfirmEmojis,
    EmojisConfirmed(Result<(), String>),
    CancelVerification,
    DeviceDeletePasswordChanged(String),
    StartRenameDevice(Arc<str>),
    CancelRenameDevice(Arc<str>),
    EditDeviceNameChanged(Arc<str>, String),
    SaveDeviceName(Arc<str>),
    DeviceRenamed(Arc<str>, Result<(), String>),
    DeleteDevice(Arc<str>),
    DeviceDeleted(Arc<str>, Result<(), String>),
    DeactivatePasswordChanged(String),
    DeactivateAccount,
    AccountDeactivated(Result<(), String>),
    GlobalNotificationModeChanged(
        bool,
        matrix_sdk::notification_settings::RoomNotificationMode,
    ),
    GlobalNotificationModeLoaded(
        bool,
        matrix_sdk::notification_settings::RoomNotificationMode,
    ),
    GlobalNotificationModeSet(Result<(), String>),
    LoadCrossSigningStatus,
    CrossSigningStatusLoaded(Result<Option<CrossSigningInfo>, String>),
    BootstrapCrossSigning,
    CrossSigningBootstrapped(Result<(), String>),
    ToggleMediaPreviewsDisplayPolicy(bool),
    ToggleInviteAvatarsDisplayPolicy(bool),
    Load3PIDs,
    ThreepidsLoaded(Result<Vec<Threepid>, String>),
    New3PIDEmailChanged(String),
    New3PIDMsisdnChanged(String),
    New3PIDCountryCodeChanged(String),
    Request3PIDEmailToken,
    Request3PIDMsisdnToken,
    ThreepidTokenRequested(Result<String, String>),
    Add3PIDPasswordChanged(String),
    Add3PID,
    ThreepidAdded(Result<(), String>),
    Delete3PID(String, matrix_sdk::ruma::thirdparty::Medium),
    ThreepidDeleted(String, Result<(), String>),
    DismissSuccessMessage,
    LoadKeywords,
    KeywordsLoaded(Vec<String>),
    NewKeywordChanged(String),
    AddKeyword,
    KeywordAdded(Result<(), String>),
    RemoveKeyword(String),
    KeywordRemoved(Result<(), String>),
    IgnoreUserById(matrix_sdk::ruma::OwnedUserId),
    UnignoreUserById(matrix_sdk::ruma::OwnedUserId),
}

impl State {
    pub fn from_config(config: &super::config::Config) -> Self {
        Self {
            media_previews_display_policy: config.media_previews_display_policy,
            invite_avatars_display_policy: config.invite_avatars_display_policy,
            ..Default::default()
        }
    }

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

                            let mut devices = Vec::new();
                            for device in user_devices.devices() {
                                devices.push(DeviceInfo {
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
                                });
                            }
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

                                    if let Some(params_box) = &info.params {
                                        if let Ok(params) =
                                            serde_json::from_str::<UiaaParams>(params_box.get())
                                        {
                                            if let Some(oauth) = params.oauth {
                                                if let Err(err) = open::that(&oauth.url) {
                                                    return Err(format!(
                                                        "Failed to open authentication link: {err}. Please open: {}",
                                                        oauth.url
                                                    ));
                                                }
                                                return Err("Authentication required in web browser. Please complete the cross-signing reset in the browser, then try bootstrapping again.".to_string());
                                            }
                                        }
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
                            let mut res = Vec::new();
                            for k in ns.enabled_keywords().await {
                                res.push(k);
                            }
                            res
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

    fn view_privacy<'a>(&'a self) -> Element<'a, Message> {
        settings::section()
            .title("Privacy & Preferences")
            .add(settings::item(
                "Display Media Previews",
                cosmic::widget::toggler(self.media_previews_display_policy)
                    .on_toggle(Message::ToggleMediaPreviewsDisplayPolicy),
            ))
            .add(settings::item(
                "Display Invite Avatars",
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
                    RoomNotificationMode::AllMessages => "All Messages",
                    RoomNotificationMode::MentionsAndKeywordsOnly => "Mentions Only",
                    RoomNotificationMode::Mute => "Muted",
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
            .title("Default Notification Settings")
            .add(settings::item(
                "Direct Messages",
                build_mode_row(self.global_notification_mode_dm, true),
            ))
            .add(settings::item(
                "Group Chats",
                build_mode_row(self.global_notification_mode_group, false),
            ))
            .into()
    }

    fn view_keywords<'a>(&'a self) -> Element<'a, Message> {
        let mut section = settings::section()
            .title("Keyword Notifications")
            .header(text::body(
                "Receive notifications when these keywords are mentioned in any room.",
            ));

        if self.is_loading_keywords {
            section = section.add(text::body(crate::fl!("loading-keywords")));
        } else {
            for keyword in &self.keywords {
                section = section.add(settings::item(
                    keyword.as_str(),
                    tooltip(
                        button::custom(cosmic::widget::icon::from_name("user-trash-symbolic"))
                            .class(cosmic::theme::Button::Destructive)
                            .on_press(Message::RemoveKeyword(keyword.clone())),
                        text::body(crate::fl!("remove-keyword")),
                        Position::Top,
                    ),
                ));
            }

            let mut add_btn = button::text(crate::fl!("add"));
            let is_empty = self.new_keyword.trim().is_empty();
            if !is_empty {
                add_btn = add_btn.on_press(Message::AddKeyword);
            }
            let btn_widget: Element<'_, Message> = if is_empty {
                tooltip(
                    add_btn,
                    text::body(crate::fl!("enter-keyword-to-add")),
                    Position::Top,
                )
                .into()
            } else {
                add_btn.into()
            };

            let add_keyword_layout = Column::new()
                .spacing(5)
                .push(text::body("Add Keyword").size(12))
                .push(
                    Row::new()
                        .spacing(10)
                        .align_y(Alignment::Center)
                        .push(
                            text_input("New keyword", &self.new_keyword)
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
            tooltip(
                save_btn,
                text::body(crate::fl!("make-changes-to-save")),
                Position::Top,
            )
            .into()
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
                tooltip(
                    pw_btn,
                    text::body(crate::fl!("fill-all-fields-to-change-password")),
                    Position::Top,
                )
                .into()
            } else if !passwords_match {
                tooltip(
                    pw_btn,
                    text::body(crate::fl!("new-passwords-do-not-match")),
                    Position::Top,
                )
                .into()
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
                action_row = action_row.push(tooltip(
                    del_btn,
                    text::body(crate::fl!("delete-device")),
                    Position::Top,
                ));

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
                        .push(tooltip(
                            button::icon(Named::new("document-edit-symbolic"))
                                .on_press(Message::StartRenameDevice(device.device_id.clone())),
                            text::body(crate::fl!("rename-device")),
                            Position::Top,
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
        let mut deactivate_btn = button::destructive(if self.is_deactivating {
            crate::fl!("deactivating")
        } else {
            crate::fl!("deactivate-account")
        });

        let is_pass_empty = self.deactivate_password.is_empty();
        if !is_pass_empty {
            deactivate_btn = deactivate_btn.on_press(Message::DeactivateAccount);
        }

        let deactivate_widget: Element<'_, Message> = if is_pass_empty {
            tooltip(
                deactivate_btn,
                text::body(crate::fl!("enter-password-to-deactivate")),
                Position::Top,
            )
            .into()
        } else {
            deactivate_btn.into()
        };

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
        let mut section = settings::section().title("Cross-signing");

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
                            "✅ Present"
                        } else {
                            "❌ Missing"
                        })),
                );
                if let Some(key) = key_val {
                    c = c.push(text::body(format!("Public Key: {}", key)).size(10));
                }
                c
            };

            section = section
                .add(settings::item(
                    "Master Key:",
                    build_key_row("", status.has_master, info.master_key.as_ref()),
                ))
                .add(settings::item(
                    "Self-signing Key:",
                    build_key_row("", status.has_self_signing, info.self_signing_key.as_ref()),
                ))
                .add(settings::item(
                    "User-signing Key:",
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

            let mut email_btn = button::text(crate::fl!("send-verification"));
            let is_email_empty = self.new_3pid_email.trim().is_empty();
            if !is_email_empty {
                email_btn = email_btn.on_press(Message::Request3PIDEmailToken);
            }
            let email_widget: Element<'_, Message> = if is_email_empty {
                tooltip(
                    email_btn,
                    text::body(crate::fl!("enter-email-to-link")),
                    Position::Top,
                )
                .into()
            } else {
                email_btn.into()
            };

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

                let mut complete_btn = button::suggested(crate::fl!("add-account"));
                let is_pass_empty = self.add_3pid_password.is_empty();
                if !is_pass_empty {
                    complete_btn = complete_btn.on_press(Message::Add3PID);
                }
                let complete_widget: Element<'_, Message> = if is_pass_empty {
                    tooltip(
                        complete_btn,
                        text::body(crate::fl!("enter-password-to-confirm")),
                        Position::Top,
                    )
                    .into()
                } else {
                    complete_btn.into()
                };

                section = section.add(settings::item(crate::fl!("complete"), complete_widget));
            }

            let mut phone_btn = button::text(crate::fl!("send-sms"));
            let is_phone_empty = self.new_3pid_msisdn.trim().is_empty();
            if !is_phone_empty {
                phone_btn = phone_btn.on_press(Message::Request3PIDMsisdnToken);
            }
            let phone_widget: Element<'_, Message> = if is_phone_empty {
                tooltip(
                    phone_btn,
                    text::body(crate::fl!("enter-phone-to-link")),
                    Position::Top,
                )
                .into()
            } else {
                phone_btn.into()
            };

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
                            text_input("Phone Number", &self.new_3pid_msisdn)
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

            let mut ignore_btn = button::destructive(crate::fl!("ignore"));
            let is_user_empty = self.new_ignore_user_id.trim().is_empty();
            if !is_user_empty {
                ignore_btn = ignore_btn.on_press(Message::IgnoreUser);
            }

            let ignore_widget: Element<'_, Message> = if is_user_empty {
                tooltip(
                    ignore_btn,
                    text::body(crate::fl!("enter-user-id-to-ignore")),
                    Position::Top,
                )
                .into()
            } else {
                ignore_btn.into()
            };

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

        let mut section = settings::section().title("Verification");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_name_changed() {
        let mut state = State::default();
        let _ = state.update(Message::DisplayNameChanged("John Doe".to_string()), &None);
        assert_eq!(state.display_name, "John Doe");
    }

    #[test]
    fn test_dismiss_error() {
        let mut state = State::default();
        state.error = Some("Error".to_string());
        let _ = state.update(Message::DismissError, &None);
        assert_eq!(state.error, None);
    }

    #[test]
    fn test_devices_loaded() {
        let mut state = State::default();
        state.is_loading_devices = true;

        let devices = vec![DeviceInfo {
            device_id: Arc::from("DEV1"),
            display_name: Some("Test Device".to_string()),
            is_verified: true,
            is_current: true,
            is_renaming: false,
            edit_name: String::new(),
            is_deleting: false,
        }];

        let _ = state.update(Message::DevicesLoaded(Ok(devices.clone())), &None);
        assert!(!state.is_loading_devices);
        assert_eq!(state.devices.len(), 1);
        assert_eq!(state.devices[0].device_id.as_ref(), "DEV1");
        assert!(state.devices[0].is_verified);
        assert!(state.devices[0].is_current);

        state.is_loading_devices = true;
        let _ = state.update(
            Message::DevicesLoaded(Err("network error".to_string())),
            &None,
        );
        assert!(!state.is_loading_devices);
        assert_eq!(
            state.error,
            Some("Failed to load devices: network error".to_string())
        );
    }
}
