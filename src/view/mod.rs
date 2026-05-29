use cosmic::{Action, Task};

use crate::{Constellations, Message};

pub mod app;
pub mod chat;
pub mod error;
pub mod login;
pub mod switcher;

const SPACE_AVATAR_WIDTH: i32 = 32;
const SPACE_AVATAR_HEIGHT: i32 = 32;
const ROOM_AVATAR_WIDTH: i32 = 24;
const ROOM_AVATAR_HEIGHT: i32 = 24;
const AVATAR_RADIUS: i32 = 15;

impl Constellations {
    pub fn get_room_name(&self, room_id: &str) -> Option<&str> {
        if let Some(room) = self.room_list.iter().find(|r| r.id.as_ref() == room_id)
            && let Some(name) = &room.name {
                return Some(name.as_str());
            }
        self.room_name_cache.get(room_id).map(|s| s.as_str())
    }

    pub fn update_title(&mut self) -> Task<Action<Message>> {
        let title = self.selected_room.as_ref()
            .and_then(|id| self.get_room_name(id))
            .unwrap_or("Constellations - Matrix Client");
        self.core.set_header_title(title.to_string());
        Task::none()
    }
}
