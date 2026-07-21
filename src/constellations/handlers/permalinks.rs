use crate::constellations::PendingAliasOp;
use crate::matrix;
use crate::{Constellations, Message};
use cosmic::{Action, Application, Task};
use std::sync::Arc;

impl Constellations {
    pub fn open_matrix_link(
        &mut self,
        raw: String,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        // Not signed in yet: hold the link and replay once login completes.
        if self.matrix.is_none() {
            self.pending_link = Some(raw.clone());
            self.set_error(crate::fl!("sign-in-to-open-link").to_string());
            return Task::none();
        }

        match crate::utils::permalink::parse(&raw) {
            Ok(target) => self.route_permalink_target(target),
            Err(_) => {
                // Not a Matrix permalink. If it parses as a URL, open it
                // externally; otherwise log and drop.
                if url::Url::parse(&raw).is_ok() {
                    Task::done(Action::from(Message::OpenUrl(raw)))
                } else {
                    tracing::warn!("Ignoring unparseable link: {raw}");
                    Task::none()
                }
            }
        }
    }

    /// Toggle the in-app "Open link…" paste dialog. Requires an active session;
    /// when the user is signed out, links can't be opened anyway, so we surface
    /// the same sign-in message instead of an inert dialog.
    pub(super) fn handle_toggle_open_link(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        if self.matrix.is_none() {
            self.set_error(crate::fl!("sign-in-to-open-link").to_string());
            return Task::none();
        }
        // Mirror the create-room/space drawer: close any other context drawer
        // so only one is visible at a time.
        let now_open = self.open_link_dialog.is_none();
        self.open_link_dialog = now_open.then(String::new);
        self.creating_room = false;
        self.creating_space = false;
        self.current_settings_panel = None;
        self.core.set_show_context(now_open);
        Task::none()
    }

    pub(super) fn handle_open_link_text_changed(
        &mut self,
        text: String,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        if self.open_link_dialog.is_some() {
            self.open_link_dialog = Some(text);
        }
        Task::none()
    }

    /// Submit the paste-link dialog: close it and route the link through the
    /// shared `open_matrix_link` path. Empty input just closes the dialog.
    pub(super) fn handle_submit_open_link(
        &mut self,
        text: String,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        self.open_link_dialog = None;
        let trimmed = text.trim().to_owned();
        if trimmed.is_empty() {
            return Task::none();
        }
        self.open_matrix_link(trimmed)
    }

