# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2025-07-09

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
