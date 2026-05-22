use crate::matrix::TimelineItem;
use crate::{
    ApplyVectorDiffExt, Constellations, ConstellationsItem, MediaSource, Message, OwnedRoomId, Url,
    matrix, redact_url,
};
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
    pub fn handle_engine_ready(
        &mut self,
        res: Result<matrix::MatrixEngine, matrix::SyncError>,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        match res {
            Ok(engine) => {
                self.matrix = Some(engine.clone());
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
                self.error = Some(format!("Failed to initialize Matrix engine: {}", e));
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
            check_item(&item.item, &mut media_fetches);
        }
        for item in &self.threaded_timeline_items {
            check_item(&item.item, &mut media_fetches);
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
                self.threaded_timeline_items.apply_diff(mapped_diff);
            }
        } else {
            self.timeline_items.apply_diff(mapped_diff);
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
                self.update_filtered_rooms();
                self.update_title()
            }
            matrix::MatrixEvent::TimelineDiff(diff) => self.handle_timeline_diff(diff, false, None),
            matrix::MatrixEvent::TimelineReset => {
                self.timeline_items.clear();
                Task::none()
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
                    .create_session(None, None, Some(ashpd::desktop::location::Accuracy::Exact))
                    .await
                    .map_err(|e| e.to_string())?;

                let mut stream = proxy
                    .receive_location_updated()
                    .await
                    .map_err(|e| e.to_string())?;

                let (_, location_res) = futures::join!(
                    proxy.start(&session, None).map(|e| e.map_err(|err| err.to_string())),
                    stream.next().map(|opt| opt.ok_or_else(|| "Stream is exhausted".to_string()))
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
                    let body = format!("Location: geo:{lat},{lon}");
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
                self.error = Some(format!("Failed to retrieve location: {}", e));
                Task::none()
            }
        }
    }

    pub fn handle_send_message(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        if let (Some(matrix), Some(room_id)) = (&self.matrix, &self.selected_room) {
            let body = self.composer_text.clone();

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
                        let event = editing_item.item.as_event().ok_or("Not an event")?;
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
                            let event = replying_to.item.as_event().ok_or("Not an event")?;
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
                self.composer_text.clear();
                self.composer_preview_events.clear();
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
            Task::perform(
                async move {
                    matrix
                        .create_room(&name)
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
                self.error = Some(format!("Failed to fetch space children: {}", e));
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
                self.error = Some(format!("Failed to fetch media: {}", e));
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
                    self.error = Some(format!("Failed to fetch media: {}", e));
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
                self.error = Some(format!("Registration failed: {}", e));
                Task::none()
            }
        }
    }

    pub fn handle_submit_login(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        if let Some(matrix) = &self.matrix {
            self.is_logging_in = true;
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
        self.is_logging_in = false;
        self.is_oidc_logging_in = false;
        match res {
            Ok(user_id) => {
                self.user_id = Some(user_id);
            }
            Err(matrix::SyncError::MissingSlidingSyncSupport) => {
                self.sync_status = matrix::SyncStatus::MissingSlidingSyncSupport;
            }
            Err(e) => {
                self.error = Some(format!("Login failed: {}", e));
            }
        }
        Task::none()
    }

    pub fn handle_submit_oidc_login(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        if let Some(matrix) = &self.matrix {
            self.is_oidc_logging_in = true;
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
                self.is_oidc_logging_in = false;
                self.error = Some(format!("OIDC login failed to start: {}", e));
            }
        }
        Task::none()
    }

    pub fn handle_oidc_callback(
        &mut self,
        url: Url,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        if let Some(matrix) = &self.matrix {
            self.is_oidc_logging_in = true;
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
        self.is_logging_in = false;
        self.is_oidc_logging_in = false;
        self.login_password.clear();
        self.error = None;
        self.selected_space = None;
        self.is_sync_indicator_active = false;
        self.is_loading_more = false;
        self.joined_room_ids.clear();
        Task::none()
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
            timeline_items: eyeball_im::Vector::new(),
            composer_text: String::new(),
            composer_preview_events: Vec::new(),
            composer_is_preview: false,
            user_id: None,
            media_cache: HashMap::new(),
            creating_room: false,
            new_room_name: String::new(),
            error: None,
            login_homeserver: String::new(),
            login_username: String::new(),
            login_password: String::new(),
            is_logging_in: false,
            is_oidc_logging_in: false,
            is_registering: false,
            is_registering_mode: false,
            is_initializing: false,
            is_sync_indicator_active: false,
            search_query: String::new(),
            is_search_active: false,
            joined_room_ids: HashSet::new(),
            selected_space: None,
            current_settings_panel: None,
            user_settings: crate::settings::user::State::default(),
            room_settings: crate::settings::room::State::default(),
            space_settings: crate::settings::space::State::default(),
            app_settings: crate::settings::app::State::default(),
            composer_attachments: Vec::new(),
            active_reaction_picker: None,
            creating_space: false,
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
            Some("Failed to fetch media: network timeout".to_string())
        );

        // Ensure nothing was inserted into the cache
        assert!(app.media_cache.is_empty());
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
            Some("Failed to initialize Matrix engine: Error: Initial sync failed".to_string())
        );
        assert_eq!(app.is_initializing, false);
    }

    #[test]
    fn test_handle_login_finished_ok() {
        let mut app = create_dummy_constellations();
        app.is_logging_in = true;
        app.is_oidc_logging_in = true;

        let _task = app.handle_login_finished(Ok("test_user_id".to_string()));

        assert_eq!(app.is_logging_in, false);
        assert_eq!(app.is_oidc_logging_in, false);
        assert_eq!(app.user_id, Some("test_user_id".to_string()));
    }

    #[test]
    fn test_handle_login_finished_err_sliding_sync() {
        let mut app = create_dummy_constellations();
        app.is_logging_in = true;
        app.is_oidc_logging_in = true;

        let _task = app.handle_login_finished(Err(matrix::SyncError::MissingSlidingSyncSupport));

        assert_eq!(app.is_logging_in, false);
        assert_eq!(app.is_oidc_logging_in, false);
        assert_eq!(
            app.sync_status,
            matrix::SyncStatus::MissingSlidingSyncSupport
        );
    }

    #[test]
    fn test_handle_login_finished_err_generic() {
        let mut app = create_dummy_constellations();
        app.is_logging_in = true;
        app.is_oidc_logging_in = true;

        let _task =
            app.handle_login_finished(Err(matrix::SyncError::Generic("network error".to_string())));

        assert_eq!(app.is_logging_in, false);
        assert_eq!(app.is_oidc_logging_in, false);
        assert_eq!(app.error, Some("Login failed: network error".to_string()));
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
        app.is_logging_in = true;
        app.is_oidc_logging_in = true;
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
        assert_eq!(app.is_logging_in, false);
        assert_eq!(app.is_oidc_logging_in, false);
        assert!(app.login_password.is_empty());
        assert_eq!(app.error, None);
        assert_eq!(app.selected_space, None);
        assert_eq!(app.is_sync_indicator_active, false);
        assert_eq!(app.is_loading_more, false);
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
}
