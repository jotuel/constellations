use crate::matrix;
use crate::settings;
use crate::{Constellations, MediaSource, Message, OwnedRoomId, SettingsPanel};
use cosmic::{Action, Application, Task};
use futures::stream::StreamExt;
use std::sync::Arc;

impl Constellations {
    pub fn handle_join_call(&mut self) -> Task<Action<Message>> {
        if let (Some(matrix), Some(room_id)) = (&self.matrix, &self.selected_room) {
            let matrix = matrix.clone();
            let room_id = room_id.clone();
            Task::perform(
                async move { matrix.join_call(&room_id).await.map_err(|e| e.to_string()) },
                |res| Action::from(Message::CallJoined(res)),
            )
        } else {
            Task::none()
        }
    }

    pub fn handle_leave_call(&mut self) -> Task<Action<Message>> {
        if let (Some(matrix), Some(room_id)) = (&self.matrix, &self.selected_room) {
            let matrix = matrix.clone();
            let room_id = room_id.to_string();
            Task::perform(
                async move { matrix.leave_call(&room_id).await.map_err(|e| e.to_string()) },
                |res| Action::from(Message::CallLeft(res)),
            )
        } else {
            Task::none()
        }
    }

    pub fn handle_create_room(
        &mut self,
        name: String,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        if let Some(matrix) = &self.matrix {
            let matrix = matrix.clone();
            let is_video = self.new_room_is_video;
            Task::perform(
                async move {
                    matrix
                        .create_room(&name, is_video)
                        .await
                        .map(|id| id.to_string())
                        .map_err(|e| e.to_string())
                },
                |res| Action::from(Message::RoomCreated(res)),
            )
        } else {
            Task::none()
        }
    }

    pub fn handle_create_space(
        &mut self,
        name: String,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        if let Some(matrix) = &self.matrix {
            let matrix = matrix.clone();
            Task::perform(
                async move {
                    matrix
                        .create_space(&name)
                        .await
                        .map(|id| id.to_string())
                        .map_err(|e| e.to_string())
                },
                |res| Action::from(Message::SpaceCreated(res)),
            )
        } else {
            Task::none()
        }
    }

