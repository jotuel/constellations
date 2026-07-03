use crate::matrix::MatrixEngine;
use cosmic::{Action, Task};
use matrix_sdk::ruma::RoomId;
use std::sync::Arc;

use super::message::Message;
use super::state::{SpaceInfo, State};

impl State {
    pub fn update(
        &mut self,
        message: Message,
        matrix: &Option<MatrixEngine>,
    ) -> Task<Action<crate::Message>> {
        match message {
            Message::LoadSpace(space_id) => {
                if let Some(matrix) = matrix {
                    self.space_id = Some(space_id.clone());
                    self.is_loading = true;
                    self.error = None;

                    let engine = matrix.clone();
                    Task::perform(
                        async move {
                            let room_id_parsed =
                                RoomId::parse(space_id.as_ref()).map_err(|e| e.to_string())?;
                            let client = engine.client().await;
                            let room = client
                                .get_room(&room_id_parsed)
                                .ok_or_else(|| "Space not found".to_string())?;

                            let visibility = engine
                                .get_room_visibility(space_id.as_ref())
                                .await
                                .unwrap_or(
                                    matrix_sdk::ruma::api::client::room::Visibility::Private,
                                );
                            let join_rule = engine
                                .get_room_join_rule(space_id.as_ref())
                                .await
                                .unwrap_or(
                                    matrix_sdk::ruma::events::room::join_rules::JoinRule::Invite,
                                );

                            Ok(SpaceInfo {
                                name: room.name().unwrap_or_default(),
                                topic: room.topic().unwrap_or_default(),
                                canonical_alias: room.canonical_alias().map(|a| a.to_string()),
                                avatar_url: room.avatar_url().map(|u| u.to_string()),
                                visibility,
                                join_rule,
                            })
                        },
                        |res| {
                            Action::from(crate::Message::SpaceSettings(Message::SpaceLoaded(res)))
                        },
                    )
                } else {
                    Task::none()
                }
            }
            Message::SpaceLoaded(res) => {
                self.is_loading = false;
                match res {
                    Ok(info) => {
                        self.name = info.name.clone();
                        self.original_name = info.name;
                        self.topic = info.topic.clone();
                        self.original_topic = info.topic;
                        self.canonical_alias = info.canonical_alias.clone().unwrap_or_default();
                        self.original_canonical_alias =
                            info.canonical_alias.clone().unwrap_or_default();
                        self.avatar_url = info.avatar_url;
                        self.is_public = info.visibility
                            == matrix_sdk::ruma::api::client::room::Visibility::Public;
                        self.original_is_public = self.is_public;
                        self.is_invite_only = info.join_rule
                            == matrix_sdk::ruma::events::room::join_rules::JoinRule::Invite;
                        self.original_is_invite_only = self.is_invite_only;
                        self.error = None;

                        let mut tasks = Vec::new();

                        if let Some(url) = &self.avatar_url
                            && let Some(matrix) = matrix
                        {
                            let engine = matrix.clone();
                            let mxc = url.clone();
                            self.is_loading_avatar = true;
                            tasks.push(Task::perform(
                                async move {
                                    use matrix_sdk::ruma::events::room::MediaSource;
                                    let mxc_uri = <&matrix_sdk::ruma::MxcUri>::from(mxc.as_str());
                                    let source = MediaSource::Plain(mxc_uri.to_owned());
                                    engine.fetch_media(source).await.map_err(|e| e.to_string())
                                },
                                |res| {
                                    Action::from(crate::Message::SpaceSettings(
                                        Message::AvatarMediaFetched(res),
                                    ))
                                },
                            ));
                        }

                        tasks.push(Task::done(Action::from(crate::Message::SpaceSettings(
                            Message::LoadChildren,
                        ))));
                        return Task::batch(tasks);
                    }
                    Err(e) => {
                        self.error = Some(e);
                    }
                }
                Task::none()
            }
            Message::ToggleChildSuggested(child_id, suggested) => {
                if let Some(matrix) = matrix
                    && let Some(space_id) = &self.space_id
                {
                    let engine = matrix.clone();
                    let space_id_clone = space_id.clone();
                    let child_id_clone = child_id.clone();
                    let order = self
                        .children
                        .iter()
                        .find(|c| c.id.as_ref() == child_id)
                        .and_then(|c| c.order.clone());

                    return Task::perform(
                        async move {
                            engine
                                .add_space_child(
                                    space_id_clone.as_ref(),
                                    &child_id_clone,
                                    order,
                                    suggested,
                                )
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| {
                            Action::from(crate::Message::SpaceSettings(
                                Message::ChildSuggestedToggled(res),
                            ))
                        },
                    );
                }
                Task::none()
            }
            Message::ChildSuggestedToggled(res) => {
                match res {
                    Ok(_) => {
                        return Task::done(Action::from(crate::Message::SpaceSettings(
                            Message::LoadChildren,
                        )));
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to update suggested status: {}", e));
                    }
                }
                Task::none()
            }
            Message::AvatarMediaFetched(res) => {
                self.is_loading_avatar = false;
                match res {
                    Ok(data) => {
                        self.avatar_handle =
                            Some(cosmic::iced::widget::image::Handle::from_bytes(data));
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to fetch avatar: {}", e));
                    }
                }
                Task::none()
            }
            Message::SelectAvatar => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif"])
                        .set_title("Select Space Avatar")
                        .pick_file()
                        .await
                        .map(|handle| handle.path().to_owned())
                },
                |res| {
                    Action::from(crate::Message::SpaceSettings(Message::AvatarFileSelected(
                        res,
                    )))
                },
            ),
            Message::AvatarFileSelected(path_opt) => {
                if let Some(path) = path_opt
                    && let Some(matrix) = matrix
                {
                    self.is_uploading_avatar = true;
                    let engine = matrix.clone();
                    let room_id = self.space_id.clone().unwrap_or_else(|| Arc::from(""));

                    return Task::perform(
                        async move {
                            let data = tokio::fs::read(&path).await.map_err(|e| e.to_string())?;
                            let mime = mime_guess::from_path(&path)
                                .first_raw()
                                .unwrap_or("image/jpeg");
                            engine
                                .upload_room_avatar(room_id.as_ref(), data, mime)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| {
                            Action::from(crate::Message::SpaceSettings(Message::AvatarUploaded(
                                res,
                            )))
                        },
                    );
                }
                Task::none()
            }
            Message::AvatarUploaded(res) => {
                self.is_uploading_avatar = false;
                match res {
                    Ok(_) => {
                        if let Some(space_id) = &self.space_id {
                            return self.update(Message::LoadSpace(space_id.clone()), matrix);
                        }
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to upload avatar: {}", e));
                    }
                }
                Task::none()
            }
            Message::LoadChildren => {
                if let Some(matrix) = matrix
                    && let Some(space_id) = &self.space_id
                {
                    self.is_loading_children = true;
                    let engine = matrix.clone();
                    let space_id_clone = space_id.clone();
                    return Task::perform(
                        async move {
                            engine
                                .get_space_children(space_id_clone.as_ref())
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| {
                            Action::from(crate::Message::SpaceSettings(Message::ChildrenLoaded(
                                res,
                            )))
                        },
                    );
                }
                Task::none()
            }
            Message::ChildrenLoaded(res) => {
                self.is_loading_children = false;
                match res {
                    Ok(children) => {
                        self.children = children;
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to load space children: {}", e));
                    }
                }
                Task::none()
            }
            Message::IsPublicChanged(is_public) => {
                self.is_public = is_public;
                Task::none()
            }
            Message::IsInviteOnlyChanged(is_invite_only) => {
                self.is_invite_only = is_invite_only;
                Task::none()
            }
            Message::NameChanged(name) => {
                self.name = name;
                Task::none()
            }
            Message::TopicChanged(topic) => {
                self.topic = topic;
                Task::none()
            }
            Message::CanonicalAliasChanged(alias) => {
                self.canonical_alias = alias;
                Task::none()
            }
            Message::SaveSpace => {
                if let Some(matrix) = matrix {
                    if let Some(space_id) = &self.space_id {
                        self.is_saving = true;
                        self.error = None;

                        let engine = matrix.clone();
                        let new_name = self.name.clone();
                        let new_topic = self.topic.clone();
                        let new_alias = self.canonical_alias.clone();
                        let space_id_clone = space_id.clone();
                        let original_name = self.original_name.clone();
                        let original_topic = self.original_topic.clone();
                        let original_alias = self.original_canonical_alias.clone();
                        let new_is_public = self.is_public;
                        let original_is_public = self.original_is_public;
                        let new_is_invite_only = self.is_invite_only;
                        let original_is_invite_only = self.original_is_invite_only;

                        Task::perform(
                            async move {
                                if new_name != original_name {
                                    engine
                                        .set_room_name(space_id_clone.as_ref(), new_name)
                                        .await
                                        .map_err(|e| e.to_string())?;
                                }
                                if new_topic != original_topic {
                                    engine
                                        .set_room_topic(space_id_clone.as_ref(), new_topic)
                                        .await
                                        .map_err(|e| e.to_string())?;
                                }
                                if new_alias != original_alias {
                                    engine
                                        .set_canonical_alias(
                                            space_id_clone.as_ref(),
                                            if new_alias.is_empty() {
                                                None
                                            } else {
                                                Some(new_alias)
                                            },
                                        )
                                        .await
                                        .map_err(|e| e.to_string())?;
                                }
                                if new_is_public != original_is_public {
                                    let visibility = if new_is_public {
                                        matrix_sdk::ruma::api::client::room::Visibility::Public
                                    } else {
                                        matrix_sdk::ruma::api::client::room::Visibility::Private
                                    };
                                    engine
                                        .set_room_visibility(space_id_clone.as_ref(), visibility)
                                        .await
                                        .map_err(|e| e.to_string())?;
                                }
                                if new_is_invite_only != original_is_invite_only {
                                    let join_rule = if new_is_invite_only {
                                        matrix_sdk::ruma::events::room::join_rules::JoinRule::Invite
                                    } else {
                                        matrix_sdk::ruma::events::room::join_rules::JoinRule::Public
                                    };
                                    engine
                                        .set_room_join_rule(space_id_clone.as_ref(), join_rule)
                                        .await
                                        .map_err(|e| e.to_string())?;
                                }
                                Ok(())
                            },
                            |res| {
                                Action::from(crate::Message::SpaceSettings(Message::SpaceSaved(
                                    res,
                                )))
                            },
                        )
                    } else {
                        Task::none()
                    }
                } else {
                    Task::none()
                }
            }
            Message::SpaceSaved(res) => {
                self.is_saving = false;
                match res {
                    Ok(_) => {
                        self.original_name = self.name.clone();
                        self.original_topic = self.topic.clone();
                        self.original_canonical_alias = self.canonical_alias.clone();
                        self.original_is_public = self.is_public;
                        self.original_is_invite_only = self.is_invite_only;
                        self.error = None;
                    }
                    Err(e) => {
                        self.error = Some(e);
                    }
                }
                Task::none()
            }
            Message::AddChild => {
                if let Some(matrix) = matrix
                    && let Some(space_id) = &self.space_id
                {
                    self.is_adding_child = true;
                    let engine = matrix.clone();
                    let space_id_clone = space_id.clone();
                    let child_id_clone = self.new_child_id.clone();
                    let order = if self.new_child_order.trim().is_empty() {
                        None
                    } else {
                        Some(self.new_child_order.clone())
                    };
                    return Task::perform(
                        async move {
                            engine
                                .add_space_child(
                                    space_id_clone.as_ref(),
                                    &child_id_clone,
                                    order,
                                    false,
                                )
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| Action::from(crate::Message::SpaceSettings(Message::ChildAdded(res))),
                    );
                }
                Task::none()
            }
            Message::ChildOrderInputChanged(child_id, order) => {
                self.pending_child_orders.insert(child_id, order);
                Task::none()
            }
            Message::SaveChildOrder(child_id) => {
                if let Some(matrix) = matrix
                    && let Some(space_id) = &self.space_id
                    && let Some(order_str) = self.pending_child_orders.get(&child_id)
                {
                    let engine = matrix.clone();
                    let space_id_clone = space_id.clone();
                    let order = if order_str.trim().is_empty() {
                        None
                    } else {
                        Some(order_str.clone())
                    };
                    return Task::perform(
                        async move {
                            engine
                                .add_space_child(space_id_clone.as_ref(), &child_id, order, false)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| {
                            Action::from(crate::Message::SpaceSettings(Message::ChildOrderSaved(
                                res,
                            )))
                        },
                    );
                }
                Task::none()
            }
            Message::ChildOrderSaved(res) => {
                match res {
                    Ok(_) => {
                        return Task::done(Action::from(crate::Message::SpaceSettings(
                            Message::LoadChildren,
                        )));
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to update child order: {}", e));
                    }
                }
                Task::none()
            }
            Message::ChildAdded(res) => {
                self.is_adding_child = false;
                match res {
                    Ok(_) => {
                        self.new_child_id = String::new();
                        self.new_child_order = String::new();
                        return Task::done(Action::from(crate::Message::SpaceSettings(
                            Message::LoadChildren,
                        )));
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to add child: {}", e));
                    }
                }
                Task::none()
            }
            Message::RemoveChild(child_id) => {
                if let Some(matrix) = matrix
                    && let Some(space_id) = &self.space_id
                {
                    let engine = matrix.clone();
                    let space_id_clone = space_id.clone();
                    let child_id_clone = child_id.clone();
                    let child_id_for_task = child_id.clone();
                    return Task::perform(
                        async move {
                            engine
                                .remove_space_child(space_id_clone.as_ref(), &child_id_for_task)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        move |res| {
                            Action::from(crate::Message::SpaceSettings(Message::ChildRemoved(
                                child_id_clone,
                                res,
                            )))
                        },
                    );
                }
                Task::none()
            }
            Message::ChildRemoved(_child_id, res) => {
                match res {
                    Ok(_) => {
                        return Task::done(Action::from(crate::Message::SpaceSettings(
                            Message::LoadChildren,
                        )));
                    }
                    Err(e) => {
                        self.error = Some(format!("Failed to remove child: {}", e));
                    }
                }
                Task::none()
            }
            Message::NewChildIdChanged(id) => {
                self.new_child_id = id;
                Task::none()
            }
            Message::NewChildOrderChanged(order) => {
                self.new_child_order = order;
                Task::none()
            }
            Message::ChildFilterChanged(filter) => {
                self.child_filter = filter;
                Task::none()
            }
            Message::DismissError => {
                self.error = None;
                Task::none()
            }
            Message::SetChildJoinRule(room_id, join_rule) => {
                if let Some(matrix) = matrix {
                    let engine = matrix.clone();
                    Task::perform(
                        async move {
                            engine
                                .set_room_join_rule(&room_id, join_rule)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| {
                            Action::from(crate::Message::SpaceSettings(match res {
                                Ok(_) => Message::LoadChildren,
                                Err(e) => Message::SpaceSaved(Err(e)),
                            }))
                        },
                    )
                } else {
                    Task::none()
                }
            }
        }
    }
}
