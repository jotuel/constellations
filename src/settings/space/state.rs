use crate::matrix::RoomData;
use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub struct State {
    pub space_id: Option<Arc<str>>,
    pub name: String,
    pub original_name: String,
    pub canonical_alias: String,
    pub original_canonical_alias: String,
    pub is_loading: bool,
    pub is_saving: bool,
    pub error: Option<String>,
    pub children: Vec<RoomData>,
    pub is_loading_children: bool,
    pub new_child_id: String,
    pub new_child_order: String,
    pub pending_child_orders: std::collections::HashMap<String, String>,
    pub is_adding_child: bool,
    pub topic: String,
    pub original_topic: String,
    pub avatar_url: Option<String>,
    pub avatar_handle: Option<cosmic::iced::widget::image::Handle>,
    pub is_uploading_avatar: bool,
    pub is_loading_avatar: bool,
    pub is_public: bool,
    pub original_is_public: bool,
    pub is_invite_only: bool,
    pub original_is_invite_only: bool,
    pub child_filter: String,
}

#[derive(Debug, Clone)]
pub struct SpaceInfo {
    pub name: String,
    pub topic: String,
    pub canonical_alias: Option<String>,
    pub avatar_url: Option<String>,
    pub visibility: matrix_sdk::ruma::api::client::room::Visibility,
    pub join_rule: matrix_sdk::ruma::events::room::join_rules::JoinRule,
}
