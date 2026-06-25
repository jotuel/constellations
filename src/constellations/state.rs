use super::Constellations;
use crate::matrix;
use crate::utils::contains_ignore_ascii_case;

fn build_error_notification(body: &str) -> notify_rust::Notification {
    let mut notification = notify_rust::Notification::new();
    notification
        .appname("Constellations")
        .summary("Constellations Error")
        .body(body)
        .icon("dialog-error");
    notification
}

impl Constellations {
    pub fn set_error(&mut self, error: String) {
        tracing::error!("Error occurred: {}", error);
        let error_clone = error.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = build_error_notification(&error_clone).show_async().await;
            });
        } else {
            let _ = build_error_notification(&error_clone).show();
        }
        self.error = Some(error);
    }

    pub fn update_filtered_rooms(&mut self) {
        let is_search_empty = self.search_query.is_empty();

        let is_query_ascii = self.search_query.is_ascii();
        let search_query_lower_fallback =
            (!is_query_ascii).then(|| self.search_query.to_lowercase());

        let filter_by_search = |room: &matrix::RoomData| {
            if is_search_empty {
                true
            } else {
                room.name
                    .as_ref()
                    .map(|n| {
                        contains_ignore_ascii_case(
                            n,
                            &self.search_query,
                            search_query_lower_fallback.as_deref(),
                        )
                    })
                    .unwrap_or(false)
                    || contains_ignore_ascii_case(
                        &room.id,
                        &self.search_query,
                        search_query_lower_fallback.as_deref(),
                    )
            }
        };

        if let Some(selected_space) = &self.selected_space {
            if let Some(matrix) = &self.matrix {
                // ⚡ Bolt Optimization: Reuse the existing vector allocation to avoid O(N) allocation on every keystroke
                let mut rooms = std::mem::take(&mut self.filtered_room_list);

                if matrix.filter_in_space_bulk_sync(
                    self.room_list
                        .iter()
                        .enumerate()
                        .filter(|(_, r)| !r.is_space),
                    selected_space,
                    &mut rooms,
                    filter_by_search,
                ) {
                    rooms.sort_by(|&a, &b| {
                        let ra = &self.room_list[a];
                        let rb = &self.room_list[b];
                        match (&ra.order, &rb.order) {
                            (Some(oa), Some(ob)) => oa.cmp(ob).then_with(|| ra.id.cmp(&rb.id)),
                            (Some(_), None) => std::cmp::Ordering::Less,
                            (None, Some(_)) => std::cmp::Ordering::Greater,
                            (None, None) => ra.id.cmp(&rb.id),
                        }
                    });
                    self.filtered_room_list = rooms;
                } else {
                    // If we couldn't get the lock, just return and keep the old list
                    self.filtered_room_list = rooms;
                    return;
                }
            }

            // Re-filter other_rooms to remove any that we've now joined
            self.other_rooms
                .retain(|r| !self.joined_room_ids.contains(r.id.as_ref()));

            let mut filtered_other = std::mem::take(&mut self.filtered_other_rooms);
            filtered_other.clear();
            filtered_other.extend(
                self.other_rooms
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| filter_by_search(r))
                    .map(|(i, _)| i),
            );
            self.filtered_other_rooms = filtered_other;
        } else {
            let mut rooms = std::mem::take(&mut self.filtered_room_list);
            rooms.clear();
            rooms.extend(
                self.room_list
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| !r.is_space && filter_by_search(r))
                    .map(|(i, _)| i),
            );
            rooms.sort_by(|&a, &b| self.room_list[a].id.cmp(&self.room_list[b].id));
            self.filtered_room_list = rooms;
            self.other_rooms.clear();
            self.filtered_other_rooms.clear();
        }
    }
}
