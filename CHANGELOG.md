# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### New Features

#### Search

- **In-room message search** — Replaced the client-side fuzzy filter (which only searched the currently loaded timeline window) with a full-text search of the entire room history. Results include sender, timestamp, and body, with the surrounding context jumpable directly from a hit.
- **Server-side search with local fallback** — Search now prefers the homeserver's `/search` endpoint and falls back to the local seshat index when the server doesn't support it, so search works regardless of homeserver capability.
- **Cross-room (global) search** — When the search bar is active with no room selected, queries run across all joined rooms via the local seshat index. Each hit shows its room of origin, and a scope toggle (All / Direct Messages / Groups) narrows the working set. Clicking a result opens the originating room and jumps to the message.
- **Search pagination** — Message search results load in batches with a "Load more" control to page through additional hits.
- **Debounced public-rooms directory search** — Typing in the public rooms/spaces directory now debounces before querying, so fast typing no longer hammers the homeserver.

#### Chat & Composer

- **Composer drag-and-drop** — Files can now be dragged onto the message composer to attach them, in addition to the existing file-picker button.
- **Inline markdown links** — Markdown hyperlinks (`[text](url)`) are rendered as clickable Link widgets inside message bubbles in all rendering modes, rather than only as plain text.
- **Tasklists in markdown** — GitHub-flavored task lists (`- [ ]` / `- [x]`) now render as interactive checkboxes in markdown messages.
- **Copy permalink buttons** — Added copy-permalink buttons for individual messages and for rooms.

#### Matrix Integration

- **Start DM from user permalink** — Opening a `matrix:` user permalink now starts a direct message with that user instead of erroring.
- **`matrix:` URI scheme handler** — Registered the `matrix:` URI scheme at the OS level (`.desktop` + metainfo), so `matrix.to` and matrix permalinks open Constellations directly.
- **Real QR code login (MSC4108)** — Replaced the non-functional QR login stub (which generated an invalid `matrix.to` URL no scanner could read) with the real MSC4108 sign-in flow from matrix-rust-sdk. The QR now encodes a valid binary `QrCodeData` payload that an existing Matrix device (e.g. Element Mobile) can scan to grant login, with full secure-channel establishment, check-code confirmation, OAuth device-authorization, and end-to-end encryption secret transfer.

#### Settings

- **COSMIC Config persistence** — User settings are now stored via the COSMIC Config system instead of a custom JSON file, integrating with `cosmic-settings` and following the COSMIC desktop convention. (Note: existing `config.json` files from earlier alphas are not migrated — settings reset to defaults on first launch after upgrade.)

### Bug Fixes

- **OIDC login actually completes** — OIDC/OAuth login (and QR login, which depends on it) previously hung on "Waiting for browser": the client was never registered with the homeserver's MAS, and the redirect URI was rejected for a non-lowercase scheme and double-slash form. Both are fixed — login now performs dynamic client registration and uses the MAS-compliant `fi.joonastuomi.constellations:/callback` redirect URI. A dedicated error is surfaced when the homeserver doesn't support OAuth.
- **Closed a homeserver URL spoofing vulnerability** — Homeserver input was matched with a loose `starts_with("http://127.0.0.1")` check, allowing look-alike hosts like `127.0.0.1.attacker.com` or `localhost@attacker.com` to bypass the localhost allowance. URLs are now parsed and validated by exact host (and userinfo is stripped), closing the spoofing vector across password, OIDC, and QR login.
- **Markdown links parsed in plain-text mode** — Fixed markdown links not being extracted when a message rendered in plain-text mode.

### Security

- **Removed insecure secret-storage fallback** — Matrix credentials (session tokens, store passphrase) no longer fall back to being written as plaintext files when Keyring is unavailable; the failure now bubbles up as an error instead of silently leaking secrets to disk.

## [0.1.0] - 2026-07-09

First alpha release. Usable, but expect bugs, missing features, and breaking changes before the eventual 1.0.


### New Features

- **UnifiedPush notifications** — Added support for UnifiedPush background notification handler, allowing real-time push notifications.
- **Room members & pinned messages panels** — Added collapsible side panels for viewing room members and pinned messages in the chat view.
- **Stable timeline scrolling** — Implemented stable timeline scrolling to prevent jumpy scroll behavior when new messages arrive.
- **Start from oldest unread** — Automatically scroll to and start from the oldest unread message when re-joining a chat room.
- **Plain-text URL parsing** — Added support for parsing plain-text URLs into clickable links in message bubbles across all rendering modes.
- **QR code login** — Implemented secure QR code login using a custom QR code scanner widget.
- **Location sharing** — Added support for viewing and sending shared locations.
- **MatrixRTC (LiveKit)** — Added experimental support for MatrixRTC group calls powered by LiveKit.
- **Multi-line chat editor** — Improved the message composer to support writing and editing multi-line messages easily.

### Bug Fixes

- Fixed a panic on start-up related to search index database corruption by automatically clearing the search index on cryptographic key mismatch (invalid MAC) or fresh store creation.
- Fixed an issue where trigger-happy system notifications would cause nested runtime panics by switching to the async notification API.
- Fixed reaction emoji rendering and interactions in chat bubbles.
- Strip reply fallback quotes from room list message previews to keep previews clean and legible.
- Fixed device verification status checks, enabled incoming room key requests, and moved the verification UI to a more intuitive location.
- Fixed a bug where message previews would display raw newline characters instead of space separation.
- Fixed a date divider bug to ensure date headers are only displayed for days containing actual, visible messages in the timeline.

### User Interface & Experience

- **Localized settings** — Fully translated User Settings and User Notification Settings into multiple languages.
- **Improved settings layout** — Stacked inputs and controls in settings pages to fit cleanly on narrow screens and mobile layouts.
- **Visual dividers** — Added subtle horizontal and vertical pane dividers to improve workspace boundaries in multi-pane layouts.
- **Unified timeline composer** — Redesigned the chat composer with a cohesive card-based UI that matches the timeline theme.
- **ListItem room lists** — Styled the sidebar room list with clean, consistent `ListItem` widgets.
- **Icon buttons in compact spaces** — Replaced text buttons with streamlined icon-only buttons in compact spaces and for destructive actions.
- **Search empty state** — Added localized helper text and clear illustration when search results are empty.
- **Close button tooltips** — Added helpful tooltips to the close buttons for the emoji picker and full-screen image viewer.

### Performance Improvements

- **Optimized localization allocations** — Prevented string allocation bottlenecks in `view_item` and view loops by caching and passing localized strings by reference.
- **Zero-allocation timeline items** — Pre-computed `TimelineEventItemId` and cached room event identifiers in the render loop to eliminate per-frame heap allocations.
- **Optimized thread rendering** — Pre-calculated thread root IDs and thread counts in background data models to avoid allocations during view traversal.
- **Optimized room name resolution** — Avoided string allocations per frame when resolving active room names in the main UI thread.
- **Media cache lookup optimization** — Eliminated unnecessary heap-allocated string copies during media cache lookups.
