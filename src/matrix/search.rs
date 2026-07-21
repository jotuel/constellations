use super::*;

impl MatrixEngine {
    pub async fn get_pinned_events(
        &self,
        room_id: &str,
    ) -> Result<Vec<matrix_sdk::ruma::OwnedEventId>> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;
        let pinned = room
            .get_state_event_static::<matrix_sdk::ruma::events::room::pinned_events::RoomPinnedEventsEventContent>()
            .await
            .ok()
            .flatten()
            .and_then(|e| e.deserialize().ok())
            .and_then(|ev| match ev {
                matrix_sdk_base::deserialized_responses::SyncOrStrippedState::Sync(
                    matrix_sdk::ruma::events::SyncStateEvent::Original(ev),
                ) => Some(ev.content.pinned),
                matrix_sdk_base::deserialized_responses::SyncOrStrippedState::Stripped(
                    ev,
                ) => ev.content.pinned,
                _ => None,
            })
            .unwrap_or_default();
        Ok(pinned)
    }

    pub async fn fetch_pinned_event_details(
        &self,
        room_id: &str,
        event_id: &matrix_sdk::ruma::EventId,
    ) -> Result<PinnedEventInfo> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;

        let timeline_event = room.event(event_id, None).await?;
        let (sender_id, origin_server_ts, body) = match timeline_event.kind {
            matrix_sdk::deserialized_responses::TimelineEventKind::Decrypted(decrypted) => {
                let ev = decrypted.event.deserialize()?;
                let sender = ev.sender().to_owned();
                let ts = ev.origin_server_ts();
                let body = match &ev {
                    matrix_sdk::ruma::events::AnyTimelineEvent::MessageLike(msg) => match msg {
                        matrix_sdk::ruma::events::AnyMessageLikeEvent::RoomMessage(
                            matrix_sdk::ruma::events::MessageLikeEvent::Original(
                                matrix_sdk::ruma::events::OriginalMessageLikeEvent {
                                    content, ..
                                },
                            ),
                        ) => content.body().to_string(),
                        _ => "Unsupported message event type".to_string(),
                    },
                    _ => "Unsupported state event type".to_string(),
                };
                (sender, ts, body)
            }
            matrix_sdk::deserialized_responses::TimelineEventKind::UnableToDecrypt {
                event,
                ..
            } => {
                let ev = event.deserialize()?;
                let sender = ev.sender().to_owned();
                let ts = ev.origin_server_ts();
                let body = match &ev {
                    matrix_sdk::ruma::events::AnySyncTimelineEvent::MessageLike(msg) => match msg {
                        matrix_sdk::ruma::events::AnySyncMessageLikeEvent::RoomMessage(
                            matrix_sdk::ruma::events::SyncMessageLikeEvent::Original(
                                matrix_sdk::ruma::events::OriginalSyncMessageLikeEvent {
                                    content,
                                    ..
                                },
                            ),
                        ) => content.body().to_string(),
                        _ => "Unsupported message event type".to_string(),
                    },
                    _ => "Unsupported state event type".to_string(),
                };
                (sender, ts, body)
            }
            matrix_sdk::deserialized_responses::TimelineEventKind::PlainText { event, .. } => {
                let ev = event.deserialize()?;
                let sender = ev.sender().to_owned();
                let ts = ev.origin_server_ts();
                let body = match &ev {
                    matrix_sdk::ruma::events::AnySyncTimelineEvent::MessageLike(msg) => match msg {
                        matrix_sdk::ruma::events::AnySyncMessageLikeEvent::RoomMessage(
                            matrix_sdk::ruma::events::SyncMessageLikeEvent::Original(
                                matrix_sdk::ruma::events::OriginalSyncMessageLikeEvent {
                                    content,
                                    ..
                                },
                            ),
                        ) => content.body().to_string(),
                        _ => "Unsupported message event type".to_string(),
                    },
                    _ => "Unsupported state event type".to_string(),
                };
                (sender, ts, body)
            }
        };

        let ts_millis = u64::from(origin_server_ts.0);
        let datetime =
            chrono::DateTime::from_timestamp_millis(ts_millis as i64).unwrap_or_default();
        let timestamp = datetime
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        // Fetch sender member profile details for name and avatar
        let (sender_name, avatar_url) = if let Ok(Some(member)) = room.get_member(&sender_id).await
        {
            (
                member
                    .display_name()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| sender_id.to_string()),
                member.avatar_url().map(|u| u.to_string()),
            )
        } else {
            (sender_id.to_string(), None)
        };

        Ok(PinnedEventInfo {
            event_id: event_id.to_string(),
            sender_id: sender_id.to_string(),
            sender_name,
            avatar_url,
            timestamp,
            body,
        })
    }

    /// Replaces the room's `m.room.pinned_events` state with the given list.
    pub async fn set_pinned_events(
        &self,
        room_id: &str,
        pinned: Vec<matrix_sdk::ruma::OwnedEventId>,
    ) -> Result<()> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let client = self.client().await;
        let room = client.get_room(&room_id_parsed).context("Room not found")?;
        use matrix_sdk::ruma::events::room::pinned_events::RoomPinnedEventsEventContent;
        let content = RoomPinnedEventsEventContent::new(pinned);
        room.send_state_event(content).await?;
        Ok(())
    }

    pub async fn search_messages_in_room(
        &self,
        room_id: &str,
        query: &str,
        max_results: usize,
    ) -> Result<(Vec<MessageSearchResult>, bool)> {
        // Check the cached decision.
        let use_server = {
            let inner = self.inner.read().await;
            inner.server_search_supported.unwrap_or(true)
        };

        if use_server {
            match self.server_search(room_id, query, max_results, None).await {
                Ok((results, next_batch)) => {
                    // Cache support on first success.
                    let mut inner = self.inner.write().await;
                    inner.server_search_supported = Some(true);
                    let has_more = next_batch.is_some();
                    inner.active_search = Some(ActiveSearch::Server {
                        query: query.to_owned(),
                        room_id: room_id.to_owned(),
                        next_batch,
                    });
                    return Ok((results, has_more));
                }
                Err(SearchError::Unsupported) => {
                    // 404/405 — cache and fall through to local backfill.
                    let mut inner = self.inner.write().await;
                    inner.server_search_supported = Some(false);
                }
                Err(SearchError::Other(e)) => return Err(e),
            }
        }

        self.local_search_with_backfill(room_id, query, max_results)
            .await
    }

    /// Server-side `/search` via `POST /_matrix/client/v3/search`, scoped to a
    /// single room. Returns a typed error so the caller can distinguish
    /// "endpoint not supported" from real failures.
    async fn server_search(
        &self,
        room_id: &str,
        query: &str,
        max_results: usize,
        next_batch: Option<String>,
    ) -> std::result::Result<(Vec<MessageSearchResult>, Option<String>), SearchError> {
        let room_id_parsed =
            RoomId::parse(room_id).map_err(|e| SearchError::Other(anyhow::anyhow!(e)))?;
        let client = self.client().await;

        use matrix_sdk::ruma::api::client::search::search_events::v3;

        let mut filter = matrix_sdk::ruma::api::client::filter::RoomEventFilter::default();
        filter.rooms = Some(vec![room_id_parsed.clone()]);
        filter.limit = Some(
            matrix_sdk::ruma::UInt::try_from(max_results).unwrap_or(matrix_sdk::ruma::UInt::MAX),
        );

        let mut criteria = v3::Criteria::new(query.to_owned());
        criteria.filter = filter;
        criteria.keys = Some(vec![v3::SearchKeys::ContentBody]);

        let mut categories = v3::Categories::new();
        categories.room_events = Some(criteria);
        let mut request = v3::Request::new(categories);
        request.next_batch = next_batch;

        let response = client.send(request).await.map_err(|e| {
            let sdk_err = matrix_sdk::Error::from(e);
            if is_search_unsupported(&sdk_err) {
                SearchError::Unsupported
            } else {
                SearchError::Other(anyhow::anyhow!(sdk_err))
            }
        })?;
        let room_results = response.search_categories.room_events;

        let new_next_batch = room_results.next_batch.clone();

        let mut results = Vec::with_capacity(room_results.results.len());
        for raw_event in room_results.results.into_iter().filter_map(|r| r.result) {
            // The server returns decrypted plain-text events (it has the keys
            // for rooms we're joined to), so a single deserialize suffices.
            let event: matrix_sdk::ruma::events::AnyTimelineEvent = raw_event
                .deserialize()
                .map_err(|e| SearchError::Other(anyhow::anyhow!(e)))?;

            let event_id = event.event_id().to_owned();
            let sender_id = event.sender().to_owned();

            let body = match &event {
                matrix_sdk::ruma::events::AnyTimelineEvent::MessageLike(msg) => match msg {
                    matrix_sdk::ruma::events::AnyMessageLikeEvent::RoomMessage(
                        matrix_sdk::ruma::events::MessageLikeEvent::Original(
                            matrix_sdk::ruma::events::OriginalMessageLikeEvent { content, .. },
                        ),
                    ) => content.body().to_string(),
                    _ => "Unsupported message event type".to_string(),
                },
                _ => "Unsupported state event type".to_string(),
            };

            let timestamp = {
                let ts_millis = u64::from(event.origin_server_ts().0);
                chrono::DateTime::from_timestamp_millis(ts_millis as i64)
                    .unwrap_or_default()
                    .with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
            };

            let plain_text = crate::preview::parse_plain_text(&body);
            let links = crate::preview::extract_links(&plain_text);

            results.push(MessageSearchResult {
                room_id: room_id_parsed.clone(),
                room_name: None,
                event_id,
                sender_id,
                body,
                timestamp,
                plain_text,
                links,
            });
        }

        Ok((results, new_next_batch))
    }

    /// Local fallback: paginate the room's full history backwards through the
    /// event cache (which auto-indexes events into the seshat store), then
    /// query the local search index.
    ///
    /// The backfill only does significant network work the first time a room is
    /// searched (the index is empty); on subsequent searches the seshat index
    /// already has the history, so `paginate_backwards` hits the on-disk event
    /// cache (fast, no network) and the local query returns instantly.
    async fn local_search_with_backfill(
        &self,
        room_id: &str,
        query: &str,
        max_results: usize,
    ) -> Result<(Vec<MessageSearchResult>, bool)> {
        let room_id_parsed = RoomId::parse(room_id)?;
        let room = {
            let inner = self.inner.read().await;
            inner
                .client
                .get_room(&room_id_parsed)
                .context("Room not found")?
        };
        let timeline = self.timeline(room_id).await?;

        // 1. Backfill: paginate backwards until the timeline start is reached.
        //    The network path feeds events into the seshat index automatically.
        //    Cap iterations as a safety valve against infinite loops on very
        //    large rooms (500 × 50 = 25 000 events).
        for _ in 0..500 {
            let reached_start = timeline.paginate_backwards(50).await?;
            if reached_start {
                break;
            }
        }

        // 2. Query the now-populated local seshat index.
        let mut search_iter = room.search_messages(query.to_owned(), max_results);
        let Some(events) = search_iter.next_events().await? else {
            let mut inner = self.inner.write().await;
            inner.active_search = Some(ActiveSearch::Local {
                room_id: room_id_parsed.clone(),
                search_iter,
            });
            return Ok((Vec::new(), false));
        };

        // 3. Map TimelineEvents → MessageSearchResult using functional style.
        // Bolt Optimization: Functional chain avoids dynamic reallocations by size hint
        let results = events
            .into_iter()
            .filter_map(|e| map_timeline_event(room_id_parsed.clone(), None, e).transpose())
            .collect::<Result<Vec<_>>>()?;

        let mut inner = self.inner.write().await;
        inner.active_search = Some(ActiveSearch::Local {
            room_id: room_id_parsed.clone(),
            search_iter,
        });

        Ok((results, true))
    }

    pub async fn search_messages_in_room_next_batch(
        &self,
        max_results: usize,
    ) -> Result<(Vec<MessageSearchResult>, bool)> {
        // Bolt Optimization: Retrieve owned ActiveSearch to avoid holding
        // the RwLockWriteGuard of inner across await boundaries.
        let active_search = {
            let mut inner = self.inner.write().await;
            inner.active_search.take()
        };

        let Some(mut search) = active_search else {
            return Ok((Vec::new(), false));
        };

        let res = match &mut search {
            ActiveSearch::Local {
                room_id,
                search_iter,
            } => {
                let events_opt: Option<Vec<matrix_sdk::deserialized_responses::TimelineEvent>> =
                    search_iter.next_events().await?;
                let has_more = events_opt.as_ref().is_some_and(|evs| !evs.is_empty());
                let events = events_opt.unwrap_or_default();

                let results = events
                    .into_iter()
                    .filter_map(|e| map_timeline_event(room_id.clone(), None, e).transpose())
                    .collect::<Result<Vec<_>>>()?;

                Ok((results, has_more))
            }
            ActiveSearch::Server {
                query,
                room_id,
                next_batch,
            } => {
                if next_batch.is_none() {
                    Ok((Vec::new(), false))
                } else {
                    match self
                        .server_search(room_id, query, max_results, next_batch.clone())
                        .await
                    {
                        Ok((results, new_next_batch)) => {
                            *next_batch = new_next_batch;
                            let has_more = next_batch.is_some();
                            Ok((results, has_more))
                        }
                        Err(SearchError::Unsupported) => Ok((Vec::new(), false)),
                        Err(SearchError::Other(e)) => Err(e),
                    }
                }
            }
        };

        {
            let mut inner = self.inner.write().await;
            inner.active_search = Some(search);
        }

        res
    }

    /// Search across all joined rooms via the local seshat index. Uses
    /// `matrix_sdk::Client::search_messages` (`GlobalSearchIterator`), which
    /// queries each room's local index (the same one the in-room local fallback
    /// populates). Does not hit the server `/search` endpoint.
    ///
    /// `scope` narrows the working set to DM / group rooms before searching.
    /// Unlike [`search_messages_in_room`], there is no server probe and no
    /// backfill step here — the global iterator only sees events already in
    /// the client's index (per-room backfill happens when each room is opened
    /// or in-room-searched). Pagination / "load more" is a separate concern
    /// (issue #303); this fetches a single batch.
    pub async fn search_messages_global(
        &self,
        query: &str,
        max_results: usize,
        scope: GlobalSearchScope,
    ) -> Result<Vec<MessageSearchResult>> {
        let client = self.client().await;

        let builder = client.search_messages(query.to_owned(), max_results);
        let builder = match scope {
            GlobalSearchScope::All => builder,
            GlobalSearchScope::DmsOnly => builder.only_dm_rooms().await?,
            GlobalSearchScope::GroupsOnly => builder.no_dms().await?,
        };
        let mut search_iter = builder.build();

        let Some(events) = search_iter.next_events().await? else {
            return Ok(Vec::new());
        };

        let mut results = Vec::with_capacity(events.len());
        for (room_id, event) in events {
            // Resolve a display name for the originating room (best-effort).
            let room_name = client.get_room(&room_id).and_then(|room| {
                room.name()
                    .or_else(|| room.cached_display_name().map(|n| n.to_string()))
            });
            if let Some(hit) = map_timeline_event(room_id, room_name, event)? {
                results.push(hit);
            }
        }

        Ok(results)
    }
}
