use crate::matrix::{self, TimelineItem};
use crate::{
    ApplyVectorDiffExt, Constellations, ConstellationsItem, MediaSource, Message,
    THREADED_TIMELINE_ID, TIMELINE_ID,
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

    pub(super) fn check_and_perform_initial_scroll(
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

    pub(super) fn handle_timeline_scrolled(
        &mut self,
        viewport: cosmic::iced::widget::scrollable::Viewport,
        is_thread: bool,
    ) -> Task<Action<Message>> {
        let current_offset = viewport.absolute_offset().y;
        let current_height = viewport.content_bounds().height;

        let is_initialized = if is_thread {
            self.is_threaded_timeline_initialized
        } else {
            self.is_timeline_initialized
        };

        if !is_initialized {
            return Task::none();
        }

        let prefix = if is_thread {
            "TimelineScrolled (thread)"
        } else {
            "TimelineScrolled"
        };
        let last_content_height = if is_thread {
            self.last_threaded_content_height
        } else {
            self.last_content_height
        };
        let last_viewport_width = if is_thread {
            self.last_threaded_viewport_width
        } else {
            self.last_viewport_width
        };
        let last_viewport_height = if is_thread {
            self.last_threaded_viewport_height
        } else {
            self.last_viewport_height
        };
        let needs_layout_scroll_restoration = if is_thread {
            self.needs_threaded_layout_scroll_restoration
        } else {
            self.needs_layout_scroll_restoration
        };
        let needs_scroll_adjustment = if is_thread {
            self.needs_threaded_scroll_adjustment
        } else {
            self.needs_scroll_adjustment
        };

        tracing::info!(
            "{}: offset={}, content_height={}, viewport_w={}, viewport_h={}, last_h={}, last_w={}, last_vh={}",
            prefix,
            current_offset,
            current_height,
            viewport.bounds().width,
            viewport.bounds().height,
            last_content_height,
            last_viewport_width,
            last_viewport_height
        );

        let mut is_layout_resize = false;
        if (needs_layout_scroll_restoration
            || (last_content_height > 0.0 && current_height != last_content_height)
            || (last_viewport_width > 0.0 && viewport.bounds().width != last_viewport_width)
            || (last_viewport_height > 0.0 && viewport.bounds().height != last_viewport_height))
            && !needs_scroll_adjustment
        {
            is_layout_resize = true;
        }

        if is_thread {
            self.needs_threaded_layout_scroll_restoration = false;
        } else {
            self.needs_layout_scroll_restoration = false;
        }

        let mut task = Task::none();
        let mut actual_offset = current_offset;
        let timeline_id = if is_thread {
            THREADED_TIMELINE_ID.clone()
        } else {
            TIMELINE_ID.clone()
        };

        let needs_adjustment = if is_thread {
            self.needs_threaded_scroll_adjustment
        } else {
            self.needs_scroll_adjustment
        };

        if needs_adjustment && last_content_height > 0.0 && current_height > last_content_height {
            if is_thread {
                self.needs_threaded_scroll_adjustment = false;
            } else {
                self.needs_scroll_adjustment = false;
            }
            let diff_height = current_height - last_content_height;
            actual_offset = current_offset + diff_height;
            task = scrollable::scroll_to(
                timeline_id,
                scrollable::AbsoluteOffset {
                    x: Some(0.0),
                    y: Some(actual_offset),
                },
            );
        } else if is_layout_resize {
            let is_at_bottom = if is_thread {
                self.is_threaded_timeline_at_bottom
            } else {
                self.is_timeline_at_bottom
            };
            if is_at_bottom {
                task = scrollable::snap_to(timeline_id, scrollable::RelativeOffset::END.into());
            } else {
                let last_offset = if is_thread {
                    self.last_threaded_timeline_offset
                } else {
                    self.last_timeline_offset
                };
                let target_offset = last_offset
                    .min(current_height - viewport.bounds().height)
                    .max(0.0);
                task = scrollable::scroll_to(
                    timeline_id,
                    scrollable::AbsoluteOffset {
                        x: Some(0.0),
                        y: Some(target_offset),
                    },
                );
                actual_offset = target_offset;
            }
        }

        if is_layout_resize {
            tracing::info!("{} layout resize: target_offset={}", prefix, actual_offset);
        }

        let last_offset = if is_thread {
            self.last_threaded_timeline_offset
        } else {
            self.last_timeline_offset
        };
        let should_load = !is_layout_resize && actual_offset < 100.0 && actual_offset < last_offset;
        let is_at_bottom = actual_offset + viewport.bounds().height >= current_height - 20.0;

        if !is_layout_resize {
            if is_thread {
                self.last_threaded_timeline_offset = actual_offset;
                self.last_threaded_content_height = current_height;
                self.last_threaded_viewport_width = viewport.bounds().width;
                self.last_threaded_viewport_height = viewport.bounds().height;
                self.is_threaded_timeline_at_bottom = is_at_bottom;
            } else {
                self.last_timeline_offset = actual_offset;
                self.last_content_height = current_height;
                self.last_viewport_width = viewport.bounds().width;
                self.last_viewport_height = viewport.bounds().height;
                self.is_timeline_at_bottom = is_at_bottom;
            }
        } else {
            if is_thread {
                self.last_threaded_content_height = current_height;
                self.last_threaded_viewport_width = viewport.bounds().width;
                self.last_threaded_viewport_height = viewport.bounds().height;
            } else {
                self.last_content_height = current_height;
                self.last_viewport_width = viewport.bounds().width;
                self.last_viewport_height = viewport.bounds().height;
            }
        }

        if should_load {
            Task::batch(vec![task, self.handle_load_more(is_thread)])
        } else {
            task
        }
    }
}
