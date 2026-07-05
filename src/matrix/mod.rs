use anyhow::{Context, Result};
use eyeball_im::VectorDiff;
use matrix_sdk::authentication::matrix::MatrixSession;
use matrix_sdk::media::MediaFormat;
pub use matrix_sdk::room::edit::EditedContent;
use matrix_sdk::room::power_levels::RoomPowerLevelChanges;
use matrix_sdk::ruma::events::SyncStateEvent;
use matrix_sdk::ruma::events::ignored_user_list::IgnoredUserListEventContent;
use matrix_sdk::ruma::events::room::MediaSource;
use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
use matrix_sdk::ruma::events::space::child::SpaceChildEventContent;
use matrix_sdk::ruma::events::space::parent::SpaceParentEventContent;
use matrix_sdk::{
    Client, Room, SessionChange, SessionTokens,
    ruma::{OwnedDeviceId, OwnedRoomId, RoomId, UserId, room::RoomType},
};
pub use matrix_sdk_ui::room_list_service::{RoomListDynamicEntriesController, RoomListService};
use matrix_sdk_ui::sync_service::SyncService;
use matrix_sdk_ui::timeline::{LatestEventValue, MsgLikeKind, TimelineItemContent};
pub use matrix_sdk_ui::timeline::{
    RoomExt, Timeline, TimelineEventFocusThreadMode, TimelineEventItemId, TimelineFocus,
    TimelineItem, VirtualTimelineItem,
};
use oo7::Keyring;
use rand::{TryRng, rngs::SysRng};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{error, info};
use url::Url;

