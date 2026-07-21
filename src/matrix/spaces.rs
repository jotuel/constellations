use super::*;

impl MatrixEngine {
    pub async fn get_space_children(&self, space_id: &str) -> Result<Vec<RoomData>> {
        let space_id_parsed = RoomId::parse(space_id)?;
        let client = self.client().await;

        let space = client
            .get_room(&space_id_parsed)
            .context("Space not found")?;

        // Fetch m.space.child events to get definitive orders
        let children_events = space
            .get_state_events_static::<SpaceChildEventContent>()
            .await?;

        let child_data = Self::parse_space_children_events(children_events);

        // Use the hierarchy API to get rich metadata for all rooms in the space
        let mut rooms = Vec::new();
        let mut request = matrix_sdk::ruma::api::client::space::get_hierarchy::v1::Request::new(
            space_id_parsed.clone(),
        );
        request.limit = Some(matrix_sdk::ruma::uint!(100));

        if let Ok(response) = client.send(request).await {
            let mut inner = self.inner.write().await;
            for room_summary in response.rooms {
                let is_space = room_summary
                    .summary
                    .room_type
                    .as_ref()
                    .map(|t| t == &RoomType::Space)
                    .unwrap_or(false);

                let (order, suggested) = child_data
                    .get(&room_summary.summary.room_id)
                    .map(|d| (d.order.clone(), d.suggested))
                    .unwrap_or((None, false));

                // Update local hierarchy knowledge
                inner.space_hierarchy.add_child(
                    space_id_parsed.clone(),
                    room_summary.summary.room_id.clone(),
                    order.clone(),
                    suggested,
                );

                let (join_rule, allowed_spaces) = (None, Vec::new());

                rooms.push(RoomData {
                    id: room_summary.summary.room_id.as_str().into(),
                    name: room_summary.summary.name.clone(),
                    last_message: None,
                    unread_count: 0,
                    unread_count_str: None,
                    avatar_url: room_summary
                        .summary
                        .avatar_url
                        .as_ref()
                        .map(|u| u.to_string()),
                    room_type: room_summary.summary.room_type.clone(),
                    is_space,
                    parent_space_id: Some(space_id.to_string()),
                    join_rule,
                    allowed_spaces,
                    order,
                    suggested,
                });
            }
        } else {
            // Fallback to state events if hierarchy API fails
            rooms = self
                .fallback_space_children_from_state(space_id, &space_id_parsed, child_data)
                .await?;
        }
        Ok(rooms)
    }

