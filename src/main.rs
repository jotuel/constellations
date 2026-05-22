#![recursion_limit = "256"]
mod handlers;
pub mod i18n;
mod ipc;
mod matrix;
pub mod rich_text;
pub mod settings;
mod view;

use anyhow::Result;
use cosmic::iced::widget::image;
use cosmic::iced::widget::scrollable;
use cosmic::iced::widget::tooltip;
use cosmic::iced::{Alignment, Subscription};
use cosmic::widget::Widget;
use cosmic::widget::icon::Named;
use cosmic::widget::menu::action::MenuAction;
use cosmic::widget::tooltip::Position;
use cosmic::widget::{Column, RcElementWrapper, Row, button, container, menu, text, text_input};
use cosmic::{Action, Application, Core, Element, Task};
use eyeball_im::Vector;
use matrix_sdk::ruma::OwnedRoomId;
use matrix_sdk::ruma::events::room::MediaSource;
use matrix_sdk_ui::sync_service::State as SyncServiceState;
use mimalloc::MiMalloc;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use url::Url;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;
const CONSTELLATIONS_ICON: &[u8] = include_bytes!("../res/const.svg");

static TIMELINE_ID: LazyLock<cosmic::iced::widget::Id> =
    LazyLock::new(cosmic::iced::widget::Id::unique);
static THREADED_TIMELINE_ID: LazyLock<cosmic::iced::widget::Id> =
    LazyLock::new(cosmic::iced::widget::Id::unique);

// ⚡ Bolt Optimization:
// We cache the parsed Markdown structure in `PreviewEvent`s to avoid running
// `pulldown_cmark::Parser` on every single render frame inside `view_preview()`.
#[derive(Clone, Debug, PartialEq)]
pub enum PreviewEvent {
    StartHeading,
    EndBlock,
    Text(String),
    Code(String),
    Break,
    StartLink(String),
    EndLink,
}

pub fn parse_markdown(text: &str, skip_first_blockquote: bool) -> Vec<PreviewEvent> {
    let mut events = Vec::new();
    let mut options = pulldown_cmark::Options::empty();
    options.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    options.insert(pulldown_cmark::Options::ENABLE_TASKLISTS);

    let parser = pulldown_cmark::Parser::new_ext(text, options);
    let mut in_blockquote = 0;
    let mut is_first_blockquote = true;

    for event in parser {
        match event {
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::BlockQuote(_)) => {
                in_blockquote += 1;
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::BlockQuote(_)) => {
                if in_blockquote > 0 {
                    in_blockquote -= 1;
                    if in_blockquote == 0 {
                        is_first_blockquote = false;
                    }
                }
            }
            _ => {
                if in_blockquote > 0 && skip_first_blockquote && is_first_blockquote {
                    continue;
                }
                match event {
                    pulldown_cmark::Event::Start(pulldown_cmark::Tag::Heading { .. }) => {
                        events.push(PreviewEvent::StartHeading)
                    }
                    pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link {
                        dest_url, ..
                    }) => events.push(PreviewEvent::StartLink(dest_url.to_string())),
                    pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Link) => {
                        events.push(PreviewEvent::EndLink)
                    }
                    pulldown_cmark::Event::End(
                        pulldown_cmark::TagEnd::Paragraph | pulldown_cmark::TagEnd::Heading(_),
                    ) => events.push(PreviewEvent::EndBlock),
                    pulldown_cmark::Event::Text(t) => {
                        events.push(PreviewEvent::Text(t.to_string()))
                    }
                    pulldown_cmark::Event::Code(c) => {
                        events.push(PreviewEvent::Code(c.to_string()))
                    }
                    pulldown_cmark::Event::SoftBreak | pulldown_cmark::Event::HardBreak => {
                        events.push(PreviewEvent::Break)
                    }
                    _ => {}
                }
            }
        }
    }
    events
}

// ⚡ Bolt Optimization: Fast path for ASCII string filtering
// Avoids costly heap allocations from `.to_lowercase()`
pub fn contains_ignore_ascii_case(
    haystack: &str,
    query: &str,
    query_lower_fallback: Option<&str>,
) -> bool {
    if query.is_empty() {
        return true;
    }

    if query.is_ascii() {
        let query_bytes = query.as_bytes();
        let query_len = query_bytes.len();
        let h_bytes = haystack.as_bytes();

        if h_bytes.len() < query_len {
            return false;
        }

        let first_char = query_bytes[0];
        let first_lower = first_char.to_ascii_lowercase();
        let first_upper = first_char.to_ascii_uppercase();

        for i in 0..=(h_bytes.len() - query_len) {
            let h_first = h_bytes[i];
            if (h_first == first_lower || h_first == first_upper)
                && h_bytes[i + 1..i + query_len].eq_ignore_ascii_case(&query_bytes[1..])
            {
                return true;
            }
        }
        false
    } else if let Some(query_lower) = query_lower_fallback {
        haystack.to_lowercase().contains(query_lower)
    } else {
        haystack.to_lowercase().contains(&query.to_lowercase())
    }
}

struct Constellations {
    core: Core,
    matrix: Option<matrix::MatrixEngine>,
    sync_status: matrix::SyncStatus,
    room_list: Vec<matrix::RoomData>,
    filtered_room_list: Vec<usize>,
    other_rooms: Vec<matrix::RoomData>,
    filtered_other_rooms: Vec<usize>,
    selected_room: Option<std::sync::Arc<str>>,
    timeline_items: Vector<ConstellationsItem>,
    composer_content: cosmic::widget::text_editor::Content,
    composer_preview_events: Vec<PreviewEvent>,
    composer_is_preview: bool,
    composer_attachments: Vec<std::path::PathBuf>,
    user_id: Option<String>,
    media_cache: HashMap<String, image::Handle>,
    creating_room: bool,
    creating_space: bool,
    new_room_name: String,
    error: Option<String>,
    login_homeserver: String,
    login_username: String,
    login_password: String,
    is_logging_in: bool,
    is_oidc_logging_in: bool,
    is_registering_mode: bool,
    is_registering: bool,
    is_initializing: bool,
    is_sync_indicator_active: bool,
    is_loading_more: bool,
    last_timeline_offset: f32,
    last_threaded_timeline_offset: f32,
    search_query: String,
    is_search_active: bool,
    active_reaction_picker: Option<matrix::TimelineEventItemId>,
    active_thread_root: Option<matrix_sdk::ruma::OwnedEventId>,
    threaded_timeline_items: Vector<ConstellationsItem>,
    joined_room_ids: std::collections::HashSet<std::sync::Arc<str>>,
    replying_to: Option<ConstellationsItem>,
    editing_item: Option<ConstellationsItem>,
    selected_space: Option<OwnedRoomId>,
    current_settings_panel: Option<SettingsPanel>,
    user_settings: settings::user::State,
    room_settings: settings::room::State,
    space_settings: settings::space::State,
    app_settings: settings::app::State,
    call_participants: HashMap<std::sync::Arc<str>, Vec<matrix_sdk::ruma::OwnedUserId>>,
    fullscreen_image: Option<image::Handle>,
    emoji_search_query: String,
    selected_emoji_group: Option<emojis::Group>,
    is_composer_emoji_picker_active: bool,
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
    SpaceFilterUpdated,
    NoOp,
    SubmitOidcLogin,
    OidcLoginStarted(Result<Url, String>),
    OidcCallback(Url),
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum MenuAct {
    AppSettings,
    UserSettings,
    Logout,
    CreateRoom,
    CreateSpace,
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
        }
    }
}

#[derive(Clone, Debug)]
struct MatrixEngineWrapper(matrix::MatrixEngine);

impl std::hash::Hash for MatrixEngineWrapper {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        "matrix-sync".hash(state);
    }
}

