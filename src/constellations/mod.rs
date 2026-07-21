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
mod handlers;
mod state;
mod subscriptions;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QrLoginStep {
    NotStarted,
    Initiating,
    ShowingQr,
    /// The other device scanned the QR; the UI must collect the two-digit
    /// check code from the user and submit it.
    AwaitingCheckCode,
    Authenticating,
    /// Transferring end-to-end encryption secrets from the existing device.
    SyncingSecrets,
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

/// What `RoomAliasResolved` should do with the resolved room ID.
///
/// Carried across the async alias-resolution hop so a single resolve path can
/// serve open-room, join-room, and open-event permalink targets.
#[derive(Debug, Clone)]
pub(crate) enum PendingAliasOp {
    /// Select the resolved room.
    OpenRoom,
    /// Join the resolved room (the link carried `action=join`).
    JoinRoom,
    /// Open the resolved room and scroll to this event (already-loaded only;
    /// Phase 3 adds the not-yet-loaded fetch path).
    OpenEvent(matrix_sdk::ruma::OwnedEventId),
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
    /// A Matrix permalink that arrived before login; replayed once the session
    /// is restored. Set by `OpenMatrixLink` when `matrix` is `None`.
    pub(crate) pending_link: Option<String>,
    /// An event a permalink asked us to scroll to, stashed while we wait for
    /// the target room's timeline to finish initialising. Consumed in the
    /// `TimelineInitFinished` handler: if the event is already in the loaded
    /// window we just scroll to it, otherwise we build an event-focused
    /// timeline around it.
    pub(crate) pending_event_focus: Option<matrix_sdk::ruma::OwnedEventId>,
    /// When set, the room is being viewed through an event-focused (permalink
    /// context) timeline instead of the live one. Drives the "viewing older
    /// messages" banner and selects the event-focused subscription. Cleared by
    /// `ReturnToLive` or a room switch.
    pub(crate) active_event_focus: Option<matrix_sdk::ruma::OwnedEventId>,
    /// In-app "Open link…" dialog state. `Some(text)` shows the paste-link
    /// context drawer with that input value; `None` hides it.
    pub(crate) open_link_dialog: Option<String>,
    /// What to do once an in-flight room-alias resolution completes. Set just
    /// before kicking off `resolve_room_alias` so `RoomAliasResolved` knows
    /// whether to open the room, join it, or open an event in it.
    pub(crate) pending_alias_op: Option<PendingAliasOp>,
    pub(crate) timeline_items: Vector<ConstellationsItem>,
    pub(crate) composer_content: cosmic::widget::text_editor::Content,
    pub(crate) composer_preview_events: Vec<PreviewEvent>,
    pub(crate) composer_preview_links: Vec<(String, String)>,
    pub(crate) composer_is_preview: bool,
    pub(crate) composer_attachments: Vec<std::path::PathBuf>,
    pub(crate) user_id: Option<String>,
    pub(crate) media_cache: HashMap<String, image::Handle>,
    pub(crate) creating_room: bool,
    pub(crate) creating_space: bool,
    pub(crate) new_room_name: String,
    pub(crate) inviting_to_space: bool,
    pub(crate) invite_to_space_id: String,
    pub(crate) inviting_to_room: bool,
    pub(crate) invite_to_room_id: String,
    pub(crate) error: Option<String>,
    pub(crate) login_homeserver: String,
    pub(crate) login_username: String,
    pub(crate) login_password: String,
    pub(crate) auth_flow: AuthFlow,
    /// Raw MSC4108 QR payload bytes to render during `QrLoginStep::ShowingQr`.
    pub(crate) qr_code_bytes: Option<Vec<u8>>,
    /// Held while `QrLoginStep::AwaitingCheckCode`: the user submits the
    /// two-digit code via this sender.
    pub(crate) qr_check_code_sender:
        Option<matrix_sdk::authentication::oauth::qrcode::CheckCodeSender>,
    /// The user code to display during `WaitingForToken`, if any.
    pub(crate) qr_user_code: Option<String>,
    /// Buffer for the check-code text input.
    pub(crate) qr_check_code_input: String,
    pub(crate) is_registering_mode: bool,
    pub(crate) is_registering: bool,
    pub(crate) is_initializing: bool,
    pub(crate) is_sync_indicator_active: bool,
    pub(crate) is_loading_more: bool,
    pub(crate) last_timeline_offset: f32,
    pub(crate) last_threaded_timeline_offset: f32,
    pub(crate) search_query: String,
    pub(crate) is_search_active: bool,
    pub(crate) public_search_results: Vec<matrix::PublicRoom>,
    pub(crate) is_searching_public: bool,
    /// Server-side message search results (full room history, not just the
    /// loaded timeline window). Populated by `MessageSearchResults`.
    pub(crate) message_search_results: Vec<matrix::MessageSearchResult>,
    /// True while a server-side message search is in flight.
    pub(crate) is_searching_messages: bool,
    pub(crate) search_has_more: bool,
    pub(crate) is_searching_more_messages: bool,
    /// Monotonic counter used to discard stale in-flight message searches
    /// (debounce). Each `SearchQueryChanged` increments it; the async task
    /// captures the value at spawn time and the result is dropped if it no
    /// longer matches. Shared by the in-room and global message searches.
    pub(crate) search_generation: u64,
    /// Cross-room message search results. Populated by
    /// `GlobalMessageSearchResults` when no room is selected and a query is
    /// entered. Each hit carries its originating `room_id`/`room_name`.
    pub(crate) global_message_search_results: Vec<matrix::MessageSearchResult>,
    /// True while a cross-room message search is in flight.
    pub(crate) is_searching_global_messages: bool,
    /// Scope (All / DMs / Groups) for the cross-room message search.
    pub(crate) global_search_scope: matrix::GlobalSearchScope,
    pub(crate) new_room_is_video: bool,
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
    ToggleInviteToRoom,
    InviteToRoomIdChanged(String),
    InviteToRoom,
    RoomUserInvited(Result<(), String>),
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
    CopyMessageLink(matrix::TimelineEventItemId),
    CopyRoomLink(std::sync::Arc<str>),
    CopyToClipboard(Result<String, String>),
    DmRoomResolved(Result<matrix_sdk::ruma::OwnedRoomId, String>),
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
    OpenMatrixLink(String),
    /// Toggle the in-app "Open link…" paste dialog open/closed.
    ToggleOpenLink,
    /// The paste-link dialog input changed.
    OpenLinkTextChanged(String),
    /// Submit the paste-link dialog; carries the raw link text.
    SubmitOpenLink(String),
    RoomAliasResolved(Box<Result<OwnedRoomId, String>>),
    StartQrLogin,
    CancelQrLogin,
    /// A progress event from the MSC4108 QR-login background task.
    QrLoginProgress(matrix::QrLoginProgress),
    /// The check-code text input changed during `AwaitingCheckCode`.
    QrCheckCodeChanged(String),
    /// Submit the entered check code back to the QR-login task.
    SubmitQrCheckCode,
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
    UnpinMessage(matrix_sdk::ruma::OwnedEventId),
    ToggleSearch,
    SearchQueryChanged(String),
    /// Public rooms / spaces directory search results. Carries the generation
    /// captured at task spawn so stale results can be discarded (debounce).
    PublicSearchResults(u64, Result<Vec<matrix::PublicRoom>, String>),
    /// Server-side message search results for the in-room search. Carries the
    /// generation captured at task spawn so stale results can be discarded.
    MessageSearchResults(
        u64,
        Result<(Vec<matrix::MessageSearchResult>, bool), String>,
    ),
    LoadMoreMessageSearch,
    MessageSearchMoreResults(Result<(Vec<matrix::MessageSearchResult>, bool), String>),
    /// Cross-room (global) message search results. Same generation guard as
    /// `MessageSearchResults` (both share `search_generation`).
    GlobalMessageSearchResults(u64, Result<Vec<matrix::MessageSearchResult>, String>),
    /// Change the scope (All / DMs / Groups) of the cross-room message search.
    /// Re-fires the current query under the new scope.
    SetGlobalSearchScope(matrix::GlobalSearchScope),
    NewRoomIsVideoChanged(bool),
    JumpToMessage(matrix_sdk::ruma::OwnedEventId),
    /// Jump to a message from a search hit, choosing the right path depending
    /// on whether it is already in the live timeline window: scroll if loaded,
    /// otherwise build an event-focused timeline via `LoadEventContext`.
    JumpToMessageOrLoadContext(matrix_sdk::ruma::OwnedEventId),
    /// Open a (possibly different) room and jump to one of its events, e.g.
    /// from a cross-room message search hit. Dispatched by `OpenRoomEvent` as
    /// a follow-up to `RoomSelected` so the focus lands *after* `RoomSelected`
    /// clears it (see `Message::OpenRoomEvent`).
    SetPendingEventFocus(matrix_sdk::ruma::OwnedEventId),
    /// Open a room and jump to one of its events. Used by cross-room message
    /// search result cards.
    OpenRoomEvent {
        room_id: std::sync::Arc<str>,
        event_id: matrix_sdk::ruma::OwnedEventId,
    },
    /// Build an event-focused (permalink context) timeline around an event not
    /// present in the live window, then scroll to it.
    LoadEventContext(matrix_sdk::ruma::OwnedEventId),
    /// Result of building an event-focused timeline. On success the event id is
    /// the one now centred; on error a reason string for a toast.
    EventContextLoaded(matrix_sdk::ruma::OwnedEventId, Result<(), String>),
    /// Leave the event-focused (permalink context) timeline and restore the
    /// live one at the bottom. Emitted by the "Jump to newest" banner button.
    ReturnToLive,
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
    ManageRoomMembers,
    ManageSpaceRooms,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum MenuAct {
    AppSettings,
    UserSettings,
    Logout,
    OpenLink,
    CreateRoom,
    CreateSpace,
    SpaceSettings,
    SpaceInvite,
    RoomSettings,
    RoomInvite,
    ManageRoomMembers,
    ManageSpaceRooms,
}

impl MenuAction for MenuAct {
    type Message = Message;
    fn message(&self) -> Self::Message {
        match self {
            MenuAct::AppSettings => Message::OpenSettings(SettingsPanel::App),
            MenuAct::UserSettings => Message::OpenSettings(SettingsPanel::User),
            MenuAct::Logout => Message::Logout,
            MenuAct::OpenLink => Message::ToggleOpenLink,
            MenuAct::CreateRoom => Message::ToggleCreateRoom,
            MenuAct::CreateSpace => Message::ToggleCreateSpace,
            MenuAct::SpaceSettings => Message::OpenSettings(SettingsPanel::Space),
            MenuAct::SpaceInvite => Message::ToggleInviteToSpace,
            MenuAct::RoomSettings => Message::OpenSettings(SettingsPanel::Room),
            MenuAct::RoomInvite => Message::ToggleInviteToRoom,
            MenuAct::ManageRoomMembers => Message::OpenSettings(SettingsPanel::ManageRoomMembers),
            MenuAct::ManageSpaceRooms => Message::OpenSettings(SettingsPanel::ManageSpaceRooms),
        }
    }
}

#[cfg(test)]
impl Constellations {
    pub fn mock() -> Self {
        let config = crate::settings::config::Config::default();
        crate::constellations::app::app(cosmic::Core::default(), config)
    }
}
