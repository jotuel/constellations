use super::*;

impl MatrixEngine {
    pub async fn client(&self) -> Client {
        self.inner.read().await.client.clone()
    }

    pub async fn sync_service(&self) -> Option<Arc<SyncService>> {
        self.inner.read().await.sync_service.clone()
    }

    pub async fn room_list_service(&self) -> Option<Arc<RoomListService>> {
        self.inner.read().await.room_list_service.clone()
    }

    pub async fn set_room_list_controller(
        &self,
        controller: Arc<RoomListDynamicEntriesController>,
    ) {
        let mut inner = self.inner.write().await;
        inner.room_list_controller = Some(controller);
    }

    pub async fn set_media_previews_display_policy(&self, enabled: bool) -> Result<()> {
        info!("Setting media previews display policy to: {}", enabled);
        // Placeholder for future SDK integration
        Ok(())
    }

    pub async fn set_invite_avatars_display_policy(&self, enabled: bool) -> Result<()> {
        info!("Setting invite avatars display policy to: {}", enabled);
        // Placeholder for future SDK integration
        Ok(())
    }

    pub async fn update_room_list_filter(&self, selected_space: Option<OwnedRoomId>) -> Result<()> {
        let inner = self.inner.read().await;
        if let Some(controller) = &inner.room_list_controller {
            use matrix_sdk_ui::room_list_service::filters;

            let filter: Box<dyn matrix_sdk_ui::room_list_service::filters::Filter + Send + Sync> =
                if let Some(space_id) = selected_space {
                    let hierarchy = inner.space_hierarchy.clone();
                    let space_id_clone = space_id.clone();
                    // Custom filter that checks if the room is in the selected space OR is a space itself
                    // This ensures the SpaceSwitcher always has access to all spaces.
                    Box::new(filters::new_filter_any(vec![Box::new(
                        move |item: &matrix_sdk_ui::room_list_service::RoomListItem| {
                            hierarchy.is_in_space(item.room_id(), &space_id_clone)
                                || hierarchy.is_known_space(item.room_id())
                        },
                    )]))
                } else {
                    // No space selected, show all rooms
                    Box::new(filters::new_filter_all(vec![]))
                };

            controller.set_filter(filter);
        }
        Ok(())
    }

    fn strip_reply_quote(body: &str) -> &str {
        let mut actual_line = None;
        let mut in_quote = false;
        for line in body.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with('>') {
                in_quote = true;
                continue;
            }
            if in_quote && trimmed.is_empty() {
                continue;
            }
            actual_line = Some(line);
            break;
        }

