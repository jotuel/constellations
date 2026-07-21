use crate::matrix;
use crate::preview::parse_markdown;
use crate::settings;
use crate::{
    AuthFlow, Constellations, MediaSource, Message, SettingsPanel, THREADED_TIMELINE_ID,
    TIMELINE_ID,
};
use cosmic::iced::widget::scrollable;
use cosmic::{Action, Task};

impl Constellations {
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
            Message::StartReply(item_id) => self.handle_start_reply(item_id),
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
                self.handle_timeline_scrolled(viewport, is_thread)
            }
            Message::RoomSelected(room_id) => self.handle_room_selected(room_id),
            Message::ComposerChanged(text) => self.handle_composer_changed(text),
            Message::ComposerAction(action) => self.handle_composer_action(action),
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
            Message::StartEdit(item_id) => self.handle_start_edit(item_id),
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
            Message::DndFileTransfer(key) => {
                cosmic::command::file_transfer_receive(key).map(|res| {
                    Action::from(Message::DndFileTransferFinished(
                        res.map_err(|e| e.to_string()),
                    ))
                })
            }
            Message::DndFileTransferFinished(res) => match res {
                Ok(paths) => {
                    let path_bufs: Vec<std::path::PathBuf> = paths
                        .into_iter()
                        .map(std::path::PathBuf::from)
                        .filter(|p| p.exists())
                        .collect();
                    if !path_bufs.is_empty() {
                        self.handle_update(Message::AttachmentsSelected(path_bufs))
                    } else {
                        Task::none()
                    }
                }
                Err(e) => {
                    self.set_error(crate::fl!("error-failed-retrieve-dragged-files", error = e));
                    Task::none()
                }
            },
            Message::DndDataReceived(mime, data) => {
                if mime == "text/uri-list"
                    && let Ok(text) = String::from_utf8(data)
                {
                    let mut paths = Vec::new();
                    for line in text.lines() {
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }
                        if let Ok(url) = url::Url::parse(line) {
                            if let Ok(path) = url.to_file_path() {
                                paths.push(path);
                            }
                        } else {
                            let path = std::path::PathBuf::from(line);
                            if path.exists() {
                                paths.push(path);
                            }
                        }
                    }
                    if !paths.is_empty() {
                        return self.handle_update(Message::AttachmentsSelected(paths));
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
            Message::OpenSettings(panel) => self.handle_open_settings(panel),
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
            Message::ToggleSearch => self.handle_toggle_search(),
            Message::SearchQueryChanged(query) => self.handle_search_query_changed(query),
            Message::PublicSearchResults(generation, res) => {
                // Discard stale results from a query the user has since edited.
                if generation != self.search_generation {
                    return Task::none();
                }
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
                    Ok((results, has_more)) => {
                        self.message_search_results = results;
                        self.search_has_more = has_more;
                    }
                    Err(e) => {
                        self.message_search_results.clear();
                        self.search_has_more = false;
                        self.error =
                            Some(crate::fl!("search-server-failed", error = e).to_string());
                    }
                }
                Task::none()
            }
            Message::LoadMoreMessageSearch => {
                if self.is_searching_more_messages {
                    return Task::none();
                }
                if let Some(matrix) = &self.matrix {
                    self.is_searching_more_messages = true;
                    let matrix = matrix.clone();
                    Task::perform(
                        async move {
                            matrix
                                .search_messages_in_room_next_batch(20)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| Action::from(Message::MessageSearchMoreResults(res)),
                    )
                } else {
                    Task::none()
                }
            }
            Message::MessageSearchMoreResults(res) => {
                self.is_searching_more_messages = false;
                match res {
                    Ok((results, has_more)) => {
                        self.message_search_results.extend(results);
                        self.search_has_more = has_more;
                    }
                    Err(e) => {
                        self.error =
                            Some(crate::fl!("search-server-failed", error = e).to_string());
                    }
                }
                Task::none()
            }
            Message::GlobalMessageSearchResults(generation, res) => {
                // Same stale-discard guard as the in-room search; both share
                // `search_generation`.
                if generation != self.search_generation {
                    return Task::none();
                }
                self.is_searching_global_messages = false;
                match res {
                    Ok(results) => {
                        self.global_message_search_results = results;
                    }
                    Err(e) => {
                        self.global_message_search_results.clear();
                        self.error =
                            Some(crate::fl!("search-server-failed", error = e).to_string());
                    }
                }
                Task::none()
            }
            Message::SetGlobalSearchScope(scope) => {
                self.global_search_scope = scope;
                // Clear stale hits immediately; the re-fired query repopulates.
                self.global_message_search_results.clear();
                // Re-run the current query under the new scope by re-entering
                // the search dispatch. This reuses the debounce so toggling
                // the filter isn't an instant DoS.
                self.handle_update(Message::SearchQueryChanged(self.search_query.clone()))
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
            Message::SetPendingEventFocus(event_id) => {
                // Set the event focus so the next `TimelineInitFinished`
                // scrolls to it (or builds an event-focused timeline). Fired by
                // `OpenRoomEvent` as a follow-up to `RoomSelected`: the
                // `RoomSelected` handler clears `pending_event_focus`, so the
                // focus must be set *after* the room switch in the same batch
                // (see `Message::OpenRoomEvent` and
                // `test_room_selected_clears_event_focus`).
                self.pending_event_focus = Some(event_id);
                Task::none()
            }
            Message::OpenRoomEvent { room_id, event_id } => {
                self.handle_open_room_event(room_id, event_id)
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
}
