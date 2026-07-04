//! Matrix permalink parsing.
//!
//! Pure parser for Matrix room/event/user permalinks, used by the "open link"
//! flow (CLI argument intake, the `fi.joonastuomi.Constellations://` URI scheme,
//! and in-app paste). Produces a [`PermalinkTarget`] with any `via` routing
//! servers and an optional `action`; resolution into actual rooms/timelines
//! happens in the app layer, not here.
//!
//! Supported input forms:
//! - `https://matrix.to/#/{id}` / `https://matrix.to/#/{id}?via=...`
//! - `matrix:/{id}` / `matrix:/{id}?via=...&action=join`
//! - The app's own wrapper scheme:
//!   `fi.joonastuomi.Constellations://open?url={any of the above, encoded}` —
//!   any Matrix link the desktop launcher hands us can be wrapped in our scheme
//!   and unwrapped here before parsing.
//!
//! Permalink parsing is delegated to ruma's [`MatrixToUri`] / [`MatrixUri`],
//! which understand the spec; this module only normalizes the entry points and
//! maps the result to a target the app can act on.

use matrix_sdk::ruma::matrix_uri::{MatrixId, UriAction};
use matrix_sdk::ruma::{
    MatrixToUri, MatrixUri, OwnedEventId, OwnedRoomAliasId, OwnedRoomId, OwnedRoomOrAliasId,
    OwnedServerName, OwnedUserId,
};
use url::Url;

/// The `fi.joonastuomi.Constellations://` scheme prefix consumed by `main.rs`.
const APP_SCHEME: &str = "fi.joonastuomi.Constellations://";

/// What a Matrix permalink points at, plus any routing servers and intent.
///
/// `via` is preserved so the app can route to rooms it isn't yet a member of
/// via the servers named in the link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermalinkTarget {
    /// A room by ID: `!room:server`.
    Room {
        room: OwnedRoomId,
        via: Vec<OwnedServerName>,
    },
    /// A room by alias: `#alias:server`. The app must resolve this to an ID
    /// (server-side) before it can be opened.
    RoomAlias {
        alias: OwnedRoomAliasId,
        via: Vec<OwnedServerName>,
    },
    /// A user: `@user:server`. MVP: opens a "start DM" affordance.
    User(OwnedUserId),
    /// An event in a room: `!room:server/$event:server` or
    /// `#alias:server/$event:server`.
    Event {
        room: OwnedRoomOrAliasId,
        event: OwnedEventId,
        via: Vec<OwnedServerName>,
    },
    /// A room the link explicitly invites the user to join (`action=join`).
    /// The app offers a join flow using the `via` servers.
    Join {
        room: OwnedRoomOrAliasId,
        via: Vec<OwnedServerName>,
    },
}

/// Why a permalink could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PermalinkError {
    /// The input was empty or not a string we recognize as a Matrix link.
    #[error("not a matrix permalink")]
    NotAPermalink,
    /// The link was wrapped in our app scheme but had no `url` query parameter.
    #[error("app scheme wrapper missing 'url' parameter")]
    MissingUrlParam,
    /// The link was structurally a Matrix link but contained invalid identifiers
    /// or malformed routing arguments. Carries the underlying ruma error.
    #[error("invalid matrix permalink: {0}")]
    Invalid(String),
}

