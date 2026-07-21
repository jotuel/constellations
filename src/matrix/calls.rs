use super::*;

impl MatrixEngine {
    pub(super) fn setup_event_handlers(&self, client: &Client) {
        self.setup_message_notification_handler(client);
        self.setup_space_hierarchy_handlers(client);
        self.setup_call_member_handler(client);
    }

    fn setup_message_notification_handler(&self, client: &Client) {
        client.add_event_handler(
            |event: matrix_sdk::ruma::events::room::message::SyncRoomMessageEvent,
             room: matrix_sdk::Room| {
                async move {
                    if let matrix_sdk::ruma::events::room::message::SyncRoomMessageEvent::Original(
                        ev,
                    ) = event
                    {
                        // Ignore our own messages
                        if let Some(user_id) = room.client().user_id()
                            && ev.sender == user_id
                        {
                            return;
                        }

                        // Avoid spamming during initial sync by checking if event is older
                        // than 5 minutes. `now` falls back to 0 (treated as stale) when the
                        // system clock reads before the Unix epoch instead of panicking.
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis();

                        if !is_recent_enough_to_notify(now, ev.origin_server_ts.0.into()) {
                            return;
                        }

                        let room_name = room.name().unwrap_or_else(|| "Unknown Room".to_string());

                        let sender = if let Ok(Some(member)) = room.get_member(&ev.sender).await {
                            member
                                .display_name()
                                .map(|n| n.to_owned())
                                .unwrap_or_else(|| ev.sender.as_str().to_string())
                        } else {
                            ev.sender.as_str().to_string()
                        };

                        let body = match &ev.content.msgtype {
                            matrix_sdk::ruma::events::room::message::MessageType::Text(text) => {
                                text.body.clone()
                            }
                            matrix_sdk::ruma::events::room::message::MessageType::Image(_) => {
                                "📷 Image".to_string()
                            }
                            matrix_sdk::ruma::events::room::message::MessageType::Video(_) => {
                                "🎥 Video".to_string()
                            }
                            matrix_sdk::ruma::events::room::message::MessageType::Audio(_) => {
                                "🎵 Audio".to_string()
                            }
                            matrix_sdk::ruma::events::room::message::MessageType::File(_) => {
                                "📎 File".to_string()
                            }
                            _ => "New message".to_string(),
                        };

                        let _ = notify_rust::Notification::new()
                            .appname("Constellations")
                            .summary(&format!("{} in {}", sender, room_name))
                            .body(&body)
                            .show_async()
                            .await;
                    }
                }
            },
        );
    }