use livekit::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LiveKitWellKnown {
    #[serde(rename = "org.matrix.msc4143.rtc_foci")]
    #[serde(default)]
    rtc_foci: Vec<LiveKitRtcFocus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LiveKitRtcFocus {
    #[serde(rename = "type")]
    focus_type: String,
    livekit_service_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LiveKitAuthResponse {
    #[serde(alias = "url", default)]
    livekit_url: Option<String>,
    #[serde(alias = "jwt")]
    token: String,
}

const OIDC_CALLBACK_URL: &str = "fi.joonastuomi.Constellations://callback";
const OIDC_CLIENT_ID: &str = "fi.joonastuomi.Constellations";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncStatus {
    Disconnected,
    Syncing,
    Connected,
    Error(String),
    MissingSlidingSyncSupport,
}

#[derive(Error, Debug, Clone)]
pub enum SyncError {
    #[error("Sliding Sync (MSC4186) is not supported by the homeserver")]
    MissingSlidingSyncSupport,
    #[error("Matrix error: {0}")]
    Matrix(String),
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("Error: {0}")]
    Anyhow(String),
    #[error("{0}")]
    Generic(String),
}

impl From<matrix_sdk::Error> for SyncError {
    fn from(e: matrix_sdk::Error) -> Self {
        Self::Matrix(e.to_string())
    }
}

impl From<matrix_sdk::HttpError> for SyncError {
    fn from(e: matrix_sdk::HttpError) -> Self {
        Self::Http(e.to_string())
    }
}

impl From<anyhow::Error> for SyncError {
    fn from(e: anyhow::Error) -> Self {
        Self::Anyhow(e.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicRoom {
    pub id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub canonical_alias: Option<String>,
    pub num_joined_members: u32,
    pub avatar_url: Option<String>,
    pub is_space: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomData {
    pub id: std::sync::Arc<str>,
    pub name: Option<String>,
    pub last_message: Option<String>,
    pub unread_count: u32,
    pub unread_count_str: Option<String>,
    pub avatar_url: Option<String>,
    pub room_type: Option<RoomType>,
    pub is_space: bool,
    pub parent_space_id: Option<String>,
    pub join_rule: Option<matrix_sdk::ruma::events::room::join_rules::JoinRule>,
    pub allowed_spaces: Vec<matrix_sdk::ruma::OwnedRoomId>,
    pub order: Option<String>,
    pub suggested: bool,
}

pub type RoomListDiff = VectorDiff<RoomData>;
pub type TimelineDiff<T> = VectorDiff<Arc<T>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomMemberInfo {
    pub user_id: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildData {
    pub order: Option<String>,
    pub suggested: bool,
}

#[derive(Debug, Default, Clone)]
pub struct SpaceHierarchy {
    /// Maps a space ID to its children (rooms or sub-spaces) and their data (order, suggested)
    pub children: HashMap<OwnedRoomId, HashMap<OwnedRoomId, ChildData>>,
    /// Maps a room/space ID to its parent spaces
    pub parents: HashMap<OwnedRoomId, HashSet<OwnedRoomId>>,
    /// Set of all known space IDs
    pub known_spaces: HashSet<OwnedRoomId>,
}

impl SpaceHierarchy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_space(&mut self, space_id: OwnedRoomId) {
        self.known_spaces.insert(space_id);
    }

    pub fn is_known_space(&self, room_id: &RoomId) -> bool {
        self.known_spaces.contains(room_id)
    }

    pub fn add_child(
        &mut self,
        space_id: OwnedRoomId,
        child_id: OwnedRoomId,
        order: Option<String>,
        suggested: bool,
    ) {
        self.add_space(space_id.clone());
        let children = self.children.entry(space_id.clone()).or_default();
        children.insert(child_id.clone(), ChildData { order, suggested });

        let parents = self.parents.entry(child_id).or_default();
        parents.insert(space_id);
    }

    pub fn add_relationship(&mut self, space_id: OwnedRoomId, child_id: OwnedRoomId) {
        self.add_space(space_id.clone());
        let children = self.children.entry(space_id.clone()).or_default();
        children.entry(child_id.clone()).or_insert(ChildData {
            order: None,
            suggested: false,
        });

        let parents = self.parents.entry(child_id).or_default();
        parents.insert(space_id);
    }

    pub fn remove_child(&mut self, space_id: &RoomId, child_id: &RoomId) {
        if let Some(children) = self.children.get_mut(space_id) {
            children.remove(child_id);
        }
        if let Some(parents) = self.parents.get_mut(child_id) {
            parents.remove(space_id);
        }
    }

    pub fn is_in_space(&self, room_id: &RoomId, space_id: &RoomId) -> bool {
        let mut visited = HashSet::new();
        // Check if the room is a direct or indirect child of the space
        self.is_child_of_recursive(room_id, space_id, &mut visited)
    }

    pub fn get_descendants_strs<'a>(&'a self, space_id: &'a RoomId) -> HashSet<&'a str> {
        let mut descendants = HashSet::new();
        let mut queue = vec![space_id];
        while let Some(current) = queue.pop() {
            if descendants.insert(current.as_str())
                && let Some(children) = self.children.get(current)
            {
                for child in children.keys() {
                    queue.push(child);
                }
            }
        }
        descendants
    }

    fn is_child_of_recursive<'a>(
        &'a self,
        current_id: &'a RoomId,
        target_space_id: &RoomId,
        visited: &mut HashSet<&'a RoomId>,
    ) -> bool {
        if current_id == target_space_id {
            return true;
        }

        if !visited.insert(current_id) {
            return false;
        }

        if let Some(parents) = self.parents.get(current_id) {
            for parent in parents {
                if self.is_child_of_recursive(parent, target_space_id, visited) {
                    return true;
                }
            }
        }

        false
    }
}

#[derive(Debug, Clone)]
pub enum MatrixEvent {
    SyncStatusChanged(SyncStatus),
    SyncIndicatorChanged(bool),
    RoomDiff(Box<RoomListDiff>),
    TimelineDiff(TimelineDiff<TimelineItem>),
    TimelineReset,
    TimelineInitFinished,
    ReactionAdded {
        room_id: String,
        event_id: String,
        reaction: String,
    },
    IgnoredUsersChanged(Vec<matrix_sdk::ruma::OwnedUserId>),
    SpaceHierarchyChanged,
    CallParticipantsChanged {
        room_id: String,
        participants: Vec<matrix_sdk::ruma::OwnedUserId>,
    },
}

#[derive(Serialize, Deserialize, Debug)]
struct SessionData {
    homeserver: String,
    user_id: String,
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
    device_id: String,
    #[serde(default)]
    is_oidc: bool,
}

#[derive(Clone, Debug)]
pub struct MatrixEngine {
    inner: Arc<RwLock<MatrixEngineInner>>,
}

struct MatrixEngineInner {
    client: Client,
    sync_service: Option<Arc<SyncService>>,
    room_list_service: Option<Arc<RoomListService>>,
    room_list_controller: Option<Arc<RoomListDynamicEntriesController>>,
    timelines: HashMap<OwnedRoomId, Arc<Timeline>>,
    threaded_timelines: HashMap<(OwnedRoomId, matrix_sdk::ruma::OwnedEventId), Arc<Timeline>>,
    /// Event-focused timelines, keyed by (room, target event). Built lazily when
    /// a permalink points at a message not present in the live window; see
    /// `event_timeline`. Cached so repeated opens are cheap.
    event_timelines: HashMap<(OwnedRoomId, matrix_sdk::ruma::OwnedEventId), Arc<Timeline>>,
    data_dir: PathBuf,
    sync_handle: Option<tokio::task::JoinHandle<()>>,
    space_hierarchy: SpaceHierarchy,
    oidc_client: Option<Client>,
    session_change_handle: Option<tokio::task::JoinHandle<()>>,
    call_participants: HashMap<OwnedRoomId, HashSet<matrix_sdk::ruma::OwnedUserId>>,
    active_call: Option<Arc<livekit::Room>>,
}

impl std::fmt::Debug for MatrixEngineInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MatrixEngineInner")
            .field("client", &self.client)
            .field(
                "sync_service",
                &self.sync_service.as_ref().map(|_| "SyncService"),
            )
            .field(
                "room_list_service",
                &self.room_list_service.as_ref().map(|_| "RoomListService"),
            )
            .field(
                "room_list_controller",
                &self
                    .room_list_controller
                    .as_ref()
                    .map(|_| "RoomListDynamicEntriesController"),
            )
            .field("timelines", &self.timelines.keys())
            .field("threaded_timelines", &self.threaded_timelines.keys())
            .field("event_timelines", &self.event_timelines.keys())
            .field("data_dir", &self.data_dir)
            .field(
                "sync_handle",
                &self.sync_handle.as_ref().map(|_| "JoinHandle"),
            )
            .field("space_hierarchy", &self.space_hierarchy)
            .field("oidc_client", &self.oidc_client.as_ref().map(|_| "Client"))
            .field(
                "session_change_handle",
                &self.session_change_handle.as_ref().map(|_| "JoinHandle"),
            )
            .finish()
    }
}

/// Maximum age (ms) of an event for which we still raise a desktop notification.
/// Events older than this (e.g. replayed during initial sync) are suppressed to
/// avoid notification spam. 5 minutes.
const NOTIFICATION_MAX_AGE_MS: u128 = 300_000;

/// Whether an event with the given server timestamp (ms since the Unix epoch) is
/// recent enough to notify about, given the current time in ms since the epoch.
///
/// `now_ms` is derived from `SystemTime::now().duration_since(UNIX_EPOCH)`, which
/// yields `0` when the wall clock reads before the epoch (broken RTC, early boot);
/// in that case the event is treated as stale instead of panicking.
fn is_recent_enough_to_notify(now_ms: u128, event_ts_ms: u64) -> bool {
    now_ms.abs_diff(u128::from(event_ts_ms)) <= NOTIFICATION_MAX_AGE_MS
}

impl MatrixEngine {
    pub async fn new(data_dir: PathBuf) -> Result<Self> {
        let client = Self::setup_client(data_dir.clone(), "https://matrix.org").await?;

        client
            .oauth()
            .restore_registered_client(matrix_sdk::authentication::oauth::ClientId::new(
                OIDC_CLIENT_ID.to_string(),
            ));

        let inner = MatrixEngineInner {
            client: client.clone(),
            sync_service: None,
            room_list_service: None,
            room_list_controller: None,
            timelines: HashMap::new(),
            threaded_timelines: HashMap::new(),
            event_timelines: HashMap::new(),
            data_dir,
            sync_handle: None,
            space_hierarchy: SpaceHierarchy::new(),
            oidc_client: None,
            session_change_handle: None,
            call_participants: HashMap::new(),
            active_call: None,
        };

        let engine = Self {
            inner: Arc::new(RwLock::new(inner)),
        };
        engine.setup_event_handlers(&client);
        engine.spawn_session_change_handler(client).await;
        Ok(engine)
    }

    fn get_fallback_path(secret_type: &str) -> PathBuf {
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("constellations");
        if !data_dir.exists() {
            let _ = std::fs::create_dir_all(&data_dir);
        }
        data_dir.join(format!("{}.fallback", secret_type))
    }

    fn write_secret_file(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
        use std::fs::OpenOptions;
        use std::io::Write;
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        file.write_all(data)
    }

    fn should_bypass_keyring() -> bool {
        cfg!(test) && std::env::var("CONSTELLATIONS_TEST_KEYRING").is_err()
    }

    async fn save_session_to_keyring(session_data: &SessionData) -> Result<()> {
        let secret = serde_json::to_vec(session_data)?;

        let keyring = match if Self::should_bypass_keyring() {
            Err(anyhow::anyhow!("Bypassing keyring in test"))
        } else {
            Keyring::new().await.map_err(|e| e.into())
        } {
            Ok(k) => k,
            Err(e) => {
                tracing::warn!(
                    "Failed to initialize Keyring: {}. Falling back to file-based session storage.",
                    e
                );
                let path = Self::get_fallback_path("matrix-session");
                Self::write_secret_file(&path, &secret)?;
                return Ok(());
            }
        };

        let mut attributes = HashMap::new();
        attributes.insert("app_id", "fi.joonastuomi.Constellations");
        attributes.insert("type", "matrix-session");

        match keyring
            .create_item("Constellations Matrix Session", &attributes, &secret, true)
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => {
                tracing::warn!(
                    "Failed to create session item in Keyring: {}. Falling back to file-based session storage.",
                    e
                );
                let path = Self::get_fallback_path("matrix-session");
                Self::write_secret_file(&path, &secret)?;
                Ok(())
            }
        }
    }

    async fn spawn_session_change_handler(&self, client: Client) {
        let mut subscriber = client.subscribe_to_session_changes();
        let homeserver = client.homeserver().to_string();

        let handle = tokio::spawn(async move {
            loop {
                match subscriber.recv().await {
                    Ok(change) => match change {
                        SessionChange::TokensRefreshed => {
                            info!("Session tokens refreshed, updating keyring...");

                            if let Some(session) = client.oauth().user_session() {
                                let session_data = SessionData {
                                    homeserver: homeserver.clone(),
                                    user_id: session.meta.user_id.to_string(),
                                    access_token: session.tokens.access_token.to_string(),
                                    refresh_token: session.tokens.refresh_token.clone(),
                                    id_token: None,
                                    device_id: session.meta.device_id.to_string(),
                                    is_oidc: true,
                                };

                                if let Err(e) = Self::save_session_to_keyring(&session_data).await {
                                    error!("Failed to update session in keyring: {}", e);
                                } else {
                                    info!("Successfully updated session in keyring.");
                                }
                            } else if let Some(session) = client.matrix_auth().session() {
                                let session_data = SessionData {
                                    homeserver: homeserver.clone(),
                                    user_id: session.meta.user_id.to_string(),
                                    access_token: session.tokens.access_token.to_string(),
                                    refresh_token: session.tokens.refresh_token.clone(),
                                    id_token: None,
                                    device_id: session.meta.device_id.to_string(),
                                    is_oidc: false,
                                };

                                if let Err(e) = Self::save_session_to_keyring(&session_data).await {
                                    error!("Failed to update session in keyring: {}", e);
                                } else {
                                    info!("Successfully updated session in keyring.");
                                }
                            } else {
                                error!("Session tokens refreshed but client has no session!");
                            }
                        }
                        SessionChange::UnknownToken { .. } => {
                            error!("Session token is no longer valid!");
                        }
                    },
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        error!("Session change subscriber lagged by {} messages", n);
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        info!("Session change subscriber closed.");
                        break;
                    }
                }
            }
        });

        let mut inner = self.inner.write().await;
        if let Some(old_handle) = inner.session_change_handle.take() {
            old_handle.abort();
        }
        inner.session_change_handle = Some(handle);
        drop(inner);
    }

    fn setup_event_handlers(&self, client: &Client) {
        client.add_event_handler(
            |event: matrix_sdk::ruma::events::room::message::SyncRoomMessageEvent,
             room: matrix_sdk::Room| {
                async move {
                    if let matrix_sdk::ruma::events::room::message::SyncRoomMessageEvent::Original(
                        ev,
                    ) = event
                    {
                        // Ignore our own messages
                        if let Some(user_id) = room.client().user_id()
                            && ev.sender == user_id
                        {
                            return;
                        }

                        // Avoid spamming during initial sync by checking if event is older
                        // than 5 minutes. `now` falls back to 0 (treated as stale) when the
                        // system clock reads before the Unix epoch instead of panicking.
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis();

                        if !is_recent_enough_to_notify(now, ev.origin_server_ts.0.into()) {
                            return;
                        }

                        let room_name = room.name().unwrap_or_else(|| "Unknown Room".to_string());

                        let sender = if let Ok(Some(member)) = room.get_member(&ev.sender).await {
                            member
                                .display_name()
                                .map(|n| n.to_owned())
                                .unwrap_or_else(|| ev.sender.as_str().to_string())
                        } else {
                            ev.sender.as_str().to_string()
                        };

                        let body = match &ev.content.msgtype {
                            matrix_sdk::ruma::events::room::message::MessageType::Text(text) => {
                                text.body.clone()
                            }
                            matrix_sdk::ruma::events::room::message::MessageType::Image(_) => {
                                "📷 Image".to_string()
                            }
                            matrix_sdk::ruma::events::room::message::MessageType::Video(_) => {
                                "🎥 Video".to_string()
                            }
                            matrix_sdk::ruma::events::room::message::MessageType::Audio(_) => {
                                "🎵 Audio".to_string()
                            }
                            matrix_sdk::ruma::events::room::message::MessageType::File(_) => {
                                "📎 File".to_string()
                            }
                            _ => "New message".to_string(),
                        };

                        let _ = notify_rust::Notification::new()
                            .appname("Constellations")
                            .summary(&format!("{} in {}", sender, room_name))
                            .body(&body)
                            .show_async()
                            .await;
                    }
                }
            },
        );

        macro_rules! handle_space_hierarchy {
            (
                $client:expr,
                $inner_clone:expr,
                $content_type:ty,
                $parent_id:expr,
                $child_id:expr,
                $add_logic:expr,
                $remove_msg:literal,
                $add_msg:literal $(, $add_arg:expr)* ;
                $redacted_msg:literal
            ) => {
                let inner_clone = $inner_clone.clone();
                $client.add_event_handler(
                    move |event: SyncStateEvent<$content_type>, room: Room| {
                        let inner = inner_clone.clone();
                        async move {
                            let room_id = room.room_id().to_owned();
                            let state_key = match RoomId::parse(event.state_key()) {
                                Ok(id) => id,
                                Err(_) => return,
                            };

                            let parent_id = $parent_id(&room_id, &state_key);
                            let child_id = $child_id(&room_id, &state_key);

                            let mut inner_write = inner.write().await;
                            match event {
                                SyncStateEvent::Original(ev) => {
                                    if ev.content.via.is_empty() {
                                        inner_write
                                            .space_hierarchy
                                            .remove_child(&parent_id, &child_id);
                                        info!(
                                            $remove_msg,
                                            state_key, room_id
                                        );
                                    } else {
                                        $add_logic(&mut inner_write, &parent_id, &child_id, &ev);
                                        info!(
                                            $add_msg,
                                            state_key, room_id $(, $add_arg(&ev))*
                                        );
                                    }
                                }
                                SyncStateEvent::Redacted(_) => {
                                    inner_write
                                        .space_hierarchy
                                        .remove_child(&parent_id, &child_id);
                                    info!(
                                        $redacted_msg,
                                        state_key, room_id
                                    );
                                }
                            }
                        }
                    },
                );
            };
        }

        handle_space_hierarchy!(
            client,
            self.inner,
            SpaceChildEventContent,
            |room_id: &OwnedRoomId, _state_key: &OwnedRoomId| room_id.clone(),
            |_room_id: &OwnedRoomId, state_key: &OwnedRoomId| state_key.clone(),
            |inner_write: &mut tokio::sync::RwLockWriteGuard<'_, MatrixEngineInner>,
             parent_id: &OwnedRoomId,
             child_id: &OwnedRoomId,
             ev: &matrix_sdk::ruma::events::OriginalSyncStateEvent<SpaceChildEventContent>| {
                inner_write.space_hierarchy.add_child(
                    parent_id.clone(),
                    child_id.clone(),
                    ev.content.order.as_ref().map(|o| o.to_string()),
                    ev.content.suggested,
                );
            },
            "Space hierarchy updated: {} removed from {}",
            "Space hierarchy updated: {} is child of {} (order: {:?})",
            |ev: &matrix_sdk::ruma::events::OriginalSyncStateEvent<SpaceChildEventContent>| ev.content.order.clone() ;
            "Space hierarchy updated: {} removed from {} (redacted)"
        );

        handle_space_hierarchy!(
            client,
            self.inner,
            SpaceParentEventContent,
            |_room_id: &OwnedRoomId, state_key: &OwnedRoomId| state_key.clone(),
            |room_id: &OwnedRoomId, _state_key: &OwnedRoomId| room_id.clone(),
            |inner_write: &mut tokio::sync::RwLockWriteGuard<'_, MatrixEngineInner>,
             parent_id: &OwnedRoomId,
             child_id: &OwnedRoomId,
             _ev: &matrix_sdk::ruma::events::OriginalSyncStateEvent<SpaceParentEventContent>| {
                inner_write.space_hierarchy.add_relationship(parent_id.clone(), child_id.clone());
            },
            "Space hierarchy updated: {} removed as parent of {}",
            "Space hierarchy updated: {} is parent of {}" ;
            "Space hierarchy updated: {} removed as parent of {} (redacted)"
        );

        let inner_clone = self.inner.clone();
        client.add_event_handler(
            move |event: SyncStateEvent<
                matrix_sdk::ruma::events::call::member::CallMemberEventContent,
            >,
                  room: Room| {
                let inner = inner_clone.clone();
                async move {
                    let room_id = room.room_id().to_owned();
                    let user_id = match UserId::parse(event.state_key()) {
                        Ok(id) => id,
                        Err(_) => return,
                    };

                    let mut inner_write = inner.write().await;
                    let participants = inner_write.call_participants.entry(room_id).or_default();

                    match event {
                        SyncStateEvent::Original(ev) => {
                            if ev.content.memberships().is_empty() {
                                participants.remove(&user_id);
                            } else {
                                participants.insert(user_id);
                            }
                        }
                        SyncStateEvent::Redacted(_) => {
                            participants.remove(&user_id);
                        }
                    }
                }
            },
        );
    }

    pub async fn register(&self, homeserver: &str, username: &str, password: &str) -> Result<()> {
        let homeserver_url = if homeserver.starts_with("https://")
            || homeserver.starts_with("http://localhost")
            || homeserver.starts_with("http://127.0.0.1")
            || homeserver.starts_with("http://[::1]")
        {
            homeserver.to_string()
        } else {
            let stripped = homeserver.strip_prefix("http://").unwrap_or(homeserver);
            format!("https://{}", stripped)
        };

        let client = {
            let mut inner = self.inner.write().await;
            if let Some(handle) = inner.sync_handle.take() {
                handle.abort();
            }
            if let Some(handle) = inner.session_change_handle.take() {
                handle.abort();
            }
            let data_dir = inner.data_dir.clone();
            let new_client = Self::setup_client(data_dir, &homeserver_url).await?;
            inner.client = new_client.clone();
            new_client
        };

        use matrix_sdk::ruma::api::client::account::register::v3::Request as RegisterRequest;
        let mut request = RegisterRequest::new();
        request.username = Some(username.to_string());
        request.password = Some(password.to_string());
        request.initial_device_display_name = Some("Constellations Matrix Client".to_string());

        client
            .matrix_auth()
            .register(request)
            .await
            .context("Failed to register")?;

        let sync_service: Arc<SyncService> =
            Arc::new(SyncService::builder(client.clone()).build().await?);
        let room_list_service = sync_service.room_list_service();

        // Save session to oo7
        if let Some(session) = client.matrix_auth().session() {
            let session_data = SessionData {
                homeserver: homeserver_url,
                user_id: session.meta.user_id.to_string(),
                access_token: session.tokens.access_token.to_string(),
                refresh_token: session.tokens.refresh_token.clone(),
                id_token: None,
                device_id: session.meta.device_id.to_string(),
                is_oidc: false,
            };

            Self::save_session_to_keyring(&session_data).await?;
        }

        self.setup_event_handlers(&client);

        let mut inner = self.inner.write().await;
        inner.client = client.clone();
        inner.sync_service = Some(sync_service);
        inner.room_list_service = Some(room_list_service);

        drop(inner);
        self.spawn_session_change_handler(client).await;

        Ok(())
    }

    pub async fn login(&self, homeserver: &str, username: &str, password: &str) -> Result<()> {
        let homeserver_url = if homeserver.starts_with("https://")
            || homeserver.starts_with("http://localhost")
            || homeserver.starts_with("http://127.0.0.1")
            || homeserver.starts_with("http://[::1]")
        {
            homeserver.to_string()
        } else {
            let stripped = homeserver.strip_prefix("http://").unwrap_or(homeserver);
            format!("https://{}", stripped)
        };

        let client = {
            let mut inner = self.inner.write().await;
            if let Some(handle) = inner.sync_handle.take() {
                handle.abort();
            }
            if let Some(handle) = inner.session_change_handle.take() {
                handle.abort();
            }
            let data_dir = inner.data_dir.clone();
            let new_client = Self::setup_client(data_dir, &homeserver_url).await?;
            inner.client = new_client.clone();
            new_client
        };

        client
            .matrix_auth()
            .login_username(username, password)
            .initial_device_display_name("Constellations Matrix Client")
            .send()
            .await
            .context("Failed to login")?;

        let sync_service: Arc<SyncService> =
            Arc::new(SyncService::builder(client.clone()).build().await?);
        let room_list_service = sync_service.room_list_service();

        // Save session to oo7
        if let Some(session) = client.matrix_auth().session() {
            let session_data = SessionData {
                homeserver: homeserver_url,
                user_id: session.meta.user_id.to_string(),
                access_token: session.tokens.access_token.to_string(),
                refresh_token: session.tokens.refresh_token.clone(),
                id_token: None,
                device_id: session.meta.device_id.to_string(),
                is_oidc: false,
            };

            Self::save_session_to_keyring(&session_data).await?;
        }

        self.setup_event_handlers(&client);

        let mut inner = self.inner.write().await;
        inner.client = client.clone();
        inner.sync_service = Some(sync_service);
        inner.room_list_service = Some(room_list_service);

        drop(inner);
        self.spawn_session_change_handler(client).await;

        Ok(())
    }

    pub async fn restore_session(&self) -> Result<bool> {
        let use_fallback = || -> Result<Option<Vec<u8>>> {
            let path = Self::get_fallback_path("matrix-session");
            if path.exists() {
                let data = std::fs::read(&path)?;
                return Ok(Some(data));
            }
            Ok(None)
        };

        let secret_data = 'find_secret: {
            let keyring = match if Self::should_bypass_keyring() {
                Err(anyhow::anyhow!("Bypassing keyring in test"))
            } else {
                Keyring::new().await.map_err(|e| e.into())
            } {
                Ok(k) => k,
                Err(e) => {
                    tracing::warn!(
                        "Failed to initialize Keyring for restore: {}. Attempting file-based fallback.",
                        e
                    );
                    if let Some(data) = use_fallback()? {
                        break 'find_secret Some(data);
                    }
                    return Ok(false);
                }
            };

            let mut attributes = HashMap::new();
            attributes.insert("app_id", "fi.joonastuomi.Constellations");
            attributes.insert("type", "matrix-session");

            match keyring.search_items(&attributes).await {
                Ok(items) => {
                    if let Some(item) = items.first()
                        && let Ok(secret) = item.secret().await
                    {
                        break 'find_secret Some(secret.to_vec());
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to search Keyring items for restore: {}. Attempting file-based fallback.",
                        e
                    );
                }
            }

            use_fallback()?
        };

        let Some(secret) = secret_data else {
            return Ok(false);
        };

        let session_data: SessionData = serde_json::from_slice(&secret)?;

        let data_dir = self.inner.read().await.data_dir.clone();
        let client = Self::setup_client(data_dir, &session_data.homeserver).await?;

        if session_data.is_oidc {
            client.oauth().restore_registered_client(
                matrix_sdk::authentication::oauth::ClientId::new(OIDC_CLIENT_ID.to_string()),
            );
            client
                .oauth()
                .restore_session(
                    matrix_sdk::authentication::oauth::OAuthSession {
                        client_id: matrix_sdk::authentication::oauth::ClientId::new(
                            OIDC_CLIENT_ID.to_string(),
                        ),
                        user: matrix_sdk::authentication::oauth::UserSession {
                            meta: matrix_sdk::SessionMeta {
                                user_id: UserId::parse(session_data.user_id.clone())?,
                                device_id: OwnedDeviceId::from(session_data.device_id),
                            },
                            tokens: SessionTokens {
                                access_token: session_data.access_token,
                                refresh_token: session_data.refresh_token,
                            },
                        },
                    },
                    matrix_sdk::store::RoomLoadSettings::default(),
                )
                .await?;
        } else {
            let matrix_session = MatrixSession {
                meta: matrix_sdk::SessionMeta {
                    user_id: UserId::parse(session_data.user_id.clone())?,
                    device_id: OwnedDeviceId::from(session_data.device_id),
                },
                tokens: SessionTokens {
                    access_token: session_data.access_token,
                    refresh_token: session_data.refresh_token,
                },
            };
            client
                .matrix_auth()
                .restore_session(
                    matrix_session,
                    matrix_sdk::store::RoomLoadSettings::default(),
                )
                .await?;
        }

        let sync_service: Arc<SyncService> =
            Arc::new(SyncService::builder(client.clone()).build().await?);
        let room_list_service = sync_service.room_list_service();

        self.setup_event_handlers(&client);

        let mut inner = self.inner.write().await;
        inner.client = client.clone();
        inner.sync_service = Some(sync_service);
        inner.room_list_service = Some(room_list_service);

        drop(inner);
        self.spawn_session_change_handler(client).await;

        Ok(true)
    }

    pub async fn logout(&self) -> Result<()> {
        // Delete fallback session file
        let session_path = Self::get_fallback_path("matrix-session");
        if session_path.exists() {
            let _ = std::fs::remove_file(session_path);
        }

        // Delete fallback passphrase file
        let pass_path = Self::get_fallback_path("store-passphrase");
        if pass_path.exists() {
            let _ = std::fs::remove_file(pass_path);
        }

        let keyring = match if Self::should_bypass_keyring() {
            Err(anyhow::anyhow!("Bypassing keyring in test"))
        } else {
            Keyring::new().await.map_err(|e| e.into())
        } {
            Ok(k) => k,
            Err(e) => {
                tracing::warn!(
                    "Failed to initialize Keyring for logout: {}. Local fallback files have been deleted.",
                    e
                );
                return Ok(());
            }
        };

        let mut session_attributes = HashMap::new();
        session_attributes.insert("app_id", "fi.joonastuomi.Constellations");
        session_attributes.insert("type", "matrix-session");

        if let Ok(items) = keyring.search_items(&session_attributes).await {
            for item in items {
                let _ = item.delete().await;
            }
        }

        let mut pass_attributes = HashMap::new();
        pass_attributes.insert("app_id", "fi.joonastuomi.Constellations");
        pass_attributes.insert("type", "store-passphrase");

        if let Ok(items) = keyring.search_items(&pass_attributes).await {
            for item in items {
                let _ = item.delete().await;
            }
        }

        let mut inner = self.inner.write().await;
        if let Some(handle) = inner.sync_handle.take() {
            handle.abort();
        }
        if let Some(sync_service) = inner.sync_service.take() {
            let _ = sync_service.stop().await;
        }
        inner.room_list_service = None;
        inner.room_list_controller = None;
        inner.timelines.clear();
        inner.threaded_timelines.clear();
        inner.space_hierarchy = SpaceHierarchy::new();

        // Try logging out properly from Matrix
        let _ = inner.client.matrix_auth().logout().await;

        let store_path = inner.data_dir.join("matrix-store");
        let _ = std::fs::remove_dir_all(&store_path);

        Ok(())
    }

    pub async fn client(&self) -> Client {
        self.inner.read().await.client.clone()
    }

    pub async fn sync_service(&self) -> Option<Arc<SyncService>> {
        self.inner.read().await.sync_service.clone()
    }

    pub async fn room_list_service(&self) -> Option<Arc<RoomListService>> {
        self.inner.read().await.room_list_service.clone()
    }

    pub async fn set_room_list_controller(
        &self,
        controller: Arc<RoomListDynamicEntriesController>,
    ) {
        let mut inner = self.inner.write().await;
        inner.room_list_controller = Some(controller);
    }

    pub async fn set_media_previews_display_policy(&self, enabled: bool) -> Result<()> {
        info!("Setting media previews display policy to: {}", enabled);
        // Placeholder for future SDK integration
        Ok(())
    }

    pub async fn set_invite_avatars_display_policy(&self, enabled: bool) -> Result<()> {
        info!("Setting invite avatars display policy to: {}", enabled);
        // Placeholder for future SDK integration
        Ok(())
    }

    pub async fn update_room_list_filter(&self, selected_space: Option<OwnedRoomId>) -> Result<()> {
        let inner = self.inner.read().await;
        if let Some(controller) = &inner.room_list_controller {
            use matrix_sdk_ui::room_list_service::filters;

            let filter: Box<dyn matrix_sdk_ui::room_list_service::filters::Filter + Send + Sync> =
                if let Some(space_id) = selected_space {
                    let hierarchy = inner.space_hierarchy.clone();
                    let space_id_clone = space_id.clone();
                    // Custom filter that checks if the room is in the selected space OR is a space itself
                    // This ensures the SpaceSwitcher always has access to all spaces.
                    Box::new(filters::new_filter_any(vec![Box::new(
                        move |item: &matrix_sdk_ui::room_list_service::RoomListItem| {
                            hierarchy.is_in_space(item.room_id(), &space_id_clone)
                                || hierarchy.is_known_space(item.room_id())
                        },
                    )]))
                } else {
                    // No space selected, show all rooms
                    Box::new(filters::new_filter_all(vec![]))
                };

            controller.set_filter(filter);
        }
        Ok(())
    }

    fn strip_reply_quote(body: &str) -> &str {
        let mut actual_line = None;
        let mut in_quote = false;
        for line in body.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with('>') {
                in_quote = true;
                continue;
            }
            if in_quote && trimmed.is_empty() {
                continue;
            }
            actual_line = Some(line);
            break;
        }

        if let Some(line) = actual_line {
            line.trim()
        } else {
            body.split('\n').next().unwrap_or("").trim()
        }
    }

    pub async fn fetch_room_data(&self, room: &matrix_sdk::Room) -> Result<RoomData> {
        let id: std::sync::Arc<str> = room.room_id().as_str().into();
        let name = match room.name() {
            Some(n) => Some(n.to_string()),
            None => room.cached_display_name().map(|n| n.to_string()),
        };

        let unread_count = room.unread_notification_counts().notification_count as u32;
        let mut avatar_url = room.avatar_url().map(|u| u.to_string());
        if room.joined_members_count() == 2 || room.active_members_count() == 2 {
            let client = room.client();
            if let Some(my_user_id) = client.user_id()
                && let Ok(members) = room
                    .members_no_sync(matrix_sdk::RoomMemberships::ACTIVE)
                    .await
            {
                let other_member = members.iter().find(|m| m.user_id() != my_user_id);
                if let Some(other_member) = other_member
                    && let Some(other_avatar) = other_member.avatar_url()
                {
                    avatar_url = Some(other_avatar.to_string());
                }
            }
        }

        let last_message = match room.latest_event().await {
            LatestEventValue::Remote {
                content: TimelineItemContent::MsgLike(m),
                ..
            } => {
                if let MsgLikeKind::Message(msg_content) = &m.kind {
                    let cleaned = Self::strip_reply_quote(msg_content.body());
                    let mut msg = cleaned.to_string();
                    if msg.len() > 30 {
                        msg.truncate(26);
                        msg.push_str("...");
                    }
                    Some(msg)
                } else {
                    None
                }
            }
            LatestEventValue::Local {
                content: TimelineItemContent::MsgLike(m),
                ..
            } => {
                if let MsgLikeKind::Message(msg_content) = &m.kind {
                    let cleaned = Self::strip_reply_quote(msg_content.body());
                    let mut msg = cleaned.to_string();
                    if msg.len() > 30 {
                        msg.truncate(26);
                        msg.push_str("...");
                    }
                    Some(msg)
                } else {
                    None
                }
            }
            _ => None,
        };

        let room_type = room.room_type();
        let is_space = room_type == Some(RoomType::Space);

        if is_space {
            let mut inner = self.inner.write().await;
            inner.space_hierarchy.add_space(room.room_id().to_owned());
        }

        let (parent_space_id, order, suggested) = {
            let inner = self.inner.read().await;
            let parent_id = inner
                .space_hierarchy
                .parents
                .get(room.room_id())
                .and_then(|parents| parents.iter().next());

            let (order, suggested) = parent_id
                .and_then(|p| {
                    inner
                        .space_hierarchy
                        .children
                        .get(p)
                        .and_then(|c| c.get(room.room_id()))
                })
                .map(|d| (d.order.clone(), d.suggested))
                .unwrap_or((None, false));

            (parent_id.map(|id| id.to_string()), order, suggested)
        };

        let unread_count_str = if unread_count > 0 {
            Some(format!("({})", unread_count))
        } else {
            None
        };

        let (join_rule, allowed_spaces) = if let Ok(Some(event)) = room
            .get_state_event_static::<matrix_sdk::ruma::events::room::join_rules::RoomJoinRulesEventContent>()
            .await
        {
            match event.deserialize()? {
                matrix_sdk_base::deserialized_responses::SyncOrStrippedState::Sync(
                    matrix_sdk::ruma::events::SyncStateEvent::Original(ev),
                ) => {
                    let content = ev.content;
                    let allowed_spaces = match &content.join_rule {
                        matrix_sdk::ruma::events::room::join_rules::JoinRule::Restricted(r) => {
                            r.allow
                                .iter()
                                .filter_map(|a| match a {
                                    matrix_sdk::ruma::events::room::join_rules::AllowRule::RoomMembership(
                                        m,
                                    ) => Some(m.room_id.clone()),
                                    _ => None,
                                })
                                .collect()
                        }
                        matrix_sdk::ruma::events::room::join_rules::JoinRule::KnockRestricted(
                            r,
                        ) => {
                            r.allow
                                .iter()
                                .filter_map(|a| match a {
                                    matrix_sdk::ruma::events::room::join_rules::AllowRule::RoomMembership(
                                        m,
                                    ) => Some(m.room_id.clone()),
                                    _ => None,
                                })
                                .collect()
                        }
                        _ => Vec::new(),
                    };
                    (Some(content.join_rule), allowed_spaces)
                }
                matrix_sdk_base::deserialized_responses::SyncOrStrippedState::Stripped(ev) => {
                    let content = ev.content;
                    let allowed_spaces = match &content.join_rule {
                        matrix_sdk::ruma::events::room::join_rules::JoinRule::Restricted(r) => {
                            r.allow
                                .iter()
                                .filter_map(|a| match a {
                                    matrix_sdk::ruma::events::room::join_rules::AllowRule::RoomMembership(
                                        m,
                                    ) => Some(m.room_id.clone()),
                                    _ => None,
                                })
                                .collect()
                        }
                        matrix_sdk::ruma::events::room::join_rules::JoinRule::KnockRestricted(
                            r,
                        ) => {
                            r.allow
                                .iter()
                                .filter_map(|a| match a {
                                    matrix_sdk::ruma::events::room::join_rules::AllowRule::RoomMembership(
                                        m,
                                    ) => Some(m.room_id.clone()),
                                    _ => None,
                                })
                                .collect()
                        }
                        _ => Vec::new(),
                    };
                    (Some(content.join_rule), allowed_spaces)
                }
                _ => (None, Vec::new()),
            }
        } else {
            (None, Vec::new())
        };

        Ok(RoomData {
            id,
            name,
            last_message,
            unread_count,
            unread_count_str,
            avatar_url,
            room_type,
            is_space,
            parent_space_id,
            join_rule,
            allowed_spaces,
            order,
            suggested,
        })
    }

    pub async fn start_sync(&self) -> Result<(), SyncError> {
        let client = self.client().await;
        let request =
            matrix_sdk::ruma::api::client::discovery::get_supported_versions::Request::new();
        let versions = client.send(request).await?;
        let supports_sliding_sync = versions
            .unstable_features
            .contains_key("org.matrix.msc4186")
            || versions.versions.iter().any(|v| v == "v1.11");

        if !supports_sliding_sync {
            return Err(SyncError::MissingSlidingSyncSupport);
        }

        let mut inner = self.inner.write().await;

        if let Some(handle) = inner.sync_handle.take() {
            handle.abort();
            if let Some(sync_service) = &inner.sync_service {
                let _ = sync_service.stop().await;
            }
        }

        if let Some(sync_service) = &inner.sync_service {
            let sync_service = sync_service.clone();
            let handle = tokio::spawn(async move {
                info!("Starting Matrix sync service...");
                sync_service.start().await;

                let mut state_stream = sync_service.state();
                while let Some(state) = state_stream.next().await {
                    match state {
                        matrix_sdk_ui::sync_service::State::Terminated
                        | matrix_sdk_ui::sync_service::State::Error(_) => {
                            error!("Matrix sync service stopped or failed. State: {:?}", state);
                            break;
                        }
                        _ => {}
                    }
                }
            });
            inner.sync_handle = Some(handle);
        }

        Ok(())
    }

    pub async fn timeline(&self, room_id: &str) -> Result<Arc<Timeline>> {
        let room_id = RoomId::parse(room_id)?;

        let rls = self
            .room_list_service()
            .await
            .context("RoomListService not initialized")?;
        rls.subscribe_to_rooms(&[&room_id]).await;

        {
            let inner = self.inner.read().await;
            if let Some(timeline) = inner.timelines.get(&room_id) {
                return Ok(timeline.clone());
            }
        }

        let room = rls
            .room(&room_id)
            .map_err(|e| anyhow::anyhow!("Failed to get room: {}", e))?;
        let timeline = Arc::new(
            room.timeline_builder()
                .with_focus(TimelineFocus::Live {
                    hide_threaded_events: false,
                })
                .build()
                .await?,
        );

        let mut inner = self.inner.write().await;
        inner.timelines.insert(room_id.to_owned(), timeline.clone());

        Ok(timeline)
    }

    pub async fn threaded_timeline(
        &self,
        room_id: &str,
        root_event_id: &matrix_sdk::ruma::EventId,
    ) -> Result<Arc<Timeline>> {
        let room_id = RoomId::parse(room_id)?;
        let root_event_id = root_event_id.to_owned();

        let rls = self
            .room_list_service()
            .await
            .context("RoomListService not initialized")?;
        rls.subscribe_to_rooms(&[&room_id]).await;

        {
            let inner = self.inner.read().await;
            if let Some(timeline) = inner
                .threaded_timelines
                .get(&(room_id.clone(), root_event_id.clone()))
            {
                return Ok(timeline.clone());
            }
        }

        let room = rls
            .room(&room_id)
            .map_err(|e| anyhow::anyhow!("Failed to get room: {}", e))?;
        let timeline = Arc::new(
            room.timeline_builder()
                .with_focus(TimelineFocus::Thread {
                    root_event_id: root_event_id.clone(),
                })
                .build()
                .await?,
        );

        let mut inner = self.inner.write().await;
        inner
            .threaded_timelines
            .insert((room_id.to_owned(), root_event_id), timeline.clone());

        Ok(timeline)
    }

    /// Build (or fetch from cache) a timeline focused on a specific event, used
    /// to open permalinks to messages that are not present in the live window.
    ///
    /// Uses [`TimelineFocus::Event`] with `num_context_events = 50` so the
    /// target is centred among surrounding context. Thread handling is
    /// `Automatic` so an event inside a thread still resolves without forcing
    /// the whole room into threaded mode.
    ///
    /// Repeated opens for the same (room, event) are served from the cache so
    /// navigating back is cheap.
    pub async fn event_timeline(
        &self,
        room_id: &str,
        target: matrix_sdk::ruma::OwnedEventId,
    ) -> Result<Arc<Timeline>> {
        let room_id_parsed = RoomId::parse(room_id)?;

        {
            let inner = self.inner.read().await;
            if let Some(timeline) = inner
                .event_timelines
                .get(&(room_id_parsed.clone(), target.clone()))
            {
                return Ok(timeline.clone());
            }
        }

        let rls = self
            .room_list_service()
            .await
            .context("RoomListService not initialized")?;
        rls.subscribe_to_rooms(&[&room_id_parsed]).await;

        let room = rls
            .room(&room_id_parsed)
            .map_err(|e| anyhow::anyhow!("Failed to get room: {}", e))?;
        let timeline = Arc::new(
            room.timeline_builder()
                .with_focus(TimelineFocus::Event {
                    target: target.clone(),
                    num_context_events: 50,
                    thread_mode: TimelineEventFocusThreadMode::Automatic {
                        hide_threaded_events: true,
                    },
                })
                .build()
                .await?,
        );

        let mut inner = self.inner.write().await;
        inner
            .event_timelines
            .insert((room_id_parsed, target), timeline.clone());

        Ok(timeline)
    }

    /// Drop a cached event-focused timeline, e.g. when returning to live. The
    /// underlying matrix-sdk timeline is simply dropped (no server teardown is
    /// needed); a future open rebuilds it.
    pub async fn drop_event_timeline(
        &self,
        room_id: &str,
        target: &matrix_sdk::ruma::EventId,
    ) -> Result<()> {
        let room_id = RoomId::parse(room_id)?;
        let mut inner = self.inner.write().await;
        inner.event_timelines.remove(&(room_id, target.to_owned()));
        Ok(())
    }

    pub async fn paginate_backwards(&self, room_id: &str, limit: u16) -> Result<()> {
        let timeline = self.timeline(room_id).await?;
        timeline.paginate_backwards(limit).await?;
        Ok(())
    }

    pub async fn is_room_encrypted(&self, room_id: &str) -> Result<bool> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;
        Ok(room.encryption_settings().is_some())
    }

    pub async fn enable_encryption(&self, room_id: &str) -> Result<()> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;
        room.enable_encryption().await?;
        Ok(())
    }

    pub async fn set_room_name(&self, room_id: &str, name: String) -> Result<()> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;
        room.set_name(name).await?;
        Ok(())
    }

    pub async fn set_room_topic(&self, room_id: &str, topic: String) -> Result<()> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;
        room.set_room_topic(&topic).await?;
        Ok(())
    }

    pub async fn set_canonical_alias(&self, room_id: &str, alias: Option<String>) -> Result<()> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;

        use matrix_sdk::ruma::RoomAliasId;
        use matrix_sdk::ruma::events::room::canonical_alias::RoomCanonicalAliasEventContent;

        let mut content = room
            .get_state_event_static::<RoomCanonicalAliasEventContent>()
            .await?
            .and_then(|e| e.deserialize().ok())
            .and_then(|e| {
                e.as_sync()
                    .and_then(|s| s.as_original().map(|o| o.content.clone()))
                    .or_else(|| e.as_stripped().map(|s| s.content.clone()))
            })
            .unwrap_or_else(RoomCanonicalAliasEventContent::new);

        content.alias = alias
            .filter(|s| !s.is_empty())
            .map(|s| RoomAliasId::parse(s).map(|a| a.to_owned()))
            .transpose()?;

        Ok(())
    }

    pub async fn set_room_history_visibility(
        &self,
        room_id: &str,
        history_visibility: matrix_sdk::ruma::events::room::history_visibility::HistoryVisibility,
    ) -> Result<()> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;
        use matrix_sdk::ruma::events::room::history_visibility::RoomHistoryVisibilityEventContent;
        let content = RoomHistoryVisibilityEventContent::new(history_visibility);
        room.send_state_event(content).await?;
        Ok(())
    }

    pub async fn update_room_aliases(
        &self,
        room_id: &str,
        canonical_alias: Option<String>,
        alt_aliases: Vec<String>,
    ) -> Result<()> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;

        use matrix_sdk::ruma::RoomAliasId;
        use matrix_sdk::ruma::events::room::canonical_alias::RoomCanonicalAliasEventContent;

        let mut content = room
            .get_state_event_static::<RoomCanonicalAliasEventContent>()
            .await?
            .and_then(|e| e.deserialize().ok())
            .and_then(|e| {
                e.as_sync()
                    .and_then(|s| s.as_original().map(|o| o.content.clone()))
                    .or_else(|| e.as_stripped().map(|s| s.content.clone()))
            })
            .unwrap_or_else(RoomCanonicalAliasEventContent::new);

        content.alias = canonical_alias
            .filter(|s| !s.is_empty())
            .map(|s| RoomAliasId::parse(s).map(|a| a.to_owned()))
            .transpose()?;

        content.alt_aliases = alt_aliases
            .into_iter()
            .map(|s| RoomAliasId::parse(s).map(|a| a.to_owned()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(())
    }

    pub async fn get_room_visibility(
        &self,
        room_id: &str,
    ) -> Result<matrix_sdk::ruma::api::client::room::Visibility> {
        let room_id_parsed = RoomId::parse(room_id).map_err(|e| anyhow::anyhow!(e))?;
        let client = self.client().await;
        let request =
            matrix_sdk::ruma::api::client::directory::get_room_visibility::v3::Request::new(
                room_id_parsed,
            );
        let response = client.send(request).await?;
        Ok(response.visibility)
    }

    pub async fn set_room_visibility(
        &self,
        room_id: &str,
        visibility: matrix_sdk::ruma::api::client::room::Visibility,
    ) -> Result<()> {
        let room_id_parsed = RoomId::parse(room_id).map_err(|e| anyhow::anyhow!(e))?;
        let client = self.client().await;
        let request =
            matrix_sdk::ruma::api::client::directory::set_room_visibility::v3::Request::new(
                room_id_parsed,
                visibility,
            );
        client.send(request).await?;
        Ok(())
    }

    pub async fn get_room_join_rule(
        &self,
        room_id: &str,
    ) -> Result<matrix_sdk::ruma::events::room::join_rules::JoinRule> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;
        Ok(room
            .join_rule()
            .unwrap_or(matrix_sdk::ruma::events::room::join_rules::JoinRule::Invite))
    }

    pub async fn set_room_join_rule(
        &self,
        room_id: &str,
        join_rule: matrix_sdk::ruma::events::room::join_rules::JoinRule,
    ) -> Result<()> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;

        use matrix_sdk::ruma::events::room::join_rules::RoomJoinRulesEventContent;
        let content = RoomJoinRulesEventContent::new(join_rule);
        room.send_state_event(content).await?;
        Ok(())
    }

    pub async fn upload_room_avatar(&self, room_id: &str, data: Vec<u8>, mime: &str) -> Result<()> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;

        let content_type = mime.parse::<mime::Mime>()?;
        room.upload_avatar(&content_type, data, None).await?;
        Ok(())
    }

    pub async fn leave_room(&self, room_id: &str) -> Result<()> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;
        room.leave().await?;
        Ok(())
    }

    pub async fn forget_room(&self, room_id: &str) -> Result<()> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;
        room.forget().await?;
        Ok(())
    }

    pub async fn get_room_power_levels(
        &self,
        room_id: &str,
    ) -> Result<(i64, HashMap<matrix_sdk::ruma::OwnedUserId, i64>)> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;
        let power_levels = room.power_levels().await?;

        let users = room.users_with_power_levels().await;
        // Also add users who have the default power level but are members
        // To avoid listing thousands of users in large rooms, maybe we only list members if the room is small?
        // Actually, let's just use what's in the power levels event first.
        // If the user wants to promote someone else, they can search for them.
        Ok((power_levels.users_default.into(), users))
    }

    pub async fn update_user_power_level(
        &self,
        room_id: &str,
        user_id: &str,
        level: i64,
    ) -> Result<()> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let user_id_parsed = matrix_sdk::ruma::UserId::parse(user_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;

        let int_level = matrix_sdk::ruma::Int::new(level)
            .ok_or_else(|| anyhow::anyhow!("Invalid power level"))?;
        room.update_power_levels(vec![(&user_id_parsed, int_level)])
            .await?;
        Ok(())
    }

    pub async fn update_room_power_level_settings(
        &self,
        room_id: &str,
        powers: RoomPowerLevelChanges,
    ) -> Result<()> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;

        let mut changes = RoomPowerLevelChanges::new();
        changes.ban = powers.ban;
        changes.invite = powers.invite;
        changes.kick = powers.kick;
        changes.redact = powers.redact;
        changes.events_default = powers.events_default;
        changes.room_name = powers.room_name;
        changes.room_topic = powers.room_topic;
        changes.room_avatar = powers.room_avatar;

        room.apply_power_level_changes(changes).await?;
        Ok(())
    }

    pub async fn invite_user(&self, room_id: &str, user_id: &str) -> Result<()> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let user_id_parsed = matrix_sdk::ruma::UserId::parse(user_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;
        room.invite_user_by_id(&user_id_parsed).await?;
        Ok(())
    }

    pub async fn kick_user(
        &self,
        room_id: &str,
        user_id: &str,
        reason: Option<String>,
    ) -> Result<()> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let user_id_parsed = matrix_sdk::ruma::UserId::parse(user_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;
        room.kick_user(&user_id_parsed, reason.as_deref()).await?;
        Ok(())
    }

    pub async fn ban_user(
        &self,
        room_id: &str,
        user_id: &str,
        reason: Option<String>,
    ) -> Result<()> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let user_id_parsed = matrix_sdk::ruma::UserId::parse(user_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;
        room.ban_user(&user_id_parsed, reason.as_deref()).await?;
        Ok(())
    }

    pub async fn join_room(&self, room_id: &RoomId) -> Result<()> {
        let client = self.client().await;
        if let Some(room) = client.get_room(room_id) {
            room.join().await?;
        } else {
            // If the room is unknown, try joining by ID directly
            client.join_room_by_id(room_id).await?;
        }
        Ok(())
    }

    /// Resolve a room alias to its canonical room ID via the homeserver.
    /// Used when opening a permalink that targets a room by alias
    /// (`#room:server`) rather than an ID.
    pub async fn resolve_room_alias(
        &self,
        alias: &matrix_sdk::ruma::RoomAliasId,
    ) -> Result<OwnedRoomId> {
        let client = self.client().await;
        let response = client.resolve_room_alias(alias).await?;
        Ok(response.room_id)
    }

    pub async fn get_room_members(&self, room_id: &str) -> Result<Vec<RoomMemberInfo>> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;
        let members = room.members(matrix_sdk::RoomMemberships::ACTIVE).await?;
        let mut member_infos = Vec::new();
        for m in members {
            member_infos.push(RoomMemberInfo {
                user_id: m.user_id().to_string(),
                display_name: m.display_name().map(|s| s.to_string()),
                avatar_url: m.avatar_url().map(|u| u.to_string()),
            });
        }
        Ok(member_infos)
    }

    pub async fn get_pinned_events(
        &self,
        room_id: &str,
    ) -> Result<Vec<matrix_sdk::ruma::OwnedEventId>> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;
        let pinned = room
            .get_state_event_static::<matrix_sdk::ruma::events::room::pinned_events::RoomPinnedEventsEventContent>()
            .await
            .ok()
            .flatten()
            .and_then(|e| e.deserialize().ok())
            .and_then(|ev| match ev {
                matrix_sdk_base::deserialized_responses::SyncOrStrippedState::Sync(
                    matrix_sdk::ruma::events::SyncStateEvent::Original(ev),
                ) => Some(ev.content.pinned),
                matrix_sdk_base::deserialized_responses::SyncOrStrippedState::Stripped(
                    ev,
                ) => ev.content.pinned,
                _ => None,
            })
            .unwrap_or_default();
        Ok(pinned)
    }

    pub async fn fetch_pinned_event_details(
        &self,
        room_id: &str,
        event_id: &matrix_sdk::ruma::EventId,
    ) -> Result<PinnedEventInfo> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;

        let timeline_event = room.event(event_id, None).await?;
        let (sender_id, origin_server_ts, body) = match timeline_event.kind {
            matrix_sdk::deserialized_responses::TimelineEventKind::Decrypted(decrypted) => {
                let ev = decrypted.event.deserialize()?;
                let sender = ev.sender().to_owned();
                let ts = ev.origin_server_ts();
                let body = match &ev {
                    matrix_sdk::ruma::events::AnyTimelineEvent::MessageLike(msg) => match msg {
                        matrix_sdk::ruma::events::AnyMessageLikeEvent::RoomMessage(
                            matrix_sdk::ruma::events::MessageLikeEvent::Original(
                                matrix_sdk::ruma::events::OriginalMessageLikeEvent {
                                    content, ..
                                },
                            ),
                        ) => content.body().to_string(),
                        _ => "Unsupported message event type".to_string(),
                    },
                    _ => "Unsupported state event type".to_string(),
                };
                (sender, ts, body)
            }
            matrix_sdk::deserialized_responses::TimelineEventKind::UnableToDecrypt {
                event,
                ..
            } => {
                let ev = event.deserialize()?;
                let sender = ev.sender().to_owned();
                let ts = ev.origin_server_ts();
                let body = match &ev {
                    matrix_sdk::ruma::events::AnySyncTimelineEvent::MessageLike(msg) => match msg {
                        matrix_sdk::ruma::events::AnySyncMessageLikeEvent::RoomMessage(
                            matrix_sdk::ruma::events::SyncMessageLikeEvent::Original(
                                matrix_sdk::ruma::events::OriginalSyncMessageLikeEvent {
                                    content,
                                    ..
                                },
                            ),
                        ) => content.body().to_string(),
                        _ => "Unsupported message event type".to_string(),
                    },
                    _ => "Unsupported state event type".to_string(),
                };
                (sender, ts, body)
            }
            matrix_sdk::deserialized_responses::TimelineEventKind::PlainText { event, .. } => {
                let ev = event.deserialize()?;
                let sender = ev.sender().to_owned();
                let ts = ev.origin_server_ts();
                let body = match &ev {
                    matrix_sdk::ruma::events::AnySyncTimelineEvent::MessageLike(msg) => match msg {
                        matrix_sdk::ruma::events::AnySyncMessageLikeEvent::RoomMessage(
                            matrix_sdk::ruma::events::SyncMessageLikeEvent::Original(
                                matrix_sdk::ruma::events::OriginalSyncMessageLikeEvent {
                                    content,
                                    ..
                                },
                            ),
                        ) => content.body().to_string(),
                        _ => "Unsupported message event type".to_string(),
                    },
                    _ => "Unsupported state event type".to_string(),
                };
                (sender, ts, body)
            }
        };

        let ts_millis = u64::from(origin_server_ts.0);
        let datetime =
            chrono::DateTime::from_timestamp_millis(ts_millis as i64).unwrap_or_default();
        let timestamp = datetime
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        // Fetch sender member profile details for name and avatar
        let (sender_name, avatar_url) = if let Ok(Some(member)) = room.get_member(&sender_id).await
        {
            (
                member
                    .display_name()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| sender_id.to_string()),
                member.avatar_url().map(|u| u.to_string()),
            )
        } else {
            (sender_id.to_string(), None)
        };

        Ok(PinnedEventInfo {
            event_id: event_id.to_string(),
            sender_id: sender_id.to_string(),
            sender_name,
            avatar_url,
            timestamp,
            body,
        })
    }

    /// Replaces the room's `m.room.pinned_events` state with the given list.
    pub async fn set_pinned_events(
        &self,
        room_id: &str,
        pinned: Vec<matrix_sdk::ruma::OwnedEventId>,
    ) -> Result<()> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;
        use matrix_sdk::ruma::events::room::pinned_events::RoomPinnedEventsEventContent;
        let content = RoomPinnedEventsEventContent::new(pinned);
        room.send_state_event(content).await?;
        Ok(())
    }

    pub async fn get_space_children(&self, space_id: &str) -> Result<Vec<RoomData>> {
        let space_id_parsed = RoomId::parse(space_id)?;
        let client = self.client().await;

        let space = client
            .get_room(&space_id_parsed)
            .context("Space not found")?;

        // Fetch m.space.child events to get definitive orders
        let children_events = space
            .get_state_events_static::<SpaceChildEventContent>()
            .await?;

        let mut child_data = HashMap::new();
        for event in children_events {
            if let Ok(event) = event.deserialize() {
                match event {
                    matrix_sdk_base::deserialized_responses::SyncOrStrippedState::Sync(
                        matrix_sdk::ruma::events::SyncStateEvent::Original(ev),
                    ) => {
                        if !ev.content.via.is_empty()
                            && let Ok(cid) = RoomId::parse(ev.state_key.as_str())
                        {
                            child_data.insert(
                                cid,
                                ChildData {
                                    order: ev.content.order.as_ref().map(|o| o.to_string()),
                                    suggested: ev.content.suggested,
                                },
                            );
                        }
                    }
                    matrix_sdk_base::deserialized_responses::SyncOrStrippedState::Stripped(ev) => {
                        if !ev
                            .content
                            .via
                            .as_ref()
                            .map(|v| v.is_empty())
                            .unwrap_or(true)
                            && let Ok(cid) = RoomId::parse(ev.state_key.as_str())
                        {
                            child_data.insert(
                                cid,
                                ChildData {
                                    order: ev.content.order.as_ref().map(|o| o.to_string()),
                                    suggested: ev.content.suggested,
                                },
                            );
                        }
                    }
                    _ => {}
                }
            }
        }

        // Use the hierarchy API to get rich metadata for all rooms in the space
        let mut rooms = Vec::new();
        let mut request = matrix_sdk::ruma::api::client::space::get_hierarchy::v1::Request::new(
            space_id_parsed.clone(),
        );
        request.limit = Some(matrix_sdk::ruma::UInt::new(100).unwrap());

        if let Ok(response) = client.send(request).await {
            let mut inner = self.inner.write().await;
            for room_summary in response.rooms {
                let is_space = room_summary
                    .summary
                    .room_type
                    .as_ref()
                    .map(|t| t == &RoomType::Space)
                    .unwrap_or(false);

                let (order, suggested) = child_data
                    .get(&room_summary.summary.room_id)
                    .map(|d| (d.order.clone(), d.suggested))
                    .unwrap_or((None, false));

                // Update local hierarchy knowledge
                inner.space_hierarchy.add_child(
                    space_id_parsed.clone(),
                    room_summary.summary.room_id.clone(),
                    order.clone(),
                    suggested,
                );

                let (join_rule, allowed_spaces) = (None, Vec::new());

                rooms.push(RoomData {
                    id: room_summary.summary.room_id.as_str().into(),
                    name: room_summary.summary.name.clone(),
                    last_message: None,
                    unread_count: 0,
                    unread_count_str: None,
                    avatar_url: room_summary
                        .summary
                        .avatar_url
                        .as_ref()
                        .map(|u| u.to_string()),
                    room_type: room_summary.summary.room_type.clone(),
                    is_space,
                    parent_space_id: Some(space_id.to_string()),
                    join_rule,
                    allowed_spaces,
                    order,
                    suggested,
                });
            }
        } else {
            // Fallback to state events if hierarchy API fails
            for (child_id_parsed, data) in child_data {
                {
                    let mut inner = self.inner.write().await;
                    inner.space_hierarchy.add_child(
                        space_id_parsed.clone(),
                        child_id_parsed.clone(),
                        data.order.clone(),
                        data.suggested,
                    );
                }

                if let Some(child_room) = client.get_room(&child_id_parsed) {
                    rooms.push(self.fetch_room_data(&child_room).await?);
                } else {
                    rooms.push(RoomData {
                        id: child_id_parsed.as_str().into(),
                        name: None,
                        last_message: None,
                        unread_count: 0,
                        unread_count_str: None,
                        avatar_url: None,
                        room_type: None,
                        is_space: false,
                        parent_space_id: Some(space_id.to_string()),
                        join_rule: None,
                        allowed_spaces: Vec::new(),
                        order: data.order,
                        suggested: data.suggested,
                    });
                }
            }
        }
        Ok(rooms)
    }

    pub async fn add_space_child(
        &self,
        space_id: &str,
        child_id: &str,
        order: Option<String>,
        suggested: bool,
    ) -> Result<()> {
        let space_id_parsed = RoomId::parse(space_id)?;
        let child_id_parsed = RoomId::parse(child_id)?;
        let client = self.client().await;
        let space = client
            .get_room(&space_id_parsed)
            .context("Space not found")?;

        use matrix_sdk::ruma::events::space::child::SpaceChildEventContent;
        let mut via = Vec::new();
        if let Some(server) = client
            .user_id()
            .and_then(|id| id.server_name().to_owned().into())
        {
            via.push(server);
        }

        let mut content = SpaceChildEventContent::new(via);
        content.order = order.map(|o| matrix_sdk::ruma::OwnedSpaceChildOrder::try_from(o).unwrap());
        content.suggested = suggested;
        space
            .send_state_event_for_key(&child_id_parsed, content)
            .await?;
        Ok(())
    }

    pub async fn remove_space_child(&self, space_id: &str, child_id: &str) -> Result<()> {
        let space_id_parsed = RoomId::parse(space_id)?;
        let child_id_parsed = RoomId::parse(child_id)?;
        let client = self.client().await;
        let space = client
            .get_room(&space_id_parsed)
            .context("Space not found")?;

        // To remove, send an empty via list
        use matrix_sdk::ruma::events::space::child::SpaceChildEventContent;
        let content = SpaceChildEventContent::new(Vec::new());
        space
            .send_state_event_for_key(&child_id_parsed, content)
            .await?;
        Ok(())
    }

    pub async fn send_message(
        &self,
        room_id: &str,
        body: String,
        html_body: Option<String>,
    ) -> Result<()> {
        let room_id = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id).context("Room not found")?;

        let content = if let Some(html) = html_body {
            RoomMessageEventContent::text_html(body, html)
        } else {
            RoomMessageEventContent::text_plain(body)
        };

        room.send(content).await?;
        Ok(())
    }

    pub async fn send_location(&self, room_id: &str, body: String, geo_uri: String) -> Result<()> {
        let room_id = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id).context("Room not found")?;

        use matrix_sdk::ruma::events::room::message::{LocationMessageEventContent, MessageType};

        let content = RoomMessageEventContent::new(MessageType::Location(
            LocationMessageEventContent::new(body, geo_uri),
        ));

        room.send(content).await?;
        Ok(())
    }

    pub async fn send_reply(
        &self,
        room_id: &str,
        reply_to_event_id: &matrix_sdk::ruma::EventId,
        reply_to_sender: &matrix_sdk::ruma::UserId,
        body: String,
        html_body: Option<String>,
    ) -> Result<()> {
        let room_id = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id).context("Room not found")?;

        let content = if let Some(html) = html_body {
            RoomMessageEventContent::text_html(body, html)
        } else {
            RoomMessageEventContent::text_plain(body)
        };

        let reply = content.make_for_thread(
            matrix_sdk::ruma::events::room::message::ReplyMetadata::new(
                reply_to_event_id,
                reply_to_sender,
                None,
            ),
            matrix_sdk::ruma::events::room::message::ReplyWithinThread::No,
            matrix_sdk::ruma::events::room::message::AddMentions::Yes,
        );

        room.send(reply).await?;
        Ok(())
    }

    pub async fn send_threaded_message(
        &self,
        room_id: &str,
        root_event_id: &matrix_sdk::ruma::EventId,
        sender: Option<&String>,
        body: String,
        html_body: Option<String>,
    ) -> Result<()> {
        let room_id = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id).context("Room not found")?;

        let content = if let Some(html) = html_body {
            RoomMessageEventContent::text_html(body, html)
        } else {
            RoomMessageEventContent::text_plain(body)
        };

        let sender_id = if let Some(s) = sender {
            UserId::parse(s)?
        } else {
            client.user_id().context("No user id")?.to_owned()
        };

        let threaded_message = content.make_for_thread(
            matrix_sdk::ruma::events::room::message::ReplyMetadata::new(
                root_event_id,
                &sender_id,
                None,
            ),
            matrix_sdk::ruma::events::room::message::ReplyWithinThread::Yes,
            matrix_sdk::ruma::events::room::message::AddMentions::Yes,
        );

        room.send(threaded_message).await?;
        Ok(())
    }

    pub async fn send_attachment(&self, room_id: &str, path: &std::path::PathBuf) -> Result<()> {
        let room_id = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id).context("Room not found")?;

        let data = tokio::fs::read(path).await?;
        let filename = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let mime_type = mime_guess::from_path(path).first_or_octet_stream();
        let config = matrix_sdk::attachment::AttachmentConfig::new();

        room.send_attachment(&filename, &mime_type, data, config)
            .await?;
        Ok(())
    }

    pub async fn edit_message(
        &self,
        room_id: &str,
        item_id: &TimelineEventItemId,
        body: String,
        html_body: Option<String>,
    ) -> Result<()> {
        let timeline = self.timeline(room_id).await?;
        let content = if let Some(html) = html_body {
            RoomMessageEventContent::text_html(body, html)
        } else {
            RoomMessageEventContent::text_plain(body)
        };
        timeline
            .edit(item_id, EditedContent::RoomMessage(content.into()))
            .await?;
        Ok(())
    }

    pub async fn redact_message(
        &self,
        room_id: &str,
        item_id: &TimelineEventItemId,
        reason: Option<String>,
    ) -> Result<()> {
        let timeline = self.timeline(room_id).await?;
        timeline.redact(item_id, reason.as_deref()).await?;
        Ok(())
    }

    pub async fn typing_notice(&self, room_id: &str, typing: bool) -> Result<()> {
        let room_id = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id).context("Room not found")?;
        room.typing_notice(typing).await?;
        Ok(())
    }

    pub async fn toggle_reaction(
        &self,
        room_id: &str,
        item_id: &TimelineEventItemId,
        reaction_key: &str,
    ) -> Result<()> {
        let timeline = self.timeline(room_id).await?;
        timeline.toggle_reaction(item_id, reaction_key).await?;
        Ok(())
    }

    pub async fn fetch_media(&self, source: MediaSource) -> Result<Vec<u8>> {
        let client = self.client().await;
        let request = matrix_sdk::media::MediaRequestParameters {
            source,
            format: MediaFormat::File,
        };
        let content = client.media().get_media_content(&request, true).await?;
        Ok(content)
    }

    pub async fn search_public_rooms(
        &self,
        query: String,
        limit: Option<u32>,
    ) -> Result<Vec<PublicRoom>> {
        let client = {
            let inner = self.inner.read().await;
            inner.client.clone()
        };

        let mut filter = matrix_sdk::ruma::directory::Filter::new();
        if !query.is_empty() {
            filter.generic_search_term = Some(query);
        }

        let mut request =
            matrix_sdk::ruma::api::client::directory::get_public_rooms_filtered::v3::Request::new();
        request.limit = limit.map(|l| l.into());
        request.filter = filter;

        let response = client.public_rooms_filtered(request).await?;

        let rooms = response
            .chunk
            .into_iter()
            .map(|chunk| {
                let is_space = chunk
                    .room_type
                    .as_ref()
                    .map(|t| t == &matrix_sdk::ruma::room::RoomType::Space)
                    .unwrap_or(false);
                PublicRoom {
                    id: chunk.room_id.to_string(),
                    name: chunk.name,
                    topic: chunk.topic,
                    canonical_alias: chunk.canonical_alias.map(|a| a.to_string()),
                    num_joined_members: u64::from(chunk.num_joined_members) as u32,
                    avatar_url: chunk.avatar_url.map(|u| u.to_string()),
                    is_space,
                }
            })
            .collect();

        Ok(rooms)
    }

    pub async fn create_room(&self, name: &str, is_video: bool) -> Result<OwnedRoomId> {
        let client = self.client().await;
        let mut request = matrix_sdk::ruma::api::client::room::create_room::v3::Request::new();
        request.name = Some(name.to_string());
        if is_video {
            let mut creation_content =
                matrix_sdk::ruma::api::client::room::create_room::v3::CreationContent::new();
            creation_content.room_type = Some(matrix_sdk::ruma::room::RoomType::from(
                "org.matrix.msc3401.call.room",
            ));
            request.creation_content = Some(matrix_sdk::ruma::serde::Raw::new(&creation_content)?);
        }
        let room = client.create_room(request).await?;
        Ok(room.room_id().to_owned())
    }

    pub async fn create_space(&self, name: &str) -> Result<OwnedRoomId> {
        let client = self.client().await;
        let mut request = matrix_sdk::ruma::api::client::room::create_room::v3::Request::new();
        request.name = Some(name.to_string());

        let mut creation_content =
            matrix_sdk::ruma::api::client::room::create_room::v3::CreationContent::new();
        creation_content.room_type = Some(RoomType::Space);
        request.creation_content = Some(matrix_sdk::ruma::serde::Raw::new(&creation_content)?);

        let room = client.create_room(request).await?;
        Ok(room.room_id().to_owned())
    }

    pub async fn is_in_space(&self, room_id: &RoomId, space_id: &RoomId) -> bool {
        let inner = self.inner.read().await;
        inner.space_hierarchy.is_in_space(room_id, space_id)
    }

    pub fn is_in_space_sync(&self, room_id: &RoomId, space_id: &RoomId) -> bool {
        match self.inner.try_read() {
            Ok(inner) => inner.space_hierarchy.is_in_space(room_id, space_id),
            Err(_) => {
                // If we can't get a read lock, we fall back to assuming it might be in the space
                // if we're currently selecting it, to avoid flickering.
                // But we don't have access to selected_space here.
                // For now, just return false but log it.
                false
            }
        }
    }

    pub fn filter_in_space_bulk_sync<'a, I, F, T>(
        &self,
        rooms: I,
        space_id: &RoomId,
        out: &mut Vec<T>,
        mut filter_by_search: F,
    ) -> bool
    where
        I: Iterator<Item = (T, &'a RoomData)>,
        F: FnMut(&RoomData) -> bool,
    {
        match self.inner.try_read() {
            Ok(inner) => {
                out.clear();
                // Bolt Optimization: Calculate all space descendants once (O(S))
                // to avoid O(N) string parsing and O(N * D) tree traversals.
                let mut descendants = inner.space_hierarchy.get_descendants_strs(space_id);
                // Also include the space itself so direct children match.
                descendants.insert(space_id.as_str());

                for (val, room) in rooms {
                    if descendants.contains(&*room.id) && filter_by_search(room) {
                        out.push(val);
                    }
                }
                true
            }
            Err(_) => {
                // If we can't get a read lock, fallback to not filtering correctly
                // or returning nothing. Usually this is transient.
                false
            }
        }
    }

    pub async fn login_oidc(&self, homeserver: &str) -> Result<Url> {
        let homeserver_url = if homeserver.starts_with("https://")
            || homeserver.starts_with("http://localhost")
            || homeserver.starts_with("http://127.0.0.1")
            || homeserver.starts_with("http://[::1]")
        {
            homeserver.to_string()
        } else {
            let stripped = homeserver.strip_prefix("http://").unwrap_or(homeserver);
            format!("https://{}", stripped)
        };

        let client = {
            let mut inner = self.inner.write().await;
            if let Some(handle) = inner.sync_handle.take() {
                handle.abort();
            }
            if let Some(handle) = inner.session_change_handle.take() {
                handle.abort();
            }
            let data_dir = inner.data_dir.clone();
            let new_client = Self::setup_client(data_dir, &homeserver_url).await?;
            inner.client = new_client.clone();
            new_client
        };

        client
            .oauth()
            .restore_registered_client(matrix_sdk::authentication::oauth::ClientId::new(
                OIDC_CLIENT_ID.to_string(),
            ));

        let redirect_uri = Url::parse(OIDC_CALLBACK_URL)?;
        let login_url = client
            .oauth()
            .login(redirect_uri, None, None, None)
            .build()
            .await?
            .url;

        let mut inner = self.inner.write().await;
        inner.oidc_client = Some(client);

        Ok(login_url)
    }

    pub async fn complete_oidc_login(&self, callback_url: Url) -> Result<()> {
        let client = {
            let mut inner = self.inner.write().await;
            inner
                .oidc_client
                .take()
                .context("No OIDC login in progress")?
        };

        client
            .oauth()
            .finish_login(callback_url.into())
            .await
            .context("Failed to complete OIDC login")?;

        let sync_service: Arc<SyncService> =
            Arc::new(SyncService::builder(client.clone()).build().await?);
        let room_list_service = sync_service.room_list_service();

        self.setup_event_handlers(&client);

        // Save session to oo7
        if let Some(session) = client.oauth().user_session() {
            let session_data = SessionData {
                homeserver: client.homeserver().to_string(),
                user_id: session.meta.user_id.to_string(),
                access_token: session.tokens.access_token.to_string(),
                refresh_token: session.tokens.refresh_token.clone(),
                id_token: None,
                device_id: session.meta.device_id.to_string(),
                is_oidc: true,
            };

            Self::save_session_to_keyring(&session_data).await?;
        }

        let mut inner = self.inner.write().await;
        inner.client = client.clone();
        inner.sync_service = Some(sync_service);
        inner.room_list_service = Some(room_list_service);

        drop(inner);
        self.spawn_session_change_handler(client).await;

        Ok(())
    }

    pub(crate) async fn get_or_create_store_passphrase() -> Result<String> {
        let use_fallback = || {
            let path = Self::get_fallback_path("store-passphrase");
            if path.exists()
                && let Ok(passphrase) = std::fs::read_to_string(&path)
            {
                return Ok(passphrase);
            }
            let mut buf = [0u8; 32];
            SysRng
                .try_fill_bytes(&mut buf)
                .context("Failed to generate secure random bytes for store passphrase")?;
            let passphrase: String = buf.iter().map(|b| format!("{:02x}", b)).collect();
            let _ = Self::write_secret_file(&path, passphrase.as_bytes());
            Ok(passphrase)
        };

        let keyring = match if Self::should_bypass_keyring() {
            Err(anyhow::anyhow!("Bypassing keyring in test"))
        } else {
            Keyring::new().await.map_err(|e| e.into())
        } {
            Ok(k) => k,
            Err(e) => {
                if std::env::var("CONSTELLATIONS_DISABLE_FALLBACK").is_ok() {
                    return Err(e);
                }
                tracing::warn!(
                    "Failed to initialize Keyring: {}. Falling back to file-based passphrase storage.",
                    e
                );
                return use_fallback();
            }
        };

        let mut attributes = HashMap::new();
        attributes.insert("app_id", "fi.joonastuomi.Constellations");
        attributes.insert("type", "store-passphrase");

        match keyring.search_items(&attributes).await {
            Ok(items) => {
                if let Some(item) = items.first()
                    && let Ok(secret) = item.secret().await
                    && let Ok(passphrase) = String::from_utf8(secret.to_vec())
                {
                    return Ok(passphrase);
                }
            }
            Err(e) => {
                if std::env::var("CONSTELLATIONS_DISABLE_FALLBACK").is_ok() {
                    return Err(e.into());
                }
                tracing::warn!(
                    "Failed to search items in Keyring: {}. Falling back to file-based passphrase storage.",
                    e
                );
                return use_fallback();
            }
        }

        // Before generating a new passphrase, check if a fallback file already exists
        // to keep it stable across calls in environments where keyring is broken/partially functional.
        let path = Self::get_fallback_path("store-passphrase");
        if path.exists()
            && let Ok(passphrase) = std::fs::read_to_string(&path)
        {
            // Try to write it to keyring in case it is now writable
            let _ = keyring
                .create_item(
                    "Constellations Store Passphrase",
                    &attributes,
                    passphrase.as_bytes(),
                    true,
                )
                .await;
            return Ok(passphrase);
        }

        let mut buf = [0u8; 32];
        SysRng
            .try_fill_bytes(&mut buf)
            .context("Failed to generate secure random bytes for store passphrase")?;

        let passphrase: String = buf.iter().map(|b| format!("{:02x}", b)).collect();

        match keyring
            .create_item(
                "Constellations Store Passphrase",
                &attributes,
                passphrase.as_bytes(),
                true,
            )
            .await
        {
            Ok(_) => Ok(passphrase),
            Err(e) => {
                if std::env::var("CONSTELLATIONS_DISABLE_FALLBACK").is_ok() {
                    return Err(e.into());
                }
                tracing::warn!(
                    "Failed to create item in Keyring: {}. Falling back to file-based passphrase storage.",
                    e
                );
                let _ = Self::write_secret_file(&path, passphrase.as_bytes());
                Ok(passphrase)
            }
        }
    }

    pub async fn search_in_room(
        &self,
        room_id: &RoomId,
        query: &str,
        max_results: usize,
    ) -> Result<Vec<matrix_sdk::ruma::OwnedEventId>> {
        let inner = self.inner.read().await;
        let room = inner.client.get_room(room_id).context("Room not found")?;

        let results = room
            .search(query, max_results, None)
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
        Ok(results)
    }

    pub async fn ignored_users(&self) -> Result<Vec<matrix_sdk::ruma::OwnedUserId>> {
        let client = self.client().await;
        let ignored = client
            .account()
            .account_data::<IgnoredUserListEventContent>()
            .await?;
        let mut users = Vec::new();
        if let Some(content) = ignored {
            let content = content.deserialize()?;
            for user_id in content.ignored_users.keys() {
                users.push(user_id.clone());
            }
        }
        Ok(users)
    }

    pub async fn ignore_user(&self, user_id: &UserId) -> Result<()> {
        let client = self.client().await;
        client.account().ignore_user(user_id).await?;
        Ok(())
    }

    pub async fn unignore_user(&self, user_id: &UserId) -> Result<()> {
        let client = self.client().await;
        client.account().unignore_user(user_id).await?;
        Ok(())
    }

    pub async fn is_user_ignored(&self, user_id: &UserId) -> Result<bool> {
        let client = self.client().await;
        let ignored = client
            .account()
            .account_data::<IgnoredUserListEventContent>()
            .await?;
        if let Some(content) = ignored {
            let content = content.deserialize()?;
            Ok(content.ignored_users.contains_key(user_id))
        } else {
            Ok(false)
        }
    }

    pub async fn get_livekit_token(&self, room_id: &RoomId) -> Result<(String, String)> {
        let client = self.client().await;

        // 1. Get OpenID token
        use matrix_sdk::ruma::api::client::account::request_openid_token;
        let user_id = client.user_id().context("No user ID")?.to_owned();
        let device_id = client.device_id().context("No device ID")?.to_string();
        let request = request_openid_token::v3::Request::new(user_id.clone());
        let openid_token = client.send(request).await?;

        // 2. Discover LiveKit service
        let homeserver = client.homeserver();
        let well_known_url = homeserver.join("/.well-known/matrix/client")?;

        let wk: LiveKitWellKnown = reqwest::get(well_known_url).await?.json().await?;

        let focus = wk
            .rtc_foci
            .iter()
            .find(|f| f.focus_type == "livekit")
            .context("No MatrixRTC configuration found")?;

        // 3. Exchange OpenID for LiveKit JWT
        let mut auth_url = Url::parse(&focus.livekit_service_url)?;
        if auth_url.path().is_empty() || auth_url.path() == "/" {
            auth_url = auth_url.join("get_token")?;
        }
        info!("Sending auth request to: {}", auth_url);

        let member_id = format!("{:016x}", rand::random::<u64>());

        let response = reqwest::Client::new()
            .post(auth_url)
            .json(&serde_json::json!({
                "room_id": room_id,
                "slot_id": "m.call#ROOM",
                "openid_token": {
                    "access_token": openid_token.access_token,
                    "expires_in": openid_token.expires_in.as_secs(),
                    "matrix_server_name": openid_token.matrix_server_name,
                    "token_type": openid_token.token_type,
                },
                "member": {
                    "id": member_id,
                    "claimed_user_id": user_id,
                    "claimed_device_id": device_id,
                }
            }))
            .send()
            .await?;

        let status = response.status();
        let body_text = response.text().await?;
        info!("Auth service response ({}): {}", status, body_text);

        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "Auth service returned error {}: {}",
                status,
                body_text
            ));
        }

        let response: LiveKitAuthResponse = serde_json::from_str(&body_text)?;

        let livekit_url = response
            .livekit_url
            .context("No LiveKit URL found in auth response")?;

        Ok((livekit_url, response.token))
    }

    pub async fn join_call(&self, room_id: &str) -> Result<()> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;
        let user_id = client.user_id().context("No user ID")?.to_owned();
        let device_id = client.device_id().context("No device ID")?.to_owned();

        use matrix_sdk::ruma::events::call::member::{
            ActiveFocus, ActiveLivekitFocus, Application, CallApplicationContent,
            CallMemberEventContent, CallMemberStateKey, CallScope,
        };

        let application =
            Application::Call(CallApplicationContent::new("".to_string(), CallScope::Room));
        let focus_active = ActiveFocus::Livekit(ActiveLivekitFocus::new());
        let foci_preferred = Vec::new();

        let content = CallMemberEventContent::new(
            application,
            device_id,
            focus_active,
            foci_preferred,
            None,
            None,
        );

        let state_key = CallMemberStateKey::new(user_id, None, false);
        room.send_state_event_for_key(&state_key, content).await?;

        // Connect to LiveKit
        let (sfu_url, token) = self.get_livekit_token(&room_id_parsed).await?;

        let (lk_room, mut room_events) =
            livekit::Room::connect(&sfu_url, &token, RoomOptions::default()).await?;

        let lk_room = Arc::new(lk_room);

        let mut inner = self.inner.write().await;
        inner.active_call = Some(lk_room.clone());
        drop(inner);

        tokio::spawn(async move {
            while let Some(event) = room_events.recv().await {
                if let RoomEvent::TrackSubscribed {
                    track,
                    publication: _,
                    participant: _,
                } = event
                {
                    info!("Track subscribed: {:?}", track.sid());
                }
            }
        });

        Ok(())
    }

    pub async fn leave_call(&self, room_id: &str) -> Result<()> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;
        let user_id = client.user_id().context("No user ID")?.to_owned();

        use matrix_sdk::ruma::events::call::member::{CallMemberEventContent, CallMemberStateKey};
        let content = CallMemberEventContent::new_empty(None);
        let state_key = CallMemberStateKey::new(user_id, None, false);
        room.send_state_event_for_key(&state_key, content).await?;

        let mut inner = self.inner.write().await;
        if let Some(lk_room) = inner.active_call.take() {
            lk_room.close().await?;
        }

        Ok(())
    }

    pub async fn get_call_participants(&self, room_id: &str) -> Vec<matrix_sdk::ruma::OwnedUserId> {
        if let Ok(room_id_parsed) = RoomId::parse(room_id) {
            let inner = self.inner.read().await;
            inner
                .call_participants
                .get(&room_id_parsed)
                .map(|p| p.iter().cloned().collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    async fn setup_client(data_dir: PathBuf, homeserver_url: &str) -> Result<Client> {
        let store_path = data_dir.join("matrix-store");
        let search_index_path = data_dir.join("search-index");

        if !data_dir.exists() {
            std::fs::create_dir_all(&data_dir)?;
        }

        if !store_path.exists() && search_index_path.exists() {
            tracing::info!(
                "Fresh SQLite store, clearing existing search index path to prevent mismatched keys."
            );
            let _ = std::fs::remove_dir_all(&search_index_path);
        }

        let passphrase = Self::get_or_create_store_passphrase().await?;

        let mut key_mismatch = false;
        if search_index_path.exists()
            && let Ok(entries) = std::fs::read_dir(&search_index_path)
        {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    let key_path = entry.path().join("seshat-index.key");
                    if key_path.exists()
                        && let Ok(bytes) = std::fs::read(&key_path)
                        && matrix_sdk_store_encryption::StoreCipher::import(&passphrase, &bytes)
                            .is_err()
                    {
                        tracing::warn!(
                            "Mismatched search index encryption key in room {:?}. Clearing search index.",
                            entry.file_name()
                        );
                        key_mismatch = true;
                        break;
                    }
                }
            }
        }

        if key_mismatch {
            let _ = std::fs::remove_dir_all(&search_index_path);
        }

        let build_client = |path: PathBuf, search_path: PathBuf, pass: String| {
            Client::builder()
                .homeserver_url(homeserver_url)
                .sqlite_store(path, Some(&pass))
                .search_index_store(
                    matrix_sdk::search_index::SearchIndexStoreKind::EncryptedDirectory(
                        search_path,
                        pass,
                    ),
                )
                .handle_refresh_tokens()
        };

        let client = match build_client(
            store_path.clone(),
            search_index_path.clone(),
            passphrase.clone(),
        )
        .build()
        .await
        {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    "Failed to initialize stores (possibly corrupted cipher): {}. Recreating store.",
                    e
                );
                let _ = std::fs::remove_dir_all(&store_path);
                let _ = std::fs::remove_dir_all(&search_index_path);
                build_client(store_path, search_index_path, passphrase)
                    .build()
                    .await?
            }
        };

        if let Some(machine) = client.olm_machine_for_testing().await.as_ref() {
            machine.set_room_key_requests_enabled(true);
            machine.set_room_key_forwarding_enabled(true);
        }

        Ok(client)
    }
}

pub fn markdown_to_html(markdown: &str) -> String {
    let mut options = pulldown_cmark::Options::empty();
    options.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    let parser = pulldown_cmark::Parser::new_ext(markdown, options);

    let mut html_output = String::new();
    pulldown_cmark::html::push_html(&mut html_output, parser);

    html_output
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PinnedEventInfo {
    pub event_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub avatar_url: Option<String>,
    pub timestamp: String,
    pub body: String,
}

#[cfg(test)]
mod tests;
