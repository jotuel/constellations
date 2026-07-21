use anyhow::{Context, Result};
use eyeball_im::VectorDiff;
use futures::StreamExt;
use matrix_sdk::authentication::matrix::MatrixSession;
use matrix_sdk::authentication::oauth::qrcode::{
    CheckCodeSender, GeneratedQrProgress, LoginProgress,
};
use matrix_sdk::authentication::oauth::registration::{
    ApplicationType, ClientMetadata, Localized, OAuthGrantType,
};
use matrix_sdk::authentication::oauth::{ClientRegistrationData, OAuthError};
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
use tokio::sync::{RwLock, mpsc};
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

/// OAuth 2.0 redirect URI sent to the homeserver during dynamic client
/// registration. MAS (Matrix Authentication Service) requires the scheme to be
/// lowercase and native custom-scheme redirect URIs to use a single slash
/// (`:/path`, not `://host/path`). The desktop handler registers
/// `x-scheme-handler/fi.joonastuomi.constellations` and catches this regardless
/// of the slash form.
const OIDC_CALLBACK_URL: &str = "fi.joonastuomi.constellations:/callback";
/// Static client ID used as a fallback for sessions saved before the client
/// began using dynamic client registration. Modern logins register with the
/// homeserver and persist the server-assigned client ID in [`SessionData`].
const OIDC_CLIENT_ID: &str = "fi.joonastuomi.Constellations";
/// Home page URL of the client, advertised to the authorization server during
/// OAuth 2.0 dynamic client registration (shown to the user when they authorize
/// the login).
const OIDC_CLIENT_URI: &str = "https://joonastuomi.fi/constellations";

/// Sentinel returned (as an `anyhow::Error` message) by [`MatrixEngine::login_oidc`]
/// when the homeserver doesn't support OAuth 2.0 / OIDC. The login handler
/// recognizes it to show a dedicated message instead of a generic failure.
pub(crate) const OIDC_NOT_SUPPORTED_SENTINEL: &str = "__constellations_oidc_not_supported__";

/// Build the [`ClientRegistrationData`] used for OAuth 2.0 dynamic client
/// registration ([RFC 7591]). The server assigns the client ID during login;
/// we do not assume a pre-registered static ID (which most homeservers —
/// including matrix.org via MAS — do not know).
///
/// [RFC 7591]: https://datatracker.ietf.org/doc/html/rfc7591
fn oauth_registration_data() -> Result<ClientRegistrationData> {
    let metadata = ClientMetadata::new(
        ApplicationType::Native,
        vec![OAuthGrantType::AuthorizationCode {
            redirect_uris: vec![Url::parse(OIDC_CALLBACK_URL)?],
        }],
        Localized::new(Url::parse(OIDC_CLIENT_URI)?, []),
    );
    Ok(ClientRegistrationData::from(
        matrix_sdk::ruma::serde::Raw::new(&metadata)?,
    ))
}

/// Inspect an error from the OIDC login *start* (`login().build()`) and, when
/// the homeserver doesn't support OAuth 2.0 / OIDC at all, replace it with the
/// [`OIDC_NOT_SUPPORTED_SENTINEL`] so the UI can show a targeted message.
/// Anything else is preserved verbatim.
fn classify_oidc_start_error(e: OAuthError) -> anyhow::Error {
    // `login().build()` fails with discovery or registration errors. The
    // "OAuth not supported" case surfaces as `Discovery(NotSupported)`. The
    // `ClientRegistration(NotSupported)` variant (server has no dynamic
    // registration endpoint) is also reported as not-supported.
    let not_supported = match &e {
        OAuthError::Discovery(d) => d.is_not_supported(),
        OAuthError::ClientRegistration(r) => r.to_string().contains("not supported"),
        _ => false,
    };
    if not_supported {
        anyhow::anyhow!(OIDC_NOT_SUPPORTED_SENTINEL)
    } else {
        anyhow::Error::from(e)
    }
}

fn sanitize_homeserver_url(homeserver: &str) -> String {
    let mut url_str = homeserver.to_string();
    if !url_str.contains("://") {
        url_str = format!("https://{}", url_str);
    }

    if let Ok(url) = Url::parse(&url_str) {
        if url.scheme() == "http" {
            #[allow(clippy::collapsible_if)]
            if let Some(host) = url.host_str() {
                if host == "localhost" || host == "127.0.0.1" || host == "[::1]" {
                    return url_str;
                }
            }
            // If it's http and not localhost, we force https
            let mut https_url = url.clone();
            https_url.set_scheme("https").unwrap();
            let _ = https_url.set_username("");
            let _ = https_url.set_password(None);
            url_str = https_url.to_string();
            // Drop trailing slash if the original didn't have a path
            if url_str.ends_with('/') && !homeserver.ends_with('/') && url.path() == "/" {
                url_str.pop();
            }
        }
    } else {
        // Fallback if parsing fails for some reason
        let stripped = homeserver.strip_prefix("http://").unwrap_or(homeserver);
        url_str = format!("https://{}", stripped);
    }

    url_str
}

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
        // Use the full error chain, not just the top-level context, so the
        // real failure (e.g. a TLS or store error hidden behind
        // `.context("Failed to login")`) is surfaced to the user.
        use std::fmt::Write as _;
        let mut s = format!("{e}");
        let mut source = e.source();
        while let Some(cause) = source {
            let _ = write!(s, ": {cause}");
            source = cause.source();
        }
        Self::Anyhow(s)
    }
}