/// Parse a Matrix permalink into a [`PermalinkTarget`].
///
/// Accepts `matrix.to` URLs, `matrix:` URIs, and the app's own
/// `fi.joonastuomi.Constellations://open?url=...` wrapper (recursing on the
/// wrapped URL). Returns [`PermalinkError::NotAPermalink`] for anything that is
/// not a Matrix link — callers can then fall back to opening it externally.
pub fn parse(input: &str) -> Result<PermalinkTarget, PermalinkError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(PermalinkError::NotAPermalink);
    }

    // Unwrap our own scheme first: ?url=<encoded Matrix link>.
    if trimmed.starts_with(APP_SCHEME) {
        let inner = extract_app_scheme_url(trimmed)?;
        return parse(&inner);
    }

    // Try the app-id-prefixed forms matrix.to and matrix:.
    // matrix.to URLs carry the meaningful id in the fragment, so ruma parses the
    // raw string rather than going through `url::Url`.
    if let Ok(to) = MatrixToUri::parse(trimmed) {
        return map_matrix_id(to.id(), to.via(), None);
    }
    if let Ok(m) = MatrixUri::parse(trimmed) {
        return map_matrix_id(m.id(), m.via(), m.action());
    }

    Err(PermalinkError::NotAPermalink)
}

/// Pull the `url` query parameter out of a `fi.joonastuomi.Constellations://`
/// wrapper. The wrapper may itself be a well-formed `Url` (preferred) or a bare
/// `scheme:path`-style string that `url` can still parse.
fn extract_app_scheme_url(raw: &str) -> Result<String, PermalinkError> {
    // `url::Url` requires a `://` separator; the app scheme already has one.
    let parsed = Url::parse(raw)
        .map_err(|e| PermalinkError::Invalid(format!("app scheme url parse: {e}")))?;
    let url_value = parsed
        .query_pairs()
        .find(|(k, _)| k == "url")
        .map(|(_, v)| v.into_owned())
        .ok_or(PermalinkError::MissingUrlParam)?;
    if url_value.trim().is_empty() {
        return Err(PermalinkError::MissingUrlParam);
    }
    Ok(url_value)
}

