use crate::{Constellations, Message, SettingsPanel};
use cosmic::{Action, Task};

impl Constellations {
    pub(super) fn handle_toggle_search(&mut self) -> Task<Action<Message>> {
        self.is_search_active = !self.is_search_active;
        if !self.is_search_active {
            self.search_query.clear();
            self.room_settings.member_filter.clear();
            self.space_settings.child_filter.clear();
            self.public_search_results.clear();
            self.is_searching_public = false;
            self.message_search_results.clear();
            self.is_searching_messages = false;
            self.search_has_more = false;
            self.is_searching_more_messages = false;
            self.global_message_search_results.clear();
            self.is_searching_global_messages = false;
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

    pub(super) fn handle_search_query_changed(&mut self, query: String) -> Task<Action<Message>> {
        self.search_query = query.clone();
        self.search_has_more = false;
        self.is_searching_more_messages = false;
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
            self.search_generation = self.search_generation.wrapping_add(1);
            let generation = self.search_generation;

            // Public rooms / spaces directory search (existing).
            if let Some(matrix) = &self.matrix {
                let query_str = self.search_query.trim().to_string();
                let matrix = matrix.clone();
                self.is_searching_public = true;

                tasks.push(Task::perform(
                    async move {
                        // Debounce: wait for typing to settle before
                        // querying the homeserver public room directory.
                        tokio::time::sleep(std::time::Duration::from_millis(350)).await;
                        matrix.search_public_rooms(query_str, Some(20)).await
                    },
                    move |res| {
                        Action::from(Message::PublicSearchResults(
                            generation,
                            res.map_err(|e| e.to_string()),
                        ))
                    },
                ));
            }

            // Message search. Exactly one of two branches fires per keystroke:
            // the in-room search when a room is selected, or the global
            // (cross-room) search when none is. Both are debounced and share
            // `search_generation` so a stale result from either is discarded.
            if let Some(matrix) = &self.matrix {
                let query_str = self.search_query.trim().to_string();

                if let Some(room_id) = &self.selected_room {
                    // In-room search.
                    self.is_searching_messages = true;
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
                } else {
                    // Global search across all joined rooms (local seshat
                    // index). The scope (All/DMs/Groups) is captured by copy;
                    // changing it re-fires the query via `SetGlobalSearchScope`.
                    self.is_searching_global_messages = true;
                    let scope = self.global_search_scope;
                    let matrix = matrix.clone();
                    tasks.push(Task::perform(
                        async move {
                            tokio::time::sleep(std::time::Duration::from_millis(350)).await;
                            matrix
                                .search_messages_global(&query_str, 20, scope)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        move |res| {
                            Action::from(Message::GlobalMessageSearchResults(generation, res))
                        },
                    ));
                }
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
            self.search_has_more = false;
            self.is_searching_more_messages = false;
            self.global_message_search_results.clear();
            self.is_searching_global_messages = false;
            // Invalidate any in-flight message search so a late result
            // doesn't repopulate stale hits for the cleared query.
            self.search_generation = self.search_generation.wrapping_add(1);
            Task::none()
        }
    }
}