/// Progress events for the QR-code (MSC4108) sign-in flow.
///
/// Produced by [`MatrixEngine::start_qr_login`] and streamed back to the UI so
/// it can render the QR, prompt for the check code, and show progress. The
/// terminal event is [`QrLoginProgress::Finished`].
#[derive(Debug, Clone)]
pub enum QrLoginProgress {
    /// The QR code is ready to display. Carries the raw MSC4108 payload bytes
    /// (a binary `MATRIX…` structure, *not* a URL string).
    QrReady(Vec<u8>),
    /// The other device has scanned the QR. The UI must now prompt the user
    /// for the two-digit check code shown on that device and feed it back via
    /// the [`CheckCodeSender`].
    QrScanned(CheckCodeSender),
    /// Waiting for the existing device to approve the login on the server.
    /// `user_code` may be shown for the user to cross-check.
    WaitingForToken { user_code: String },
    /// Transferring end-to-end encryption secrets from the existing device.
    SyncingSecrets,
    /// Terminal: `Ok(user_id)` on success, `Err(message)` on failure.
    Finished(Result<String, String>),
}

impl QrLoginProgress {
    /// Map an SDK [`LoginProgress<GeneratedQrProgress>`] into our
    /// [`QrLoginProgress`]. `Starting` and `Done` are dropped: `Starting` has
    /// no useful payload, and `Done` is implied by the login future resolving
    /// (which produces the terminal [`QrLoginProgress::Finished`]).
    fn from_sdk(progress: LoginProgress<GeneratedQrProgress>) -> Option<Self> {
        match progress {
            LoginProgress::Starting => None,
            LoginProgress::EstablishingSecureChannel(GeneratedQrProgress::QrReady(data)) => {
                Some(Self::QrReady(data.to_bytes()))
            }
            LoginProgress::EstablishingSecureChannel(GeneratedQrProgress::QrScanned(sender)) => {
                Some(Self::QrScanned(sender))
            }
            LoginProgress::WaitingForToken { user_code } => {
                Some(Self::WaitingForToken { user_code })
            }
            LoginProgress::SyncingSecrets => Some(Self::SyncingSecrets),
            LoginProgress::Done => None,
        }
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
                queue.extend(children.keys().map(|k| &**k));
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
    /// OAuth 2.0 client ID assigned by the homeserver during dynamic client
    /// registration. Absent for password-logins and for OIDC sessions saved
    /// before this field existed; `restore_session` falls back to
    /// [`OIDC_CLIENT_ID`] in that case.
    #[serde(default)]
    client_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct MatrixEngine {
    inner: Arc<RwLock<MatrixEngineInner>>,
}

#[derive(Debug)]
pub enum ActiveSearch {
    Local {
        room_id: matrix_sdk::ruma::OwnedRoomId,
        search_iter: matrix_sdk::message_search::RoomSearchIterator,
    },
    Server {
        query: String,
        room_id: String,
        next_batch: Option<String>,
    },
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
    /// Background task driving a QR-code (MSC4108) login. Held so that
    /// cancellation can abort it cleanly.
    qr_login_handle: Option<tokio::task::JoinHandle<()>>,
    call_participants: HashMap<OwnedRoomId, HashSet<matrix_sdk::ruma::OwnedUserId>>,
    active_call: Option<Arc<livekit::Room>>,
    /// Whether the homeserver supports the server-side `/search` endpoint.
    /// `None` = not yet probed, `Some(true)` = supported, `Some(false)` =
    /// unsupported (404/405) → fall back to local seshat index backfill.
    /// Reset on login/register/restore (new client → new homeserver).
    server_search_supported: Option<bool>,
    active_search: Option<ActiveSearch>,
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
            .field(
                "qr_login_handle",
                &self.qr_login_handle.as_ref().map(|_| "JoinHandle"),
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
            qr_login_handle: None,
            call_participants: HashMap::new(),
            active_call: None,
            server_search_supported: None,
            active_search: None,
        };

        let engine = Self {
            inner: Arc::new(RwLock::new(inner)),
        };
        engine.setup_event_handlers(&client);
        engine.spawn_session_change_handler(client).await;
        Ok(engine)
    }
}

pub fn markdown_to_html(markdown: &str) -> String {
    let mut options = pulldown_cmark::Options::empty();
    options.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    options.insert(pulldown_cmark::Options::ENABLE_TASKLISTS);
    let parser = pulldown_cmark::Parser::new_ext(markdown, options);

    let mut html_output = String::new();
    pulldown_cmark::html::push_html(&mut html_output, parser);

    html_output
}

#[derive(Clone, Debug)]
pub struct MessageSearchResult {
    /// Room the hit originated in. For the in-room search this is the searched
    /// room; for the global search it is each hit's room of origin.
    pub room_id: matrix_sdk::ruma::OwnedRoomId,
    /// Best-effort display name of the originating room. `None` for the in-room
    /// search (the UI already has the room context); populated by global search
    /// so each hit can show its room.
    pub room_name: Option<String>,
    pub event_id: matrix_sdk::ruma::OwnedEventId,
    pub sender_id: matrix_sdk::ruma::OwnedUserId,
    pub body: String,
    pub timestamp: String,
    // Pre-parsed body, so the render loop doesn't re-parse on every frame.
    // Mirrors the pre-compute optimization in `ConstellationsItem::new`.
    pub plain_text: Vec<crate::PreviewEvent>,
    pub links: Vec<(String, String)>,
}

/// Scope for global (cross-room) message search. Mirrors the narrowing options
/// on `matrix_sdk::message_search::GlobalSearchBuilder`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlobalSearchScope {
    /// Search all joined rooms (the default).
    #[default]
    All,
    /// Restrict to direct-message rooms (`only_dm_rooms`).
    DmsOnly,
    /// Restrict to non-DM / group rooms (`no_dms`).
    GroupsOnly,
}