    fn setup_space_hierarchy_handlers(&self, client: &Client) {
        macro_rules! handle_space_hierarchy {
            (
                $client:expr,
                $inner_clone:expr,
                $content_type:ty,
                $parent_id:expr,
                $child_id:expr,
                $add_logic:expr,
                $remove_msg:literal,
                $add_msg:literal $(, $add_arg:expr)* ;
                $redacted_msg:literal
            ) => {
                let inner_clone = $inner_clone.clone();
                $client.add_event_handler(
                    move |event: SyncStateEvent<$content_type>, room: Room| {
                        let inner = inner_clone.clone();
                        async move {
                            let room_id = room.room_id().to_owned();
                            let state_key = match RoomId::parse(event.state_key()) {
                                Ok(id) => id,
                                Err(_) => return,
                            };

                            let parent_id = $parent_id(&room_id, &state_key);
                            let child_id = $child_id(&room_id, &state_key);

                            let mut inner_write = inner.write().await;
                            match event {
                                SyncStateEvent::Original(ev) => {
                                    if ev.content.via.is_empty() {
                                        inner_write
                                            .space_hierarchy
                                            .remove_child(&parent_id, &child_id);
                                        info!(
                                            $remove_msg,
                                            state_key, room_id
                                        );
                                    } else {
                                        $add_logic(&mut inner_write, &parent_id, &child_id, &ev);
                                        info!(
                                            $add_msg,
                                            state_key, room_id $(, $add_arg(&ev))*
                                        );
                                    }
                                }
                                SyncStateEvent::Redacted(_) => {
                                    inner_write
                                        .space_hierarchy
                                        .remove_child(&parent_id, &child_id);
                                    info!(
                                        $redacted_msg,
                                        state_key, room_id
                                    );
                                }
                            }
                        }
                    },
                );
            };
        }

        handle_space_hierarchy!(
            client,
            self.inner,
            SpaceChildEventContent,
            |room_id: &OwnedRoomId, _state_key: &OwnedRoomId| room_id.clone(),
            |_room_id: &OwnedRoomId, state_key: &OwnedRoomId| state_key.clone(),
            |inner_write: &mut tokio::sync::RwLockWriteGuard<'_, MatrixEngineInner>,
             parent_id: &OwnedRoomId,
             child_id: &OwnedRoomId,
             ev: &matrix_sdk::ruma::events::OriginalSyncStateEvent<SpaceChildEventContent>| {
                inner_write.space_hierarchy.add_child(
                    parent_id.clone(),
                    child_id.clone(),
                    ev.content.order.as_ref().map(|o| o.to_string()),
                    ev.content.suggested,
                );
            },
            "Space hierarchy updated: {} removed from {}",
            "Space hierarchy updated: {} is child of {} (order: {:?})",
            |ev: &matrix_sdk::ruma::events::OriginalSyncStateEvent<SpaceChildEventContent>| ev.content.order.clone() ;
            "Space hierarchy updated: {} removed from {} (redacted)"
        );

        handle_space_hierarchy!(
            client,
            self.inner,
            SpaceParentEventContent,
            |_room_id: &OwnedRoomId, state_key: &OwnedRoomId| state_key.clone(),
            |room_id: &OwnedRoomId, _state_key: &OwnedRoomId| room_id.clone(),
            |inner_write: &mut tokio::sync::RwLockWriteGuard<'_, MatrixEngineInner>,
             parent_id: &OwnedRoomId,
             child_id: &OwnedRoomId,
             _ev: &matrix_sdk::ruma::events::OriginalSyncStateEvent<SpaceParentEventContent>| {
                inner_write.space_hierarchy.add_relationship(parent_id.clone(), child_id.clone());
            },
            "Space hierarchy updated: {} removed as parent of {}",
            "Space hierarchy updated: {} is parent of {}" ;
            "Space hierarchy updated: {} removed as parent of {} (redacted)"
        );
    }

    fn setup_call_member_handler(&self, client: &Client) {
        let inner_clone = self.inner.clone();
        client.add_event_handler(
            move |event: SyncStateEvent<
                matrix_sdk::ruma::events::call::member::CallMemberEventContent,
            >,
                  room: Room| {
                let inner = inner_clone.clone();
                async move {
                    let room_id = room.room_id().to_owned();
                    let user_id = match UserId::parse(event.state_key()) {
                        Ok(id) => id,
                        Err(_) => return,
                    };

                    let mut inner_write = inner.write().await;
                    let participants = inner_write.call_participants.entry(room_id).or_default();

                    match event {
                        SyncStateEvent::Original(ev) => {
                            if ev.content.memberships().is_empty() {
                                participants.remove(&user_id);
                            } else {
                                participants.insert(user_id);
                            }
                        }
                        SyncStateEvent::Redacted(_) => {
                            participants.remove(&user_id);
                        }
                    }
                }
            },
        );
    }

