use crate::matrix::RoomData;
use std::sync::Arc;

use super::state::SpaceInfo;

#[derive(Debug, Clone)]
pub enum Message {
    LoadSpace(Arc<str>),
    SpaceLoaded(Result<SpaceInfo, String>),
    IsPublicChanged(bool),
    IsInviteOnlyChanged(bool),
    NameChanged(String),
    TopicChanged(String),
    CanonicalAliasChanged(String),
    SaveSpace,
    SpaceSaved(Result<(), String>),
    DismissError,
    LoadChildren,
    ChildrenLoaded(Result<Vec<RoomData>, String>),
    AddChild,
    ChildAdded(Result<(), String>),
    RemoveChild(String),
    ChildRemoved(String, Result<(), String>),
    NewChildIdChanged(String),
    NewChildOrderChanged(String),
    ChildOrderInputChanged(String, String),
    SaveChildOrder(String),
    ChildOrderSaved(Result<(), String>),
    ToggleChildSuggested(String, bool),
    ChildSuggestedToggled(Result<(), String>),
    AvatarMediaFetched(Result<Vec<u8>, String>),
    SelectAvatar,
    AvatarFileSelected(Option<std::path::PathBuf>),
    AvatarUploaded(Result<(), String>),
    SetChildJoinRule(String, matrix_sdk::ruma::events::room::join_rules::JoinRule),
    ChildFilterChanged(String),
}
