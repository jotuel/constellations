#![recursion_limit = "256"]

pub mod item;
pub mod preview;
pub mod utils;

mod handlers;
pub mod i18n;
mod ipc;
mod matrix;
pub mod rich_text;
pub mod settings;
mod view;

pub use item::ConstellationsItem;
pub use preview::{PreviewEvent, parse_markdown};
pub use utils::{
    ApplyVectorDiffExt, contains_ignore_ascii_case, fuzzy_match_ignore_case, redact_url,
};

use anyhow::Result;
use cosmic::iced::widget::image;
use cosmic::iced::widget::tooltip;
use cosmic::iced::{Alignment, Subscription};
use cosmic::widget::icon::Named;
use cosmic::widget::menu::action::MenuAction;
use cosmic::widget::tooltip::Position;
use cosmic::widget::{RcElementWrapper, Row, button, menu, text, text_input};
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
pub const CONSTELLATIONS_ICON: &[u8] = include_bytes!("../res/const.svg");

pub static TIMELINE_ID: LazyLock<cosmic::iced::widget::Id> =
    LazyLock::new(cosmic::iced::widget::Id::unique);
pub static THREADED_TIMELINE_ID: LazyLock<cosmic::iced::widget::Id> =
    LazyLock::new(cosmic::iced::widget::Id::unique);

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

pub struct Constellations {
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
    is_qr_logging_in: bool,
    qr_login_step: QrLoginStep,
    qr_rendezvous_url: Option<String>,
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
    room_name_cache: std::collections::HashMap<std::sync::Arc<str>, String>,
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
    StartQrLogin,
    CancelQrLogin,
    SimulateQrScan,
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

impl Constellations {
    pub fn set_error(&mut self, error: String) {
        tracing::error!("Error occurred: {}", error);
        let _ = notify_rust::Notification::new()
            .appname("Constellations")
            .summary("Constellations Error")
            .body(&error)
            .icon("dialog-error")
            .show();
        self.error = Some(error);
    }

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
            is_qr_logging_in: false,
            qr_login_step: QrLoginStep::NotStarted,
            qr_rendezvous_url: None,
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
            room_name_cache: std::collections::HashMap::new(),
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
        self.handle_update(message)
    }

    fn view(&self) -> Element<'_, Message> {
        self.view_app()
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
            is_qr_logging_in: false,
            qr_login_step: QrLoginStep::NotStarted,
            qr_rendezvous_url: None,
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
            room_name_cache: std::collections::HashMap::new(),
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

        app.update_filtered_rooms();

        assert_eq!(app.filtered_room_list.len(), 0);
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

    #[test]
    fn test_room_name_cache() {
        let mut app = create_test_app();
        let room_id: std::sync::Arc<str> = std::sync::Arc::from("!room1:matrix.org");

        assert_eq!(app.get_room_name(&room_id), None);

        app.room_name_cache
            .insert(room_id.clone(), "Cached Room Name".to_string());
        assert_eq!(app.get_room_name(&room_id), Some("Cached Room Name"));

        app.room_list = vec![matrix::RoomData {
            id: room_id.clone(),
            name: Some("Active Room Name".to_string()),
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
        assert_eq!(app.get_room_name(&room_id), Some("Active Room Name"));
    }
}
