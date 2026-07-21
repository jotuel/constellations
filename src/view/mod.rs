use cosmic::{Action, Task};

use crate::{Constellations, Message};

use std::sync::LazyLock;

pub mod app;
pub mod chat;
pub mod error;
pub mod login;
pub mod switcher;
mod tests;

const SPACE_AVATAR_WIDTH: i32 = 32;
const SPACE_AVATAR_HEIGHT: i32 = 32;
const ROOM_AVATAR_WIDTH: i32 = 24;
const ROOM_AVATAR_HEIGHT: i32 = 24;
const AVATAR_RADIUS: i32 = 15;
const ROOM_SWITCHER_WIDTH: i32 = 250;

// chat.rs
static ADD_REACTION: LazyLock<String> = LazyLock::new(|| crate::fl!("add-reaction"));
static TOOLTIP_REPLY: LazyLock<String> = LazyLock::new(|| crate::fl!("tooltip-reply"));
static TOOLTIP_THREAD: LazyLock<String> = LazyLock::new(|| crate::fl!("tooltip-thread"));
static TOOLTIP_EDIT: LazyLock<String> = LazyLock::new(|| crate::fl!("tooltip-edit"));
static TOOLTIP_DELETE: LazyLock<String> = LazyLock::new(|| crate::fl!("tooltip-delete"));
static TOOLTIP_ATTACH: LazyLock<String> = LazyLock::new(|| crate::fl!("tooltip-attach"));
static TOOLTIP_LOCATION: LazyLock<String> = LazyLock::new(|| crate::fl!("tooltip-share-location"));
static TOOLTIP_FIND: LazyLock<String> = LazyLock::new(|| crate::fl!("tooltip-preview"));
static TOOLTIP_EMOJIS: LazyLock<String> = LazyLock::new(|| crate::fl!("tooltip-emojis"));
static UNIGNORE_USER: LazyLock<String> = LazyLock::new(|| crate::fl!("unignore-user"));
static IGNORE: LazyLock<String> = LazyLock::new(|| crate::fl!("ignore"));
static REPLY: LazyLock<String> = LazyLock::new(|| crate::fl!("reply"));
static REPLIES: LazyLock<String> = LazyLock::new(|| crate::fl!("replies"));
static DOWNLOAD_IMAGE: LazyLock<String> = LazyLock::new(|| crate::fl!("download-image"));
static DOWNLOAD_FILE: LazyLock<String> = LazyLock::new(|| crate::fl!("download-file"));
static DOWNLOADED: LazyLock<String> = LazyLock::new(|| crate::fl!("downloaded"));
static OPEN_THREAD: LazyLock<String> = LazyLock::new(|| crate::fl!("thread"));
static CLOSE_THREAD: LazyLock<String> = LazyLock::new(|| crate::fl!("close-thread"));
static TOOLTIP_COPY_LINK: LazyLock<String> = LazyLock::new(|| crate::fl!("copy-link"));
static TOOLTIP_COPY_ROOM_LINK: LazyLock<String> = LazyLock::new(|| crate::fl!("copy-room-link"));
// switcher.rs
static ALL_ROOMS: LazyLock<String> = LazyLock::new(|| crate::fl!("all-rooms"));
static ROOM_NAME: LazyLock<String> = LazyLock::new(|| crate::fl!("room-name"));
static SPACE_NAME: LazyLock<String> = LazyLock::new(|| crate::fl!("space-name"));
static CREATE: LazyLock<String> = LazyLock::new(|| crate::fl!("create"));
static ENTER_ROOM_NAME: LazyLock<String> = LazyLock::new(|| crate::fl!("enter-room-name"));
static ENTER_SPACE_NAME: LazyLock<String> = LazyLock::new(|| crate::fl!("enter-space-name"));
static CANCEL: LazyLock<String> = LazyLock::new(|| crate::fl!("cancel"));
static UNKNOWN_SPACE: LazyLock<String> = LazyLock::new(|| crate::fl!("unknown-space"));
static JOINED_ROOMS: LazyLock<String> = LazyLock::new(|| crate::fl!("joined-rooms"));
static SUBSPACES: LazyLock<String> = LazyLock::new(|| crate::fl!("subspaces"));
static OTHER_ROOMS: LazyLock<String> = LazyLock::new(|| crate::fl!("other-rooms"));
static JOIN: LazyLock<String> = LazyLock::new(|| crate::fl!("join"));
static UNKNOWN_ROOM: LazyLock<String> = LazyLock::new(|| crate::fl!("unknown-room"));
static ROOM_HAS_NO_AVATAR: LazyLock<String> = LazyLock::new(|| crate::fl!("room-has-no-avatar"));
static CREATE_ROOM: LazyLock<String> = LazyLock::new(|| crate::fl!("create-room"));
static CREATE_SPACE: LazyLock<String> = LazyLock::new(|| crate::fl!("create-space"));

impl Constellations {
    pub fn get_room_name(&self, room_id: &str) -> Option<&str> {
        if let Some(room) = self.room_list.iter().find(|r| r.id.as_ref() == room_id)
            && let Some(name) = &room.name
        {
            return Some(name.as_str());
        }
        self.room_name_cache.get(room_id).map(|s| s.as_str())
    }

    pub fn update_title(&mut self) -> Task<Action<Message>> {
        let title = self
            .selected_room
            .as_ref()
            .and_then(|id| self.get_room_name(id))
            .map(str::to_string)
            .unwrap_or_else(|| crate::fl!("app-subtitle"));
        self.core.set_header_title(title.to_string());
        Task::none()
    }
}