    pub fn handle_select_space(
        &mut self,
        space_id: Option<OwnedRoomId>,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        self.selected_space = space_id.clone();
        // Clear other_rooms immediately when switching to avoid stale data from previous space
        self.other_rooms.clear();

        let mut tasks = Vec::new();

        if let Some(matrix) = &self.matrix {
            let matrix_clone = matrix.clone();
            let sid = space_id.clone();
            tasks.push(Task::perform(
                async move {
                    let _ = matrix_clone.update_room_list_filter(sid).await;
                },
                |_| Action::from(Message::SpaceFilterUpdated),
            ));
            if let Some(space_id) = space_id {
                let matrix_clone = matrix.clone();
                tasks.push(Task::perform(
                    async move {
                        let res = matrix_clone
                            .get_space_children(space_id.as_str())
                            .await
                            .map_err(|e| e.to_string());
                        (space_id, res)
                    },
                    move |(space_id, res)| {
                        Action::from(Message::SpaceChildrenFetched(space_id, res))
                    },
                ));
            } else {
                self.other_rooms.clear();
            }
        }

        self.update_filtered_rooms();
        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    pub fn handle_space_children_fetched(
        &mut self,
        space_id: OwnedRoomId,
        res: Result<Vec<matrix::RoomData>, String>,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        // Only update if the fetched children are for the currently selected space
        if Some(&space_id) != self.selected_space.as_ref() {
            return Task::none();
        }

        let mut tasks = Vec::new();

        match res {
            Ok(children) => {
                // First, update the filtered_room_list because the hierarchy in matrix engine was updated
                self.update_filtered_rooms();

                // Re-trigger the SDK filter with the new hierarchy data
                if let Some(matrix) = &self.matrix {
                    let matrix_clone = matrix.clone();
                    let sid = space_id.clone();
                    tasks.push(Task::perform(
                        async move {
                            let _ = matrix_clone.update_room_list_filter(Some(sid)).await;
                        },
                        |_| Action::from(Message::SpaceFilterUpdated),
                    ));
                }

                if let Some(matrix) = &self.matrix
                    && self.user_settings.invite_avatars_display_policy
                {
                    let mut urls_to_fetch = Vec::new();
                    for child in &children {
                        if let Some(avatar_url) = &child.avatar_url
                            && !self.media_cache.contains_key(avatar_url)
                        {
                            let uri = matrix_sdk::ruma::OwnedMxcUri::from(avatar_url.as_str());
                            let source = MediaSource::Plain(uri);
                            urls_to_fetch.push((avatar_url.clone(), source));
                        }
                    }

                    if !urls_to_fetch.is_empty() {
                        let matrix_clone = matrix.clone();
                        tasks.push(Task::perform(
                            async move {
                                futures::stream::iter(urls_to_fetch)
                                    .map(|(url_str, source)| {
                                        let matrix = matrix_clone.clone();
                                        async move {
                                            let res = matrix
                                                .fetch_media(source)
                                                .await
                                                .map_err(|e| e.to_string());
                                            (url_str, res)
                                        }
                                    })
                                    .buffer_unordered(10)
                                    .collect::<Vec<_>>()
                                    .await
                            },
                            |batch| Message::MediaFetchedBatch(batch).into(),
                        ));
                    }
                }

                let mut other_rooms: Vec<_> = children
                    .into_iter()
                    .filter(|r| !self.joined_room_ids.contains(r.id.as_ref()) && !r.is_space)
                    .collect();

                other_rooms.sort_by(|a, b| match (&a.order, &b.order) {
                    (Some(oa), Some(ob)) => oa.cmp(ob).then_with(|| a.id.cmp(&b.id)),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => a.id.cmp(&b.id),
                });

                self.other_rooms = other_rooms;
            }
            Err(e) => {
                self.set_error(
                    crate::fl!("error-failed-fetch-space-children", error = e.to_string())
                        .to_string(),
                );
            }
        }

        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    pub fn handle_unpin_message(
        &mut self,
        event_id: matrix_sdk::ruma::OwnedEventId,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        self.is_loading_pinned = true;
        self.unpin_message_task(event_id)
    }

    pub(super) fn fetch_members_task(&self) -> Task<Action<Message>> {
        let Some(room_id) = self.selected_room.clone() else {
            return Task::none();
        };
        let Some(matrix) = self.matrix.clone() else {
            return Task::none();
        };
        Task::perform(
            async move {
                matrix
                    .get_room_members(&room_id)
                    .await
                    .map_err(|e| e.to_string())
            },
            |res| Action::from(Message::MembersFetched(res)),
        )
    }

    pub(super) fn fetch_pinned_events_task(&self) -> Task<Action<Message>> {
        let Some(room_id) = self.selected_room.clone() else {
            return Task::none();
        };
        let Some(matrix) = self.matrix.clone() else {
            return Task::none();
        };
        Task::perform(
            async move {
                let ids = matrix
                    .get_pinned_events(&room_id)
                    .await
                    .map_err(|e| e.to_string())?;
                let futures = ids.into_iter().map(|id| {
                    let matrix = matrix.clone();
                    let room_id = room_id.clone();
                    async move {
                        match matrix.fetch_pinned_event_details(&room_id, &id).await {
                            Ok(detail) => detail,
                            Err(e) => {
                                tracing::error!(
                                    "Failed to fetch details for pinned event {}: {}",
                                    id,
                                    e
                                );
                                matrix::PinnedEventInfo {
                                    event_id: id.to_string(),
                                    sender_id: "@unknown:example.com".to_string(),
                                    sender_name: crate::fl!("unknown-sender").to_string(),
                                    avatar_url: None,
                                    timestamp: crate::fl!("unknown-time").to_string(),
                                    body: crate::fl!(
                                        "error-failed-load-message-content",
                                        error = e.to_string()
                                    )
                                    .to_string(),
                                }
                            }
                        }
                    }
                });
                let details = futures::future::join_all(futures).await;
                Ok(details)
            },
            |res| Action::from(Message::PinnedEventsFetched(res)),
        )
    }

    /// Removes an event from the room's pinned list, then refreshes the panel.
    fn unpin_message_task(
        &self,
        event_id: matrix_sdk::ruma::OwnedEventId,
    ) -> Task<Action<Message>> {
        let Some(room_id) = self.selected_room.clone() else {
            return Task::none();
        };
        let Some(matrix) = self.matrix.clone() else {
            return Task::none();
        };
        Task::perform(
            async move {
                let current = matrix
                    .get_pinned_events(&room_id)
                    .await
                    .map_err(|e| e.to_string())?;
                let updated: Vec<_> = current.into_iter().filter(|id| id != &event_id).collect();
                matrix
                    .set_pinned_events(&room_id, updated)
                    .await
                    .map_err(|e| e.to_string())?;

                // Rebuild the panel from the server's view of pinned events.
                let ids = matrix
                    .get_pinned_events(&room_id)
                    .await
                    .map_err(|e| e.to_string())?;
                let futures = ids.into_iter().map(|id| {
                    let matrix = matrix.clone();
                    let room_id = room_id.clone();
                    async move {
                        match matrix.fetch_pinned_event_details(&room_id, &id).await {
                            Ok(detail) => detail,
                            Err(e) => {
                                tracing::error!(
                                    "Failed to fetch details for pinned event {}: {}",
                                    id,
                                    e
                                );
                                matrix::PinnedEventInfo {
                                    event_id: id.to_string(),
                                    sender_id: "@unknown:example.com".to_string(),
                                    sender_name: crate::fl!("unknown-sender").to_string(),
                                    avatar_url: None,
                                    timestamp: crate::fl!("unknown-time").to_string(),
                                    body: crate::fl!(
                                        "error-failed-load-message-content",
                                        error = e.to_string()
                                    )
                                    .to_string(),
                                }
                            }
                        }
                    }
                });
                let details = futures::future::join_all(futures).await;
                Ok(details)
            },
            |res| Action::from(Message::PinnedEventsFetched(res)),
        )
    }

    pub(super) fn handle_room_selected(
        &mut self,
        room_id: std::sync::Arc<str>,
    ) -> Task<Action<Message>> {
        if let Some(room) = self.room_list.iter().find(|r| r.id == room_id)
            && let Some(name) = &room.name
        {
            self.room_name_cache.insert(room_id.clone(), name.clone());
        }
        self.selected_room = Some(room_id.clone());
        self.timeline_items.clear();
        self.room_members.clear();
        self.pinned_events.clear();
        self.pinned_events_details.clear();
        // Message search results are scoped to the previous room;
        // clear them so stale hits don't bleed into the new room. Global
        // search results are also room-context-sensitive (they only run when
        // no room is selected), so clear them too.
        self.message_search_results.clear();
        self.is_searching_messages = false;
        self.search_has_more = false;
        self.is_searching_more_messages = false;
        self.global_message_search_results.clear();
        self.is_searching_global_messages = false;
        self.inviting_to_room = false;
        self.invite_to_room_id.clear();
        // A room switch always leaves the event-focused (permalink
        // context) view: clear any pending/active event focus so the
        // new room opens on its live timeline and the banner hides.
        self.pending_event_focus = None;
        self.active_event_focus = None;
        let fetch_members_task = if self.show_members_panel {
            self.is_loading_members = true;
            self.fetch_members_task()
        } else {
            Task::none()
        };
        let fetch_pinned_task = self.fetch_pinned_events_task();
        self.recompute_thread_counts();
        self.last_timeline_offset = 0.0;
        self.last_content_height = 0.0;
        self.last_viewport_width = 0.0;
        self.last_viewport_height = 0.0;
        self.needs_scroll_adjustment = false;
        self.is_timeline_at_bottom = true;
        self.is_threaded_timeline_at_bottom = true;
        self.is_timeline_initialized = false;
        self.is_first_time_joining = false;
        self.visited_room_ids.insert(room_id.clone());
        self.needs_initial_scroll = true;

        Task::batch(vec![
            self.update_title(),
            self.handle_load_more(false),
            fetch_members_task,
            fetch_pinned_task,
        ])
    }

    pub(super) fn handle_open_settings(&mut self, panel: SettingsPanel) -> Task<Action<Message>> {
        self.needs_layout_scroll_restoration = true;
        self.needs_threaded_layout_scroll_restoration = true;
        self.show_members_panel = false;
        self.show_pinned_panel = false;
        self.creating_room = false;
        self.creating_space = false;
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

        let task = if panel == SettingsPanel::User {
            self.user_settings
                .update(settings::user::Message::LoadProfile, &self.matrix)
        } else if matches!(
            panel,
            SettingsPanel::Room | SettingsPanel::ManageRoomMembers
        ) {
            if let Some(room_id) = &self.selected_room {
                self.room_settings.update(
                    settings::room::Message::LoadRoom(room_id.clone()),
                    &self.matrix,
                )
            } else {
                Task::none()
            }
        } else if panel == SettingsPanel::Space
            && let Some(space_id) = &self.selected_space
        {
            self.space_settings.update(
                settings::space::Message::LoadSpace(Arc::from(space_id.as_str())),
                &self.matrix,
            )
        } else {
            Task::none()
        };
        Task::batch(vec![task, self.restore_scroll_task()])
    }
}
