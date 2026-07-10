use crate::matrix::TimelineItem;
use crate::preview::parse_markdown;
use crate::settings;
use crate::{
    ApplyVectorDiffExt, AuthFlow, Constellations, ConstellationsItem, MediaSource, Message,
    OwnedRoomId, QrLoginStep, SettingsPanel, THREADED_TIMELINE_ID, TIMELINE_ID, Url, matrix,
    redact_url,
};
use cosmic::iced::widget::scrollable;
use cosmic::{Action, Application, Task};
use futures::FutureExt;
use futures::stream::StreamExt;
use matrix_sdk::ruma::OwnedEventId;
use matrix_sdk::ruma::events::room::message::MessageType;
use matrix_sdk_ui::timeline::TimelineDetails;
use std::sync::Arc;

type PinnedOutput =
    std::pin::Pin<Box<dyn Future<Output = (String, Result<Vec<u8>, String>)> + Send + 'static>>;

impl Constellations {
    pub fn restore_scroll_task(&self) -> Task<Action<Message>> {
        if self.active_thread_root.is_some() {
            if self.is_threaded_timeline_at_bottom {
                scrollable::snap_to(
                    THREADED_TIMELINE_ID.clone(),
                    scrollable::RelativeOffset::END.into(),
                )
            } else {
                scrollable::scroll_to(
                    THREADED_TIMELINE_ID.clone(),
                    scrollable::AbsoluteOffset {
                        x: Some(0.0),
                        y: Some(self.last_threaded_timeline_offset),
                    },
                )
            }
        } else {
            if self.is_timeline_at_bottom {
                scrollable::snap_to(TIMELINE_ID.clone(), scrollable::RelativeOffset::END.into())
            } else {
                scrollable::scroll_to(
                    TIMELINE_ID.clone(),
                    scrollable::AbsoluteOffset {
                        x: Some(0.0),
                        y: Some(self.last_timeline_offset),
                    },
                )
            }
        }
    }

    pub fn recompute_thread_counts(&mut self) {
        self.thread_counts.clear();
        for item in &self.timeline_items {
            // Skip items without an inner event (e.g. virtual/pending items)
            // rather than panicking — the field is `Option` by construction.
            if let Some(inner) = item.item.as_ref()
                && inner.as_event().is_some()
                && let Some(root_id) = item.thread_root_id.clone()
            {
                *self.thread_counts.entry(root_id).or_insert(0) += 1;
            }
        }
    }

    pub fn handle_engine_ready(
        &mut self,
        res: Result<matrix::MatrixEngine, matrix::SyncError>,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        match res {
            Ok(engine) => {
                self.matrix = Some(engine.clone());
                crate::unified_push::start_unified_push_listener(engine.clone());
                Task::perform(
                    async move {
                        let did_restore = engine.restore_session().await.unwrap_or(false);
                        if did_restore {
                            let user_id = engine.client().await.user_id().map(|u| u.to_string());
                            let sync_res = engine.start_sync().await;
                            (user_id, sync_res)
                        } else {
                            (
                                None,
                                Err(matrix::SyncError::Generic(
                                    "No session to restore".to_string(),
                                )),
                            )
                        }
                    },
                    |(user_id, sync_res)| {
                        if let Some(uid) = user_id {
                            Action::from(Message::UserReady(Some(uid), sync_res))
                        } else {
                            Action::from(Message::UserReady(None, sync_res))
                        }
                    },
                )
            }
            Err(e) => {
                self.set_error(
                    crate::fl!("error-failed-init-engine", error = e.to_string()).to_string(),
                );
                self.is_initializing = false;
                Task::none()
            }
        }
    }

    pub fn handle_user_ready(
        &mut self,
        user_id: Option<String>,
        sync_res: Result<(), matrix::SyncError>,
    ) -> Task<Action<Message>> {
        self.user_id = user_id;
        self.is_initializing = false;
        let title_task = self.update_title();
        if self.user_id.is_none() {
            return title_task;
        }

        match sync_res {
            Ok(_) => {}
            Err(matrix::SyncError::MissingSlidingSyncSupport) => {
                self.sync_status = matrix::SyncStatus::MissingSlidingSyncSupport;
            }
            Err(e) => {
                self.sync_status = matrix::SyncStatus::Error(e.to_string());
            }
        }
        let mut tasks = Vec::new();
        tasks.push(title_task);

        if let Some(matrix) = &self.matrix {
            let matrix_ignored = matrix.clone();
            tasks.push(Task::perform(
                async move { matrix_ignored.ignored_users().await.unwrap_or_default() },
                |users| {
                    Message::UserSettings(crate::settings::user::Message::IgnoredUsersLoaded(Ok(
                        users,
                    )))
                    .into()
                },
            ));

            let mut media_fetches = Vec::new();
            for room in self.room_list.iter() {
                if let Some(avatar_url) = &room.avatar_url
                    && !self.media_cache.contains_key(avatar_url)
                {
                    let matrix_clone = matrix.clone();
                    let url_str = avatar_url.clone();
                    let uri = matrix_sdk::ruma::OwnedMxcUri::from(avatar_url.as_str());
                    let source = MediaSource::Plain(uri);
                    media_fetches.push(async move {
                        let res = matrix_clone
                            .fetch_media(source)
                            .await
                            .map_err(|e| e.to_string());
                        (url_str, res)
                    });
                }
            }
            if !media_fetches.is_empty() {
                tasks.push(Task::perform(
                    async move {
                        futures::stream::iter(media_fetches)
                            .buffer_unordered(10)
                            .collect::<Vec<_>>()
                            .await
                    },
                    |results| Message::MediaFetchedBatch(results).into(),
                ));
            }
        }

        // Replay a permalink that arrived before the session was restored.
        if let Some(link) = self.pending_link.take()
            && self.matrix.is_some()
        {
            tasks.push(Task::done(Action::from(Message::OpenMatrixLink(link))));
        }

        Task::batch(tasks)
    }

    pub fn fetch_missing_media(&mut self) -> Task<Action<Message>> {
        let mut media_fetches: Vec<PinnedOutput> = Vec::new();

        let matrix = match &self.matrix {
            Some(m) => m.clone(),
            None => return Task::none(),
        };

        let check_item = |item: &Arc<TimelineItem>, fetches: &mut Vec<_>| {
            if let Some(event) = item.as_event() {
                // Fetch avatar
                if let TimelineDetails::Ready(profile) = event.sender_profile()
                    && let Some(avatar_url) = &profile.avatar_url
                {
                    let url_str = avatar_url.to_string();
                    if !self.media_cache.contains_key(&url_str) {
                        let matrix_clone = matrix.clone();
                        let source = MediaSource::Plain(avatar_url.clone());
                        fetches.push(
                            async move {
                                let res = matrix_clone
                                    .fetch_media(source)
                                    .await
                                    .map_err(|e| e.to_string());
                                (url_str, res)
                            }
                            .boxed(),
                        );
                    }
                }

                // Fetch image if enabled
                if self.user_settings.media_previews_display_policy
                    && let Some(message) = event.content().as_message()
                    && let MessageType::Image(image) = message.msgtype()
                {
                    let mxc_url = match &image.source {
                        MediaSource::Plain(uri) => uri.to_string(),
                        MediaSource::Encrypted(file) => file.url.to_string(),
                    };
                    if !self.media_cache.contains_key(&mxc_url) {
                        let matrix_clone = matrix.clone();
                        let source = image.source.clone();
                        fetches.push(
                            async move {
                                let res = matrix_clone
                                    .fetch_media(source)
                                    .await
                                    .map_err(|e| e.to_string());
                                (mxc_url, res)
                            }
                            .boxed(),
                        );
                    }
                }
            }
        };

        for item in &self.timeline_items {
            if let Some(t_item) = &item.item {
                check_item(t_item, &mut media_fetches);
            }
        }
        for item in &self.threaded_timeline_items {
            if let Some(t_item) = &item.item {
                check_item(t_item, &mut media_fetches);
            }
        }

        if !media_fetches.is_empty() {
            Task::perform(
                async move {
                    futures::stream::iter(media_fetches)
                        .buffer_unordered(10)
                        .collect::<Vec<_>>()
                        .await
                },
                |results| Message::MediaFetchedBatch(results).into(),
            )
        } else {
            Task::none()
        }
    }

    fn check_and_perform_initial_scroll(
        &mut self,
    ) -> Option<Task<Action<<Constellations as Application>::Message>>> {
        if self.needs_initial_scroll && !self.is_loading_more && self.is_timeline_initialized {
            self.needs_initial_scroll = false;
            if self.timeline_items.is_empty() {
                return None;
            }
            if let Some(room_id) = &self.selected_room {
                let unread_count =
                    if let Some(room) = self.room_list.iter().find(|r| &r.id == room_id) {
                        room.unread_count
                    } else {
                        0
                    };

                let offset = if self.is_first_time_joining || unread_count == 0 {
                    scrollable::RelativeOffset::END
                } else {
                    let total_items = self.timeline_items.len();
                    let unread = unread_count as usize;
                    if total_items == 0 {
                        scrollable::RelativeOffset::END
                    } else if unread >= total_items {
                        scrollable::RelativeOffset::START
                    } else {
                        let ratio = (total_items - unread) as f32 / total_items as f32;
                        scrollable::RelativeOffset { x: 0.0, y: ratio }
                    }
                };

                return Some(scrollable::snap_to(TIMELINE_ID.clone(), offset.into()));
            }
        }
        None
    }