    pub async fn get_livekit_token(&self, room_id: &RoomId) -> Result<(String, String)> {
        let client = self.client().await;

        // 1. Get OpenID token
        use matrix_sdk::ruma::api::client::account::request_openid_token;
        let user_id = client.user_id().context("No user ID")?.to_owned();
        let device_id = client.device_id().context("No device ID")?.to_string();
        let request = request_openid_token::v3::Request::new(user_id.clone());
        let openid_token = client.send(request).await?;

        // 2. Discover LiveKit service
        let homeserver = client.homeserver();
        let well_known_url = homeserver.join("/.well-known/matrix/client")?;

        let wk: LiveKitWellKnown = reqwest::get(well_known_url).await?.json().await?;

        let focus = wk
            .rtc_foci
            .iter()
            .find(|f| f.focus_type == "livekit")
            .context("No MatrixRTC configuration found")?;

        // 3. Exchange OpenID for LiveKit JWT
        let mut auth_url = Url::parse(&focus.livekit_service_url)?;
        if auth_url.path().is_empty() || auth_url.path() == "/" {
            auth_url = auth_url.join("get_token")?;
        }
        info!("Sending auth request to: {}", auth_url);

        let member_id = format!("{:016x}", rand::random::<u64>());

        let response = reqwest::Client::new()
            .post(auth_url)
            .json(&serde_json::json!({
                "room_id": room_id,
                "slot_id": "m.call#ROOM",
                "openid_token": {
                    "access_token": openid_token.access_token,
                    "expires_in": openid_token.expires_in.as_secs(),
                    "matrix_server_name": openid_token.matrix_server_name,
                    "token_type": openid_token.token_type,
                },
                "member": {
                    "id": member_id,
                    "claimed_user_id": user_id,
                    "claimed_device_id": device_id,
                }
            }))
            .send()
            .await?;

        let status = response.status();
        let body_text = response.text().await?;
        info!("Auth service response ({}): {}", status, body_text);

        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "Auth service returned error {}: {}",
                status,
                body_text
            ));
        }

        let response: LiveKitAuthResponse = serde_json::from_str(&body_text)?;

        let livekit_url = response
            .livekit_url
            .context("No LiveKit URL found in auth response")?;

        Ok((livekit_url, response.token))
    }

    pub async fn join_call(&self, room_id: &str) -> Result<()> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;
        let user_id = client.user_id().context("No user ID")?.to_owned();
        let device_id = client.device_id().context("No device ID")?.to_owned();

        use matrix_sdk::ruma::events::call::member::{
            ActiveFocus, ActiveLivekitFocus, Application, CallApplicationContent,
            CallMemberEventContent, CallMemberStateKey, CallScope,
        };

        let application =
            Application::Call(CallApplicationContent::new("".to_string(), CallScope::Room));
        let focus_active = ActiveFocus::Livekit(ActiveLivekitFocus::new());
        let foci_preferred = Vec::new();

        let content = CallMemberEventContent::new(
            application,
            device_id,
            focus_active,
            foci_preferred,
            None,
            None,
        );

        let state_key = CallMemberStateKey::new(user_id, None, false);
        room.send_state_event_for_key(&state_key, content).await?;

        // Connect to LiveKit
        let (sfu_url, token) = self.get_livekit_token(&room_id_parsed).await?;

        let (lk_room, mut room_events) =
            livekit::Room::connect(&sfu_url, &token, RoomOptions::default()).await?;

        let lk_room = Arc::new(lk_room);

        let mut inner = self.inner.write().await;
        inner.active_call = Some(lk_room.clone());
        drop(inner);

        tokio::spawn(async move {
            while let Some(event) = room_events.recv().await {
                if let RoomEvent::TrackSubscribed {
                    track,
                    publication: _,
                    participant: _,
                } = event
                {
                    info!("Track subscribed: {:?}", track.sid());
                }
            }
        });

        Ok(())
    }

    pub async fn leave_call(&self, room_id: &str) -> Result<()> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;
        let user_id = client.user_id().context("No user ID")?.to_owned();

        use matrix_sdk::ruma::events::call::member::{CallMemberEventContent, CallMemberStateKey};
        let content = CallMemberEventContent::new_empty(None);
        let state_key = CallMemberStateKey::new(user_id, None, false);
        room.send_state_event_for_key(&state_key, content).await?;

        let mut inner = self.inner.write().await;
        if let Some(lk_room) = inner.active_call.take() {
            lk_room.close().await?;
        }

        Ok(())
    }

    pub async fn get_call_participants(&self, room_id: &str) -> Vec<matrix_sdk::ruma::OwnedUserId> {
        if let Ok(room_id_parsed) = RoomId::parse(room_id) {
            let inner = self.inner.read().await;
            inner
                .call_participants
                .get(&room_id_parsed)
                .map(|p| p.iter().cloned().collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    }
}