    fn parse_space_children_events(
        children_events: Vec<
            matrix_sdk_base::deserialized_responses::RawSyncOrStrippedState<SpaceChildEventContent>,
        >,
    ) -> HashMap<OwnedRoomId, ChildData> {
        children_events
            .into_iter()
            .filter_map(|event| event.deserialize().ok())
            .filter_map(|event| match event {
                matrix_sdk_base::deserialized_responses::SyncOrStrippedState::Sync(
                    matrix_sdk::ruma::events::SyncStateEvent::Original(ev),
                ) => {
                    if !ev.content.via.is_empty() {
                        RoomId::parse(ev.state_key.as_str()).ok().map(|cid| {
                            (
                                cid,
                                ChildData {
                                    order: ev.content.order.as_ref().map(|o| o.to_string()),
                                    suggested: ev.content.suggested,
                                },
                            )
                        })
                    } else {
                        None
                    }
                }
                matrix_sdk_base::deserialized_responses::SyncOrStrippedState::Stripped(ev) => {
                    if !ev
                        .content
                        .via
                        .as_ref()
                        .map(|v| v.is_empty())
                        .unwrap_or(true)
                    {
                        RoomId::parse(ev.state_key.as_str()).ok().map(|cid| {
                            (
                                cid,
                                ChildData {
                                    order: ev.content.order.as_ref().map(|o| o.to_string()),
                                    suggested: ev.content.suggested,
                                },
                            )
                        })
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect()
    }

    async fn fallback_space_children_from_state(
        &self,
        space_id: &str,
        space_id_parsed: &RoomId,
        child_data: HashMap<OwnedRoomId, ChildData>,
    ) -> Result<Vec<RoomData>> {
        let client = self.client().await;
        let mut rooms = Vec::new();
        for (child_id_parsed, data) in child_data {
            {
                let mut inner = self.inner.write().await;
                inner.space_hierarchy.add_child(
                    space_id_parsed.to_owned(),
                    child_id_parsed.clone(),
                    data.order.clone(),
                    data.suggested,
                );
            }

            if let Some(child_room) = client.get_room(&child_id_parsed) {
                rooms.push(self.fetch_room_data(&child_room).await?);
            } else {
                rooms.push(RoomData {
                    id: child_id_parsed.as_str().into(),
                    name: None,
                    last_message: None,
                    unread_count: 0,
                    unread_count_str: None,
                    avatar_url: None,
                    room_type: None,
                    is_space: false,
                    parent_space_id: Some(space_id.to_string()),
                    join_rule: None,
                    allowed_spaces: Vec::new(),
                    order: data.order,
                    suggested: data.suggested,
                });
            }
        }
        Ok(rooms)
    }

    pub async fn add_space_child(
        &self,
        space_id: &str,
        child_id: &str,
        order: Option<String>,
        suggested: bool,
    ) -> Result<()> {
        let space_id_parsed = RoomId::parse(space_id)?;
        let child_id_parsed = RoomId::parse(child_id)?;
        let client = self.client().await;
        let space = client
            .get_room(&space_id_parsed)
            .context("Space not found")?;

        use matrix_sdk::ruma::events::space::child::SpaceChildEventContent;
        let mut via = Vec::new();
        if let Some(server) = client
            .user_id()
            .and_then(|id| id.server_name().to_owned().into())
        {
            via.push(server);
        }

        let mut content = SpaceChildEventContent::new(via);
        content.order = order
            .map(matrix_sdk::ruma::OwnedSpaceChildOrder::try_from)
            .transpose()?;
        content.suggested = suggested;
        space
            .send_state_event_for_key(&child_id_parsed, content)
            .await?;
        Ok(())
    }

    pub async fn remove_space_child(&self, space_id: &str, child_id: &str) -> Result<()> {
        let space_id_parsed = RoomId::parse(space_id)?;
        let child_id_parsed = RoomId::parse(child_id)?;
        let client = self.client().await;
        let space = client
            .get_room(&space_id_parsed)
            .context("Space not found")?;

        // To remove, send an empty via list
        use matrix_sdk::ruma::events::space::child::SpaceChildEventContent;
        let content = SpaceChildEventContent::new(Vec::new());
        space
            .send_state_event_for_key(&child_id_parsed, content)
            .await?;
        Ok(())
    }

    pub async fn create_room(&self, name: &str, is_video: bool) -> Result<OwnedRoomId> {
        let client = self.client().await;
        let mut request = matrix_sdk::ruma::api::client::room::create_room::v3::Request::new();
        request.name = Some(name.to_string());
        if is_video {
            let mut creation_content =
                matrix_sdk::ruma::api::client::room::create_room::v3::CreationContent::new();
            creation_content.room_type = Some(matrix_sdk::ruma::room::RoomType::from(
                "org.matrix.msc3401.call.room",
            ));
            request.creation_content = Some(matrix_sdk::ruma::serde::Raw::new(&creation_content)?);
        }
        let room = client.create_room(request).await?;
        Ok(room.room_id().to_owned())
    }

    pub async fn create_space(&self, name: &str) -> Result<OwnedRoomId> {
        let client = self.client().await;
        let mut request = matrix_sdk::ruma::api::client::room::create_room::v3::Request::new();
        request.name = Some(name.to_string());

        let mut creation_content =
            matrix_sdk::ruma::api::client::room::create_room::v3::CreationContent::new();
        creation_content.room_type = Some(RoomType::Space);
        request.creation_content = Some(matrix_sdk::ruma::serde::Raw::new(&creation_content)?);

        let room = client.create_room(request).await?;
        Ok(room.room_id().to_owned())
    }

    pub async fn get_or_create_dm(
        &self,
        user_id: &matrix_sdk::ruma::UserId,
    ) -> Result<OwnedRoomId> {
        let client = self.client().await;
        if let Some(room) = client.get_dm_room(user_id) {
            Ok(room.room_id().to_owned())
        } else {
            let room = client.create_dm(user_id).await?;
            Ok(room.room_id().to_owned())
        }
    }

    pub async fn is_in_space(&self, room_id: &RoomId, space_id: &RoomId) -> bool {
        let inner = self.inner.read().await;
        inner.space_hierarchy.is_in_space(room_id, space_id)
    }

    pub fn is_in_space_sync(&self, room_id: &RoomId, space_id: &RoomId) -> bool {
        match self.inner.try_read() {
            Ok(inner) => inner.space_hierarchy.is_in_space(room_id, space_id),
            Err(_) => {
                // If we can't get a read lock, we fall back to assuming it might be in the space
                // if we're currently selecting it, to avoid flickering.
                // But we don't have access to selected_space here.
                // For now, just return false but log it.
                false
            }
        }
    }

    pub fn filter_in_space_bulk_sync<'a, I, F, T>(
        &self,
        rooms: I,
        space_id: &RoomId,
        out: &mut Vec<T>,
        mut filter_by_search: F,
    ) -> bool
    where
        I: Iterator<Item = (T, &'a RoomData)>,
        F: FnMut(&RoomData) -> bool,
    {
        match self.inner.try_read() {
            Ok(inner) => {
                out.clear();
                // Bolt Optimization: Calculate all space descendants once (O(S))
                // to avoid O(N) string parsing and O(N * D) tree traversals.
                let mut descendants = inner.space_hierarchy.get_descendants_strs(space_id);
                // Also include the space itself so direct children match.
                descendants.insert(space_id.as_str());

                for (val, room) in rooms {
                    if descendants.contains(&*room.id) && filter_by_search(room) {
                        out.push(val);
                    }
                }
                true
            }
            Err(_) => {
                // If we can't get a read lock, fallback to not filtering correctly
                // or returning nothing. Usually this is transient.
                false
            }
        }
    }
}