impl PartialEq for MatrixEngineWrapper {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for MatrixEngineWrapper {}

trait ApplyVectorDiffExt<T> {
    fn apply_diff(&mut self, diff: eyeball_im::VectorDiff<T>);
}

trait VectorOperations<T> {
    fn v_len(&self) -> usize;
    fn v_insert(&mut self, index: usize, value: T);
    fn v_remove(&mut self, index: usize);
    fn v_set(&mut self, index: usize, value: T);
    fn v_push_back(&mut self, value: T);
    fn v_push_front(&mut self, value: T);
    fn v_pop_back(&mut self);
    fn v_pop_front(&mut self);
    fn v_clear(&mut self);
    fn v_reset(&mut self, values: eyeball_im::Vector<T>);
    fn v_extend(&mut self, values: eyeball_im::Vector<T>);
    fn v_truncate(&mut self, length: usize);
}

impl<T: Clone> VectorOperations<T> for Vec<T> {
    fn v_len(&self) -> usize {
        self.len()
    }
    fn v_insert(&mut self, index: usize, value: T) {
        self.insert(index, value);
    }
    fn v_remove(&mut self, index: usize) {
        self.remove(index);
    }
    fn v_set(&mut self, index: usize, value: T) {
        self[index] = value;
    }
    fn v_push_back(&mut self, value: T) {
        self.push(value);
    }
    fn v_push_front(&mut self, value: T) {
        self.insert(0, value);
    }
    fn v_pop_back(&mut self) {
        self.pop();
    }
    fn v_pop_front(&mut self) {
        if !self.is_empty() {
            self.remove(0);
        }
    }
    fn v_clear(&mut self) {
        self.clear();
    }
    fn v_reset(&mut self, values: eyeball_im::Vector<T>) {
        *self = values.into_iter().collect();
    }
    fn v_extend(&mut self, values: eyeball_im::Vector<T>) {
        self.extend(values);
    }
    fn v_truncate(&mut self, length: usize) {
        self.truncate(length);
    }
}

impl<T: Clone> VectorOperations<T> for eyeball_im::Vector<T> {
    fn v_len(&self) -> usize {
        self.len()
    }
    fn v_insert(&mut self, index: usize, value: T) {
        self.insert(index, value);
    }
    fn v_remove(&mut self, index: usize) {
        self.remove(index);
    }
    fn v_set(&mut self, index: usize, value: T) {
        self.set(index, value);
    }
    fn v_push_back(&mut self, value: T) {
        self.push_back(value);
    }
    fn v_push_front(&mut self, value: T) {
        self.push_front(value);
    }
    fn v_pop_back(&mut self) {
        self.pop_back();
    }
    fn v_pop_front(&mut self) {
        self.pop_front();
    }
    fn v_clear(&mut self) {
        self.clear();
    }
    fn v_reset(&mut self, values: eyeball_im::Vector<T>) {
        *self = values;
    }
    fn v_extend(&mut self, values: eyeball_im::Vector<T>) {
        self.extend(values);
    }
    fn v_truncate(&mut self, length: usize) {
        self.truncate(length);
    }
}

impl<C: VectorOperations<T>, T: Clone> ApplyVectorDiffExt<T> for C {
    fn apply_diff(&mut self, diff: eyeball_im::VectorDiff<T>) {
        match diff {
            eyeball_im::VectorDiff::Insert { index, value } => {
                if index <= self.v_len() {
                    self.v_insert(index, value);
                } else {
                    self.v_push_back(value);
                }
            }
            eyeball_im::VectorDiff::Remove { index } => {
                if index < self.v_len() {
                    self.v_remove(index);
                }
            }
            eyeball_im::VectorDiff::Set { index, value } => {
                if index < self.v_len() {
                    self.v_set(index, value);
                }
            }
            eyeball_im::VectorDiff::Reset { values } => {
                self.v_reset(values);
            }
            eyeball_im::VectorDiff::PushBack { value } => {
                self.v_push_back(value);
            }
            eyeball_im::VectorDiff::PushFront { value } => {
                self.v_push_front(value);
            }
            eyeball_im::VectorDiff::PopBack => {
                self.v_pop_back();
            }
            eyeball_im::VectorDiff::PopFront => {
                self.v_pop_front();
            }
            eyeball_im::VectorDiff::Clear => {
                self.v_clear();
            }
            eyeball_im::VectorDiff::Append { values } => {
                self.v_extend(values);
            }
            eyeball_im::VectorDiff::Truncate { length } => {
                self.v_truncate(length);
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct ConstellationsItem {
    pub item: Arc<matrix::TimelineItem>,
    pub sender_id: matrix_sdk::ruma::OwnedUserId,
    pub sender_name: String,
    pub avatar_url: Option<String>,
    pub timestamp: String,
    pub is_me: bool,
    pub markdown: Vec<PreviewEvent>,
}

impl ConstellationsItem {
    fn new(item: Arc<matrix::TimelineItem>, user_id: Option<&str>) -> Self {
        let mut sender_id = matrix_sdk::ruma::user_id!("@unknown:example.com").to_owned();
        let mut sender_name = String::new();
        let mut avatar_url = None;
        let mut timestamp = String::new();
        let mut is_me = false;
        let mut markdown = Vec::new();

        if let Some(event) = item.as_event() {
            sender_id = event.sender().to_owned();
            if let Some(msg) = event.content().as_message() {
                let is_reply = event.content().in_reply_to().is_some();
                markdown = crate::parse_markdown(msg.body(), is_reply);
            }
            let (name, url) = match event.sender_profile() {
                matrix_sdk_ui::timeline::TimelineDetails::Ready(profile) => (
                    profile
                        .display_name
                        .as_deref()
                        .unwrap_or(event.sender().as_ref())
                        .to_string(),
                    profile.avatar_url.as_ref().map(|uri| uri.to_string()),
                ),
                _ => (event.sender().to_string(), None),
            };
            sender_name = name;
            avatar_url = url;

            let ts_millis = u64::from(event.timestamp().0);
            let datetime =
                chrono::DateTime::from_timestamp_millis(ts_millis as i64).unwrap_or_default();
            timestamp = datetime
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string();

            is_me = user_id == Some(event.sender().as_str());
        }

        Self {
            item,
            sender_id,
            sender_name,
            avatar_url,
            timestamp,
            is_me,
            markdown,
        }
    }
}

impl Constellations {
    pub fn update_filtered_rooms(&mut self) {
        let is_search_empty = self.search_query.is_empty();

        let is_query_ascii = self.search_query.is_ascii();
        let search_query_lower_fallback =
            (!is_query_ascii).then(|| self.search_query.to_lowercase());

        let filter_by_search = |room: &matrix::RoomData| {
            if is_search_empty {
                true
            } else {
                room.name
                    .as_ref()
                    .map(|n| {
                        contains_ignore_ascii_case(
                            n,
                            &self.search_query,
                            search_query_lower_fallback.as_deref(),
                        )
                    })
                    .unwrap_or(false)
                    || contains_ignore_ascii_case(
                        &room.id,
                        &self.search_query,
                        search_query_lower_fallback.as_deref(),
                    )
            }
        };

        if let Some(selected_space) = &self.selected_space {
            if let Some(matrix) = &self.matrix {
                // ⚡ Bolt Optimization: Reuse the existing vector allocation to avoid O(N) allocation on every keystroke
                let mut rooms = std::mem::take(&mut self.filtered_room_list);

                if matrix.filter_in_space_bulk_sync(
                    self.room_list
                        .iter()
                        .enumerate()
                        .filter(|(_, r)| !r.is_space),
                    selected_space,
                    &mut rooms,
                    filter_by_search,
                ) {
                    rooms.sort_by(|&a, &b| {
                        let ra = &self.room_list[a];
                        let rb = &self.room_list[b];
                        match (&ra.order, &rb.order) {
                            (Some(oa), Some(ob)) => oa.cmp(ob).then_with(|| ra.id.cmp(&rb.id)),
                            (Some(_), None) => std::cmp::Ordering::Less,
                            (None, Some(_)) => std::cmp::Ordering::Greater,
                            (None, None) => ra.id.cmp(&rb.id),
                        }
                    });
                    self.filtered_room_list = rooms;
                } else {
                    // If we couldn't get the lock, just return and keep the old list
                    self.filtered_room_list = rooms;
                    return;
                }
            }

            // Re-filter other_rooms to remove any that we've now joined
            self.other_rooms
                .retain(|r| !self.joined_room_ids.contains(r.id.as_ref()));

            let mut filtered_other = std::mem::take(&mut self.filtered_other_rooms);
            filtered_other.clear();
            filtered_other.extend(
                self.other_rooms
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| filter_by_search(r))
                    .map(|(i, _)| i),
            );
            self.filtered_other_rooms = filtered_other;
        } else {
            let mut rooms = std::mem::take(&mut self.filtered_room_list);
            rooms.clear();
            rooms.extend(
                self.room_list
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| !r.is_space && filter_by_search(r))
                    .map(|(i, _)| i),
            );
            rooms.sort_by(|&a, &b| self.room_list[a].id.cmp(&self.room_list[b].id));
            self.filtered_room_list = rooms;
            self.other_rooms.clear();
            self.filtered_other_rooms.clear();
        }
    }

    fn ipc_subscription(&self) -> Subscription<Message> {
        Subscription::run_with((), |_| {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            tokio::spawn(async move {
                match ipc::start_server(tx).await {
                    Ok(_conn) => {
                        tracing::info!("IPC server started and waiting");
                    }
                    Err(e) => {
                        tracing::error!("Failed to start IPC server: {}", e);
                        return;
                    }
                }
                std::future::pending::<()>().await;
            });
            cosmic::iced::futures::stream::unfold(rx, |mut rx| async move {
                loop {
                    if let Some(uri) = rx.recv().await {
                        if let Ok(url) = Url::parse(&uri) {
                            return Some((Message::OidcCallback(url), rx));
                        }
                    } else {
                        return None;
                    }
                }
            })
        })
    }

    fn sync_subscription(&self, matrix: &matrix::MatrixEngine) -> Subscription<Message> {
        Subscription::run_with(MatrixEngineWrapper(matrix.clone()), |wrapper| {
            let engine = wrapper.0.clone();
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

            let tx_status = tx.clone();
            let engine_status = engine.clone();
            tokio::spawn(async move {
                let sync_service = loop {
                    if let Some(s) = engine_status.sync_service().await {
                        break s;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                };

                let mut status_stream = sync_service.state();
                while let Some(status) = status_stream.next().await {
                    let sync_status = match status {
                            SyncServiceState::Idle => matrix::SyncStatus::Connected,
                            SyncServiceState::Running => matrix::SyncStatus::Syncing,
                            SyncServiceState::Terminated => matrix::SyncStatus::Disconnected,
                            SyncServiceState::Offline => matrix::SyncStatus::Disconnected,
                            SyncServiceState::Error(_) => {
                                matrix::SyncStatus::Error("Sync error encountered. This may be due to missing server support for Sliding Sync (MSC4186) or network issues.".to_string())
                            }
                        };
                    let _ = tx_status.send(Message::Matrix(
                        matrix::MatrixEvent::SyncStatusChanged(sync_status),
                    ));
                }
            });

            let tx_indicator = tx.clone();
            let engine_indicator = engine.clone();
            tokio::spawn(async move {
                let room_list_service = loop {
                    if let Some(rls) = engine_indicator.room_list_service().await {
                        break rls;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                };

                let mut indicator_stream = Box::pin(room_list_service.sync_indicator(
                    std::time::Duration::from_millis(500),
                    std::time::Duration::from_millis(500),
                ));
                use cosmic::iced::futures::StreamExt;
                while let Some(indicator) = indicator_stream.next().await {
                    let show = indicator == matrix_sdk_ui::room_list_service::SyncIndicator::Show;
                    let _ = tx_indicator.send(Message::Matrix(
                        matrix::MatrixEvent::SyncIndicatorChanged(show),
                    ));
                }
            });

            let tx_ignored = tx.clone();
            let engine_ignored = engine.clone();
            tokio::spawn(async move {
                let client = engine_ignored.client().await;
                client.add_event_handler(
                    move |ev: matrix_sdk::ruma::events::ignored_user_list::IgnoredUserListEvent| {
                        let tx = tx_ignored.clone();
                        async move {
                            let users = ev.content.ignored_users.keys().cloned().collect();
                            let _ = tx.send(Message::Matrix(
                                matrix::MatrixEvent::IgnoredUsersChanged(users),
                            ));
                        }
                    },
                );
            });

            let tx_calls = tx.clone();
            let engine_calls = engine.clone();
            tokio::spawn(async move {
                let client = engine_calls.client().await;
                client.add_event_handler(
                    move |_ev: matrix_sdk::ruma::events::SyncStateEvent<
                        matrix_sdk::ruma::events::call::member::CallMemberEventContent,
                    >,
                          room: matrix_sdk::Room| {
                        let tx = tx_calls.clone();
                        let engine = engine_calls.clone();
                        async move {
                            let room_id = room.room_id().to_string();
                            let participants = engine.get_call_participants(&room_id).await;
                            let _ = tx.send(Message::Matrix(
                                matrix::MatrixEvent::CallParticipantsChanged {
                                    room_id,
                                    participants,
                                },
                            ));
                        }
                    },
                );
            });

            let tx_hierarchy = tx.clone();
            let engine_hierarchy = engine.clone();
            tokio::spawn(async move {
                let client = engine_hierarchy.client().await;
                let tx_child = tx_hierarchy.clone();
                client.add_event_handler(
                    move |_ev: matrix_sdk::ruma::events::SyncStateEvent<
                        matrix_sdk::ruma::events::space::child::SpaceChildEventContent,
                    >,
                          _room: matrix_sdk::Room| {
                        let tx = tx_child.clone();
                        async move {
                            let _ = tx
                                .send(Message::Matrix(matrix::MatrixEvent::SpaceHierarchyChanged));
                        }
                    },
                );
                let tx_parent = tx_hierarchy.clone();
                client.add_event_handler(
                    move |_ev: matrix_sdk::ruma::events::SyncStateEvent<
                        matrix_sdk::ruma::events::space::parent::SpaceParentEventContent,
                    >,
                          _room: matrix_sdk::Room| {
                        let tx = tx_parent.clone();
                        async move {
                            let _ = tx
                                .send(Message::Matrix(matrix::MatrixEvent::SpaceHierarchyChanged));
                        }
                    },
                );
            });

            let tx_rooms = tx.clone();
            let engine_rooms = engine.clone();
            tokio::spawn(async move {
                let room_list_service = loop {
                    if let Some(rls) = engine_rooms.room_list_service().await {
                        break rls;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                };
                let rooms = match room_list_service.all_rooms().await {
                    Ok(rooms) => rooms,
                    Err(_) => return,
                };
                let (stream, controller) = rooms.entries_with_dynamic_adapters(20);
                let controller = Arc::new(controller);
                engine_rooms
                    .set_room_list_controller(controller.clone())
                    .await;

                use matrix_sdk_ui::room_list_service::filters;
                controller.set_filter(Box::new(filters::new_filter_all(vec![])));

                use cosmic::iced::futures::StreamExt;
                let mut stream = Box::pin(stream);
                while let Some(diffs) = stream.next().await {
                    for diff in diffs {
                        let room_diff = match diff {
                            eyeball_im::VectorDiff::Insert { index, value } => {
                                get_room_data(&engine_rooms, value.room_id())
                                    .await
                                    .map(|data| eyeball_im::VectorDiff::Insert {
                                        index,
                                        value: data,
                                    })
                            }
                            eyeball_im::VectorDiff::Remove { index } => {
                                Some(eyeball_im::VectorDiff::Remove { index })
                            }
                            eyeball_im::VectorDiff::Set { index, value } => {
                                get_room_data(&engine_rooms, value.room_id())
                                    .await
                                    .map(|data| eyeball_im::VectorDiff::Set { index, value: data })
                            }
                            eyeball_im::VectorDiff::Reset { values } => {
                                let futures: Vec<_> = values
                                    .iter()
                                    .map(|v| get_room_data(&engine_rooms, v.room_id()))
                                    .collect();
                                let new_values: Vec<_> =
                                    cosmic::iced::futures::future::join_all(futures)
                                        .await
                                        .into_iter()
                                        .flatten()
                                        .collect();
                                Some(eyeball_im::VectorDiff::Reset {
                                    values: new_values.into(),
                                })
                            }
                            eyeball_im::VectorDiff::Append { values } => {
                                let futures: Vec<_> = values
                                    .iter()
                                    .map(|v| get_room_data(&engine_rooms, v.room_id()))
                                    .collect();
                                let new_values: Vec<_> =
                                    cosmic::iced::futures::future::join_all(futures)
                                        .await
                                        .into_iter()
                                        .flatten()
                                        .collect();
                                Some(eyeball_im::VectorDiff::Append {
                                    values: new_values.into(),
                                })
                            }
                            eyeball_im::VectorDiff::Truncate { length } => {
                                Some(eyeball_im::VectorDiff::Truncate { length })
                            }
                            eyeball_im::VectorDiff::PushBack { value } => {
                                get_room_data(&engine_rooms, value.room_id())
                                    .await
                                    .map(|data| eyeball_im::VectorDiff::PushBack { value: data })
                            }
                            eyeball_im::VectorDiff::PushFront { value } => {
                                get_room_data(&engine_rooms, value.room_id())
                                    .await
                                    .map(|data| eyeball_im::VectorDiff::PushFront { value: data })
                            }
                            eyeball_im::VectorDiff::PopBack => {
                                Some(eyeball_im::VectorDiff::PopBack)
                            }
                            eyeball_im::VectorDiff::PopFront => {
                                Some(eyeball_im::VectorDiff::PopFront)
                            }
                            eyeball_im::VectorDiff::Clear => Some(eyeball_im::VectorDiff::Clear),
                        };

                        if let Some(diff) = room_diff {
                            let _ = tx_rooms.send(Message::Matrix(matrix::MatrixEvent::RoomDiff(
                                Box::new(diff),
                            )));
                        }
                    }
                }
            });

            cosmic::iced::futures::stream::unfold(rx, |mut rx| async move {
                rx.recv().await.map(|msg| (msg, rx))
            })
        })
    }

    fn timeline_subscription(
        &self,
        matrix: &matrix::MatrixEngine,
        room_id: Arc<str>,
    ) -> Subscription<Message> {
        Subscription::run_with(
            (MatrixEngineWrapper(matrix.clone()), room_id.clone()),
            |(wrapper, room_id)| {
                let engine = wrapper.0.clone();
                let room_id = room_id.clone();
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

                tokio::spawn(async move {
                    let timeline = match engine.timeline(&room_id).await {
                        Ok(t) => t,
                        Err(_) => return,
                    };

                    let (items, mut stream) = timeline.subscribe().await;
                    let _ = tx.send(Message::Matrix(matrix::MatrixEvent::TimelineReset));

                    for (index, item) in items.into_iter().enumerate() {
                        let _ = tx.send(Message::Matrix(matrix::MatrixEvent::TimelineDiff(
                            eyeball_im::VectorDiff::Insert { index, value: item },
                        )));
                    }

                    use cosmic::iced::futures::StreamExt;
                    while let Some(diff) = stream.next().await {
                        for d in diff {
                            let _ = tx.send(Message::Matrix(matrix::MatrixEvent::TimelineDiff(d)));
                        }
                    }
                });

                cosmic::iced::futures::stream::unfold(rx, |mut rx| async move {
                    rx.recv().await.map(|msg| (msg, rx))
                })
            },
        )
    }

    fn threaded_timeline_subscription(
        &self,
        matrix: &matrix::MatrixEngine,
        room_id: Arc<str>,
        root_id: matrix_sdk::ruma::OwnedEventId,
    ) -> Subscription<Message> {
        Subscription::run_with(
            (
                MatrixEngineWrapper(matrix.clone()),
                room_id.clone(),
                root_id.clone(),
            ),
            |(wrapper, room_id, root_id)| {
                let engine = wrapper.0.clone();
                let room_id = room_id.clone();
                let root_id = root_id.clone();
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

                tokio::spawn(async move {
                    let timeline = match engine.threaded_timeline(&room_id, &root_id).await {
                        Ok(t) => t,
                        Err(_) => return,
                    };

                    let (items, mut stream) = timeline.subscribe().await;
                    let _ = tx.send(Message::MatrixThreadReset(root_id.clone()));

                    for (index, item) in items.into_iter().enumerate() {
                        let _ = tx.send(Message::MatrixThreadDiff(
                            root_id.clone(),
                            eyeball_im::VectorDiff::Insert { index, value: item },
                        ));
                    }

                    use cosmic::iced::futures::StreamExt;
                    while let Some(diff) = stream.next().await {
                        for d in diff {
                            let _ = tx.send(Message::MatrixThreadDiff(root_id.clone(), d));
                        }
                    }
                });

                cosmic::iced::futures::stream::unfold(rx, |mut rx| async move {
                    rx.recv().await.map(|msg| (msg, rx))
                })
            },
        )
    }
}

impl Application for Constellations {
    type Executor = cosmic::executor::Default;
    type Message = Message;
    type Flags = Option<String>;
    const APP_ID: &'static str = "fi.joonastuomi.CosmicExtConstellations";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn header_start(&self) -> Vec<Element<'_, Self::Message>> {
        let mut start = Vec::new();

        if self.is_search_active {
            let search_btn =
                button::icon(Named::new("edit-find-symbolic")).on_press(Message::ToggleSearch);
            let search_tooltip = tooltip(
                search_btn,
                text::body(crate::fl!("close-search")),
                Position::Bottom,
            );
            let row = Row::new()
                .align_y(Alignment::Center)
                .push(search_tooltip)
                .push(
                    text_input(crate::fl!("search-placeholder"), &self.search_query)
                        .on_input(Message::SearchQueryChanged)
                        .width(200.0),
                );
            start.push(row.into());
        } else {
            let search_btn =
                button::icon(Named::new("edit-find-symbolic")).on_press(Message::ToggleSearch);
            let search_tooltip = tooltip(
                search_btn,
                text::body(crate::fl!("search")),
                Position::Bottom,
            );
            start.push(search_tooltip.into());
        }

        start
    }

    fn header_end(&self) -> Vec<Element<'_, Self::Message>> {
        let mut end = Vec::new();

        if self.user_id.is_some() {
            let user_btn = button::icon(Named::new("user-available-symbolic"));
            let user_tooltip = tooltip(
                user_btn,
                text::body(crate::fl!("user-menu")),
                Position::Bottom,
            );
            let key_binds = std::collections::HashMap::new();

            let menu_tree = menu::Tree::with_children(
                RcElementWrapper::new(Element::from(user_tooltip)),
                menu::items(
                    &key_binds,
                    vec![
                        menu::Item::Button(
                            crate::fl!("app-settings"),
                            Some(cosmic::widget::icon::Handle::from(
                                cosmic::widget::icon::Named::new("applications-system"),
                            )),
                            MenuAct::AppSettings,
                        ),
                        menu::Item::Button(
                            crate::fl!("user-settings"),
                            Some(cosmic::widget::icon::Handle::from(
                                cosmic::widget::icon::Named::new("preferences-system-and-accounts"),
                            )),
                            MenuAct::UserSettings,
                        ),
                        menu::Item::Button(
                            crate::fl!("logout"),
                            Some(cosmic::widget::icon::Handle::from(
                                cosmic::widget::icon::Named::new("system-log-out"),
                            )),
                            MenuAct::Logout,
                        ),
                    ],
                ),
            );

            let user_menu = menu::bar(vec![menu_tree])
                .item_height(menu::ItemHeight::Dynamic(40))
                .item_width(menu::ItemWidth::Uniform(160))
                .spacing(4.0);

            end.push(user_menu.into());
        }

        end
    }

    fn init(core: Core, flags: Self::Flags) -> (Self, Task<Action<Self::Message>>) {
        let data_dir = dirs::data_dir().map(|d| d.join("fi.joonastuomi.Constellations"));

        let mut tasks = Vec::new();
        tasks.push(Task::perform(
            async move {
                let dir = data_dir.ok_or_else(|| {
                    matrix::SyncError::from(anyhow::anyhow!("No standard data directory found"))
                })?;
                matrix::MatrixEngine::new(dir)
                    .await
                    .map_err(matrix::SyncError::from)
            },
            |res| Action::from(Message::EngineReady(res)),
        ));

        if let Some(uri) = flags
            && let Ok(url) = Url::parse(&uri)
        {
            tasks.push(Task::done(Action::from(Message::OidcCallback(url))));
        }

        let config = settings::config::Config::load();

        let mut app = Constellations {
            core: core.clone(),
            matrix: None,
            sync_status: matrix::SyncStatus::Disconnected,
            room_list: Vec::new(),
            filtered_room_list: Vec::new(),
            other_rooms: Vec::new(),
            filtered_other_rooms: Vec::new(),
            selected_room: None,
            timeline_items: Vector::new(),
            composer_content: cosmic::widget::text_editor::Content::new(),
            composer_preview_events: Vec::new(),
            composer_is_preview: false,
            composer_attachments: Vec::new(),
            user_id: None,
            media_cache: HashMap::new(),
            creating_room: false,
            creating_space: false,
            new_room_name: String::new(),
            error: None,
            login_homeserver: "https://matrix.org".to_string(),
            login_username: String::new(),
            login_password: String::new(),
            is_logging_in: false,
            is_oidc_logging_in: false,
            is_registering_mode: false,
            is_registering: false,
            is_initializing: true,
            is_sync_indicator_active: false,
            is_loading_more: false,
            last_timeline_offset: 0.0,
            last_threaded_timeline_offset: 0.0,
            search_query: String::new(),
            is_search_active: false,
            active_reaction_picker: None,
            active_thread_root: None,
            threaded_timeline_items: Vector::new(),
            joined_room_ids: std::collections::HashSet::new(),
            replying_to: None,
            editing_item: None,
            selected_space: None,
            current_settings_panel: None,
            user_settings: settings::user::State::from_config(&config),
            room_settings: Default::default(),
            space_settings: Default::default(),
            app_settings: settings::app::State::from_config(&config),
            call_participants: HashMap::new(),
            fullscreen_image: None,
            emoji_search_query: String::new(),
            selected_emoji_group: None,
            is_composer_emoji_picker_active: false,
        };

        let title_task = app.update_title();
        tasks.push(title_task);

        (app, Task::batch(tasks))
    }

    fn context_drawer(
        &self,
    ) -> Option<cosmic::app::context_drawer::ContextDrawer<'_, Self::Message>> {
        if let Some(panel) = &self.current_settings_panel {
            let title = match panel {
                SettingsPanel::App => crate::fl!("app-settings"),
                SettingsPanel::User => crate::fl!("user-settings"),
                SettingsPanel::Room => crate::fl!("room-settings"),
                SettingsPanel::Space => crate::fl!("space-settings"),
            };

            let panel_content = match panel {
                SettingsPanel::User => self.user_settings.view().map(Message::UserSettings),
                SettingsPanel::Room => self.room_settings.view().map(Message::RoomSettings),
                SettingsPanel::Space => self.space_settings.view().map(Message::SpaceSettings),
                SettingsPanel::App => self.app_settings.view().map(Message::AppSettings),
            };

            Some(
                cosmic::app::context_drawer::context_drawer(panel_content, Message::CloseSettings)
                    .title(title.to_string()),
            )
        } else {
            None
        }
    }

    fn update(&mut self, message: Message) -> Task<Action<Self::Message>> {
        match message {
            Message::EngineReady(res) => self.handle_engine_ready(res),
            Message::UserReady(user_id, sync_res) => self.handle_user_ready(user_id, sync_res),

            Message::Matrix(event) => self.handle_matrix_event(event),
            Message::MatrixThreadDiff(root_id, diff) => {
                self.handle_timeline_diff(diff, true, Some(root_id))
            }
            Message::MatrixThreadReset(root_id) => {
                if self.active_thread_root.as_ref() == Some(&root_id) {
                    self.threaded_timeline_items.clear();
                }
                Task::none()
            }
            Message::OpenThread(root_id) => {
                self.active_thread_root = Some(root_id);
                self.threaded_timeline_items.clear();
                self.last_threaded_timeline_offset = 0.0;
                Task::batch(vec![
                    self.handle_load_more(true),
                    scrollable::snap_to(
                        THREADED_TIMELINE_ID.clone(),
                        scrollable::RelativeOffset::END.into(),
                    ),
                ])
            }
            Message::StartReply(item_id) => {
                let mut found_item = None;
                for item in self
                    .timeline_items
                    .iter()
                    .chain(self.threaded_timeline_items.iter())
                {
                    if let Some(event) = item.item.as_event()
                        && event.identifier() == item_id
                    {
                        found_item = Some(item.clone());
                        break;
                    }
                }
                self.replying_to = found_item;
                Task::none()
            }
            Message::CancelReply => {
                self.replying_to = None;
                Task::none()
            }
            Message::CloseThread => {
                self.active_thread_root = None;
                self.threaded_timeline_items.clear();
                Task::none()
            }
            Message::LoadMoreFinished(res) => {
                self.is_loading_more = false;
                if let Err(e) = res {
                    self.error = Some(format!("Failed to load more messages: {}", e));
                }
                Task::none()
            }
            Message::TimelineScrolled(viewport, is_thread) => {
                let current_offset = viewport.absolute_offset().y;
                let last_offset = if is_thread {
                    self.last_threaded_timeline_offset
                } else {
                    self.last_timeline_offset
                };

                let should_load = current_offset < 100.0 && current_offset < last_offset;

                if is_thread {
                    self.last_threaded_timeline_offset = current_offset;
                } else {
                    self.last_timeline_offset = current_offset;
                }

                if should_load {
                    self.handle_load_more(is_thread)
                } else {
                    Task::none()
                }
            }
            Message::RoomSelected(room_id) => {
                self.selected_room = Some(room_id.clone());
                self.timeline_items.clear();
                self.last_timeline_offset = 0.0;
                Task::batch(vec![
                    self.update_title(),
                    self.handle_load_more(false),
                    scrollable::snap_to(
                        TIMELINE_ID.clone(),
                        scrollable::RelativeOffset::END.into(),
                    ),
                ])
            }
            Message::ComposerChanged(text) => {
                self.composer_preview_events = parse_markdown(&text, false);
                self.composer_content = cosmic::widget::text_editor::Content::with_text(&text);

                if self.app_settings.send_typing_notifications
                    && let Some(matrix) = &self.matrix
                    && let Some(room_id) = &self.selected_room
                {
                    let matrix = matrix.clone();
                    let room_id = room_id.clone();
                    let typing = !self.composer_content.is_empty();
                    return Task::perform(
                        async move {
                            let _ = matrix.typing_notice(&room_id, typing).await;
                        },
                        |_| Action::from(Message::NoOp),
                    );
                }

                Task::none()
            }
            Message::ComposerAction(action) => {
                self.composer_content.perform(action);
                let text = self.composer_content.text();
                self.composer_preview_events = parse_markdown(&text, false);

                if self.app_settings.send_typing_notifications
                    && let Some(matrix) = &self.matrix
                    && let Some(room_id) = &self.selected_room
                {
                    let matrix = matrix.clone();
                    let room_id = room_id.clone();
                    let typing = !self.composer_content.is_empty();
                    return Task::perform(
                        async move {
                            let _ = matrix.typing_notice(&room_id, typing).await;
                        },
                        |_| Action::from(Message::NoOp),
                    );
                }

                Task::none()
            }
            Message::TogglePreview => {
                self.composer_is_preview = !self.composer_is_preview;
                Task::none()
            }
            Message::SendMessage => self.handle_send_message(),
            Message::ShareLocation => self.handle_share_location(),
            Message::LocationRetrieved(res) => self.handle_location_retrieved(res),
            Message::MessageSent(res) => {
                match res {
                    Ok(_) => {
                        self.composer_content = cosmic::widget::text_editor::Content::new();
                        self.composer_preview_events.clear();
                        self.composer_is_preview = false;
                        self.replying_to = None;
                        self.editing_item = None;
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to send message: {}", e));
                    }
                }
                Task::none()
            }
            Message::MessageEdited(res) => {
                match res {
                    Ok(_) => {
                        self.composer_content = cosmic::widget::text_editor::Content::new();
                        self.composer_preview_events.clear();
                        self.composer_is_preview = false;
                        self.editing_item = None;
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to edit message: {}", e));
                    }
                }
                Task::none()
            }
            Message::MessageRedacted(res) => {
                if let Err(e) = res {
                    self.error = Some(format!("Failed to redact message: {}", e));
                }
                Task::none()
            }
            Message::StartEdit(item_id) => {
                let mut found_item = None;
                for item in self
                    .timeline_items
                    .iter()
                    .chain(self.threaded_timeline_items.iter())
                {
                    if let Some(event) = item.item.as_event()
                        && event.identifier() == item_id
                    {
                        found_item = Some(item.clone());
                        break;
                    }
                }
                if let Some(item) = found_item
                    && let Some(event) = item.item.as_event()
                    && let Some(msg) = event.content().as_message()
                {
                    self.composer_content = cosmic::widget::text_editor::Content::with_text(msg.body());
                    self.composer_preview_events = parse_markdown(&self.composer_content.text(), false);
                    self.editing_item = Some(item);
                    self.replying_to = None;
                }
                Task::none()
            }
            Message::CancelEdit => {
                self.editing_item = None;
                self.composer_content = cosmic::widget::text_editor::Content::new();
                self.composer_preview_events.clear();
                Task::none()
            }
            Message::RedactMessage(item_id) => self.handle_redact_message(item_id),
            Message::AddAttachment => self.handle_add_attachment(),
            Message::AttachmentsSelected(paths) => {
                for path in paths {
                    if !self.composer_attachments.contains(&path) {
                        self.composer_attachments.push(path);
                    }
                }
                Task::none()
            }
            Message::RemoveAttachment(index) => {
                if index < self.composer_attachments.len() {
                    self.composer_attachments.remove(index);
                }
                Task::none()
            }
            Message::AttachmentSent(path, res) => {
                match res {
                    Ok(_) => {
                        // Successfully sent, could remove from ui if we were tracking it per-message
                    }
                    Err(e) => {
                        self.error = Some(format!(
                            "Failed to send attachment {}: {}",
                            path.display(),
                            e
                        ));
                    }
                }
                Task::none()
            }
            Message::OpenReactionPicker(item_id) => {
                self.active_reaction_picker = item_id;
                if self.active_reaction_picker.is_some() {
                    self.is_composer_emoji_picker_active = false;
                }
                self.emoji_search_query.clear();
                self.selected_emoji_group = Some(emojis::Group::SmileysAndEmotion);
                Task::none()
            }
            Message::EmojiSearchQueryChanged(query) => {
                self.emoji_search_query = query;
                Task::none()
            }
            Message::SelectEmojiGroup(group) => {
                self.selected_emoji_group = group;
                Task::none()
            }
            Message::ToggleComposerEmojiPicker => {
                self.is_composer_emoji_picker_active = !self.is_composer_emoji_picker_active;
                if self.is_composer_emoji_picker_active {
                    self.emoji_search_query.clear();
                    self.selected_emoji_group = Some(emojis::Group::SmileysAndEmotion);
                    self.active_reaction_picker = None;
                }
                Task::none()
            }
            Message::EmojiPickerSelected(emoji) => {
                if let Some(item_id) = self.active_reaction_picker.clone() {
                    self.update(Message::ToggleReaction(item_id, emoji.to_string()))
                } else {
                    self.update(Message::InsertEmoji(emoji.to_string()))
                }
            }
            Message::InsertEmoji(emoji) => {
                let mut text = self.composer_content.text();
                text.push_str(&emoji);
                self.composer_content = cosmic::widget::text_editor::Content::with_text(&text);
                self.composer_preview_events = parse_markdown(&text, false);

                if self.app_settings.send_typing_notifications
                    && let Some(matrix) = &self.matrix
                    && let Some(room_id) = &self.selected_room
                {
                    let matrix = matrix.clone();
                    let room_id = room_id.clone();
                    let typing = !self.composer_content.is_empty();
                    return Task::perform(
                        async move {
                            let _ = matrix.typing_notice(&room_id, typing).await;
                        },
                        |_| Action::from(Message::NoOp),
                    );
                }

                Task::none()
            }
            Message::ToggleReaction(item_id, key) => {
                self.active_reaction_picker = None;
                if let (Some(matrix), Some(room_id)) = (&self.matrix, &self.selected_room) {
                    let matrix_clone = matrix.clone();
                    let room_id_clone = room_id.clone();
                    return Task::perform(
                        async move {
                            matrix_clone
                                .toggle_reaction(&room_id_clone, &item_id, &key)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| Message::ReactionToggled(res).into(),
                    );
                }
                Task::none()
            }
            Message::ReactionToggled(res) => {
                if let Err(e) = res {
                    self.error = Some(format!("Failed to toggle reaction: {}", e));
                }
                Task::none()
            }
            Message::FetchMedia(source) => self.handle_fetch_media(source),
            Message::MediaFetched(mxc_url, res) => self.handle_media_fetched(mxc_url, res),
            Message::MediaFetchedBatch(batch) => self.handle_media_fetched_batch(batch),
            Message::DismissError => {
                self.error = None;
                Task::none()
            }
            Message::ToggleCreateRoom => {
                self.creating_room = !self.creating_room;
                self.creating_space = false;
                self.new_room_name.clear();
                Task::none()
            }
            Message::ToggleCreateSpace => {
                self.creating_space = !self.creating_space;
                self.creating_room = false;
                self.new_room_name.clear();
                Task::none()
            }
            Message::NewRoomNameChanged(name) => {
                self.new_room_name = name;
                Task::none()
            }
            Message::CreateRoom(name) => self.handle_create_room(name),
            Message::RoomCreated(res) => {
                match res {
                    Ok(room_id) => {
                        self.creating_room = false;
                        self.new_room_name.clear();
                        self.selected_room = Some(room_id.as_str().into());
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to create room: {}", e));
                    }
                }
                Task::none()
            }
            Message::CreateSpace(name) => self.handle_create_space(name),
            Message::SpaceCreated(res) => {
                match res {
                    Ok(space_id) => {
                        self.creating_space = false;
                        self.new_room_name.clear();
                        if let Ok(rid) = space_id.as_str().try_into() {
                            return self.handle_select_space(Some(rid));
                        }
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to create space: {}", e));
                    }
                }
                Task::none()
            }
            Message::LoginHomeserverChanged(homeserver) => {
                self.login_homeserver = homeserver;
                Task::none()
            }
            Message::LoginUsernameChanged(username) => {
                self.login_username = username;
                Task::none()
            }
            Message::LoginPasswordChanged(password) => {
                self.login_password = password;
                Task::none()
            }
            Message::SubmitLogin => self.handle_submit_login(),
            Message::LoginFinished(res) => self.handle_login_finished(res),
            Message::ToggleLoginMode => self.handle_toggle_login_mode(),
            Message::SubmitRegister => self.handle_submit_register(),
            Message::RegisterFinished(res) => self.handle_register_finished(res),
            Message::SelectSpace(space_id) => {
                let parsed_id = space_id.and_then(|id| matrix_sdk::ruma::RoomId::parse(&*id).ok());
                self.handle_select_space(parsed_id)
            }
            Message::SpaceChildrenFetched(space_id, res) => {
                self.handle_space_children_fetched(space_id, res)
            }
            Message::SpaceFilterUpdated => {
                self.update_filtered_rooms();
                Task::none()
            }
            Message::NoOp => Task::none(),
            Message::SubmitOidcLogin => self.handle_submit_oidc_login(),
            Message::OidcLoginStarted(res) => self.handle_oidc_login_started(res),
            Message::OidcCallback(url) => self.handle_oidc_callback(url),
            Message::JoinRoom(room_id) => {
                if let Some(matrix) = &self.matrix {
                    let matrix = matrix.clone();
                    return Task::perform(
                        async move {
                            let rid = matrix_sdk::ruma::RoomId::parse(&*room_id)
                                .map_err(|e| e.to_string())?;
                            matrix
                                .join_room(&rid)
                                .await
                                .map(|_| rid)
                                .map_err(|e| e.to_string())
                        },
                        |res| Message::RoomJoined(res).into(),
                    );
                }
                Task::none()
            }
            Message::RoomJoined(res) => {
                match res {
                    Ok(room_id) => {
                        self.selected_room = Some(room_id.as_str().into());
                        // Refresh both lists
                        self.update_filtered_rooms();
                        if let (Some(matrix), Some(space_id)) = (&self.matrix, &self.selected_space)
                        {
                            let matrix = matrix.clone();
                            let sid = space_id.clone();
                            let sid_clone = sid.clone();
                            return Task::perform(
                                async move {
                                    matrix
                                        .get_space_children(sid_clone.as_str())
                                        .await
                                        .map_err(|e| e.to_string())
                                },
                                move |res| Message::SpaceChildrenFetched(sid, res).into(),
                            );
                        }
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to join room: {}", e));
                    }
                }
                Task::none()
            }
            Message::Logout => self.handle_logout(),
            Message::LogoutFinished => self.handle_logout_finished(),
            Message::OpenSettings(panel) => {
                self.current_settings_panel = Some(panel.clone());
                self.core.set_show_context(true);

                if self.is_search_active {
                    match panel {
                        SettingsPanel::Room => {
                            self.room_settings.member_filter = self.search_query.clone();
                        }
                        SettingsPanel::Space => {
                            self.space_settings.child_filter = self.search_query.clone();
                        }
                        _ => {}
                    }
                }

                if panel == SettingsPanel::User {
                    return self
                        .user_settings
                        .update(settings::user::Message::LoadProfile, &self.matrix);
                } else if panel == SettingsPanel::Room {
                    if let Some(room_id) = &self.selected_room {
                        return self.room_settings.update(
                            settings::room::Message::LoadRoom(room_id.clone()),
                            &self.matrix,
                        );
                    }
                } else if panel == SettingsPanel::Space
                    && let Some(space_id) = &self.selected_space
                {
                    return self.space_settings.update(
                        settings::space::Message::LoadSpace(Arc::from(space_id.as_str())),
                        &self.matrix,
                    );
                }
                Task::none()
            }
            Message::CloseSettings => {
                self.current_settings_panel = None;
                self.core.set_show_context(false);
                Task::none()
            }
            Message::UserSettings(msg) => self.user_settings.update(msg, &self.matrix),
            Message::RoomSettings(msg) => self.room_settings.update(msg, &self.matrix),
            Message::SpaceSettings(msg) => self.space_settings.update(msg, &self.matrix),
            Message::AppSettings(msg) => match msg {
                settings::app::Message::ClearCache => {
                    self.media_cache.clear();
                    Task::none()
                }
                _ => self.app_settings.update(msg),
            },
            Message::AppSettingChanged => {
                let config = settings::config::Config {
                    show_sync_indicator: self.app_settings.show_sync_indicator,
                    send_typing_notifications: self.app_settings.send_typing_notifications,
                    render_markdown: self.app_settings.render_markdown,
                    compact_mode: self.app_settings.compact_mode,
                    hide_threaded_messages: self.app_settings.hide_threaded_messages,
                    media_previews_display_policy: self.user_settings.media_previews_display_policy,
                    invite_avatars_display_policy: self.user_settings.invite_avatars_display_policy,
                };
                let save_task = Task::perform(async move { config.save() }, |_| {
                    Action::from(Message::NoOp)
                });
                let fetch_task = self.fetch_missing_media();
                Task::batch(vec![save_task, fetch_task])
            }
            Message::ToggleSearch => {
                self.is_search_active = !self.is_search_active;
                if !self.is_search_active {
                    self.search_query.clear();
                    self.room_settings.member_filter.clear();
                    self.space_settings.child_filter.clear();
                } else if let Some(panel) = &self.current_settings_panel {
                    match panel {
                        SettingsPanel::Room => {
                            self.search_query = self.room_settings.member_filter.clone();
                        }
                        SettingsPanel::Space => {
                            self.search_query = self.space_settings.child_filter.clone();
                        }
                        _ => {}
                    }
                }
                self.update_filtered_rooms();
                Task::none()
            }
            Message::SearchQueryChanged(query) => {
                self.search_query = query.clone();
                if let Some(panel) = &self.current_settings_panel {
                    match panel {
                        SettingsPanel::Room => {
                            self.room_settings.member_filter = query;
                        }
                        SettingsPanel::Space => {
                            self.space_settings.child_filter = query;
                        }
                        _ => {}
                    }
                }
                self.update_filtered_rooms();
                Task::none()
            }
            Message::JoinCall => self.handle_join_call(),
            Message::LeaveCall => self.handle_leave_call(),
            Message::CallJoined(res) => {
                if let Err(e) = res {
                    self.error = Some(format!("Failed to join call: {}", e));
                }
                Task::none()
            }
            Message::CallLeft(res) => {
                if let Err(e) = res {
                    self.error = Some(format!("Failed to leave call: {}", e));
                }
                Task::none()
            }
            Message::OpenUrl(url) => Task::perform(
                async move {
                    let _ = open::that(url);
                },
                |_| Action::from(Message::NoOp),
            ),
            Message::OpenImage(handle) => {
                self.fullscreen_image = Some(handle);
                Task::none()
            }
            Message::CloseImage => {
                self.fullscreen_image = None;
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        if self.is_initializing {
            let content = Column::new()
                .push(
                    cosmic::widget::svg(cosmic::widget::svg::Handle::from_memory(
                        CONSTELLATIONS_ICON,
                    ))
                    .width(cosmic::iced::Length::Fixed(128.0))
                    .height(cosmic::iced::Length::Fixed(128.0)),
                )
                .push(cosmic::widget::progress_bar::indeterminate_circular())
                .spacing(32)
                .align_x(Alignment::Center);

            return container(content)
                .width(cosmic::iced::Length::Fill)
                .height(cosmic::iced::Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .into();
        }

        if self.user_id.is_none() {
            return self.view_login();
        }

        let status_text = match &self.sync_status {
            matrix::SyncStatus::Disconnected => "Disconnected".to_string(),
            matrix::SyncStatus::Syncing => "Syncing...".to_string(),
            matrix::SyncStatus::Connected => "Connected".to_string(),
            matrix::SyncStatus::Error(e) => format!("⚠️ Sync Error: {}", e),
            matrix::SyncStatus::MissingSlidingSyncSupport => "Error: Your homeserver does not support Sliding Sync (MSC4186), which is required by Constellations.".to_string(),
        };

        let sidebar = self.view_sidebar();
        let content = self.view_main_content(status_text);

        let main_view = Row::new()
            .push(self.view_space_switcher())
            .push(sidebar)
            .push(content);

        let mut final_view: Element<'_, Message> = main_view.into();
        if self.app_settings.show_sync_indicator && self.is_sync_indicator_active {
            let sync_widget: Element<'_, Message> = match self.sync_status {
                matrix::SyncStatus::Syncing => {
                    container(cosmic::widget::progress_bar::indeterminate_circular().size(24.0))
                        .into()
                }
                matrix::SyncStatus::Connected => {
                    container(cosmic::widget::icon::from_name("network-idle-symbolic").size(24))
                        .into()
                }
                matrix::SyncStatus::Disconnected => {
                    container(cosmic::widget::icon::from_name("network-offline-symbolic").size(24))
                        .into()
                }
                matrix::SyncStatus::Error(_) | matrix::SyncStatus::MissingSlidingSyncSupport => {
                    container(cosmic::widget::icon::from_name("network-error-symbolic").size(24))
                        .into()
                }
            };

            let sync_overlay = container(sync_widget)
                .padding(20)
                .width(cosmic::iced::Length::Fill)
                .height(cosmic::iced::Length::Fill)
                .align_x(Alignment::End)
                .align_y(Alignment::End);

            final_view = cosmic::iced::widget::stack![final_view, sync_overlay].into();
        }

        if let Some(handle) = &self.fullscreen_image {
            let image: image::Image<'_> = cosmic::widget::image(handle.clone())
                .width(cosmic::iced::Length::Fill)
                .height(cosmic::iced::Length::Fill)
                .content_fit(cosmic::iced::ContentFit::Contain);
            let image_viewer = container(image)
                .width(cosmic::iced::Length::Fill)
                .height(cosmic::iced::Length::Fill)
                .padding(40)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center);

            let close_button = container(
                button::icon(cosmic::widget::icon::from_name("window-close-symbolic"))
                    .on_press(Message::CloseImage),
            )
            .width(cosmic::iced::Length::Fill)
            .height(cosmic::iced::Length::Fill)
            .padding(10)
            .align_right(image_viewer.size_hint().width)
            .align_top(image_viewer.size_hint().height);

            // Overlay that closes on click
            let dismiss_overlay = button::custom(
                container(cosmic::iced::widget::Space::new())
                    .width(cosmic::iced::Length::Fill)
                    .height(cosmic::iced::Length::Fill),
            )
            .on_press(Message::CloseImage)
            .padding(0);

            final_view = cosmic::iced::widget::stack![
                final_view,
                dismiss_overlay,
                image_viewer,
                close_button
            ]
            .into();
        }

        final_view
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let ipc_sub = self.ipc_subscription();

        let matrix = match &self.matrix {
            Some(m) => m,
            None => return ipc_sub,
        };

        let sync_sub = self.sync_subscription(matrix);

        let mut subs = vec![ipc_sub, sync_sub];

        if let Some(room_id) = self.selected_room.clone() {
            subs.push(self.timeline_subscription(matrix, room_id));
        }

        if let (Some(room_id), Some(root_id)) =
            (self.selected_room.clone(), self.active_thread_root.clone())
        {
            subs.push(self.threaded_timeline_subscription(matrix, room_id, root_id));
        }

        Subscription::batch(subs)
    }
}

async fn get_room_data(
    engine: &matrix::MatrixEngine,
    room_id: &matrix_sdk::ruma::RoomId,
) -> Option<matrix::RoomData> {
    let client = engine.client().await;
    let room = client.get_room(room_id)?;

    engine.fetch_room_data(&room).await.ok()
}

fn redact_url(url: &Url) -> String {
    let mut redacted = url.clone();
    let pairs: Vec<(String, String)> = redacted
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    redacted.set_query(None);
    for (k, mut v) in pairs {
        if k == "code" || k == "state" {
            v = "[REDACTED]".to_string();
        }
        redacted.query_pairs_mut().append_pair(&k, &v);
    }
    redacted.to_string()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    LazyLock::force(&i18n::LOAD_LOCALIZATION);

    tracing_subscriber::fmt()
        .with_env_filter("matrix_sdk=debug,matrix_sdk_ui=debug,cosmic_ext_constellations=debug")
        .with_writer(std::io::stderr)
        .init();
    let args: Vec<String> = std::env::args().collect();
    let uri = args
        .get(1)
        .filter(|u| u.starts_with("fi.joonastuomi.CosmicExtConstellations://"))
        .cloned();

    let rt = tokio::runtime::Runtime::new()?;
    let is_running = rt.block_on(async {
        let connection = match zbus::Connection::session().await {
            Ok(conn) => conn,
            Err(_) => return false,
        };
        let dbus = match zbus::fdo::DBusProxy::new(&connection).await {
            Ok(proxy) => proxy,
            Err(_) => return false,
        };
        dbus.name_has_owner(ipc::DBUS_NAME.try_into().unwrap())
            .await
            .unwrap_or(false)
    });

    if is_running {
        if let Some(uri) = uri {
            rt.block_on(async {
                if let Err(e) = ipc::call_handle_callback(uri).await {
                    tracing::error!("Failed to send URI to existing instance: {}", e);
                }
            });
        }
        tracing::info!("Another instance is already running, exiting.");
        return Ok(());
    }

    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();

    cosmic::app::run::<Constellations>(cosmic::app::Settings::default(), uri)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_app() -> Constellations {
        Constellations {
            core: cosmic::app::Core::default(),
            matrix: None,
            sync_status: matrix::SyncStatus::Disconnected,
            room_list: Vec::new(),
            filtered_room_list: Vec::new(),
            other_rooms: Vec::new(),
            filtered_other_rooms: Vec::new(),
            selected_room: None,
            timeline_items: eyeball_im::Vector::new(),
            composer_content: cosmic::widget::text_editor::Content::new(),
            composer_preview_events: Vec::new(),
            composer_is_preview: false,
            composer_attachments: Vec::new(),
            user_id: None,
            media_cache: std::collections::HashMap::new(),
            creating_room: false,
            creating_space: false,
            new_room_name: String::new(),
            error: None,
            login_homeserver: String::new(),
            login_username: String::new(),
            login_password: String::new(),
            is_logging_in: false,
            is_oidc_logging_in: false,
            is_registering_mode: false,
            is_registering: false,
            is_initializing: false,
            is_sync_indicator_active: false,
            search_query: String::new(),
            is_search_active: false,
            active_reaction_picker: None,
            joined_room_ids: std::collections::HashSet::new(),
            selected_space: None,
            current_settings_panel: None,
            user_settings: settings::user::State::default(),
            room_settings: settings::room::State::default(),
            space_settings: settings::space::State::default(),
            app_settings: settings::app::State::default(),
            active_thread_root: None,
            threaded_timeline_items: eyeball_im::Vector::new(),
            is_loading_more: false,
            replying_to: None,
            editing_item: None,
            call_participants: HashMap::new(),
            last_timeline_offset: Default::default(),
            last_threaded_timeline_offset: Default::default(),
            fullscreen_image: None,
            emoji_search_query: String::new(),
            selected_emoji_group: None,
            is_composer_emoji_picker_active: false,
        }
    }

    #[test]
    fn test_update_filtered_rooms_no_search_no_space() {
        let mut app = create_test_app();
        app.room_list = vec![
            matrix::RoomData {
                id: std::sync::Arc::from("!room1:matrix.org"),
                name: Some("Room 1".to_string()),
                last_message: None,
                unread_count: 0,
                unread_count_str: None,
                avatar_url: None,
                room_type: None,
                is_space: false,
                parent_space_id: None,
                order: None,
                join_rule: None,
                allowed_spaces: Vec::new(),
                suggested: false,
            },
            matrix::RoomData {
                id: std::sync::Arc::from("!space1:matrix.org"),
                name: Some("Space 1".to_string()),
                last_message: None,
                unread_count: 0,
                unread_count_str: None,
                avatar_url: None,
                room_type: None,
                is_space: true,
                parent_space_id: None,
                order: None,
                join_rule: None,
                allowed_spaces: Vec::new(),
                suggested: false,
            },
        ];

        app.update_filtered_rooms();

        assert_eq!(app.filtered_room_list.len(), 1);
        assert_eq!(
            app.room_list[app.filtered_room_list[0]].id.as_ref(),
            "!room1:matrix.org"
        );
    }

    #[test]
    fn test_update_filtered_rooms_search_by_name() {
        let mut app = create_test_app();
        app.room_list = vec![
            matrix::RoomData {
                id: std::sync::Arc::from("!room1:matrix.org"),
                name: Some("Alpha Room".to_string()),
                last_message: None,
                unread_count: 0,
                unread_count_str: None,
                avatar_url: None,
                room_type: None,
                is_space: false,
                parent_space_id: None,
                order: None,
                join_rule: None,
                allowed_spaces: Vec::new(),
                suggested: false,
            },
            matrix::RoomData {
                id: std::sync::Arc::from("!room2:matrix.org"),
                name: Some("Beta Room".to_string()),
                last_message: None,
                unread_count: 0,
                unread_count_str: None,
                avatar_url: None,
                room_type: None,
                is_space: false,
                parent_space_id: None,
                order: None,
                join_rule: None,
                allowed_spaces: Vec::new(),
                suggested: false,
            },
        ];

        app.search_query = "alpha".to_string();
        app.update_filtered_rooms();

        assert_eq!(app.filtered_room_list.len(), 1);
        assert_eq!(
            app.room_list[app.filtered_room_list[0]].id.as_ref(),
            "!room1:matrix.org"
        );
    }

    #[test]
    fn test_update_filtered_rooms_search_by_id() {
        let mut app = create_test_app();
        app.room_list = vec![
            matrix::RoomData {
                id: std::sync::Arc::from("!room1:matrix.org"),
                name: Some("Alpha Room".to_string()),
                last_message: None,
                unread_count: 0,
                unread_count_str: None,
                avatar_url: None,
                room_type: None,
                is_space: false,
                parent_space_id: None,
                order: None,
                join_rule: None,
                allowed_spaces: Vec::new(),
                suggested: false,
            },
            matrix::RoomData {
                id: std::sync::Arc::from("!room2:matrix.org"),
                name: Some("Beta Room".to_string()),
                last_message: None,
                unread_count: 0,
                unread_count_str: None,
                avatar_url: None,
                room_type: None,
                is_space: false,
                parent_space_id: None,
                order: None,
                join_rule: None,
                allowed_spaces: Vec::new(),
                suggested: false,
            },
        ];

        app.search_query = "!ROOM2".to_string();
        app.update_filtered_rooms();

        assert_eq!(app.filtered_room_list.len(), 1);
        assert_eq!(
            app.room_list[app.filtered_room_list[0]].id.as_ref(),
            "!room2:matrix.org"
        );
    }

    #[test]
    fn test_update_filtered_rooms_search_no_match() {
        let mut app = create_test_app();
        app.room_list = vec![matrix::RoomData {
            id: std::sync::Arc::from("!room1:matrix.org"),
            name: Some("Alpha Room".to_string()),
            last_message: None,
            unread_count: 0,
            unread_count_str: None,
            avatar_url: None,
            room_type: None,
            is_space: false,
            parent_space_id: None,
            order: None,
            join_rule: None,
            allowed_spaces: Vec::new(),
            suggested: false,
        }];

        app.search_query = "gamma".to_string();
        app.update_filtered_rooms();

        assert_eq!(app.filtered_room_list.len(), 0);
    }

    #[test]
    fn test_update_filtered_rooms_with_selected_space_no_matrix() {
        let mut app = create_test_app();
        app.room_list = vec![matrix::RoomData {
            id: std::sync::Arc::from("!room1:matrix.org"),
            name: Some("Alpha Room".to_string()),
            last_message: None,
            unread_count: 0,
            unread_count_str: None,
            avatar_url: None,
            room_type: None,
            is_space: false,
            parent_space_id: None,
            order: None,
            join_rule: None,
            allowed_spaces: Vec::new(),
            suggested: false,
        }];

        app.selected_space = Some(matrix_sdk::ruma::RoomId::parse("!space1:matrix.org").unwrap());
        // matrix is None by default in create_test_app()

        app.update_filtered_rooms();

        // Since matrix is None, it won't populate rooms based on space hierarchy
        assert_eq!(app.filtered_room_list.len(), 0);
    }

    #[test]
    fn test_parse_markdown_paragraph() {
        let text = "This is a simple paragraph.";
        let events = parse_markdown(text, false);
        assert_eq!(
            events,
            vec![
                PreviewEvent::Text("This is a simple paragraph.".to_string()),
                PreviewEvent::EndBlock
            ]
        );
    }

    #[test]
    fn test_parse_markdown_heading() {
        let text = "# Heading 1\nSome text.";
        let events = parse_markdown(text, false);
        assert_eq!(
            events,
            vec![
                PreviewEvent::StartHeading,
                PreviewEvent::Text("Heading 1".to_string()),
                PreviewEvent::EndBlock,
                PreviewEvent::Text("Some text.".to_string()),
                PreviewEvent::EndBlock,
            ]
        );
    }

    #[test]
    fn test_parse_markdown_code() {
        let text = "Here is `some code` inline.";
        let events = parse_markdown(text, false);
        assert_eq!(
            events,
            vec![
                PreviewEvent::Text("Here is ".to_string()),
                PreviewEvent::Code("some code".to_string()),
                PreviewEvent::Text(" inline.".to_string()),
                PreviewEvent::EndBlock,
            ]
        );
    }

    #[test]
    fn test_parse_markdown_breaks() {
        let text = "Line 1\nLine 2  \nLine 3";
        let events = parse_markdown(text, false);
        assert_eq!(
            events,
            vec![
                PreviewEvent::Text("Line 1".to_string()),
                PreviewEvent::Break,
                PreviewEvent::Text("Line 2".to_string()),
                PreviewEvent::Break,
                PreviewEvent::Text("Line 3".to_string()),
                PreviewEvent::EndBlock,
            ]
        );
    }

    #[test]
    fn test_parse_markdown_ignored_formatting() {
        // Italics and bold should just emit Text events without wrapping them in special formatting events.
        let text = "Some **bold** and *italic* text.";
        let events = parse_markdown(text, false);
        assert_eq!(
            events,
            vec![
                PreviewEvent::Text("Some ".to_string()),
                PreviewEvent::Text("bold".to_string()),
                PreviewEvent::Text(" and ".to_string()),
                PreviewEvent::Text("italic".to_string()),
                PreviewEvent::Text(" text.".to_string()),
                PreviewEvent::EndBlock,
            ]
        );
    }

    #[test]
    fn test_parse_markdown_skip_fallback() {
        let text = "> <@alice:example.com> Hello\n\nHi!";
        let events = parse_markdown(text, true);
        assert_eq!(
            events,
            vec![
                PreviewEvent::Text("Hi!".to_string()),
                PreviewEvent::EndBlock
            ]
        );
    }

    #[test]
    fn test_parse_markdown_no_skip_normal_blockquote() {
        let text = "> This is a quote\n\nAnd this is text.";
        let events = parse_markdown(text, false);
        // Currently we don't have a BlockQuote event, so it just returns the text inside it if not skipped.
        // Wait, I should check what it actually returns.
        // Based on my logic: in_blockquote > 0 but skip_first_blockquote is false.
        // Match event -> pulldown_cmark::Event::Text -> events.push(...)
        assert_eq!(
            events,
            vec![
                PreviewEvent::Text("This is a quote".to_string()),
                PreviewEvent::EndBlock,
                PreviewEvent::Text("And this is text.".to_string()),
                PreviewEvent::EndBlock,
            ]
        );
    }

    #[tokio::test]
    async fn test_get_room_data_not_found() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let engine = match matrix::MatrixEngine::new(tmp_dir.path().to_path_buf()).await {
            Ok(e) => e,
            Err(e) => {
                tracing::info!(
                    "Skipping test due to engine initialization failure (likely dbus/keyring): {}",
                    e
                );
                return;
            }
        };

        let room_id = matrix_sdk::ruma::RoomId::parse("!nonexistent:example.com").unwrap();

        let result = get_room_data(&engine, &room_id).await;

        assert!(result.is_none());
    }

    #[test]
    fn test_update_room_joined_error() {
        let mut app = create_test_app();
        let _ = app.update(Message::RoomJoined(
            Err("some connection error".to_string()),
        ));

        assert_eq!(
            app.error,
            Some("Failed to join room: some connection error".to_string())
        );
    }
}
