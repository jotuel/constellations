use crate::matrix;
use crate::settings;
use crate::utils::item::ConstellationsItem;
use crate::utils::preview::PreviewEvent;

use anyhow::Result;
use cosmic::Core;
use cosmic::iced::widget::image;
use cosmic::widget::menu::action::MenuAction;
use eyeball_im::Vector;
use matrix_sdk::ruma::OwnedRoomId;
use matrix_sdk::ruma::events::room::MediaSource;
use std::collections::HashMap;
use url::Url;

mod app;
mod state;
mod subscriptions;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QrLoginStep {
    NotStarted,
    Initiating,
    ShowingQr,
    RendezvousEstablished,
    Authenticating,
    Success,
    Error,
}

/// Which login flow (if any) is currently in progress.
///
/// Replaces three booleans (`is_logging_in`, `is_oidc_logging_in`,
/// `is_qr_logging_in` + `qr_login_step`) that had to be kept mutually exclusive
/// by hand. With this enum, two flows being active at once is unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthFlow {
    Idle,
    Password,
    Oidc,
    Qr { step: QrLoginStep },
}

pub struct Constellations {
    pub(crate) core: Core,
    pub(crate) matrix: Option<matrix::MatrixEngine>,
    pub(crate) sync_status: matrix::SyncStatus,
    pub(crate) room_list: Vec<matrix::RoomData>,
    pub(crate) filtered_room_list: Vec<usize>,
    pub(crate) other_rooms: Vec<matrix::RoomData>,
    pub(crate) filtered_other_rooms: Vec<usize>,
    pub(crate) selected_room: Option<std::sync::Arc<str>>,
    pub(crate) timeline_items: Vector<ConstellationsItem>,
    pub(crate) composer_content: cosmic::widget::text_editor::Content,
    pub(crate) composer_preview_events: Vec<PreviewEvent>,
    pub(crate) composer_is_preview: bool,
    pub(crate) composer_attachments: Vec<std::path::PathBuf>,
    pub(crate) user_id: Option<String>,
    pub(crate) media_cache: HashMap<String, image::Handle>,
    pub(crate) creating_room: bool,
    pub(crate) creating_space: bool,
    pub(crate) new_room_name: String,
    pub(crate) inviting_to_space: bool,
    pub(crate) invite_to_space_id: String,
    pub(crate) error: Option<String>,
    pub(crate) login_homeserver: String,
    pub(crate) login_username: String,
    pub(crate) login_password: String,
    pub(crate) auth_flow: AuthFlow,
    pub(crate) qr_rendezvous_url: Option<String>,
    pub(crate) is_registering_mode: bool,
    pub(crate) is_registering: bool,
    pub(crate) is_initializing: bool,
    pub(crate) is_sync_indicator_active: bool,
    pub(crate) is_loading_more: bool,
    pub(crate) last_timeline_offset: f32,
    pub(crate) last_threaded_timeline_offset: f32,
    pub(crate) search_query: String,
    pub(crate) is_search_active: bool,
    pub(crate) active_reaction_picker: Option<matrix::TimelineEventItemId>,
    pub(crate) active_thread_root: Option<matrix_sdk::ruma::OwnedEventId>,
    pub(crate) threaded_timeline_items: Vector<ConstellationsItem>,
    pub(crate) joined_room_ids: std::collections::HashSet<std::sync::Arc<str>>,
    pub(crate) visited_room_ids: std::collections::HashSet<std::sync::Arc<str>>,
    pub(crate) is_first_time_joining: bool,
    pub(crate) needs_initial_scroll: bool,
    pub(crate) needs_scroll_restoration: bool,
    pub(crate) needs_threaded_scroll_restoration: bool,
    pub(crate) is_timeline_at_bottom: bool,
    pub(crate) is_threaded_timeline_at_bottom: bool,
    pub(crate) is_timeline_initialized: bool,
    pub(crate) is_threaded_timeline_initialized: bool,
    pub(crate) last_content_height: f32,
    pub(crate) last_threaded_content_height: f32,
    pub(crate) last_viewport_width: f32,
    pub(crate) last_viewport_height: f32,
    pub(crate) last_threaded_viewport_width: f32,
    pub(crate) last_threaded_viewport_height: f32,
    pub(crate) needs_layout_scroll_restoration: bool,
    pub(crate) needs_threaded_layout_scroll_restoration: bool,
    pub(crate) needs_scroll_adjustment: bool,
    pub(crate) needs_threaded_scroll_adjustment: bool,
    pub(crate) replying_to: Option<ConstellationsItem>,
    pub(crate) editing_item: Option<ConstellationsItem>,
    pub(crate) selected_space: Option<OwnedRoomId>,
    pub(crate) current_settings_panel: Option<SettingsPanel>,
    pub(crate) user_settings: settings::user::State,
    pub(crate) room_settings: settings::room::State,
    pub(crate) space_settings: settings::space::State,
    pub(crate) app_settings: settings::app::State,
    pub(crate) call_participants: HashMap<std::sync::Arc<str>, Vec<matrix_sdk::ruma::OwnedUserId>>,
    pub(crate) fullscreen_image: Option<image::Handle>,
    pub(crate) emoji_search_query: String,
    pub(crate) selected_emoji_group: Option<emojis::Group>,
    pub(crate) is_composer_emoji_picker_active: bool,
    pub(crate) room_name_cache: std::collections::HashMap<std::sync::Arc<str>, String>,
    pub(crate) thread_counts: std::collections::HashMap<matrix_sdk::ruma::OwnedEventId, u32>,
    pub(crate) show_pinned_panel: bool,
    pub(crate) is_loading_pinned: bool,
    pub(crate) pinned_events: std::collections::HashSet<matrix_sdk::ruma::OwnedEventId>,
    pub(crate) pinned_events_details: Vec<matrix::PinnedEventInfo>,
    pub(crate) show_members_panel: bool,
    pub(crate) room_members: Vec<matrix::RoomMemberInfo>,
    pub(crate) is_loading_members: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    Matrix(matrix::MatrixEvent),
    RoomSelected(std::sync::Arc<str>),
    EngineReady(Result<matrix::MatrixEngine, matrix::SyncError>),
    ComposerChanged(String),
    ComposerAction(cosmic::widget::text_editor::Action),
    TogglePreview,
    SendMessage,
    ShareLocation,
    LocationRetrieved(Result<(f64, f64), String>),
    MessageSent(Result<(), String>),
    MessageEdited(Result<(), String>),
    MessageRedacted(Result<(), String>),
    AddAttachment,
    AttachmentsSelected(Vec<std::path::PathBuf>),
    RemoveAttachment(usize),
    AttachmentSent(std::path::PathBuf, Result<(), String>),
    ToggleReaction(matrix::TimelineEventItemId, String),
    ReactionToggled(Result<(), String>),
    OpenReactionPicker(Option<matrix::TimelineEventItemId>),
    EmojiSearchQueryChanged(String),
    SelectEmojiGroup(Option<emojis::Group>),
    ToggleComposerEmojiPicker,
    InsertEmoji(String),
    EmojiPickerSelected(&'static str),

    LoadMoreFinished(Result<(), String>),
    TimelineScrolled(cosmic::iced::widget::scrollable::Viewport, bool),
    UserReady(Option<String>, Result<(), matrix::SyncError>),
    FetchMedia(MediaSource),
    MediaFetched(String, Result<Vec<u8>, String>),
    MediaFetchedBatch(Vec<(String, Result<Vec<u8>, String>)>),
    CreateRoom(String),
    RoomCreated(Result<String, String>),
    CreateSpace(String),
    SpaceCreated(Result<String, String>),
    NewRoomNameChanged(String),
    ToggleCreateRoom,
    ToggleCreateSpace,
    ToggleInviteToSpace,
    InviteToSpaceIdChanged(String),
    InviteToSpace,
    SpaceUserInvited(Result<(), String>),
    DismissError,
    LoginHomeserverChanged(String),
    LoginUsernameChanged(String),
    LoginPasswordChanged(String),
    SubmitLogin,
    LoginFinished(Result<String, matrix::SyncError>),
    ToggleLoginMode,
    SubmitRegister,
    RegisterFinished(Result<String, matrix::SyncError>),
    SelectSpace(Option<std::sync::Arc<str>>),
    SpaceChildrenFetched(OwnedRoomId, Result<Vec<matrix::RoomData>, String>),
    OpenThread(matrix_sdk::ruma::OwnedEventId),
    CloseThread,
    StartReply(matrix::TimelineEventItemId),
    CancelReply,
    StartEdit(matrix::TimelineEventItemId),
    CancelEdit,
    RedactMessage(matrix::TimelineEventItemId),
    MatrixThreadDiff(
        matrix_sdk::ruma::OwnedEventId,
        eyeball_im::VectorDiff<std::sync::Arc<matrix::TimelineItem>>,
    ),
    MatrixThreadReset(matrix_sdk::ruma::OwnedEventId),
    MatrixThreadInitFinished(matrix_sdk::ruma::OwnedEventId),
    SpaceFilterUpdated,
    NoOp,
    SubmitOidcLogin,
    CancelOidcLogin,
    OidcLoginStarted(Result<Url, String>),
    OidcCallback(Url),
    StartQrLogin,
    CancelQrLogin,
    QrLoginStepChanged(QrLoginStep),
    JoinRoom(std::sync::Arc<str>),
    RoomJoined(Result<OwnedRoomId, String>),
    Logout,
    LogoutFinished,
    OpenSettings(SettingsPanel),
    CloseSettings,
    UserSettings(settings::user::Message),
    RoomSettings(settings::room::Message),
    SpaceSettings(settings::space::Message),
    AppSettings(settings::app::Message),
    AppSettingChanged,
    ToggleMembersPanel,
    MembersFetched(Result<Vec<matrix::RoomMemberInfo>, String>),
    TogglePinnedPanel,
    PinnedEventsFetched(Result<Vec<matrix::PinnedEventInfo>, String>),
    ToggleSearch,
    SearchQueryChanged(String),
    JoinCall,
    LeaveCall,
    CallJoined(Result<(), String>),
    CallLeft(Result<(), String>),
    OpenUrl(String),
    OpenImage(image::Handle),
    CloseImage,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SettingsPanel {
    App,
    User,
    Room,
    Space,
    Members,
    Pinned,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum MenuAct {
    AppSettings,
    UserSettings,
    Logout,
    CreateRoom,
    CreateSpace,
    SpaceSettings,
    SpaceInvite,
}

impl MenuAction for MenuAct {
    type Message = Message;
    fn message(&self) -> Self::Message {
        match self {
            MenuAct::AppSettings => Message::OpenSettings(SettingsPanel::App),
            MenuAct::UserSettings => Message::OpenSettings(SettingsPanel::User),
            MenuAct::Logout => Message::Logout,
            MenuAct::CreateRoom => Message::ToggleCreateRoom,
            MenuAct::CreateSpace => Message::ToggleCreateSpace,
            MenuAct::SpaceSettings => Message::OpenSettings(SettingsPanel::Space),
            MenuAct::SpaceInvite => Message::ToggleInviteToSpace,
        }
    }
}