    /// Map a parsed `PermalinkTarget` onto existing room/jump/join messages.
    fn route_permalink_target(
        &mut self,
        target: crate::utils::permalink::PermalinkTarget,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        use crate::utils::permalink::PermalinkTarget;

        match target {
            PermalinkTarget::Room { room, .. } => {
                Task::done(Action::from(Message::RoomSelected(room.as_str().into())))
            }
            PermalinkTarget::RoomAlias { alias, .. } => {
                self.pending_alias_op = Some(PendingAliasOp::OpenRoom);
                self.kick_off_alias_resolution(alias)
            }
            PermalinkTarget::User(user_id) => {
                if let Some(matrix) = self.matrix.as_ref() {
                    let matrix = matrix.clone();
                    Task::perform(
                        async move {
                            matrix
                                .get_or_create_dm(&user_id)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| Action::from(Message::DmRoomResolved(res)),
                    )
                } else {
                    Task::none()
                }
            }
            PermalinkTarget::Event { room, event, .. } => {
                // If the room half is an alias, resolve it first and remember
                // the event to focus on afterwards.
                if room.is_room_alias_id()
                    && let Ok(alias) = matrix_sdk::ruma::RoomAliasId::parse(room.as_str())
                {
                    self.pending_alias_op = Some(PendingAliasOp::OpenEvent(event));
                    return self.kick_off_alias_resolution(alias);
                }
                // Room half is already an ID: stash the event and select the
                // room. The jump-vs-fetch decision happens once the room's
                // timeline finishes initialising (see `TimelineInitFinished`):
                // if the event is already in the loaded window we scroll to
                // it, otherwise we build an event-focused timeline around it.
                self.pending_event_focus = Some(event);
                let room_id: Arc<str> = room.as_str().into();
                Task::done(Action::from(Message::RoomSelected(room_id)))
            }
            PermalinkTarget::Join { room, .. } => {
                if room.is_room_alias_id()
                    && let Ok(alias) = matrix_sdk::ruma::RoomAliasId::parse(room.as_str())
                {
                    self.pending_alias_op = Some(PendingAliasOp::JoinRoom);
                    return self.kick_off_alias_resolution(alias);
                }
                Task::done(Action::from(Message::JoinRoom(room.as_str().into())))
            }
        }
    }

    /// Consume a pending event-focus request once the room's timeline has
    /// finished initialising.
    ///
    /// If the target event is already in the loaded window, scroll to it
    /// (cheap path, reuses the live timeline). Otherwise emit
    /// [`Message::LoadEventContext`] to build an event-focused timeline around
    /// it. Returns `Task::none()` when no focus is pending.
    pub(super) fn check_pending_event_focus(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        let Some(event_id) = self.pending_event_focus.take() else {
            return Task::none();
        };

        // Already in the loaded window? Just scroll to it.
        let already_loaded = self.timeline_items.iter().any(|item| {
            item.item_id.as_ref().is_some_and(
                |id| matches!(id, matrix::TimelineEventItemId::EventId(eid) if eid == &event_id),
            )
        });
        if already_loaded {
            return Task::done(Action::from(Message::JumpToMessage(event_id)));
        }

        // Not loaded: build an event-focused timeline around it.
        Task::done(Action::from(Message::LoadEventContext(event_id)))
    }

    /// Build an event-focused (permalink context) timeline around `event_id`
    /// and swap the displayed timeline to it. On failure, surface a toast.
    pub(super) fn handle_load_event_context(
        &mut self,
        event_id: matrix_sdk::ruma::OwnedEventId,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        let (matrix, room_id) = match (&self.matrix, &self.selected_room) {
            (Some(m), Some(r)) => (m.clone(), r.clone()),
            _ => {
                tracing::warn!("LoadEventContext without an active room");
                self.set_error(crate::fl!("message-not-found").to_string());
                return Task::none();
            }
        };

        // Setting `active_event_focus` switches the room's subscription to the
        // event-focused timeline (see `subscription()`). The timeline items
        // are repopulated when that subscription initialises.
        self.active_event_focus = Some(event_id.clone());
        self.is_timeline_initialized = false;
        self.timeline_items.clear();
        self.last_content_height = 0.0;
        self.last_viewport_height = 0.0;

        let event_id_for_task = event_id.clone();
        Task::perform(
            async move {
                matrix
                    .event_timeline(&room_id, event_id_for_task)
                    .await
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            },
            move |res| Action::from(Message::EventContextLoaded(event_id.clone(), res)),
        )
    }

    /// Open a (possibly different) room and jump to one of its events, e.g.
    /// from a cross-room message search result.
    ///
    /// If the hit's room is already open, jump to the event directly. Otherwise
    /// select the room and queue a follow-up `SetPendingEventFocus` **after**
    /// `RoomSelected`. The ordering is load-bearing: `RoomSelected` clears
    /// `pending_event_focus` (see `test_room_selected_clears_event_focus`), so
    /// the focus must be re-asserted afterwards; `TimelineInitFinished` then
    /// consumes it via `check_pending_event_focus` (scroll / load-context).
    pub(super) fn handle_open_room_event(
        &mut self,
        room_id: std::sync::Arc<str>,
        event_id: matrix_sdk::ruma::OwnedEventId,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        if self.selected_room.as_deref() == Some(room_id.as_ref()) {
            return Task::done(Action::from(Message::JumpToMessageOrLoadContext(event_id)));
        }
        Task::batch(vec![
            Task::done(Action::from(Message::RoomSelected(room_id))),
            Task::done(Action::from(Message::SetPendingEventFocus(event_id))),
        ])
    }

    /// Handle the result of building an event-focused timeline. On success,
    /// schedule a jump to the centred event; on failure, surface a toast and
    /// return to live.
    pub(super) fn handle_event_context_loaded(
        &mut self,
        event_id: matrix_sdk::ruma::OwnedEventId,
        res: Result<(), String>,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        match res {
            Ok(()) => {
                // The event-focused subscription is now feeding the timeline;
                // jump to the event once it appears.
                Task::done(Action::from(Message::JumpToMessage(event_id)))
            }
            Err(e) => {
                tracing::warn!("Failed to load event context for {event_id}: {e}");
                // Roll back to the live timeline.
                self.active_event_focus = None;
                self.set_error(crate::fl!("message-not-found").to_string());
                Task::none()
            }
        }
    }

    /// Leave the event-focused timeline and restore the live one at the bottom.
    /// Triggered by the "Jump to newest" button in the viewing-older-messages
    /// banner.
    pub(super) fn handle_return_to_live(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        // If we were viewing an event-focused timeline, optionally drop the
        // cached entry so a later re-open rebuilds it fresh.
        if let (Some(matrix), Some(room_id), Some(event_id)) =
            (&self.matrix, &self.selected_room, &self.active_event_focus)
        {
            let matrix = matrix.clone();
            let room_id = room_id.clone();
            let event_id = event_id.clone();
            // Best-effort cache drop; ignore errors.
            tokio::spawn(async move {
                let _ = matrix.drop_event_timeline(&room_id, &event_id).await;
            });
        }

        self.active_event_focus = None;
        // Clearing items + the init flag lets the live subscription (now
        // re-selected because active_event_focus is None) repopulate the
        // timeline, and the existing initial-scroll path snaps to the end.
        self.timeline_items.clear();
        self.is_timeline_initialized = false;
        self.is_timeline_at_bottom = true;
        self.needs_initial_scroll = true;
        self.last_content_height = 0.0;
        self.last_viewport_height = 0.0;
        Task::none()
    }

    /// Spawn the async `resolve_room_alias` call. Caller must set
    /// `pending_alias_op` first so the result handler knows what to do.
    fn kick_off_alias_resolution(
        &self,
        alias: matrix_sdk::ruma::OwnedRoomAliasId,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        let Some(matrix) = &self.matrix else {
            return Task::none();
        };
        let matrix = matrix.clone();
        Task::perform(
            async move {
                matrix
                    .resolve_room_alias(&alias)
                    .await
                    .map_err(|e| e.to_string())
            },
            |res| Action::from(Message::RoomAliasResolved(Box::new(res))),
        )
    }

    /// Handle a completed alias resolution: carry out the operation stashed in
    /// `pending_alias_op` against the resolved room ID.
    pub(super) fn handle_room_alias_resolved(
        &mut self,
        res: Box<Result<matrix_sdk::ruma::OwnedRoomId, String>>,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        let op = self.pending_alias_op.take();
        match res.as_ref() {
            Ok(room_id) => {
                let room_arc: Arc<str> = room_id.as_str().into();
                match op {
                    Some(PendingAliasOp::OpenRoom) => {
                        Task::done(Action::from(Message::RoomSelected(room_arc)))
                    }
                    Some(PendingAliasOp::JoinRoom) => {
                        Task::done(Action::from(Message::JoinRoom(room_arc)))
                    }
                    Some(PendingAliasOp::OpenEvent(event)) => {
                        // Stash the event to focus on once the room's timeline
                        // initialises; same deferred jump-vs-fetch path as the
                        // room-id case above.
                        self.pending_event_focus = Some(event);
                        Task::done(Action::from(Message::RoomSelected(room_arc)))
                    }
                    // No op stashed (e.g. link superseded): just log.
                    None => {
                        tracing::warn!(
                            "Alias resolved to {room_id} but no pending operation was set"
                        );
                        Task::none()
                    }
                }
            }
            Err(e) => {
                self.set_error(
                    crate::fl!("error-failed-join-room", error = e.to_string()).to_string(),
                );
                Task::none()
            }
        }
    }
}