        if let Some(line) = actual_line {
            line.trim()
        } else {
            body.split('\n').next().unwrap_or("").trim()
        }
    }

    #[allow(clippy::collapsible_if)]
    async fn fetch_avatar_url(room: &matrix_sdk::Room) -> Option<String> {
        let mut avatar_url = room.avatar_url().map(|u| u.to_string());
        if room.joined_members_count() == 2 || room.active_members_count() == 2 {
            let client = room.client();
            if let Some(my_user_id) = client.user_id() {
                if let Ok(members) = room
                    .members_no_sync(matrix_sdk::RoomMemberships::ACTIVE)
                    .await
                {
                    let other_member = members.iter().find(|m| m.user_id() != my_user_id);
                    if let Some(other_member) = other_member {
                        if let Some(other_avatar) = other_member.avatar_url() {
                            avatar_url = Some(other_avatar.to_string());
                        }
                    }
                }
            }
        }
        avatar_url
    }

    async fn fetch_last_message(room: &matrix_sdk::Room) -> Option<String> {
        match room.latest_event().await {
            LatestEventValue::Remote {
                content: TimelineItemContent::MsgLike(m),
                ..
            }
            | LatestEventValue::Local {
                content: TimelineItemContent::MsgLike(m),
                ..
            } => {
                if let MsgLikeKind::Message(msg_content) = &m.kind {
                    let cleaned = Self::strip_reply_quote(msg_content.body());
                    let mut msg = cleaned.to_string();
                    if msg.len() > 30 {
                        msg.truncate(26);
                        msg.push_str("...");
                    }
                    Some(msg)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    async fn fetch_space_hierarchy_data(
        &self,
        room: &matrix_sdk::Room,
        is_space: bool,
    ) -> (Option<String>, Option<String>, bool) {
        if is_space {
            let mut inner = self.inner.write().await;
            inner.space_hierarchy.add_space(room.room_id().to_owned());
        }

        let inner = self.inner.read().await;
        let parent_id = inner
            .space_hierarchy
            .parents
            .get(room.room_id())
            .and_then(|parents| parents.iter().next());

        let (order, suggested) = parent_id
            .and_then(|p| {
                inner
                    .space_hierarchy
                    .children
                    .get(p)
                    .and_then(|c| c.get(room.room_id()))
            })
            .map(|d| (d.order.clone(), d.suggested))
            .unwrap_or((None, false));

        (parent_id.map(|id| id.to_string()), order, suggested)
    }

    fn extract_allowed_spaces_from_join_rule(
        join_rule: &matrix_sdk::ruma::events::room::join_rules::JoinRule,
    ) -> Vec<matrix_sdk::ruma::OwnedRoomId> {
        match join_rule {
            matrix_sdk::ruma::events::room::join_rules::JoinRule::Restricted(r) => r
                .allow
                .iter()
                .filter_map(|a| match a {
                    matrix_sdk::ruma::events::room::join_rules::AllowRule::RoomMembership(m) => {
                        Some(m.room_id.clone())
                    }
                    _ => None,
                })
                .collect(),
            matrix_sdk::ruma::events::room::join_rules::JoinRule::KnockRestricted(r) => r
                .allow
                .iter()
                .filter_map(|a| match a {
                    matrix_sdk::ruma::events::room::join_rules::AllowRule::RoomMembership(m) => {
                        Some(m.room_id.clone())
                    }
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    async fn fetch_join_rule_and_allowed_spaces(
        room: &matrix_sdk::Room,
    ) -> Result<(
        Option<matrix_sdk::ruma::events::room::join_rules::JoinRule>,
        Vec<matrix_sdk::ruma::OwnedRoomId>,
    )> {
        if let Ok(Some(event)) = room
            .get_state_event_static::<matrix_sdk::ruma::events::room::join_rules::RoomJoinRulesEventContent>()
            .await
        {
            let (join_rule, allowed_spaces) = match event.deserialize()? {
                matrix_sdk_base::deserialized_responses::SyncOrStrippedState::Sync(
                    matrix_sdk::ruma::events::SyncStateEvent::Original(ev),
                ) => {
                    let join_rule = ev.content.join_rule;
                    let allowed_spaces = Self::extract_allowed_spaces_from_join_rule(&join_rule);
                    (Some(join_rule), allowed_spaces)
                }
                matrix_sdk_base::deserialized_responses::SyncOrStrippedState::Stripped(ev) => {
                    let join_rule = ev.content.join_rule;
                    let allowed_spaces = Self::extract_allowed_spaces_from_join_rule(&join_rule);
                    (Some(join_rule), allowed_spaces)
                }
                _ => (None, Vec::new()),
            };
            Ok((join_rule, allowed_spaces))
        } else {
            Ok((None, Vec::new()))
        }
    }

    pub async fn fetch_room_data(&self, room: &matrix_sdk::Room) -> Result<RoomData> {
        let id: std::sync::Arc<str> = room.room_id().as_str().into();
        let name = match room.name() {
            Some(n) => Some(n.to_string()),
            None => room.cached_display_name().map(|n| n.to_string()),
        };

        let unread_count = room.unread_notification_counts().notification_count as u32;
        let avatar_url = Self::fetch_avatar_url(room).await;
        let last_message = Self::fetch_last_message(room).await;

        let room_type = room.room_type();
        let is_space = room_type == Some(RoomType::Space);

        let (parent_space_id, order, suggested) =
            self.fetch_space_hierarchy_data(room, is_space).await;

        let unread_count_str = if unread_count > 0 {
            Some(format!("({})", unread_count))
        } else {
            None
        };

        let (join_rule, allowed_spaces) = Self::fetch_join_rule_and_allowed_spaces(room).await?;

        Ok(RoomData {
            id,
            name,
            last_message,
            unread_count,
            unread_count_str,
            avatar_url,
            room_type,
            is_space,
            parent_space_id,
            join_rule,
            allowed_spaces,
            order,
            suggested,
        })
    }

    pub async fn start_sync(&self) -> Result<(), SyncError> {
        let client = self.client().await;
        let request =
            matrix_sdk::ruma::api::client::discovery::get_supported_versions::Request::new();
        let versions = client.send(request).await?;
        let supports_sliding_sync = versions
            .unstable_features
            .contains_key("org.matrix.msc4186")
            || versions.versions.iter().any(|v| v == "v1.11");

        if !supports_sliding_sync {
            return Err(SyncError::MissingSlidingSyncSupport);
        }

        let mut inner = self.inner.write().await;

        if let Some(handle) = inner.sync_handle.take() {
            handle.abort();
            if let Some(sync_service) = &inner.sync_service {
                let _ = sync_service.stop().await;
            }
        }

        if let Some(sync_service) = &inner.sync_service {
            let sync_service = sync_service.clone();
            let handle = tokio::spawn(async move {
                info!("Starting Matrix sync service...");
                sync_service.start().await;

                let mut state_stream = sync_service.state();
                while let Some(state) = state_stream.next().await {
                    match state {
                        matrix_sdk_ui::sync_service::State::Terminated
                        | matrix_sdk_ui::sync_service::State::Error(_) => {
                            error!("Matrix sync service stopped or failed. State: {:?}", state);
                            break;
                        }
                        _ => {}
                    }
                }
            });
            inner.sync_handle = Some(handle);
        }

        Ok(())
    }

    pub async fn search_public_rooms(
        &self,
        query: String,
        limit: Option<u32>,
    ) -> Result<Vec<PublicRoom>> {
        let client = {
            let inner = self.inner.read().await;
            inner.client.clone()
        };

        let mut filter = matrix_sdk::ruma::directory::Filter::new();
        if !query.is_empty() {
            filter.generic_search_term = Some(query);
        }

        let mut request =
            matrix_sdk::ruma::api::client::directory::get_public_rooms_filtered::v3::Request::new();
        request.limit = limit.map(|l| l.into());
        request.filter = filter;

        let response = client.public_rooms_filtered(request).await?;

        let rooms = response
            .chunk
            .into_iter()
            .map(|chunk| {
                let is_space = chunk
                    .room_type
                    .as_ref()
                    .map(|t| t == &matrix_sdk::ruma::room::RoomType::Space)
                    .unwrap_or(false);
                PublicRoom {
                    id: chunk.room_id.to_string(),
                    name: chunk.name,
                    topic: chunk.topic,
                    canonical_alias: chunk.canonical_alias.map(|a| a.to_string()),
                    num_joined_members: u64::from(chunk.num_joined_members) as u32,
                    avatar_url: chunk.avatar_url.map(|u| u.to_string()),
                    is_space,
                }
            })
            .collect();

        Ok(rooms)
    }
}