/// Error from the server-side search probe, distinguishing "endpoint not
/// supported" (→ fall back to local index) from real failures.
enum SearchError {
    /// The homeserver returned 404 / 405 for `/search`.
    Unsupported,
    /// Any other error (network, auth, deserialization).
    Other(anyhow::Error),
}

/// Returns true if the error indicates the homeserver doesn't implement the
/// `/search` endpoint. Checks the structured status code first (works for
/// Synapse-style JSON error responses), then falls back to string matching
/// for servers that return a raw 404 without a Matrix error body (Dendrite,
/// Conduit).
fn is_search_unsupported(e: &matrix_sdk::Error) -> bool {
    // Structured path: a Matrix error response with a 404/405 status.
    if let Some(api_err) = e.as_client_api_error()
        && (api_err.status_code == matrix_sdk::ruma::exports::http::StatusCode::NOT_FOUND
            || api_err.status_code
                == matrix_sdk::ruma::exports::http::StatusCode::METHOD_NOT_ALLOWED)
    {
        return true;
    }

    // Fallback: string match for raw HTTP errors without a Matrix body.
    let msg = e.to_string();
    msg.contains("404 Not Found") || msg.contains("405 Method Not Allowed")
}

/// Extracts the display body from a deserialized sync timeline event,
/// returning a placeholder for non-message events.
fn message_body_from_sync_event(ev: &matrix_sdk::ruma::events::AnySyncTimelineEvent) -> String {
    match ev {
        matrix_sdk::ruma::events::AnySyncTimelineEvent::MessageLike(msg) => match msg {
            matrix_sdk::ruma::events::AnySyncMessageLikeEvent::RoomMessage(
                matrix_sdk::ruma::events::SyncMessageLikeEvent::Original(
                    matrix_sdk::ruma::events::OriginalSyncMessageLikeEvent { content, .. },
                ),
            ) => content.body().to_string(),
            _ => "Unsupported message event type".to_string(),
        },
        _ => "Unsupported state event type".to_string(),
    }
}

fn map_timeline_event(
    room_id: matrix_sdk::ruma::OwnedRoomId,
    room_name: Option<String>,
    event: matrix_sdk::deserialized_responses::TimelineEvent,
) -> Result<Option<MessageSearchResult>> {
    let Some(event_id) = event.event_id() else {
        return Ok(None);
    };
    let sender_id = event
        .sender()
        .unwrap_or_else(|| matrix_sdk::ruma::user_id!("@unknown:example.com").to_owned());

    let body = match &event.kind {
        matrix_sdk::deserialized_responses::TimelineEventKind::Decrypted(decrypted) => {
            let ev: matrix_sdk::ruma::events::AnySyncTimelineEvent =
                decrypted.event.deserialize()?.into();
            message_body_from_sync_event(&ev)
        }
        matrix_sdk::deserialized_responses::TimelineEventKind::UnableToDecrypt {
            event, ..
        }
        | matrix_sdk::deserialized_responses::TimelineEventKind::PlainText { event, .. } => {
            message_body_from_sync_event(&event.deserialize()?)
        }
    };

    let timestamp = event
        .timestamp()
        .map(|ts| {
            let ts_millis = u64::from(ts.0);
            chrono::DateTime::from_timestamp_millis(ts_millis as i64)
                .unwrap_or_default()
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_default();

    let plain_text = crate::preview::parse_plain_text(&body);
    let links = crate::preview::extract_links(&plain_text);

    Ok(Some(MessageSearchResult {
        room_id,
        room_name,
        event_id,
        sender_id,
        body,
        timestamp,
        plain_text,
        links,
    }))
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

mod auth;
mod calls;
mod messaging;
mod search;
mod spaces;
mod sync_room_data;
mod timeline_state;

#[cfg(test)]
mod tests;