/// Map a parsed ruma `MatrixId` (+ routing + optional action) to a
/// [`PermalinkTarget`]. `action=join` takes precedence over the id shape so a
/// join intent on a room is surfaced as [`PermalinkTarget::Join`].
fn map_matrix_id(
    id: &MatrixId,
    via: &[OwnedServerName],
    action: Option<&UriAction>,
) -> Result<PermalinkTarget, PermalinkError> {
    // `action=join` is meaningful for rooms regardless of id variant.
    let is_join = action.is_some_and(|a| matches!(a, UriAction::Join));
    match id {
        MatrixId::Room(room_id) => {
            if is_join {
                Ok(PermalinkTarget::Join {
                    room: room_id.clone().into(),
                    via: via.to_vec(),
                })
            } else {
                Ok(PermalinkTarget::Room {
                    room: room_id.clone(),
                    via: via.to_vec(),
                })
            }
        }
        MatrixId::RoomAlias(alias) => {
            if is_join {
                Ok(PermalinkTarget::Join {
                    room: alias.clone().into(),
                    via: via.to_vec(),
                })
            } else {
                Ok(PermalinkTarget::RoomAlias {
                    alias: alias.clone(),
                    via: via.to_vec(),
                })
            }
        }
        MatrixId::User(user_id) => Ok(PermalinkTarget::User(user_id.clone())),
        MatrixId::Event(room, event) => Ok(PermalinkTarget::Event {
            room: room.clone(),
            event: event.clone(),
            via: via.to_vec(),
        }),
        // `MatrixId` is `#[non_exhaustive]`; surface anything ruma adds later
        // as a parse error rather than failing to compile.
        _ => Err(PermalinkError::Invalid(format!(
            "unsupported matrix id variant: {id:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // `parse` is defined on the borrowed id types (e.g. `RoomId::parse`) and
    // returns the corresponding `Owned*` value. `OwnedRoomOrAliasId` is needed
    // to build expected values for the room-or-alias field comparisons.
    use matrix_sdk::ruma::{EventId, OwnedRoomOrAliasId, RoomAliasId, RoomId, ServerName, UserId};

    /// Helper: parse and assert success, returning the target.
    fn ok(input: &str) -> PermalinkTarget {
        parse(input).unwrap_or_else(|e| panic!("expected parse ok for {input:?}: {e:?}"))
    }

    /// Helper: assert a target carries exactly these via servers.
    fn assert_via(target: &PermalinkTarget, expected: &[&str]) {
        let via: Vec<String> = match target {
            PermalinkTarget::Room { via, .. }
            | PermalinkTarget::RoomAlias { via, .. }
            | PermalinkTarget::Event { via, .. }
            | PermalinkTarget::Join { via, .. } => {
                via.iter().map(|s| s.as_str().to_owned()).collect()
            }
            PermalinkTarget::User(_) => vec![],
        };
        let expected: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        assert_eq!(via, expected, "via servers mismatch");
    }

    // --- matrix.to ---

    #[test]
    fn matrix_to_room() {
        let t = ok("https://matrix.to/#/!abc:example.org");
        assert_eq!(
            t,
            PermalinkTarget::Room {
                room: RoomId::parse("!abc:example.org").unwrap(),
                via: vec![]
            }
        );
    }

    #[test]
    fn matrix_to_room_with_via() {
        let t = ok("https://matrix.to/#/!abc:example.org?via=server.one&via=server.two");
        assert_via(&t, &["server.one", "server.two"]);
    }

    #[test]
    fn matrix_to_room_alias() {
        let t = ok("https://matrix.to/#/#room:example.org");
        assert_eq!(
            t,
            PermalinkTarget::RoomAlias {
                alias: RoomAliasId::parse("#room:example.org").unwrap(),
                via: vec![]
            }
        );
    }

    #[test]
    fn matrix_to_user() {
        let t = ok("https://matrix.to/#/@alice:example.org");
        assert_eq!(
            t,
            PermalinkTarget::User(UserId::parse("@alice:example.org").unwrap())
        );
    }

    #[test]
    fn matrix_to_event() {
        let t = ok("https://matrix.to/#/!abc:example.org/$def:example.org");
        assert_eq!(
            t,
            PermalinkTarget::Event {
                room: RoomId::parse("!abc:example.org").unwrap().into(),
                event: EventId::parse("$def:example.org").unwrap(),
                via: vec![]
            }
        );
    }

    #[test]
    fn matrix_to_event_by_alias() {
        match ok("https://matrix.to/#/#room:example.org/$def:example.org") {
            PermalinkTarget::Event { room, event, .. } => {
                assert_eq!(
                    room,
                    OwnedRoomOrAliasId::from(RoomAliasId::parse("#room:example.org").unwrap())
                );
                assert_eq!(event, EventId::parse("$def:example.org").unwrap());
            }
            other => panic!("expected Event, got {other:?}"),
        }
    }

    // --- matrix: scheme ---

    #[test]
    fn matrix_uri_room() {
        // `matrix:` URIs use typed path segments: `roomid/<id>`, not sigils.
        let t = ok("matrix:roomid/abc:example.org");
        assert_eq!(
            t,
            PermalinkTarget::Room {
                room: RoomId::parse("!abc:example.org").unwrap(),
                via: vec![]
            }
        );
    }

    #[test]
    fn matrix_uri_event_with_via() {
        let t = ok("matrix:roomid/abc:example.org/e/def:example.org?via=srv.one&via=srv.two");
        match t {
            PermalinkTarget::Event { room, event, via } => {
                assert_eq!(
                    room,
                    OwnedRoomOrAliasId::from(RoomId::parse("!abc:example.org").unwrap())
                );
                assert_eq!(event, EventId::parse("$def:example.org").unwrap());
                assert_eq!(
                    via,
                    vec![
                        ServerName::parse("srv.one").unwrap(),
                        ServerName::parse("srv.two").unwrap()
                    ]
                );
            }
            other => panic!("expected Event, got {other:?}"),
        }
    }

    #[test]
    fn matrix_uri_action_join() {
        let t = ok("matrix:roomid/abc:example.org?action=join");
        match t {
            PermalinkTarget::Join { room, via } => {
                assert_eq!(
                    room,
                    OwnedRoomOrAliasId::from(RoomId::parse("!abc:example.org").unwrap())
                );
                assert!(via.is_empty());
            }
            other => panic!("expected Join, got {other:?}"),
        }
    }

    #[test]
    fn matrix_uri_action_join_with_via() {
        // join via a room alias (`r/<alias>`).
        let t = ok("matrix:r/room:example.org?action=join&via=join.via");
        match t {
            PermalinkTarget::Join { room, via } => {
                assert_eq!(
                    room,
                    OwnedRoomOrAliasId::from(RoomAliasId::parse("#room:example.org").unwrap())
                );
                assert_eq!(via, vec![ServerName::parse("join.via").unwrap()]);
            }
            other => panic!("expected Join, got {other:?}"),
        }
    }

    // --- app scheme wrapper ---

    #[test]
    fn app_scheme_wraps_matrix_to() {
        // ?url=<percent-encoded inner matrix.to link>
        // "https://matrix.to/#/!abc:example.org/$def:example.org"
        let wrapped = "fi.joonastuomi.Constellations://open?url=https%3A%2F%2Fmatrix.to%2F%23%2F%21abc%3Aexample.org%2F%24def%3Aexample.org";
        let t = ok(wrapped);
        match t {
            PermalinkTarget::Event { room, event, .. } => {
                assert_eq!(
                    room,
                    OwnedRoomOrAliasId::from(RoomId::parse("!abc:example.org").unwrap())
                );
                assert_eq!(event, EventId::parse("$def:example.org").unwrap());
            }
            other => panic!("expected Event, got {other:?}"),
        }
    }

    #[test]
    fn app_scheme_wraps_matrix_uri() {
        // "matrix:u/alice:example.org"
        let wrapped = "fi.joonastuomi.Constellations://open?url=matrix%3Au%2Falice%3Aexample.org";
        let t = ok(wrapped);
        assert_eq!(
            t,
            PermalinkTarget::User(UserId::parse("@alice:example.org").unwrap())
        );
    }

    #[test]
    fn app_scheme_missing_url_param() {
        let err = parse("fi.joonastuomi.Constellations://open").unwrap_err();
        assert_eq!(err, PermalinkError::MissingUrlParam);
    }

    #[test]
    fn app_scheme_empty_url_param() {
        let err = parse("fi.joonastuomi.Constellations://open?url=").unwrap_err();
        assert_eq!(err, PermalinkError::MissingUrlParam);
    }

    // --- failures ---

    #[test]
    fn empty_input() {
        assert_eq!(parse("").unwrap_err(), PermalinkError::NotAPermalink);
        assert_eq!(parse("   ").unwrap_err(), PermalinkError::NotAPermalink);
    }

    #[test]
    fn non_matrix_url() {
        // An ordinary web URL is not a Matrix permalink; the caller may open it
        // externally. Must NOT be misread as one.
        assert_eq!(
            parse("https://example.com/some/path").unwrap_err(),
            PermalinkError::NotAPermalink
        );
    }

    #[test]
    fn garbage_string() {
        assert_eq!(
            parse("not a link at all").unwrap_err(),
            PermalinkError::NotAPermalink
        );
    }

    #[test]
    fn trims_whitespace() {
        let t = ok("  https://matrix.to/#/!abc:example.org  \n");
        assert_eq!(
            t,
            PermalinkTarget::Room {
                room: RoomId::parse("!abc:example.org").unwrap(),
                via: vec![]
            }
        );
    }

    #[test]
    fn matrix_to_with_trailing_slash() {
        // ruma tolerates a trailing slash on matrix.to links.
        let t = ok("https://matrix.to/#/!abc:example.org/");
        assert_eq!(
            t,
            PermalinkTarget::Room {
                room: RoomId::parse("!abc:example.org").unwrap(),
                via: vec![]
            }
        );
    }
}
