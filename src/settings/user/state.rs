use matrix_sdk::encryption::CrossSigningStatus;
use matrix_sdk::encryption::verification::{SasVerification, VerificationRequest};
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

impl State {
    pub fn from_config(config: &crate::settings::config::Config) -> Self {
        Self {
            media_previews_display_policy: config.media_previews_display_policy,
            invite_avatars_display_policy: config.invite_avatars_display_policy,
            ..Default::default()
        }
    }
}