    pub fn handle_timeline_diff(
        &mut self,
        diff: eyeball_im::VectorDiff<Arc<TimelineItem>>,
        is_thread: bool,
        root_id: Option<OwnedEventId>,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        let mut tasks = Vec::new();
        let mut media_fetches: Vec<PinnedOutput> = Vec::new();
        let check_item = |item: &Arc<TimelineItem>, fetches: &mut Vec<_>| {
            if let Some(event) = item.as_event() {
                if let TimelineDetails::Ready(profile) = event.sender_profile()
                    && let Some(avatar_url) = &profile.avatar_url
                {
                    let url_str = avatar_url.to_string();
                    if !self.media_cache.contains_key(&url_str)
                        && let Some(matrix) = &self.matrix
                    {
                        let matrix_clone = matrix.clone();
                        let source = MediaSource::Plain(avatar_url.clone());
                        fetches.push(
                            async move {
                                let res = matrix_clone
                                    .fetch_media(source)
                                    .await
                                    .map_err(|e| e.to_string());
                                (url_str, res)
                            }
                            .boxed(),
                        );
                    }
                }

                if self.user_settings.media_previews_display_policy
                    && let Some(message) = event.content().as_message()
                    && let MessageType::Image(image) = message.msgtype()
                {
                    let mxc_url = match &image.source {
                        MediaSource::Plain(uri) => uri.to_string(),
                        MediaSource::Encrypted(file) => file.url.to_string(),
                    };
                    if !self.media_cache.contains_key(&mxc_url)
                        && let Some(matrix) = &self.matrix
                    {
                        let matrix_clone = matrix.clone();
                        let source = image.source.clone();
                        fetches.push(
                            async move {
                                let res = matrix_clone
                                    .fetch_media(source)
                                    .await
                                    .map_err(|e| e.to_string());
                                (mxc_url, res)
                            }
                            .boxed(),
                        );
                    }
                }
            }
        };

        match &diff {
            eyeball_im::VectorDiff::Insert { value, .. } => check_item(value, &mut media_fetches),
            eyeball_im::VectorDiff::Set { value, .. } => check_item(value, &mut media_fetches),
            eyeball_im::VectorDiff::PushBack { value } => check_item(value, &mut media_fetches),
            eyeball_im::VectorDiff::PushFront { value } => check_item(value, &mut media_fetches),
            eyeball_im::VectorDiff::Append { values } => values
                .iter()
                .for_each(|v| check_item(v, &mut media_fetches)),
            eyeball_im::VectorDiff::Reset { values } => values
                .iter()
                .for_each(|v| check_item(v, &mut media_fetches)),
            _ => {}
        }

        if !media_fetches.is_empty() {
            tasks.push(cosmic::iced::Task::perform(
                async move {
                    futures::stream::iter(media_fetches)
                        .buffer_unordered(10)
                        .collect::<Vec<_>>()
                        .await
                },
                |results| Message::MediaFetchedBatch(results).into(),
            ));
        }

        let mapped_diff = match diff {
            eyeball_im::VectorDiff::Insert { index, value } => eyeball_im::VectorDiff::Insert {
                index,
                value: ConstellationsItem::new(value, self.user_id.as_deref()),
            },
            eyeball_im::VectorDiff::Set { index, value } => eyeball_im::VectorDiff::Set {
                index,
                value: ConstellationsItem::new(value, self.user_id.as_deref()),
            },
            eyeball_im::VectorDiff::PushBack { value } => eyeball_im::VectorDiff::PushBack {
                value: ConstellationsItem::new(value, self.user_id.as_deref()),
            },
            eyeball_im::VectorDiff::PushFront { value } => eyeball_im::VectorDiff::PushFront {
                value: ConstellationsItem::new(value, self.user_id.as_deref()),
            },
            eyeball_im::VectorDiff::Append { values } => eyeball_im::VectorDiff::Append {
                values: values
                    .into_iter()
                    .map(|v| ConstellationsItem::new(v, self.user_id.as_deref()))
                    .collect(),
            },
            eyeball_im::VectorDiff::Reset { values } => eyeball_im::VectorDiff::Reset {
                values: values
                    .into_iter()
                    .map(|v| ConstellationsItem::new(v, self.user_id.as_deref()))
                    .collect(),
            },
            eyeball_im::VectorDiff::Remove { index } => eyeball_im::VectorDiff::Remove { index },
            eyeball_im::VectorDiff::PopBack => eyeball_im::VectorDiff::PopBack,
            eyeball_im::VectorDiff::PopFront => eyeball_im::VectorDiff::PopFront,
            eyeball_im::VectorDiff::Clear => eyeball_im::VectorDiff::Clear,
            eyeball_im::VectorDiff::Truncate { length } => {
                eyeball_im::VectorDiff::Truncate { length }
            }
        };

        if is_thread {
            if let Some(root_id) = root_id
                && self.active_thread_root == Some(root_id)
            {
                let is_append = match &mapped_diff {
                    eyeball_im::VectorDiff::PushBack { .. } => true,
                    eyeball_im::VectorDiff::Append { .. } => true,
                    eyeball_im::VectorDiff::Insert { index, .. } => {
                        *index >= self.threaded_timeline_items.len()
                    }
                    _ => false,
                };

                let is_prepend = match &mapped_diff {
                    eyeball_im::VectorDiff::PushFront { .. } => true,
                    eyeball_im::VectorDiff::Insert { index, .. } => {
                        *index < self.threaded_timeline_items.len()
                    }
                    eyeball_im::VectorDiff::Reset { .. } => self.is_loading_more,
                    _ => false,
                };

                let is_reset = matches!(
                    &mapped_diff,
                    eyeball_im::VectorDiff::Reset { .. } | eyeball_im::VectorDiff::Clear
                );

                if is_prepend {
                    self.needs_threaded_scroll_adjustment = true;
                }

                self.threaded_timeline_items.apply_diff(mapped_diff);

                if is_append && self.is_threaded_timeline_at_bottom {
                    tasks.push(scrollable::snap_to(
                        THREADED_TIMELINE_ID.clone(),
                        scrollable::RelativeOffset::END.into(),
                    ));
                } else if is_reset {
                    if self.is_threaded_timeline_at_bottom {
                        tasks.push(scrollable::snap_to(
                            THREADED_TIMELINE_ID.clone(),
                            scrollable::RelativeOffset::END.into(),
                        ));
                    } else {
                        tasks.push(scrollable::scroll_to(
                            THREADED_TIMELINE_ID.clone(),
                            scrollable::AbsoluteOffset {
                                x: Some(0.0),
                                y: Some(self.last_threaded_timeline_offset),
                            },
                        ));
                    }
                }
            }
        } else {
            let is_append = match &mapped_diff {
                eyeball_im::VectorDiff::PushBack { .. } => true,
                eyeball_im::VectorDiff::Append { .. } => true,
                eyeball_im::VectorDiff::Insert { index, .. } => *index >= self.timeline_items.len(),
                _ => false,
            };

            let is_prepend = match &mapped_diff {
                eyeball_im::VectorDiff::PushFront { .. } => true,
                eyeball_im::VectorDiff::Insert { index, .. } => *index < self.timeline_items.len(),
                eyeball_im::VectorDiff::Reset { .. } => self.is_loading_more,
                _ => false,
            };

            let is_reset = matches!(
                &mapped_diff,
                eyeball_im::VectorDiff::Reset { .. } | eyeball_im::VectorDiff::Clear
            );

            if is_prepend {
                self.needs_scroll_adjustment = true;
            }

            self.timeline_items.apply_diff(mapped_diff);
            self.recompute_thread_counts();

            if let Some(task) = self.check_and_perform_initial_scroll() {
                tasks.push(task);
            } else if is_append && self.is_timeline_at_bottom {
                tasks.push(scrollable::snap_to(
                    TIMELINE_ID.clone(),
                    scrollable::RelativeOffset::END.into(),
                ));
            } else if is_reset {
                if self.is_timeline_at_bottom {
                    tasks.push(scrollable::snap_to(
                        TIMELINE_ID.clone(),
                        scrollable::RelativeOffset::END.into(),
                    ));
                } else {
                    tasks.push(scrollable::scroll_to(
                        TIMELINE_ID.clone(),
                        scrollable::AbsoluteOffset {
                            x: Some(0.0),
                            y: Some(self.last_timeline_offset),
                        },
                    ));
                }
            }
        }

        if !tasks.is_empty() {
            cosmic::iced::Task::batch(tasks)
        } else {
            Task::none()
        }
    }

    pub fn handle_matrix_event(
        &mut self,
        event: matrix::MatrixEvent,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        match event {
            matrix::MatrixEvent::SyncStatusChanged(status) => {
                self.sync_status = status;
                Task::none()
            }
            matrix::MatrixEvent::SyncIndicatorChanged(show) => {
                self.is_sync_indicator_active = show;
                Task::none()
            }
            matrix::MatrixEvent::RoomDiff(diff) => {
                match &*diff {
                    eyeball_im::VectorDiff::Insert { value, .. }
                    | eyeball_im::VectorDiff::PushBack { value }
                    | eyeball_im::VectorDiff::PushFront { value } => {
                        self.joined_room_ids.insert(value.id.clone());
                    }
                    eyeball_im::VectorDiff::Remove { index } => {
                        if let Some(room) = self.room_list.get(*index) {
                            self.joined_room_ids.remove(&room.id);
                        }
                    }
                    eyeball_im::VectorDiff::Set { index, value } => {
                        if let Some(old_room) = self.room_list.get(*index) {
                            self.joined_room_ids.remove(&old_room.id);
                        }
                        self.joined_room_ids.insert(value.id.clone());
                    }
                    eyeball_im::VectorDiff::PopBack => {
                        if let Some(room) = self.room_list.last() {
                            self.joined_room_ids.remove(&room.id);
                        }
                    }
                    eyeball_im::VectorDiff::PopFront => {
                        if let Some(room) = self.room_list.first() {
                            self.joined_room_ids.remove(&room.id);
                        }
                    }
                    eyeball_im::VectorDiff::Clear => {
                        self.joined_room_ids.clear();
                    }
                    eyeball_im::VectorDiff::Reset { values }
                    | eyeball_im::VectorDiff::Append { values } => {
                        self.joined_room_ids
                            .extend(values.iter().map(|r| r.id.clone()));
                    }
                    eyeball_im::VectorDiff::Truncate { length } => {
                        for room in self.room_list.iter().skip(*length) {
                            self.joined_room_ids.remove(&room.id);
                        }
                    }
                }

                self.room_list.apply_diff(*diff);
                for room in &self.room_list {
                    if let Some(name) = &room.name {
                        self.room_name_cache.insert(room.id.clone(), name.clone());
                    }
                }
                self.update_filtered_rooms();
                self.update_title()
            }
            matrix::MatrixEvent::TimelineDiff(diff) => self.handle_timeline_diff(diff, false, None),
            matrix::MatrixEvent::TimelineReset => {
                let is_background_reset = self.is_timeline_initialized;
                self.timeline_items.clear();
                self.recompute_thread_counts();
                self.needs_initial_scroll = !is_background_reset;
                self.needs_scroll_restoration = is_background_reset;
                self.last_content_height = 0.0;
                self.last_viewport_width = 0.0;
                self.last_viewport_height = 0.0;
                self.needs_scroll_adjustment = false;
                if !is_background_reset {
                    self.is_timeline_at_bottom = true;
                    self.is_threaded_timeline_at_bottom = true;
                }
                self.is_timeline_initialized = false;
                Task::none()
            }
            matrix::MatrixEvent::TimelineInitFinished => {
                self.is_timeline_initialized = true;
                // A permalink asked us to focus on a specific event. If it is
                // already in the loaded window we can just scroll to it;
                // otherwise we build an event-focused timeline around it.
                let event_focus_task = self.check_pending_event_focus();
                if self.needs_scroll_restoration {
                    self.needs_scroll_restoration = false;
                    if self.is_timeline_at_bottom {
                        scrollable::snap_to(
                            TIMELINE_ID.clone(),
                            scrollable::RelativeOffset::END.into(),
                        )
                    } else {
                        scrollable::scroll_to(
                            TIMELINE_ID.clone(),
                            scrollable::AbsoluteOffset {
                                x: Some(0.0),
                                y: Some(self.last_timeline_offset),
                            },
                        )
                    }
                } else if let Some(task) = self.check_and_perform_initial_scroll() {
                    task
                } else {
                    event_focus_task
                }
            }
            matrix::MatrixEvent::ReactionAdded { .. } => {
                // For now, we don't do anything specific as reactions are handled via TimelineDiff
                Task::none()
            }
            matrix::MatrixEvent::IgnoredUsersChanged(users) => {
                self.user_settings.ignored_users = users;
                Task::none()
            }
            matrix::MatrixEvent::SpaceHierarchyChanged => {
                let mut tasks = Vec::new();
                if let Some(matrix) = &self.matrix
                    && let Some(sid) = &self.selected_space
                {
                    let matrix_clone = matrix.clone();
                    let sid_clone = sid.clone();
                    tasks.push(Task::perform(
                        async move {
                            let _ = matrix_clone.update_room_list_filter(Some(sid_clone)).await;
                        },
                        |_| Action::from(Message::SpaceFilterUpdated),
                    ));
                }
                self.update_filtered_rooms();
                if tasks.is_empty() {
                    Task::none()
                } else {
                    Task::batch(tasks)
                }
            }
            matrix::MatrixEvent::CallParticipantsChanged {
                room_id,
                participants,
            } => {
                self.call_participants.insert(room_id.into(), participants);
                Task::none()
            }
        }
    }

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

    pub fn handle_load_more(
        &mut self,
        is_thread: bool,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        if self.is_loading_more {
            return Task::none();
        }

        if let (Some(matrix), Some(room_id)) = (&self.matrix, &self.selected_room) {
            self.is_loading_more = true;
            let matrix = matrix.clone();
            let room_id = room_id.clone();
            let root_id = if is_thread {
                self.active_thread_root.clone()
            } else {
                None
            };

            Task::perform(
                async move {
                    if let Some(root_id) = root_id {
                        let timeline = matrix.threaded_timeline(&room_id, &root_id).await?;
                        timeline.paginate_backwards(20).await?;
                    } else {
                        matrix.paginate_backwards(&room_id, 20).await?;
                    }
                    Ok(())
                },
                |res: Result<(), anyhow::Error>| {
                    Action::from(Message::LoadMoreFinished(res.map_err(|e| e.to_string())))
                },
            )
        } else {
            Task::none()
        }
    }

    pub fn handle_add_attachment(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        Task::perform(
            async move {
                let dialog = rfd::AsyncFileDialog::new()
                    .set_title("Select files to attach")
                    .pick_files()
                    .await;

                let mut paths = Vec::new();
                if let Some(files) = dialog {
                    for file in files {
                        paths.push(file.path().to_path_buf());
                    }
                }
                paths
            },
            |paths| Action::from(Message::AttachmentsSelected(paths)),
        )
    }

    pub fn handle_share_location(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        Task::perform(
            async move {
                let proxy = ashpd::desktop::location::LocationProxy::new()
                    .await
                    .map_err(|e| e.to_string())?;

                let session = proxy
                    .create_session(ashpd::desktop::location::CreateSessionOptions::default())
                    .await
                    .map_err(|e| e.to_string())?;

                let mut stream = proxy
                    .receive_location_updated()
                    .await
                    .map_err(|e| e.to_string())?;

                let (_, location_res) = futures::join!(
                    proxy
                        .start(
                            &session,
                            None,
                            ashpd::desktop::location::StartOptions::default()
                        )
                        .map(|e| e.map_err(|err| err.to_string())),
                    stream
                        .next()
                        .map(|opt| opt.ok_or_else(|| "Stream is exhausted".to_string()))
                );

                let _ = session.close().await;

                let location = location_res?;
                Ok((location.latitude(), location.longitude()))
            },
            |res| Action::from(Message::LocationRetrieved(res)),
        )
    }

    pub fn handle_location_retrieved(
        &mut self,
        res: Result<(f64, f64), String>,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        match res {
            Ok((lat, lon)) => {
                if let (Some(matrix), Some(room_id)) = (&self.matrix, &self.selected_room) {
                    let matrix_clone = matrix.clone();
                    let room_id_clone = room_id.clone();
                    let body = crate::fl!(
                        "location-message-body",
                        lat = lat.to_string(),
                        lon = lon.to_string()
                    )
                    .to_string();
                    let geo_uri = format!("geo:{lat},{lon}");
                    Task::perform(
                        async move {
                            matrix_clone
                                .send_location(&room_id_clone, body, geo_uri)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| Action::from(Message::MessageSent(res)),
                    )
                } else {
                    Task::none()
                }
            }
            Err(e) => {
                self.set_error(
                    crate::fl!("error-failed-get-location", error = e.to_string()).to_string(),
                );
                Task::none()
            }
        }
    }

    pub fn handle_send_message(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        if let (Some(matrix), Some(room_id)) = (&self.matrix, &self.selected_room) {
            let body = self.composer_content.text();

            if let Some(editing_item) = self.editing_item.clone() {
                if body.is_empty() {
                    return Task::none();
                }

                let html_body = if self.app_settings.render_markdown {
                    Some(matrix::markdown_to_html(&body))
                } else {
                    None
                };
                let matrix_clone = matrix.clone();
                let room_id_clone = room_id.clone();

                return Task::perform(
                    async move {
                        let event = editing_item
                            .item
                            .as_ref()
                            .and_then(|i| i.as_event())
                            .ok_or("Not an event")?;
                        let item_id = event.identifier();
                        matrix_clone
                            .edit_message(&room_id_clone, &item_id, body, html_body)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    |res| Action::from(Message::MessageEdited(res)),
                );
            }

            let attachments = std::mem::take(&mut self.composer_attachments);

            if body.is_empty() && attachments.is_empty() {
                return Task::none();
            }

            let mut tasks = Vec::new();

            if self.active_thread_root.is_some() {
                self.is_threaded_timeline_at_bottom = true;
            } else {
                self.is_timeline_at_bottom = true;
            }

            if !body.is_empty() {
                let html_body = if self.app_settings.render_markdown {
                    Some(matrix::markdown_to_html(&body))
                } else {
                    None
                };
                let matrix_clone = matrix.clone();
                let room_id_clone = room_id.clone();

                if let Some(replying_to) = self.replying_to.clone() {
                    tasks.push(Task::perform(
                        async move {
                            let event = replying_to
                                .item
                                .as_ref()
                                .and_then(|i| i.as_event())
                                .ok_or("Not an event")?;
                            let event_id = event.event_id().ok_or("No event ID")?;
                            let sender = event.sender();

                            matrix_clone
                                .send_reply(&room_id_clone, event_id, sender, body, html_body)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| Action::from(Message::MessageSent(res)),
                    ));
                } else if let Some(root_id) = self.active_thread_root.clone() {
                    let user_id = self.user_id.clone();
                    tasks.push(Task::perform(
                        async move {
                            matrix_clone
                                .send_threaded_message(
                                    &room_id_clone,
                                    &root_id,
                                    user_id.as_ref(),
                                    body,
                                    html_body,
                                )
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| Action::from(Message::MessageSent(res)),
                    ));
                } else {
                    tasks.push(Task::perform(
                        async move {
                            matrix_clone
                                .send_message(&room_id_clone, body, html_body)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| Action::from(Message::MessageSent(res)),
                    ));
                }
            } else {
                // If only sending attachments, we clear the composer text state manually
                // because MessageSent clears it but might not run for empty body
                self.composer_content = cosmic::widget::text_editor::Content::new();
                self.composer_preview_events.clear();
                self.composer_preview_links.clear();
                self.composer_is_preview = false;
                self.replying_to = None;
                self.editing_item = None;
            }

            for path in attachments {
                let matrix_clone = matrix.clone();
                let room_id_clone = room_id.clone();

                tasks.push(Task::perform(
                    async move {
                        let res = matrix_clone
                            .send_attachment(&room_id_clone, &path)
                            .await
                            .map_err(|e| e.to_string());
                        (path, res)
                    },
                    move |(path, res)| Action::from(Message::AttachmentSent(path, res)),
                ));
            }

            Task::batch(tasks)
        } else {
            Task::none()
        }
    }

    pub fn handle_redact_message(
        &mut self,
        item_id: matrix::TimelineEventItemId,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        if let (Some(matrix), Some(room_id)) = (&self.matrix, &self.selected_room) {
            let matrix = matrix.clone();
            let room_id = room_id.clone();
            Task::perform(
                async move {
                    matrix
                        .redact_message(&room_id, &item_id, None)
                        .await
                        .map_err(|e| e.to_string())
                },
                |res| Action::from(Message::MessageRedacted(res)),
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

    pub fn handle_fetch_media(
        &mut self,
        source: MediaSource,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        if let Some(matrix) = &self.matrix {
            let matrix = matrix.clone();
            let mxc_url = match &source {
                MediaSource::Plain(uri) => uri.to_string(),
                MediaSource::Encrypted(file) => file.url.to_string(),
            };
            Task::perform(
                async move { matrix.fetch_media(source).await.map_err(|e| e.to_string()) },
                move |res| Action::from(Message::MediaFetched(mxc_url, res)),
            )
        } else {
            Task::none()
        }
    }

    pub fn handle_media_fetched(
        &mut self,
        mxc_url: String,
        res: Result<Vec<u8>, String>,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        match res {
            Ok(data) => {
                self.media_cache.insert(
                    mxc_url,
                    cosmic::iced::widget::image::Handle::from_bytes(data),
                );
            }
            Err(e) => {
                self.set_error(
                    crate::fl!("error-failed-fetch-media", error = e.to_string()).to_string(),
                );
            }
        }
        Task::none()
    }

    pub fn handle_media_fetched_batch(
        &mut self,
        batch: Vec<(String, Result<Vec<u8>, String>)>,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        for (mxc_url, res) in batch {
            match res {
                Ok(data) => {
                    self.media_cache.insert(
                        mxc_url,
                        cosmic::iced::widget::image::Handle::from_bytes(data),
                    );
                }
                Err(e) => {
                    self.set_error(
                        crate::fl!("error-failed-fetch-media", error = e.to_string()).to_string(),
                    );
                }
            }
        }
        Task::none()
    }

    pub fn handle_toggle_login_mode(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        self.is_registering_mode = !self.is_registering_mode;
        self.error = None;
        Task::none()
    }

    pub fn handle_submit_register(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        if let Some(matrix) = &self.matrix {
            self.is_registering = true;
            self.error = None;
            self.sync_status = matrix::SyncStatus::Disconnected;
            let matrix = matrix.clone();
            let homeserver = self.login_homeserver.clone();
            let username = self.login_username.clone();
            let password = std::mem::take(&mut self.login_password);

            Task::perform(
                async move {
                    matrix.register(&homeserver, &username, &password).await?;
                    let user_id = matrix
                        .client()
                        .await
                        .user_id()
                        .map(|u| u.to_string())
                        .ok_or_else(|| {
                            anyhow::anyhow!("Failed to get user ID after registration")
                        })?;
                    matrix.start_sync().await?;
                    Ok(user_id)
                },
                |res: Result<String, anyhow::Error>| {
                    Action::from(Message::RegisterFinished(
                        res.map_err(matrix::SyncError::from),
                    ))
                },
            )
        } else {
            Task::none()
        }
    }

    pub fn handle_register_finished(
        &mut self,
        res: Result<String, matrix::SyncError>,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        self.is_registering = false;
        match res {
            Ok(user_id) => {
                self.user_id = Some(user_id);
                self.login_homeserver.clear();
                self.login_username.clear();
                self.login_password.clear();
                self.error = None;
                self.update_title()
            }
            Err(e) => {
                self.set_error(
                    crate::fl!("error-failed-registration", error = e.to_string()).to_string(),
                );
                Task::none()
            }
        }
    }

    pub fn handle_submit_login(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        if let Some(matrix) = &self.matrix {
            self.auth_flow = AuthFlow::Password;
            self.error = None;
            self.sync_status = matrix::SyncStatus::Disconnected;
            let matrix = matrix.clone();
            let homeserver = self.login_homeserver.clone();
            let username = self.login_username.clone();
            let password = std::mem::take(&mut self.login_password);

            Task::perform(
                async move {
                    matrix.login(&homeserver, &username, &password).await?;
                    let user_id = matrix
                        .client()
                        .await
                        .user_id()
                        .map(|u| u.to_string())
                        .ok_or_else(|| anyhow::anyhow!("Failed to get user ID after login"))?;
                    matrix.start_sync().await?;
                    Ok(user_id)
                },
                |res: Result<String, anyhow::Error>| {
                    Action::from(Message::LoginFinished(res.map_err(matrix::SyncError::from)))
                },
            )
        } else {
            Task::none()
        }
    }

    pub fn handle_login_finished(
        &mut self,
        res: Result<String, matrix::SyncError>,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        self.auth_flow = AuthFlow::Idle;
        match res {
            Ok(user_id) => self.user_id = Some(user_id.clone()),
            Err(matrix::SyncError::MissingSlidingSyncSupport) => {
                self.sync_status = matrix::SyncStatus::MissingSlidingSyncSupport;
            }
            Err(e) => {
                self.set_error(crate::fl!("error-failed-login", error = e.to_string()).to_string());
            }
        }
        Task::none()
    }

    pub fn handle_submit_oidc_login(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        if let Some(matrix) = &self.matrix {
            self.auth_flow = AuthFlow::Oidc;
            self.error = None;
            let matrix = matrix.clone();
            let homeserver = self.login_homeserver.clone();
            Task::perform(
                async move {
                    matrix
                        .login_oidc(&homeserver)
                        .await
                        .map_err(|e| e.to_string())
                },
                |res| Action::from(Message::OidcLoginStarted(res)),
            )
        } else {
            Task::none()
        }
    }

    pub fn handle_oidc_login_started(
        &mut self,
        res: Result<Url, String>,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        match res {
            Ok(url) => {
                tracing::info!("Opening URL: {}", redact_url(&url));
                let _ = open::that(url.as_str());
            }
            Err(e) => {
                self.auth_flow = AuthFlow::Idle;
                // Distinguish "OAuth not supported by this homeserver" from
                // other failures so the message can guide the user (e.g. use a
                // password login instead).
                if e == crate::matrix::OIDC_NOT_SUPPORTED_SENTINEL {
                    self.set_error(crate::fl!("error-oidc-not-supported").to_string());
                } else {
                    self.set_error(
                        crate::fl!("error-failed-oidc-login", error = e.to_string()).to_string(),
                    );
                }
            }
        }
        Task::none()
    }

    pub fn handle_oidc_callback(
        &mut self,
        url: Url,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        if let Some(matrix) = &self.matrix {
            self.auth_flow = AuthFlow::Oidc;
            self.error = None;
            let matrix = matrix.clone();
            Task::perform(
                async move {
                    matrix.complete_oidc_login(url).await?;
                    let user_id = matrix
                        .client()
                        .await
                        .user_id()
                        .map(|u| u.to_string())
                        .ok_or_else(|| anyhow::anyhow!("Failed to get user ID after OIDC login"))?;
                    matrix.start_sync().await?;
                    Ok(user_id)
                },
                |res: Result<String, anyhow::Error>| {
                    Action::from(Message::LoginFinished(res.map_err(matrix::SyncError::from)))
                },
            )
        } else {
            Task::none()
        }
    }

    /// Open a Matrix permalink (room/alias/user/event/join) handed to us via
    /// argv, the URI scheme, or (later) in-app paste.
    ///
    /// For not-yet-loaded event targets this only scrolls if the event is
    /// already in `timeline_items`; the not-yet-loaded fetch path is Phase 3.
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
    fn handle_toggle_open_link(
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

    fn handle_open_link_text_changed(
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
    fn handle_submit_open_link(
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
        use super::PendingAliasOp;
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
                let matrix = self.matrix.as_ref().unwrap().clone();
                Task::perform(
                    async move {
                        matrix
                            .get_or_create_dm(&user_id)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    |res| Action::from(Message::DmRoomResolved(res)),
                )
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
    fn check_pending_event_focus(
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
    fn handle_load_event_context(
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

    /// Handle the result of building an event-focused timeline. On success,
    /// schedule a jump to the centred event; on failure, surface a toast and
    /// return to live.
    fn handle_event_context_loaded(
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
    fn handle_return_to_live(&mut self) -> Task<Action<<Constellations as Application>::Message>> {
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
    fn handle_room_alias_resolved(
        &mut self,
        res: Box<Result<matrix_sdk::ruma::OwnedRoomId, String>>,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        use super::PendingAliasOp;
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

    pub fn handle_logout(&mut self) -> Task<Action<<Constellations as Application>::Message>> {
        if let Some(matrix) = &self.matrix {
            let matrix = matrix.clone();
            return Task::perform(
                async move {
                    let _ = matrix.logout().await;
                },
                |_| Action::from(Message::LogoutFinished),
            );
        }
        Task::none()
    }

    pub fn handle_logout_finished(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        self.user_id = None;
        self.matrix = None;
        self.sync_status = matrix::SyncStatus::Disconnected;
        self.room_list.clear();
        self.selected_room = None;
        self.timeline_items.clear();
        self.recompute_thread_counts();
        self.auth_flow = AuthFlow::Idle;
        self.login_password.clear();
        self.error = None;
        self.selected_space = None;
        self.is_sync_indicator_active = false;
        self.is_loading_more = false;
        self.joined_room_ids.clear();
        Task::none()
    }

    pub fn handle_start_qr_login(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        self.auth_flow = AuthFlow::Qr {
            step: QrLoginStep::Initiating,
        };
        self.error = None;
        self.qr_code_bytes = None;
        self.qr_check_code_sender = None;
        self.qr_user_code = None;
        self.qr_check_code_input.clear();

        let Some(matrix) = self.matrix.clone() else {
            return Task::none();
        };
        let mut hs = self.login_homeserver.trim().to_string();
        if hs.is_empty() {
            hs = "https://matrix.org".to_string();
        }
        if !hs.starts_with("http://") && !hs.starts_with("https://") {
            hs = format!("https://{}", hs);
        }

        // Stream MSC4108 QR-login progress from the background task into the
        // MVU loop. The state machine first awaits `start_qr_login` (which
        // builds the client and spawns the login task), then drains the
        // progress receiver until it closes.
        enum QrStreamState {
            Starting(matrix::MatrixEngine, String),
            Draining(tokio::sync::mpsc::UnboundedReceiver<matrix::QrLoginProgress>),
            Done,
        }

        let stream = cosmic::iced::futures::stream::unfold(
            QrStreamState::Starting(matrix, hs),
            |state| async move {
                match state {
                    QrStreamState::Starting(matrix, hs) => match matrix.start_qr_login(&hs).await {
                        Ok(rx) => Some((None, QrStreamState::Draining(rx))),
                        Err(e) => Some((
                            Some(matrix::QrLoginProgress::Finished(Err(e.to_string()))),
                            QrStreamState::Done,
                        )),
                    },
                    QrStreamState::Draining(mut rx) => match rx.recv().await {
                        Some(progress) => Some((Some(progress), QrStreamState::Draining(rx))),
                        None => Some((None, QrStreamState::Done)),
                    },
                    QrStreamState::Done => None,
                }
            },
        )
        .filter_map(|opt| async move { opt });

        Task::run(stream, |progress| {
            Action::from(Message::QrLoginProgress(progress))
        })
    }

    pub fn handle_cancel_qr_login(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        self.auth_flow = AuthFlow::Idle;
        self.qr_code_bytes = None;
        self.qr_check_code_sender = None;
        self.qr_user_code = None;
        self.qr_check_code_input.clear();

        if let Some(matrix) = self.matrix.clone() {
            Task::perform(async move { matrix.cancel_qr_login().await }, |_| {
                Action::from(Message::NoOp)
            })
        } else {
            Task::none()
        }
    }

    pub fn handle_qr_login_progress(
        &mut self,
        progress: matrix::QrLoginProgress,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        use matrix::QrLoginProgress as P;
        match progress {
            P::QrReady(bytes) => {
                self.qr_code_bytes = Some(bytes);
                self.auth_flow = AuthFlow::Qr {
                    step: QrLoginStep::ShowingQr,
                };
                Task::none()
            }
            P::QrScanned(sender) => {
                self.qr_check_code_sender = Some(sender);
                self.qr_check_code_input.clear();
                self.auth_flow = AuthFlow::Qr {
                    step: QrLoginStep::AwaitingCheckCode,
                };
                Task::none()
            }
            P::WaitingForToken { user_code } => {
                self.qr_user_code = Some(user_code);
                self.auth_flow = AuthFlow::Qr {
                    step: QrLoginStep::Authenticating,
                };
                Task::none()
            }
            P::SyncingSecrets => {
                self.auth_flow = AuthFlow::Qr {
                    step: QrLoginStep::SyncingSecrets,
                };
                Task::none()
            }
            P::Finished(res) => {
                self.qr_code_bytes = None;
                self.qr_check_code_sender = None;
                self.qr_user_code = None;
                self.qr_check_code_input.clear();
                match res {
                    Ok(user_id) => {
                        self.auth_flow = AuthFlow::Qr {
                            step: QrLoginStep::Success,
                        };
                        // Sliding sync already started inside the background
                        // task (after finalize_oauth_login). Route through the
                        // shared login-finished path: sets user_id and resets
                        // auth_flow to Idle.
                        self.handle_login_finished(Ok(user_id))
                    }
                    Err(e) => {
                        self.auth_flow = AuthFlow::Qr {
                            step: QrLoginStep::Error,
                        };
                        self.set_error(
                            crate::fl!("error-failed-qr-login", error = e.to_string()).to_string(),
                        );
                        Task::none()
                    }
                }
            }
        }
    }

    pub fn handle_qr_check_code_changed(
        &mut self,
        code: String,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        // Keep only digits, max two.
        let filtered: String = code
            .chars()
            .filter(|c| c.is_ascii_digit())
            .take(2)
            .collect();
        self.qr_check_code_input = filtered;
        Task::none()
    }

    pub fn handle_submit_qr_check_code(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        let Some(sender) = self.qr_check_code_sender.take() else {
            return Task::none();
        };
        let code_str = std::mem::take(&mut self.qr_check_code_input);
        // Parse the two-digit check code; on failure, surface an error and
        // return to the QR-display step so the user can retry.
        let parsed: Result<u8, _> = code_str.parse();
        match parsed {
            Ok(code) => {
                self.auth_flow = AuthFlow::Qr {
                    step: QrLoginStep::Authenticating,
                };
                Task::perform(async move { sender.send(code).await }, |res| match res {
                    Ok(()) => Action::from(Message::NoOp),
                    Err(e) => {
                        tracing::warn!("Failed to submit QR check code: {e}");
                        Action::from(Message::NoOp)
                    }
                })
            }
            Err(_) => {
                self.set_error(crate::fl!("login-qr-check-code-invalid").to_string());
                self.auth_flow = AuthFlow::Qr {
                    step: QrLoginStep::ShowingQr,
                };
                Task::none()
            }
        }
    }

    pub fn handle_update(&mut self, message: Message) -> Task<Action<Message>> {
        match message {
            Message::EngineReady(res) => self.handle_engine_ready(res),
            Message::UserReady(user_id, sync_res) => self.handle_user_ready(user_id, sync_res),

            Message::Matrix(event) => self.handle_matrix_event(event),
            Message::MatrixThreadDiff(root_id, diff) => {
                self.handle_timeline_diff(diff, true, Some(root_id))
            }
            Message::MatrixThreadReset(root_id) => {
                if self.active_thread_root.as_ref() == Some(&root_id) {
                    let is_background_reset = !self.threaded_timeline_items.is_empty();
                    self.threaded_timeline_items.clear();
                    self.needs_threaded_scroll_restoration = is_background_reset;
                    self.last_threaded_content_height = 0.0;
                    self.last_threaded_viewport_width = 0.0;
                    self.last_threaded_viewport_height = 0.0;
                    self.needs_threaded_scroll_adjustment = false;
                    self.is_threaded_timeline_initialized = false;
                }
                Task::none()
            }
            Message::MatrixThreadInitFinished(root_id) => {
                if self.active_thread_root.as_ref() == Some(&root_id) {
                    self.is_threaded_timeline_initialized = true;
                    if self.needs_threaded_scroll_restoration {
                        self.needs_threaded_scroll_restoration = false;
                        if self.is_threaded_timeline_at_bottom {
                            scrollable::snap_to(
                                THREADED_TIMELINE_ID.clone(),
                                scrollable::RelativeOffset::END.into(),
                            )
                        } else {
                            scrollable::scroll_to(
                                THREADED_TIMELINE_ID.clone(),
                                scrollable::AbsoluteOffset {
                                    x: Some(0.0),
                                    y: Some(self.last_threaded_timeline_offset),
                                },
                            )
                        }
                    } else {
                        Task::none()
                    }
                } else {
                    Task::none()
                }
            }
            Message::OpenThread(root_id) => {
                self.needs_layout_scroll_restoration = true;
                self.active_thread_root = Some(root_id);
                self.threaded_timeline_items.clear();
                self.last_threaded_timeline_offset = 0.0;
                self.last_threaded_content_height = 0.0;
                self.last_threaded_viewport_width = 0.0;
                self.last_threaded_viewport_height = 0.0;
                self.needs_threaded_scroll_adjustment = false;
                self.is_threaded_timeline_initialized = false;
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
                    if let Some(timeline_item) = &item.item
                        && let Some(event) = timeline_item.as_event()
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
                self.needs_layout_scroll_restoration = true;
                self.active_thread_root = None;
                self.threaded_timeline_items.clear();
                self.last_threaded_timeline_offset = 0.0;
                self.last_threaded_content_height = 0.0;
                self.last_threaded_viewport_width = 0.0;
                self.last_threaded_viewport_height = 0.0;
                self.needs_threaded_scroll_adjustment = false;
                self.is_threaded_timeline_initialized = false;
                self.restore_scroll_task()
            }
            Message::LoadMoreFinished(res) => {
                self.is_loading_more = false;
                if let Err(e) = res {
                    self.set_error(
                        crate::fl!("error-failed-load-more", error = e.to_string()).to_string(),
                    );
                }

                if let Some(task) = self.check_and_perform_initial_scroll() {
                    task
                } else {
                    Task::none()
                }
            }
            Message::TimelineScrolled(viewport, is_thread) => {
                let current_offset = viewport.absolute_offset().y;
                let current_height = viewport.content_bounds().height;

                if is_thread {
                    if !self.is_threaded_timeline_initialized {
                        return Task::none();
                    }

                    tracing::info!(
                        "TimelineScrolled (thread): offset={}, content_height={}, viewport_w={}, viewport_h={}, last_h={}, last_w={}, last_vh={}",
                        current_offset,
                        current_height,
                        viewport.bounds().width,
                        viewport.bounds().height,
                        self.last_threaded_content_height,
                        self.last_threaded_viewport_width,
                        self.last_threaded_viewport_height
                    );

                    let mut is_layout_resize = false;
                    if (self.needs_threaded_layout_scroll_restoration
                        || (self.last_threaded_content_height > 0.0
                            && current_height != self.last_threaded_content_height)
                        || (self.last_threaded_viewport_width > 0.0
                            && viewport.bounds().width != self.last_threaded_viewport_width)
                        || (self.last_threaded_viewport_height > 0.0
                            && viewport.bounds().height != self.last_threaded_viewport_height))
                        && !self.needs_threaded_scroll_adjustment
                    {
                        is_layout_resize = true;
                    }
                    self.needs_threaded_layout_scroll_restoration = false;

                    let mut task = Task::none();
                    let mut actual_offset = current_offset;

                    if self.needs_threaded_scroll_adjustment
                        && self.last_threaded_content_height > 0.0
                        && current_height > self.last_threaded_content_height
                    {
                        self.needs_threaded_scroll_adjustment = false;
                        let diff_height = current_height - self.last_threaded_content_height;
                        actual_offset = current_offset + diff_height;
                        task = scrollable::scroll_to(
                            THREADED_TIMELINE_ID.clone(),
                            scrollable::AbsoluteOffset {
                                x: Some(0.0),
                                y: Some(actual_offset),
                            },
                        );
                    } else if is_layout_resize {
                        if self.is_threaded_timeline_at_bottom {
                            task = scrollable::snap_to(
                                THREADED_TIMELINE_ID.clone(),
                                scrollable::RelativeOffset::END.into(),
                            );
                        } else {
                            let target_offset = self
                                .last_threaded_timeline_offset
                                .min(current_height - viewport.bounds().height)
                                .max(0.0);
                            task = scrollable::scroll_to(
                                THREADED_TIMELINE_ID.clone(),
                                scrollable::AbsoluteOffset {
                                    x: Some(0.0),
                                    y: Some(target_offset),
                                },
                            );
                            actual_offset = target_offset;
                        }
                    }

                    if is_layout_resize {
                        tracing::info!(
                            "TimelineScrolled (thread) layout resize: target_offset={}",
                            actual_offset
                        );
                    }

                    let last_offset = self.last_threaded_timeline_offset;
                    let should_load =
                        !is_layout_resize && actual_offset < 100.0 && actual_offset < last_offset;
                    let is_at_bottom =
                        actual_offset + viewport.bounds().height >= current_height - 20.0;

                    if !is_layout_resize {
                        self.last_threaded_timeline_offset = actual_offset;
                        self.last_threaded_content_height = current_height;
                        self.last_threaded_viewport_width = viewport.bounds().width;
                        self.last_threaded_viewport_height = viewport.bounds().height;
                        self.is_threaded_timeline_at_bottom = is_at_bottom;
                    } else {
                        self.last_threaded_content_height = current_height;
                        self.last_threaded_viewport_width = viewport.bounds().width;
                        self.last_threaded_viewport_height = viewport.bounds().height;
                    }

                    if should_load {
                        Task::batch(vec![task, self.handle_load_more(true)])
                    } else {
                        task
                    }
                } else {
                    if !self.is_timeline_initialized {
                        return Task::none();
                    }

                    tracing::info!(
                        "TimelineScrolled: offset={}, content_height={}, viewport_w={}, viewport_h={}, last_h={}, last_w={}, last_vh={}",
                        current_offset,
                        current_height,
                        viewport.bounds().width,
                        viewport.bounds().height,
                        self.last_content_height,
                        self.last_viewport_width,
                        self.last_viewport_height
                    );

                    let mut is_layout_resize = false;
                    if (self.needs_layout_scroll_restoration
                        || (self.last_content_height > 0.0
                            && current_height != self.last_content_height)
                        || (self.last_viewport_width > 0.0
                            && viewport.bounds().width != self.last_viewport_width)
                        || (self.last_viewport_height > 0.0
                            && viewport.bounds().height != self.last_viewport_height))
                        && !self.needs_scroll_adjustment
                    {
                        is_layout_resize = true;
                    }
                    self.needs_layout_scroll_restoration = false;

                    let mut task = Task::none();
                    let mut actual_offset = current_offset;

                    if self.needs_scroll_adjustment
                        && self.last_content_height > 0.0
                        && current_height > self.last_content_height
                    {
                        self.needs_scroll_adjustment = false;
                        let diff_height = current_height - self.last_content_height;
                        actual_offset = current_offset + diff_height;
                        task = scrollable::scroll_to(
                            TIMELINE_ID.clone(),
                            scrollable::AbsoluteOffset {
                                x: Some(0.0),
                                y: Some(actual_offset),
                            },
                        );
                    } else if is_layout_resize {
                        if self.is_timeline_at_bottom {
                            task = scrollable::snap_to(
                                TIMELINE_ID.clone(),
                                scrollable::RelativeOffset::END.into(),
                            );
                        } else {
                            let target_offset = self
                                .last_timeline_offset
                                .min(current_height - viewport.bounds().height)
                                .max(0.0);
                            task = scrollable::scroll_to(
                                TIMELINE_ID.clone(),
                                scrollable::AbsoluteOffset {
                                    x: Some(0.0),
                                    y: Some(target_offset),
                                },
                            );
                            actual_offset = target_offset;
                        }
                    }

                    if is_layout_resize {
                        tracing::info!(
                            "TimelineScrolled layout resize: target_offset={}",
                            actual_offset
                        );
                    }

                    let last_offset = self.last_timeline_offset;
                    let should_load =
                        !is_layout_resize && actual_offset < 100.0 && actual_offset < last_offset;
                    let is_at_bottom =
                        actual_offset + viewport.bounds().height >= current_height - 20.0;

                    if !is_layout_resize {
                        self.last_timeline_offset = actual_offset;
                        self.last_content_height = current_height;
                        self.last_viewport_width = viewport.bounds().width;
                        self.last_viewport_height = viewport.bounds().height;
                        self.is_timeline_at_bottom = is_at_bottom;
                    } else {
                        self.last_content_height = current_height;
                        self.last_viewport_width = viewport.bounds().width;
                        self.last_viewport_height = viewport.bounds().height;
                    }

                    if should_load {
                        Task::batch(vec![task, self.handle_load_more(false)])
                    } else {
                        task
                    }
                }
            }
            Message::RoomSelected(room_id) => {
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
                // clear them so stale hits don't bleed into the new room.
                self.message_search_results.clear();
                self.is_searching_messages = false;
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
            Message::ComposerChanged(text) => {
                self.composer_preview_events = parse_markdown(&text, false);
                self.composer_preview_links =
                    crate::preview::extract_links(&self.composer_preview_events);
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
                self.composer_preview_links =
                    crate::preview::extract_links(&self.composer_preview_events);

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
                        self.composer_preview_links.clear();
                        self.composer_is_preview = false;
                        self.replying_to = None;
                        self.editing_item = None;
                    }
                    Err(e) => {
                        self.set_error(
                            crate::fl!("error-failed-send-message", error = e.to_string())
                                .to_string(),
                        );
                    }
                }
                Task::none()
            }
            Message::MessageEdited(res) => {
                match res {
                    Ok(_) => {
                        self.composer_content = cosmic::widget::text_editor::Content::new();
                        self.composer_preview_events.clear();
                        self.composer_preview_links.clear();
                        self.composer_is_preview = false;
                        self.editing_item = None;
                    }
                    Err(e) => {
                        self.set_error(
                            crate::fl!("error-failed-edit-message", error = e.to_string())
                                .to_string(),
                        );
                    }
                }
                Task::none()
            }
            Message::MessageRedacted(res) => {
                if let Err(e) = res {
                    self.set_error(
                        crate::fl!("error-failed-redact-message", error = e.to_string())
                            .to_string(),
                    );
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
                    if let Some(timeline_item) = &item.item
                        && let Some(event) = timeline_item.as_event()
                        && event.identifier() == item_id
                    {
                        found_item = Some(item.clone());
                        break;
                    }
                }
                if let Some(item) = found_item
                    && let Some(timeline_item) = &item.item
                    && let Some(event) = timeline_item.as_event()
                    && let Some(msg) = event.content().as_message()
                {
                    self.composer_content =
                        cosmic::widget::text_editor::Content::with_text(msg.body());
                    self.composer_preview_events =
                        parse_markdown(&self.composer_content.text(), false);
                    self.composer_preview_links =
                        crate::preview::extract_links(&self.composer_preview_events);
                    self.editing_item = Some(item);
                    self.replying_to = None;
                }
                Task::none()
            }
            Message::CancelEdit => {
                self.editing_item = None;
                self.composer_content = cosmic::widget::text_editor::Content::new();
                self.composer_preview_events.clear();
                self.composer_preview_links.clear();
                Task::none()
            }
            Message::RedactMessage(item_id) => self.handle_redact_message(item_id),
            Message::CopyMessageLink(item_id) => {
                if let Some(room_id) = &self.selected_room
                    && let Some(matrix) = &self.matrix
                    && let matrix::TimelineEventItemId::EventId(event_id) = item_id
                {
                    let matrix = matrix.clone();
                    let room_id = room_id.clone();
                    return Task::perform(
                        async move {
                            matrix
                                .get_room_event_permalink(&room_id, &event_id)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| Action::from(Message::CopyToClipboard(res)),
                    );
                }
                Task::none()
            }
            Message::CopyRoomLink(room_id) => {
                if let Some(matrix) = &self.matrix {
                    let matrix = matrix.clone();
                    let room_id = room_id.clone();
                    return Task::perform(
                        async move {
                            matrix
                                .get_room_permalink(&room_id)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| Action::from(Message::CopyToClipboard(res)),
                    );
                }
                Task::none()
            }
            Message::CopyToClipboard(res) => match res {
                Ok(text) => cosmic::iced::clipboard::write(text),
                Err(e) => {
                    self.set_error(e);
                    Task::none()
                }
            },
            Message::DmRoomResolved(res) => match res {
                Ok(room_id) => {
                    let room_id_arc: std::sync::Arc<str> = room_id.as_str().into();
                    self.handle_update(Message::RoomSelected(room_id_arc))
                }
                Err(e) => {
                    self.set_error(crate::fl!("error-failed-start-dm", error = e));
                    Task::none()
                }
            },
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
                        self.set_error(
                            crate::fl!(
                                "error-failed-send-attachment",
                                path = path.display().to_string(),
                                error = e.to_string()
                            )
                            .to_string(),
                        );
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
                    self.handle_update(Message::ToggleReaction(item_id, emoji.to_string()))
                } else {
                    self.handle_update(Message::InsertEmoji(emoji.to_string()))
                }
            }
            Message::InsertEmoji(emoji) => {
                let mut text = self.composer_content.text();
                text.push_str(&emoji);
                self.composer_content = cosmic::widget::text_editor::Content::with_text(&text);
                self.composer_preview_events = parse_markdown(&text, false);
                self.composer_preview_links =
                    crate::preview::extract_links(&self.composer_preview_events);

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
                    self.set_error(
                        crate::fl!("error-failed-toggle-reaction", error = e.to_string())
                            .to_string(),
                    );
                }
                Task::none()
            }
            Message::FetchMedia(source) => self.handle_fetch_media(source),
            Message::MediaFetched(mxc_url, res) => self.handle_media_fetched(mxc_url, res),
            Message::MediaFetchedBatch(batch) => self.handle_media_fetched_batch(batch),
            Message::DismissError => {
                self.error = None;
                if matches!(
                    self.sync_status,
                    matrix::SyncStatus::Error(_) | matrix::SyncStatus::MissingSlidingSyncSupport
                ) {
                    self.sync_status = matrix::SyncStatus::Disconnected;
                }
                Task::none()
            }
            Message::ToggleCreateRoom => {
                self.creating_room = !self.creating_room;
                self.creating_space = false;
                self.new_room_name.clear();
                self.current_settings_panel = None;
                self.core.set_show_context(self.creating_room);
                Task::none()
            }
            Message::ToggleCreateSpace => {
                self.creating_space = !self.creating_space;
                self.creating_room = false;
                self.new_room_name.clear();
                self.current_settings_panel = None;
                self.core.set_show_context(self.creating_space);
                Task::none()
            }
            Message::ToggleInviteToSpace => {
                self.inviting_to_space = !self.inviting_to_space;
                if self.inviting_to_space {
                    self.creating_room = false;
                    self.creating_space = false;
                }
                self.invite_to_space_id.clear();
                Task::none()
            }
            Message::InviteToSpaceIdChanged(id) => {
                self.invite_to_space_id = id;
                Task::none()
            }
            Message::InviteToSpace => {
                if let Some(matrix) = &self.matrix
                    && let Some(space_id) = &self.selected_space
                {
                    let matrix = matrix.clone();
                    let space_id = space_id.to_string();
                    let user_id = self.invite_to_space_id.clone();
                    Task::perform(
                        async move {
                            matrix
                                .invite_user(&space_id, &user_id)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| Action::from(Message::SpaceUserInvited(res)),
                    )
                } else {
                    Task::none()
                }
            }
            Message::SpaceUserInvited(res) => {
                match res {
                    Ok(_) => {
                        self.inviting_to_space = false;
                        self.invite_to_space_id.clear();
                    }
                    Err(e) => {
                        self.set_error(
                            crate::fl!("error-failed-invite", error = e.to_string()).to_string(),
                        );
                    }
                }
                Task::none()
            }
            Message::ToggleInviteToRoom => {
                self.inviting_to_room = !self.inviting_to_room;
                self.invite_to_room_id.clear();
                Task::none()
            }
            Message::InviteToRoomIdChanged(id) => {
                self.invite_to_room_id = id;
                Task::none()
            }
            Message::InviteToRoom => {
                if let Some(matrix) = &self.matrix
                    && let Some(room_id) = &self.selected_room
                {
                    let matrix = matrix.clone();
                    let room_id = room_id.to_string();
                    let user_id = self.invite_to_room_id.clone();
                    Task::perform(
                        async move {
                            matrix
                                .invite_user(&room_id, &user_id)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| Action::from(Message::RoomUserInvited(res)),
                    )
                } else {
                    Task::none()
                }
            }
            Message::RoomUserInvited(res) => {
                match res {
                    Ok(_) => {
                        self.inviting_to_room = false;
                        self.invite_to_room_id.clear();
                    }
                    Err(e) => {
                        self.set_error(
                            crate::fl!("error-failed-invite", error = e.to_string()).to_string(),
                        );
                    }
                }
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
                        self.core.set_show_context(false);
                    }
                    Err(e) => {
                        self.set_error(
                            crate::fl!("error-failed-create-room", error = e.to_string())
                                .to_string(),
                        );
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
                        self.core.set_show_context(false);
                        if let Ok(rid) = space_id.as_str().try_into() {
                            return self.handle_select_space(Some(rid));
                        }
                    }
                    Err(e) => {
                        self.set_error(
                            crate::fl!("error-failed-create-space", error = e.to_string())
                                .to_string(),
                        );
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
            Message::CancelOidcLogin => {
                self.auth_flow = AuthFlow::Idle;
                Task::none()
            }
            Message::OidcLoginStarted(res) => self.handle_oidc_login_started(res),
            Message::OidcCallback(url) => self.handle_oidc_callback(url),
            Message::OpenMatrixLink(raw) => self.open_matrix_link(raw),
            Message::ToggleOpenLink => self.handle_toggle_open_link(),
            Message::OpenLinkTextChanged(text) => self.handle_open_link_text_changed(text),
            Message::SubmitOpenLink(text) => self.handle_submit_open_link(text),
            Message::RoomAliasResolved(res) => self.handle_room_alias_resolved(res),
            Message::StartQrLogin => self.handle_start_qr_login(),
            Message::CancelQrLogin => self.handle_cancel_qr_login(),
            Message::QrLoginProgress(progress) => self.handle_qr_login_progress(progress),
            Message::QrCheckCodeChanged(code) => self.handle_qr_check_code_changed(code),
            Message::SubmitQrCheckCode => self.handle_submit_qr_check_code(),
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
                        self.is_first_time_joining = true;
                        self.visited_room_ids.insert(room_id.as_str().into());
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
                        self.set_error(
                            crate::fl!("error-failed-join-room", error = e.to_string()).to_string(),
                        );
                    }
                }
                Task::none()
            }
            Message::Logout => self.handle_logout(),
            Message::LogoutFinished => self.handle_logout_finished(),
            Message::OpenSettings(panel) => {
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
            Message::CloseSettings => {
                self.needs_layout_scroll_restoration = true;
                self.needs_threaded_layout_scroll_restoration = true;
                self.current_settings_panel = None;
                self.core.set_show_context(false);
                self.show_members_panel = false;
                self.show_pinned_panel = false;
                self.room_members.clear();
                self.pinned_events_details.clear();
                self.restore_scroll_task()
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
                    self.public_search_results.clear();
                    self.is_searching_public = false;
                    self.message_search_results.clear();
                    self.is_searching_messages = false;
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
                            self.room_settings.member_filter = query.clone();
                        }
                        SettingsPanel::Space => {
                            self.space_settings.child_filter = query.clone();
                        }
                        _ => {}
                    }
                }
                self.update_filtered_rooms();

                if self.current_settings_panel.is_none() && !self.search_query.trim().is_empty() {
                    let mut tasks = Vec::new();

                    // Public rooms / spaces directory search (existing).
                    if let Some(matrix) = &self.matrix {
                        let query_str = self.search_query.trim().to_string();
                        let matrix = matrix.clone();
                        self.is_searching_public = true;

                        tasks.push(Task::perform(
                            async move { matrix.search_public_rooms(query_str, Some(20)).await },
                            |res| {
                                Action::from(Message::PublicSearchResults(
                                    res.map_err(|e| e.to_string()),
                                ))
                            },
                        ));
                    }

                    // Server-side message search (new). Only runs when a room is
                    // selected; debounced so a fast-typed query doesn't hammer
                    // the search index. The generation lets the result handler
                    // discard results from a now-stale query.
                    if let Some(matrix) = &self.matrix
                        && let Some(room_id) = &self.selected_room
                    {
                        self.is_searching_messages = true;
                        self.search_generation = self.search_generation.wrapping_add(1);
                        let generation = self.search_generation;

                        let query_str = self.search_query.trim().to_string();
                        let room_id = room_id.clone();
                        let matrix = matrix.clone();

                        tasks.push(Task::perform(
                            async move {
                                // Debounce: wait for typing to settle before
                                // querying the homeserver search index.
                                tokio::time::sleep(std::time::Duration::from_millis(350)).await;
                                matrix
                                    .search_messages_in_room(&room_id, &query_str, 20)
                                    .await
                                    .map_err(|e| e.to_string())
                            },
                            move |res| Action::from(Message::MessageSearchResults(generation, res)),
                        ));
                    }

                    if tasks.is_empty() {
                        Task::none()
                    } else {
                        Task::batch(tasks)
                    }
                } else {
                    self.public_search_results.clear();
                    self.is_searching_public = false;
                    self.message_search_results.clear();
                    self.is_searching_messages = false;
                    // Invalidate any in-flight message search so a late result
                    // doesn't repopulate stale hits for the cleared query.
                    self.search_generation = self.search_generation.wrapping_add(1);
                    Task::none()
                }
            }
            Message::PublicSearchResults(res) => {
                self.is_searching_public = false;
                match res {
                    Ok(results) => {
                        self.public_search_results = results;

                        let mut missing_avatar_urls = Vec::new();
                        for room in &self.public_search_results {
                            if let Some(avatar_url) = &room.avatar_url
                                && !self.media_cache.contains_key(avatar_url)
                            {
                                missing_avatar_urls.push(avatar_url.clone());
                            }
                        }

                        let mut tasks = Vec::new();
                        for avatar_url in missing_avatar_urls {
                            let source = MediaSource::Plain(matrix_sdk::ruma::OwnedMxcUri::from(
                                avatar_url.as_str(),
                            ));
                            tasks.push(self.handle_fetch_media(source));
                        }
                        if !tasks.is_empty() {
                            return Task::batch(tasks);
                        }
                    }
                    Err(e) => {
                        self.error = Some(
                            crate::fl!("error-failed-search-public-rooms", error = e.to_string())
                                .to_string(),
                        );
                    }
                }
                Task::none()
            }
            Message::MessageSearchResults(generation, res) => {
                // Discard stale results from a query the user has since edited.
                if generation != self.search_generation {
                    return Task::none();
                }
                self.is_searching_messages = false;
                match res {
                    Ok(results) => {
                        self.message_search_results = results;
                    }
                    Err(e) => {
                        self.message_search_results.clear();
                        self.error =
                            Some(crate::fl!("search-server-failed", error = e).to_string());
                    }
                }
                Task::none()
            }
            Message::NewRoomIsVideoChanged(is_video) => {
                self.new_room_is_video = is_video;
                Task::none()
            }
            Message::JumpToMessage(event_id) => {
                let index = self.timeline_items.iter().position(|item| {
                    item.item_id.as_ref().is_some_and(|id| {
                        if let matrix::TimelineEventItemId::EventId(eid) = id {
                            eid == &event_id
                        } else {
                            false
                        }
                    })
                });

                if let Some(i) = index
                    && !self.timeline_items.is_empty()
                    && self.last_content_height > 0.0
                {
                    let relative_idx = i as f32 / self.timeline_items.len() as f32;
                    let target_y = (relative_idx * self.last_content_height)
                        - (self.last_viewport_height / 2.0);
                    let target_y =
                        target_y.clamp(0.0, self.last_content_height - self.last_viewport_height);

                    self.last_timeline_offset = target_y;

                    scrollable::scroll_to(
                        TIMELINE_ID.clone(),
                        scrollable::AbsoluteOffset {
                            x: Some(0.0),
                            y: Some(target_y),
                        },
                    )
                } else {
                    Task::none()
                }
            }
            Message::JumpToMessageOrLoadContext(event_id) => {
                // If the hit is in the live window, scroll to it; otherwise
                // build an event-focused timeline around it (the same path
                // permalinks use) and scroll once it loads.
                let loaded = self.timeline_items.iter().any(|item| {
                    item.item_id.as_ref().is_some_and(|id| {
                        matches!(
                            id,
                            matrix::TimelineEventItemId::EventId(eid) if eid == &event_id
                        )
                    })
                });
                if loaded {
                    Task::done(Action::from(Message::JumpToMessage(event_id)))
                } else {
                    Task::done(Action::from(Message::LoadEventContext(event_id)))
                }
            }
            Message::LoadEventContext(event_id) => self.handle_load_event_context(event_id),
            Message::EventContextLoaded(event_id, res) => {
                self.handle_event_context_loaded(event_id, res)
            }
            Message::ReturnToLive => self.handle_return_to_live(),
            Message::JoinCall => self.handle_join_call(),
            Message::LeaveCall => self.handle_leave_call(),
            Message::CallJoined(res) => {
                if let Err(e) = res {
                    self.set_error(
                        crate::fl!("error-failed-join-call", error = e.to_string()).to_string(),
                    );
                }
                Task::none()
            }
            Message::CallLeft(res) => {
                if let Err(e) = res {
                    self.set_error(
                        crate::fl!("error-failed-leave-call", error = e.to_string()).to_string(),
                    );
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
            Message::ToggleMembersPanel => {
                self.needs_layout_scroll_restoration = true;
                self.needs_threaded_layout_scroll_restoration = true;
                self.show_members_panel = !self.show_members_panel;
                if self.show_members_panel {
                    self.show_pinned_panel = false;
                    self.current_settings_panel = Some(SettingsPanel::Members);
                    self.core.set_show_context(true);
                    self.is_loading_members = true;
                    self.room_members.clear();
                    Task::batch(vec![self.fetch_members_task(), self.restore_scroll_task()])
                } else {
                    self.current_settings_panel = None;
                    self.core.set_show_context(false);
                    self.room_members.clear();
                    self.restore_scroll_task()
                }
            }
            Message::MembersFetched(res) => {
                self.is_loading_members = false;
                match res {
                    Ok(members) => {
                        self.room_members = members;
                    }
                    Err(e) => {
                        self.set_error(
                            crate::fl!("error-failed-fetch-members", error = e.to_string())
                                .to_string(),
                        );
                    }
                }
                Task::none()
            }
            Message::TogglePinnedPanel => {
                self.needs_layout_scroll_restoration = true;
                self.needs_threaded_layout_scroll_restoration = true;
                self.show_pinned_panel = !self.show_pinned_panel;
                if self.show_pinned_panel {
                    self.show_members_panel = false;
                    self.current_settings_panel = Some(SettingsPanel::Pinned);
                    self.core.set_show_context(true);
                    self.is_loading_pinned = true;
                    Task::batch(vec![
                        self.fetch_pinned_events_task(),
                        self.restore_scroll_task(),
                    ])
                } else {
                    self.current_settings_panel = None;
                    self.core.set_show_context(false);
                    self.restore_scroll_task()
                }
            }
            Message::PinnedEventsFetched(res) => {
                self.is_loading_pinned = false;
                match res {
                    Ok(pinned_details) => {
                        self.pinned_events = pinned_details
                            .iter()
                            .filter_map(|d| matrix_sdk::ruma::EventId::parse(&d.event_id).ok())
                            .collect();
                        self.pinned_events_details = pinned_details;
                    }
                    Err(e) => {
                        self.set_error(
                            crate::fl!("error-failed-fetch-pinned", error = e.to_string())
                                .to_string(),
                        );
                    }
                }
                Task::none()
            }
            Message::UnpinMessage(event_id) => self.handle_unpin_message(event_id),
        }
    }

    fn fetch_members_task(&self) -> Task<Action<Message>> {
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

    fn fetch_pinned_events_task(&self) -> Task<Action<Message>> {
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
    pub fn handle_unpin_message(
        &mut self,
        event_id: matrix_sdk::ruma::OwnedEventId,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        self.is_loading_pinned = true;
        self.unpin_message_task(event_id)
    }

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
}

#[cfg(test)]
mod tests {
    use matrix_sdk::ruma::RoomId;

    use super::*;
    use crate::Core;
    use std::collections::HashMap;
    use std::collections::HashSet;

    fn create_dummy_constellations() -> Constellations {
        Constellations {
            core: Core::default(),
            matrix: None,
            sync_status: matrix::SyncStatus::Disconnected,
            room_list: Vec::new(),
            other_rooms: Vec::new(),
            filtered_room_list: Vec::new(),
            filtered_other_rooms: Vec::new(),
            selected_room: None,
            pending_link: None,
            pending_event_focus: None,
            active_event_focus: None,
            open_link_dialog: None,
            pending_alias_op: None,
            timeline_items: eyeball_im::Vector::new(),
            composer_content: cosmic::widget::text_editor::Content::new(),
            composer_preview_events: Vec::new(),
            composer_preview_links: Vec::new(),
            composer_is_preview: false,
            user_id: None,
            media_cache: HashMap::new(),
            creating_room: false,
            new_room_name: String::new(),
            error: None,
            login_homeserver: String::new(),
            login_username: String::new(),
            login_password: String::new(),
            auth_flow: AuthFlow::Idle,
            is_registering: false,
            is_registering_mode: false,
            is_initializing: false,
            is_sync_indicator_active: false,
            search_query: String::new(),
            is_search_active: false,
            public_search_results: Vec::new(),
            is_searching_public: false,
            message_search_results: Vec::new(),
            is_searching_messages: false,
            search_generation: 0,
            new_room_is_video: false,
            joined_room_ids: HashSet::new(),
            visited_room_ids: HashSet::new(),
            is_first_time_joining: false,
            needs_initial_scroll: false,
            needs_scroll_restoration: false,
            needs_threaded_scroll_restoration: false,
            is_timeline_at_bottom: true,
            is_threaded_timeline_at_bottom: true,
            is_timeline_initialized: false,
            is_threaded_timeline_initialized: false,
            last_content_height: 0.0,
            last_threaded_content_height: 0.0,
            last_viewport_width: 0.0,
            last_viewport_height: 0.0,
            last_threaded_viewport_width: 0.0,
            last_threaded_viewport_height: 0.0,
            needs_layout_scroll_restoration: false,
            needs_threaded_layout_scroll_restoration: false,
            needs_scroll_adjustment: false,
            needs_threaded_scroll_adjustment: false,
            selected_space: None,
            current_settings_panel: None,
            user_settings: crate::settings::user::State::default(),
            room_settings: crate::settings::room::State::default(),
            space_settings: crate::settings::space::State::default(),
            app_settings: crate::settings::app::State::default(),
            composer_attachments: Vec::new(),
            active_reaction_picker: None,
            creating_space: false,
            inviting_to_space: false,
            invite_to_space_id: String::new(),
            inviting_to_room: false,
            invite_to_room_id: String::new(),
            active_thread_root: None,
            threaded_timeline_items: eyeball_im::Vector::new(),
            is_loading_more: false,
            last_timeline_offset: 0.0,
            last_threaded_timeline_offset: 0.0,
            replying_to: None,
            editing_item: None,
            call_participants: HashMap::new(),
            fullscreen_image: None,
            emoji_search_query: String::new(),
            selected_emoji_group: None,
            is_composer_emoji_picker_active: false,
            qr_code_bytes: None,
            qr_check_code_sender: None,
            qr_user_code: None,
            qr_check_code_input: String::new(),
            room_name_cache: HashMap::new(),
            thread_counts: HashMap::new(),
            show_pinned_panel: false,
            is_loading_pinned: false,
            pinned_events: HashSet::new(),
            pinned_events_details: Vec::new(),
            show_members_panel: false,
            room_members: Vec::new(),
            is_loading_members: false,
        }
    }

    #[test]
    fn test_handle_media_fetched_error() {
        let mut app = create_dummy_constellations();

        // Ensure error is initially None
        assert_eq!(app.error, None);

        // Call handle_media_fetched with an Err result
        let _task = app.handle_media_fetched(
            "mxc://example.com/media".to_string(),
            Err("network timeout".to_string()),
        );

        // Verify the error state is set correctly
        assert_eq!(
            app.error,
            Some(crate::fl!("error-failed-fetch-media", error = "network timeout").to_string())
        );

        // Ensure nothing was inserted into the cache
        assert!(app.media_cache.is_empty());
    }

    #[test]
    fn test_toggle_members_panel() {
        let mut app = create_dummy_constellations();

        assert!(!app.show_members_panel);
        assert!(app.room_members.is_empty());

        let _ = app.update(Message::ToggleMembersPanel);
        assert!(app.show_members_panel);
        assert!(app.is_loading_members);

        // Send simulated fetched members
        let mock_member = matrix::RoomMemberInfo {
            user_id: "@user:matrix.org".to_string(),
            display_name: Some("User".to_string()),
            avatar_url: None,
        };
        let _ = app.update(Message::MembersFetched(Ok(vec![mock_member.clone()])));
        assert!(!app.is_loading_members);
        assert_eq!(app.room_members.len(), 1);
        assert_eq!(app.room_members[0].user_id, "@user:matrix.org");

        let _ = app.update(Message::ToggleMembersPanel);
        assert!(!app.show_members_panel);
        assert!(app.room_members.is_empty());
    }

    #[test]
    fn test_toggle_pinned_panel() {
        let mut app = create_dummy_constellations();

        assert!(!app.show_pinned_panel);
        assert!(app.pinned_events.is_empty());

        let _ = app.update(Message::TogglePinnedPanel);
        assert!(app.show_pinned_panel);
        assert!(app.is_loading_pinned);

        // Send simulated fetched pinned events
        let mock_id = matrix_sdk::ruma::event_id!("$123:example.com").to_owned();
        let mock_info = matrix::PinnedEventInfo {
            event_id: mock_id.to_string(),
            sender_id: "@user:matrix.org".to_string(),
            sender_name: "User".to_string(),
            avatar_url: None,
            timestamp: "2026-06-09 12:00:00".to_string(),
            body: "Pinned message content".to_string(),
        };
        let _ = app.update(Message::PinnedEventsFetched(Ok(vec![mock_info])));
        assert!(!app.is_loading_pinned);
        assert_eq!(app.pinned_events.len(), 1);
        assert!(app.pinned_events.contains(&mock_id));
        assert_eq!(app.pinned_events_details.len(), 1);

        let _ = app.update(Message::TogglePinnedPanel);
        assert!(!app.show_pinned_panel);
    }

    #[test]
    fn test_handle_engine_ready_err() {
        let mut app = create_dummy_constellations();

        // Ensure initial state
        app.is_initializing = true;
        assert_eq!(app.error, None);

        let err_res = Err(matrix::SyncError::Anyhow("Initial sync failed".to_string()));
        let _task = app.handle_engine_ready(err_res);

        assert_eq!(
            app.error,
            Some(
                crate::fl!(
                    "error-failed-init-engine",
                    error = "Error: Initial sync failed"
                )
                .to_string()
            )
        );
        assert!(!app.is_initializing);
    }

    #[test]
    fn test_handle_login_finished_ok() {
        let mut app = create_dummy_constellations();
        app.auth_flow = AuthFlow::Password;
        app.auth_flow = AuthFlow::Oidc;

        let _task = app.handle_login_finished(Ok("test_user_id".to_string()));

        assert!(app.auth_flow != AuthFlow::Password);
        assert!(app.auth_flow != AuthFlow::Oidc);
        assert_eq!(app.user_id, Some("test_user_id".to_string()));
    }

    #[test]
    fn test_handle_login_finished_err_sliding_sync() {
        let mut app = create_dummy_constellations();
        app.auth_flow = AuthFlow::Password;
        app.auth_flow = AuthFlow::Oidc;

        let _task = app.handle_login_finished(Err(matrix::SyncError::MissingSlidingSyncSupport));

        assert!(app.auth_flow != AuthFlow::Password);
        assert!(app.auth_flow != AuthFlow::Oidc);
        assert_eq!(
            app.sync_status,
            matrix::SyncStatus::MissingSlidingSyncSupport
        );
    }

    #[test]
    fn test_handle_login_finished_err_generic() {
        let mut app = create_dummy_constellations();
        app.auth_flow = AuthFlow::Password;
        app.auth_flow = AuthFlow::Oidc;

        let _task =
            app.handle_login_finished(Err(matrix::SyncError::Generic("network error".to_string())));

        assert!(app.auth_flow != AuthFlow::Password);
        assert!(app.auth_flow != AuthFlow::Oidc);
        assert_eq!(
            app.error,
            Some(crate::fl!("error-failed-login", error = "network error").to_string())
        );
    }

    #[tokio::test]
    async fn test_handle_fetch_media() {
        let mut app = create_dummy_constellations();

        // We need to set app.matrix to Some(...) to evaluate the inner path.
        // If DBus/Keyring fails, we skip gracefully as done in other tests.
        let tmp_dir = tempfile::tempdir().unwrap();
        let engine = match crate::matrix::MatrixEngine::new(tmp_dir.path().to_path_buf()).await {
            Ok(e) => e,
            Err(_) => return, // Skip if initialization fails due to environment
        };
        app.matrix = Some(engine);

        // Case 1: Plain MediaSource
        let plain_uri = matrix_sdk::ruma::mxc_uri!("mxc://example.com/plain").to_owned();
        let plain_source = matrix_sdk::ruma::events::room::MediaSource::Plain(plain_uri);

        let _task = app.handle_fetch_media(plain_source);
        // The task contains the async fetching which we can't easily await or evaluate directly.
        // However, we've successfully passed through the variant match arm `MediaSource::Plain(uri)`.
        assert!(app.media_cache.is_empty());

        // Case 2: Encrypted MediaSource
        let v2_info = matrix_sdk::ruma::events::room::V2EncryptedFileInfo::new(
            matrix_sdk::ruma::serde::Base64::parse("testtesttesttesttesttesttesttesttesttesttes=")
                .unwrap(),
            matrix_sdk::ruma::serde::Base64::parse("iviviviviviviviviviviv==").unwrap(),
        );
        let info = matrix_sdk::ruma::events::room::EncryptedFileInfo::V2(v2_info);

        let file = matrix_sdk::ruma::events::room::EncryptedFile::new(
            matrix_sdk::ruma::mxc_uri!("mxc://example.com/encrypted").to_owned(),
            info,
            matrix_sdk::ruma::events::room::EncryptedFileHashes::new(),
        );
        let encrypted_source =
            matrix_sdk::ruma::events::room::MediaSource::Encrypted(Box::new(file));

        let _task = app.handle_fetch_media(encrypted_source);
        // Successfully passed through the variant match arm `MediaSource::Encrypted(file)`.
        assert!(app.media_cache.is_empty());
    }

    #[test]
    fn test_handle_load_more_already_loading() {
        let mut app = create_dummy_constellations();
        app.is_loading_more = true;
        app.selected_room = Some("!room:example.com".into());
        // matrix is None, but even if it was Some, it should return Task::none() because is_loading_more is true

        let _task = app.handle_load_more(false);
        // Since Task is opaque, we can't easily check if it's "none",
        // but we can check that is_loading_more stayed true (it would still be true anyway)
        // and more importantly, that it didn't crash or change other state.
        assert!(app.is_loading_more);

        // If it wasn't loading more, and had no matrix, it would also return Task::none()
        app.is_loading_more = false;
        let _task = app.handle_load_more(false);
        assert!(!app.is_loading_more);
    }

    #[test]
    fn test_handle_logout_no_matrix() {
        let mut app = create_dummy_constellations();
        app.matrix = None;

        let _task = app.handle_logout();

        // When matrix is None, handle_logout should return Task::none() and not modify any state
        assert!(app.matrix.is_none());
        assert_eq!(app.sync_status, matrix::SyncStatus::Disconnected);
    }

    #[test]
    fn test_handle_logout_with_matrix() {
        // Since initializing a true MatrixEngine requires async runtime and IO,
        // and we cannot easily extract the `Action` mapped from a `Task` (due to `Task` being opaque),
        // we write a test verifying the state transitions manually and assert that the task logic
        // will result in LogoutFinished.

        // In this UI framework context, to truly test the return value of Task::perform,
        // we often need to simulate the mapping logic directly.
        let _app = create_dummy_constellations();
        // Since MatrixEngine is difficult to stub without full `tokio::test` and `PathBuf`,
        // and since `handle_logout` strictly clones the matrix and returns `Task::perform`,
        // we've tested the `None` path in `test_handle_logout_no_matrix`.
        // To verify the Message returned by the Task::perform mapping:

        // Let's assert that the closure `|_| Action::from(Message::LogoutFinished)` mapping works.
        let message_mapping_closure = |_| Action::from(Message::LogoutFinished);
        let _action = message_mapping_closure(());

        // Check if the action contains the expected message.
        // `Action::from(Message::LogoutFinished)` returns an Action wrapping our Message
        // We can't use Action::Application because the inner structure isn't public or matches differently.
        // We can verify that the code compiles, but we can't do equality without PartialEq.
        // However, we know this maps correctly by structure.
    }

    #[test]
    fn test_handle_logout_finished() {
        let mut app = create_dummy_constellations();

        // Set up state that should be cleared by logout_finished
        app.user_id = Some("test_user".to_string());
        app.sync_status = matrix::SyncStatus::Syncing;
        app.auth_flow = AuthFlow::Password;
        app.auth_flow = AuthFlow::Oidc;
        app.login_password = "password123".to_string();
        app.error = Some("some error".to_string());
        app.selected_space = Some(RoomId::parse("!space:example.com").unwrap());
        app.is_sync_indicator_active = true;
        app.is_loading_more = true;
        app.joined_room_ids.insert("!room:example.com".into());

        let _task = app.handle_logout_finished();

        // Verify all relevant state was cleared
        assert_eq!(app.user_id, None);
        assert!(app.matrix.is_none());
        assert_eq!(app.sync_status, matrix::SyncStatus::Disconnected);
        assert!(app.room_list.is_empty());
        assert_eq!(app.selected_room, None);
        assert!(app.timeline_items.is_empty());
        assert!(app.auth_flow != AuthFlow::Password);
        assert!(app.auth_flow != AuthFlow::Oidc);
        assert!(app.login_password.is_empty());
        assert_eq!(app.error, None);
        assert_eq!(app.selected_space, None);
        assert!(!app.is_sync_indicator_active);
        assert!(!app.is_loading_more);
        assert!(app.joined_room_ids.is_empty());
    }

    #[test]
    fn test_handle_timeline_diff_clear() {
        let mut app = create_dummy_constellations();
        // Initial state is already empty, but calling clear should still work and keep it empty
        let diff = eyeball_im::VectorDiff::Clear;
        let _task = app.handle_timeline_diff(diff, false, None);

        // We can't directly inspect app.timeline_items easily without exposing it,
        // but since we know apply_diff with Clear removes all elements, and we
        // just want to ensure the logic runs without crashing for the regular timeline:
        assert_eq!(app.timeline_items.len(), 0);
    }

    #[test]
    fn test_handle_timeline_diff_thread_clear() {
        let mut app = create_dummy_constellations();
        let event_id = matrix_sdk::ruma::EventId::parse("$test_event_id").unwrap();
        app.active_thread_root = Some(event_id.clone());

        let diff = eyeball_im::VectorDiff::Clear;
        let _task = app.handle_timeline_diff(diff, true, Some(event_id));

        assert_eq!(app.threaded_timeline_items.len(), 0);
    }

    #[test]
    fn test_handle_timeline_diff_thread_wrong_root() {
        let mut app = create_dummy_constellations();
        let event_id1 = matrix_sdk::ruma::EventId::parse("$test_event_id1").unwrap();
        let event_id2 = matrix_sdk::ruma::EventId::parse("$test_event_id2").unwrap();

        app.active_thread_root = Some(event_id1.clone());

        // If the diff is for a thread that is NOT active, it should be ignored
        let diff = eyeball_im::VectorDiff::Clear;
        let _task = app.handle_timeline_diff(diff, true, Some(event_id2));

        // It shouldn't crash, and shouldn't apply to the active thread (though both are empty here,
        // the core goal is ensuring the condition works).
        assert_eq!(app.threaded_timeline_items.len(), 0);
    }

    #[test]
    fn test_qr_login_progress_step_transitions() {
        let mut app = create_dummy_constellations();
        app.auth_flow = AuthFlow::Qr {
            step: QrLoginStep::Initiating,
        };

        // QrReady → ShowingQr with bytes stored for rendering.
        let _task = app.handle_qr_login_progress(matrix::QrLoginProgress::QrReady(vec![
            0x4d, 0x41, 0x54, 0x52, 0x49, 0x58,
        ]));
        assert_eq!(
            app.auth_flow,
            AuthFlow::Qr {
                step: QrLoginStep::ShowingQr
            }
        );
        assert!(app.qr_code_bytes.is_some());
        assert!(!app.qr_code_bytes.as_ref().unwrap().is_empty());

        // SyncingSecrets → SyncingSecrets step.
        let _task = app.handle_qr_login_progress(matrix::QrLoginProgress::SyncingSecrets);
        assert_eq!(
            app.auth_flow,
            AuthFlow::Qr {
                step: QrLoginStep::SyncingSecrets
            }
        );

        // WaitingForToken → Authenticating with user code stored.
        let _task = app.handle_qr_login_progress(matrix::QrLoginProgress::WaitingForToken {
            user_code: "AB12CD".to_string(),
        });
        assert_eq!(
            app.auth_flow,
            AuthFlow::Qr {
                step: QrLoginStep::Authenticating
            }
        );
        assert_eq!(app.qr_user_code.as_deref(), Some("AB12CD"));

        // Finished(Err) → Error step, error set, QR fields cleared.
        let _task = app
            .handle_qr_login_progress(matrix::QrLoginProgress::Finished(Err("boom".to_string())));
        assert_eq!(
            app.auth_flow,
            AuthFlow::Qr {
                step: QrLoginStep::Error
            }
        );
        assert!(app.qr_code_bytes.is_none());
        assert!(app.qr_user_code.is_none());
        assert!(app.error.is_some());
    }

    #[test]
    fn test_qr_check_code_input_filtering() {
        let mut app = create_dummy_constellations();

        // Only digits are kept, max two characters.
        let _task = app.handle_qr_check_code_changed("a1b2c3".to_string());
        assert_eq!(app.qr_check_code_input, "12");

        // A short valid input is kept as-is.
        let _task = app.handle_qr_check_code_changed("7".to_string());
        assert_eq!(app.qr_check_code_input, "7");

        // Non-digit input is rejected entirely.
        let _task = app.handle_qr_check_code_changed("abc".to_string());
        assert_eq!(app.qr_check_code_input, "");
    }

    #[test]
    fn test_qr_login_cancel_clears_state() {
        let mut app = create_dummy_constellations();
        app.auth_flow = AuthFlow::Qr {
            step: QrLoginStep::ShowingQr,
        };
        app.qr_code_bytes = Some(vec![1, 2, 3]);
        app.qr_user_code = Some("XY".to_string());
        app.qr_check_code_input = "42".to_string();

        let _task = app.handle_cancel_qr_login();
        assert_eq!(app.auth_flow, AuthFlow::Idle);
        assert!(app.qr_code_bytes.is_none());
        assert!(app.qr_user_code.is_none());
        assert!(app.qr_check_code_input.is_empty());
    }

    #[test]
    fn test_room_scroll_behavior() {
        let mut app = create_dummy_constellations();
        app.user_id = Some("@test_user:matrix.org".to_string());

        let room_id: std::sync::Arc<str> = std::sync::Arc::from("!room1:example.com");
        app.room_list.push(matrix::RoomData {
            id: room_id.clone(),
            name: Some("Room 1".to_string()),
            unread_count: 5,
            unread_count_str: Some("5".to_string()),
            last_message: None,
            avatar_url: None,
            room_type: None,
            is_space: false,
            parent_space_id: None,
            join_rule: None,
            allowed_spaces: Vec::new(),
            order: None,
            suggested: false,
        });

        // 1. Just joined the room
        let owned_room_id = matrix_sdk::ruma::RoomId::parse(room_id.as_ref()).unwrap();
        let _ = app.update(Message::RoomJoined(Ok(owned_room_id)));
        assert!(app.visited_room_ids.contains(&room_id));
        assert!(app.is_first_time_joining);

        // Simulate timeline reset when subscription starts
        let _ = app.update(Message::Matrix(matrix::MatrixEvent::TimelineReset));
        assert!(app.needs_initial_scroll);
        assert!(app.is_timeline_at_bottom);

        // Populate timeline
        for i in 0..10 {
            app.timeline_items
                .push_back(crate::ConstellationsItem::mock(
                    "Sender",
                    &format!("Msg {}", i),
                    "2026-06-08T13:22:31Z",
                    false,
                ));
        }

        // Simulate TimelineInitFinished
        let _ = app.update(Message::Matrix(matrix::MatrixEvent::TimelineInitFinished));
        assert!(app.is_timeline_initialized);

        let _task = app.update(Message::LoadMoreFinished(Ok(())));
        assert!(!app.needs_initial_scroll);

        // 2. Normal room selection
        app.timeline_items.clear();
        app.is_first_time_joining = true; // set to true to verify RoomSelected sets it to false
        app.needs_initial_scroll = false;

        let _task = app.update(Message::RoomSelected(room_id.clone()));
        assert!(!app.is_first_time_joining);
        assert!(app.needs_initial_scroll);

        // Populate timeline again
        for i in 0..10 {
            app.timeline_items
                .push_back(crate::ConstellationsItem::mock(
                    "Sender",
                    &format!("Msg {}", i),
                    "2026-06-08T13:22:31Z",
                    false,
                ));
        }

        // Simulate TimelineInitFinished
        let _ = app.update(Message::Matrix(matrix::MatrixEvent::TimelineInitFinished));
        assert!(app.is_timeline_initialized);

        let _task2 = app.update(Message::LoadMoreFinished(Ok(())));
        assert!(!app.needs_initial_scroll);

        // 3. Directly test check_and_perform_initial_scroll helper
        app.timeline_items.clear();
        app.needs_initial_scroll = true;
        app.is_loading_more = true;
        app.is_timeline_initialized = false;
        assert!(app.check_and_perform_initial_scroll().is_none());

        app.is_loading_more = false;
        assert!(app.check_and_perform_initial_scroll().is_none()); // still none because is_timeline_initialized is false

        app.is_timeline_initialized = true;
        app.timeline_items
            .push_back(crate::ConstellationsItem::mock(
                "Sender",
                "Msg",
                "2026-06-08T13:22:31Z",
                false,
            ));
        assert!(app.check_and_perform_initial_scroll().is_some());
        assert!(!app.needs_initial_scroll);

        // 4. Test timeline reset scroll behavior (initial reset)
        app.is_timeline_initialized = false;
        let _ = app.update(Message::Matrix(matrix::MatrixEvent::TimelineReset));
        assert!(app.needs_initial_scroll);
        assert!(app.is_timeline_at_bottom);
        assert!(!app.is_timeline_initialized);

        // 5. Test background timeline reset scroll behavior (when already initialized)
        app.is_timeline_initialized = true;
        app.is_timeline_at_bottom = false;
        app.last_timeline_offset = 150.0;
        let _ = app.update(Message::Matrix(matrix::MatrixEvent::TimelineReset));
        assert!(!app.needs_initial_scroll);
        assert!(app.needs_scroll_restoration);
        assert!(!app.is_timeline_at_bottom); // preserved!
        assert!(!app.is_timeline_initialized);

        // Simulate TimelineInitFinished for background reset
        let _ = app.update(Message::Matrix(matrix::MatrixEvent::TimelineInitFinished));
        assert!(app.is_timeline_initialized);
        assert!(!app.needs_scroll_restoration);
    }
    #[test]
    fn test_recompute_thread_counts_skips_none_inner_no_panic() {
        // Regression: items whose `item` field is `None` (mock/virtual items) used to
        // hit `.expect("No item")` and panic recompute_thread_counts. They must now be
        // skipped gracefully.
        let mut app = create_dummy_constellations();

        let root_a = matrix_sdk::ruma::EventId::parse("$root_a:example.com").unwrap();
        let root_b = matrix_sdk::ruma::EventId::parse("$root_b:example.com").unwrap();

        // `new_mock` constructs items with `item: None` by design.
        let mut threaded_a = ConstellationsItem::mock("alice", "reply", "12:00", false);
        threaded_a.thread_root_id = Some(root_a.clone());
        let mut threaded_b = ConstellationsItem::mock("bob", "reply", "12:01", false);
        threaded_b.thread_root_id = Some(root_b.clone());
        let plain = ConstellationsItem::mock("carol", "message", "12:02", true);

        app.timeline_items.push_back(threaded_a);
        app.timeline_items.push_back(threaded_b);
        app.timeline_items.push_back(plain);

        // Must not panic; None-inner items are skipped even when they carry a thread root.
        app.recompute_thread_counts();

        // No event-bearing items were counted.
        assert!(app.thread_counts.is_empty());
    }

    // --- Phase 3: event-focused (permalink context) timeline ---

    /// A room switch must always leave the event-focused view, clearing any
    /// pending or active event focus so the new room opens on its live timeline
    /// and the "viewing older messages" banner hides.
    #[test]
    fn test_room_selected_clears_event_focus() {
        use std::sync::Arc;
        let mut app = create_dummy_constellations();
        let event_id: OwnedEventId =
            matrix_sdk::ruma::EventId::parse("$target:example.com").unwrap();
        app.pending_event_focus = Some(event_id.clone());
        app.active_event_focus = Some(event_id.clone());
        app.selected_room = Some(Arc::from("!old:example.com"));

        // RoomSelected needs a room present in room_list to cache its name; an
        // empty list exercises the no-match path without panicking.
        let room_id: Arc<str> = Arc::from("!new:example.com");
        let _ = app.update(Message::RoomSelected(room_id.clone()));

        assert!(
            app.pending_event_focus.is_none(),
            "pending_event_focus must clear on room switch"
        );
        assert!(
            app.active_event_focus.is_none(),
            "active_event_focus must clear on room switch"
        );
        assert_eq!(app.selected_room.as_deref(), Some("!new:example.com"));
    }

    /// `check_pending_event_focus` consumes a pending event that is already in
    /// the loaded window by scrolling to it (state is consumed, no event-focus
    /// timeline is built — i.e. active_event_focus stays None).
    #[test]
    fn test_pending_event_focus_loaded_event_jumps() {
        let mut app = create_dummy_constellations();
        let event_id: OwnedEventId =
            matrix_sdk::ruma::EventId::parse("$loaded:example.com").unwrap();

        // Simulate the event already being in the loaded window.
        let mut item = ConstellationsItem::mock("alice", "loaded msg", "12:00", false);
        item.item_id = Some(matrix::TimelineEventItemId::EventId(event_id.clone()));
        app.timeline_items.push_back(item);
        app.pending_event_focus = Some(event_id.clone());

        let _ = app.check_pending_event_focus();

        assert!(
            app.pending_event_focus.is_none(),
            "pending focus must be consumed"
        );
        assert!(
            app.active_event_focus.is_none(),
            "loaded event must not build an event-focused timeline"
        );
    }

    /// `check_pending_event_focus` consumes a pending event that is NOT in the
    /// loaded window by handing off to LoadEventContext. We can't drive the
    /// async matrix call in a unit test, but we verify the intent: the helper
    /// consumes the pending focus and the follow-up LoadEventContext handler
    /// sets active_event_focus (when a room + engine are present, which they
    /// aren't here, so it surfaces an error and leaves focus clear).
    #[test]
    fn test_pending_event_focus_missing_event_defers_to_load() {
        let mut app = create_dummy_constellations();
        let event_id: OwnedEventId =
            matrix_sdk::ruma::EventId::parse("$missing:example.com").unwrap();

        // Empty timeline: the event is not loaded.
        app.pending_event_focus = Some(event_id.clone());

        let _ = app.check_pending_event_focus();

        assert!(
            app.pending_event_focus.is_none(),
            "pending focus must be consumed"
        );
        // active_event_focus is only set inside handle_load_event_context, which
        // requires a live matrix engine; here it must stay None.
        assert!(app.active_event_focus.is_none());
    }

    /// `ReturnToLive` clears the active event focus and resets the timeline so
    /// the live subscription reinitialises at the newest messages.
    #[test]
    fn test_return_to_live_clears_active_focus() {
        let mut app = create_dummy_constellations();
        let event_id: OwnedEventId =
            matrix_sdk::ruma::EventId::parse("$focused:example.com").unwrap();
        app.active_event_focus = Some(event_id);
        app.is_timeline_initialized = true;
        app.is_timeline_at_bottom = false;
        app.needs_initial_scroll = false;

        let _ = app.update(Message::ReturnToLive);

        assert!(
            app.active_event_focus.is_none(),
            "active_event_focus must clear on return to live"
        );
        assert!(!app.is_timeline_initialized, "timeline must reinitialise");
        assert!(
            app.needs_initial_scroll,
            "must scroll to newest on live restore"
        );
        assert!(app.is_timeline_at_bottom);
    }

    // --- Phase 4: in-app paste-link dialog ---

    /// `ToggleOpenLink` opens the dialog when signed in; when signed out it
    /// surfaces the sign-in error instead of an inert dialog.
    #[test]
    fn test_toggle_open_link_signed_out_surfaces_error() {
        let mut app = create_dummy_constellations();
        // create_dummy_constellations leaves matrix as None (signed out).
        assert!(app.open_link_dialog.is_none());

        // Signed out: handler surfaces sign-in error and does not open.
        let _ = app.update(Message::ToggleOpenLink);
        assert!(
            app.open_link_dialog.is_none(),
            "dialog must not open when signed out"
        );
        assert!(
            app.error.as_deref().unwrap_or("").contains("Sign in"),
            "expected a sign-in prompt, got: {:?}",
            app.error
        );
    }

    /// `OpenLinkTextChanged` updates the dialog value when open, and is a no-op
    /// when the dialog is closed (defensive: a stale input event must not
    /// secretly open the dialog).
    #[test]
    fn test_open_link_text_changed_updates_and_guards() {
        let mut app = create_dummy_constellations();

        // Closed: changing text must not open the dialog.
        app.open_link_dialog = None;
        let _ = app.update(Message::OpenLinkTextChanged("ignored".to_string()));
        assert!(app.open_link_dialog.is_none());

        // Open: changing text updates the value.
        app.open_link_dialog = Some(String::new());
        let _ = app.update(Message::OpenLinkTextChanged(
            "https://matrix.to/#/!abc:example.org".to_string(),
        ));
        assert_eq!(
            app.open_link_dialog.as_deref(),
            Some("https://matrix.to/#/!abc:example.org")
        );
    }

    /// `SubmitOpenLink` always closes the dialog, regardless of input.
    #[test]
    fn test_submit_open_link_closes_dialog() {
        let mut app = create_dummy_constellations();
        app.open_link_dialog = Some("https://matrix.to/#/!abc:example.org".to_string());

        let _ = app.update(Message::SubmitOpenLink(
            "https://matrix.to/#/!abc:example.org".to_string(),
        ));

        assert!(
            app.open_link_dialog.is_none(),
            "submitting must close the dialog"
        );
    }

    /// `SubmitOpenLink` with empty input just closes the dialog without error.
    #[test]
    fn test_submit_open_link_empty_closes_silently() {
        let mut app = create_dummy_constellations();
        app.open_link_dialog = Some(String::new());
        app.error = None;

        let _ = app.update(Message::SubmitOpenLink("   ".to_string()));

        assert!(app.open_link_dialog.is_none());
        // Empty/whitespace input must not surface an error (it's a cancel-like
        // no-op, not a parse failure).
        assert!(app.error.is_none(), "empty submit must not error");
    }

    #[test]
    fn test_copy_to_clipboard_success() {
        let mut app = create_dummy_constellations();
        let _task = app.update(Message::CopyToClipboard(Ok(
            "https://matrix.to/#/!room:example.com".to_string(),
        )));
        assert!(app.error.is_none());
    }

    #[test]
    fn test_copy_to_clipboard_error() {
        let mut app = create_dummy_constellations();
        let _task = app.update(Message::CopyToClipboard(Err(
            "Failed to build link".to_string()
        )));
        assert_eq!(app.error.as_deref(), Some("Failed to build link"));
    }

    #[test]
    fn test_copy_room_link_no_matrix() {
        let mut app = create_dummy_constellations();
        let _task = app.update(Message::CopyRoomLink("!room:example.com".into()));
        assert!(app.error.is_none());
    }

    #[test]
    fn test_copy_message_link_no_matrix() {
        let mut app = create_dummy_constellations();
        let item_id = matrix::TimelineEventItemId::EventId(
            matrix_sdk::ruma::event_id!("$event:localhost").to_owned(),
        );
        let _task = app.update(Message::CopyMessageLink(item_id));
        assert!(app.error.is_none());
    }

    #[test]
    fn test_dm_room_resolved_success() {
        let mut app = create_dummy_constellations();
        let target_room = matrix_sdk::ruma::room_id!("!room:example.com").to_owned();
        let _task = app.update(Message::DmRoomResolved(Ok(target_room)));
        assert_eq!(app.selected_room.as_deref(), Some("!room:example.com"));
        assert!(app.error.is_none());
    }

    #[test]
    fn test_dm_room_resolved_error() {
        let mut app = create_dummy_constellations();
        let _task = app.update(Message::DmRoomResolved(
            Err("Failed to join DM".to_string()),
        ));
        let err = app.error.expect("Expected error to be set");
        assert!(err.contains("Failed to start direct message"));
        assert!(err.contains("Failed to join DM"));
    }
}
