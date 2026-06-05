use crate::matrix::{MatrixEngine, RoomData};
use cosmic::iced::Alignment;
use cosmic::widget::{Column, Row, button, settings, text, text_input, tooltip, tooltip::Position};
use cosmic::{Action, Element, Task};
use matrix_sdk::ruma::RoomId;
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

#[derive(Debug, Clone)]
pub struct SpaceInfo {
    pub name: String,
    pub topic: String,
    pub canonical_alias: Option<String>,
    pub avatar_url: Option<String>,
    pub visibility: matrix_sdk::ruma::api::client::room::Visibility,
    pub join_rule: matrix_sdk::ruma::events::room::join_rules::JoinRule,
}

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

    pub fn view(&self) -> Element<'_, Message> {
        if self.is_loading {
            return settings::view_column(vec![
                text::body(crate::fl!("loading-space-data")).into(),
            ])
            .into();
        }

        let mut col = settings::view_column(vec![
            self.view_profile(),
            self.view_discovery(),
            self.view_hierarchy(),
            self.view_add_child(),
        ]);

        if let Some(error_view) = self.view_error() {
            col = col.push(error_view);
        }

        if let Some(save_btn) = self.view_save_button() {
            col = col.push(save_btn);
        }

        col.into()
    }

    fn view_error(&self) -> Option<Element<'_, Message>> {
        self.error.as_ref().map(|error| {
            settings::section()
                .add(settings::item(
                    error,
                    button::text(crate::fl!("dismiss")).on_press(Message::DismissError),
                ))
                .into()
        })
    }

    fn view_profile(&self) -> Element<'_, Message> {
        let mut section = settings::section().title(crate::fl!("space-profile"));

        // Avatar Section
        let mut avatar_row = Row::new().spacing(20).align_y(Alignment::Center);
        if let Some(handle) = &self.avatar_handle {
            avatar_row = avatar_row.push(
                cosmic::widget::image(handle.clone())
                    .width(cosmic::iced::Length::Fixed(64.0))
                    .height(cosmic::iced::Length::Fixed(64.0)),
            );
        } else if self.is_loading_avatar {
            avatar_row = avatar_row.push(text::body(crate::fl!("loading")));
        } else {
            avatar_row = avatar_row.push(
                cosmic::widget::container(text::body(crate::fl!("no-avatar")))
                    .width(cosmic::iced::Length::Fixed(64.0))
                    .height(cosmic::iced::Length::Fixed(64.0))
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center),
            );
        }

        let mut upload_btn = button::text(if self.is_uploading_avatar {
            crate::fl!("uploading")
        } else {
            crate::fl!("change-avatar")
        });
        if !self.is_uploading_avatar {
            upload_btn = upload_btn.on_press(Message::SelectAvatar);
        }
        avatar_row = avatar_row.push(upload_btn);
        section = section.add(avatar_row);

        section = section
            .add(settings::item(
                crate::fl!("space-name-label"),
                text_input::text_input(crate::fl!("space-name-label"), &self.name)
                    .on_input(Message::NameChanged),
            ))
            .add(settings::item(
                crate::fl!("space-topic-label"),
                text_input::text_input(crate::fl!("space-topic-label"), &self.topic)
                    .on_input(Message::TopicChanged),
            ))
            .add(settings::item(
                crate::fl!("canonical-alias-label"),
                text_input::text_input("#space_name:server.com", &self.canonical_alias)
                    .on_input(Message::CanonicalAliasChanged),
            ));

        section.into()
    }

    fn view_discovery(&self) -> Element<'_, Message> {
        settings::section()
            .title(crate::fl!("discovery-access"))
            .add(settings::item(
                crate::fl!("public-discoverable"),
                cosmic::widget::toggler(self.is_public).on_toggle(Message::IsPublicChanged),
            ))
            .add(settings::item(
                crate::fl!("invite-only"),
                cosmic::widget::toggler(self.is_invite_only)
                    .on_toggle(Message::IsInviteOnlyChanged),
            ))
            .into()
    }

    fn view_hierarchy(&self) -> Element<'_, Message> {
        let mut section = settings::section().title(crate::fl!("space-hierarchy"));

        section = section.add(
            text_input::text_input(crate::fl!("filter-rooms-subspaces"), &self.child_filter)
                .on_input(Message::ChildFilterChanged),
        );

        if self.is_loading_children {
            section = section.add(text::body(crate::fl!("loading-children")));
        } else {
            let filter_is_ascii = self.child_filter.is_ascii();
            let filter_lower_fallback =
                (!filter_is_ascii).then(|| self.child_filter.to_lowercase());

            for child in &self.children {
                let name = child.name.as_deref().unwrap_or(&child.id);

                if !self.child_filter.is_empty() {
                    let matches = crate::contains_ignore_ascii_case(
                        name,
                        &self.child_filter,
                        filter_lower_fallback.as_deref(),
                    ) || crate::contains_ignore_ascii_case(
                        child.id.as_ref(),
                        &self.child_filter,
                        filter_lower_fallback.as_deref(),
                    );

                    if !matches {
                        continue;
                    }
                }

                let current_order = child.order.as_deref().unwrap_or_default();
                let order_to_show = self
                    .pending_child_orders
                    .get(child.id.as_ref())
                    .map(|s| s.as_str())
                    .unwrap_or(current_order);

                let header_row = Row::new()
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .push(
                        Column::new()
                            .push(text::body(name.to_string()))
                            .push(text::body(child.id.to_string()).size(10)),
                    )
                    .push(cosmic::widget::space().width(cosmic::iced::Length::Fill))
                    .push(
                        button::destructive(crate::fl!("remove"))
                            .on_press(Message::RemoveChild(child.id.to_string())),
                    );

                let mut layout = Column::new().spacing(8).push(header_row);

                if !child.is_space {
                    use matrix_sdk::ruma::events::room::join_rules::{
                        AllowRule, JoinRule, Restricted,
                    };

                    let is_restricted = if let Some(JoinRule::Restricted(r)) = &child.join_rule {
                        r.allow.iter().any(|a| {
                            if let AllowRule::RoomMembership(ra) = a {
                                self.space_id.as_deref() == Some(ra.room_id.as_str())
                            } else {
                                false
                            }
                        })
                    } else {
                        false
                    };

                    let mut invite_btn = if !is_restricted {
                        button::suggested(crate::fl!("invite-only-btn"))
                    } else {
                        button::text(crate::fl!("invite-only-btn"))
                    };

                    if is_restricted {
                        invite_btn = invite_btn.on_press(Message::SetChildJoinRule(
                            child.id.to_string(),
                            JoinRule::Invite,
                        ));
                    }

                    let mut restricted_btn = if is_restricted {
                        button::suggested(crate::fl!("restricted-access"))
                    } else {
                        button::text(crate::fl!("restricted-access"))
                    };

                    if !is_restricted
                        && let Some(space_id) = &self.space_id
                        && let Ok(space_id_parsed) =
                            matrix_sdk::ruma::RoomId::parse(space_id.as_ref())
                    {
                        let mut restricted = Restricted::new(vec![AllowRule::room_membership(
                            space_id_parsed.to_owned(),
                        )]);
                        // Keep other existing allowed spaces if any
                        if let Some(JoinRule::Restricted(r)) = &child.join_rule {
                            for allow in &r.allow {
                                if let AllowRule::RoomMembership(ra) = allow
                                    && ra.room_id != space_id_parsed
                                {
                                    restricted.allow.push(allow.clone());
                                }
                            }
                        }
                        restricted_btn = restricted_btn.on_press(Message::SetChildJoinRule(
                            child.id.to_string(),
                            JoinRule::Restricted(restricted),
                        ));
                    }

                    let join_rule_row = Row::new().spacing(5).push(invite_btn).push(restricted_btn);
                    layout = layout.push(join_rule_row);
                }

                let child_id_for_suggested = child.id.to_string();
                let suggested_widget = Row::new()
                    .spacing(5)
                    .align_y(Alignment::Center)
                    .push(text::body(crate::fl!("suggested")).size(12))
                    .push(
                        cosmic::widget::toggler(child.suggested).on_toggle(move |suggested| {
                            Message::ToggleChildSuggested(child_id_for_suggested.clone(), suggested)
                        }),
                    );

                let child_id_clone = child.id.to_string();
                let mut order_row = Row::new().spacing(5).align_y(Alignment::Center).push(
                    text_input::text_input(crate::fl!("order"), order_to_show)
                        .on_input(move |new_order| {
                            Message::ChildOrderInputChanged(child_id_clone.clone(), new_order)
                        })
                        .width(80),
                );

                if order_to_show != current_order {
                    order_row = order_row.push(
                        button::text(crate::fl!("apply"))
                            .on_press(Message::SaveChildOrder(child.id.to_string())),
                    );
                }

                let controls_row = Row::new()
                    .spacing(15)
                    .align_y(Alignment::Center)
                    .push(suggested_widget)
                    .push(order_row);

                layout = layout.push(controls_row);

                section = section.add(settings::item_row(vec![layout.into()]));
            }
        }
        section.into()
    }

    fn view_add_child(&self) -> Element<'_, Message> {
        let mut section = settings::section()
            .title(crate::fl!("add-child"))
            .header(text::body(crate::fl!("add-child-by-id")).size(12));

        let mut add_btn = button::text(crate::fl!("add-child"));
        let is_empty = self.new_child_id.trim().is_empty();
        if !is_empty {
            add_btn = add_btn.on_press(Message::AddChild);
        }
        let btn_widget: Element<'_, Message> = if is_empty {
            tooltip(
                add_btn,
                text::body(crate::fl!("enter-id-to-add")),
                Position::Top,
            )
            .into()
        } else {
            add_btn.into()
        };

        let add_child_col = Column::new()
            .spacing(10)
            .push(
                text_input::text_input("!room_id:server.com", &self.new_child_id)
                    .on_input(Message::NewChildIdChanged),
            )
            .push(
                Row::new()
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .push(
                        text_input::text_input(crate::fl!("order-optional"), &self.new_child_order)
                            .on_input(Message::NewChildOrderChanged),
                    )
                    .push(btn_widget),
            );

        section = section.add(settings::item_row(vec![add_child_col.into()]));

        section.into()
    }

    fn view_save_button(&self) -> Option<Element<'_, Message>> {
        let mut save_btn = button::text(if self.is_saving {
            crate::fl!("saving")
        } else {
            crate::fl!("save-changes")
        });

        let has_changes = self.name != self.original_name
            || self.topic != self.original_topic
            || self.canonical_alias != self.original_canonical_alias
            || self.is_public != self.original_is_public
            || self.is_invite_only != self.original_is_invite_only;

        if has_changes && !self.is_saving {
            save_btn = save_btn.on_press(Message::SaveSpace);
        }

        if !self.is_saving && !has_changes {
            Some(
                tooltip(
                    save_btn,
                    text::body(crate::fl!("make-changes-to-save")),
                    Position::Top,
                )
                .into(),
            )
        } else {
            Some(save_btn.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_changed() {
        let mut state = State::default();
        let _ = state.update(Message::NameChanged("New Space Name".to_string()), &None);
        assert_eq!(state.name, "New Space Name");
    }

    #[test]
    fn test_topic_changed() {
        let mut state = State::default();
        let _ = state.update(Message::TopicChanged("New Topic".to_string()), &None);
        assert_eq!(state.topic, "New Topic");
    }

    #[test]
    fn test_canonical_alias_changed() {
        let mut state = State::default();
        let _ = state.update(
            Message::CanonicalAliasChanged("#new_alias:example.com".to_string()),
            &None,
        );
        assert_eq!(state.canonical_alias, "#new_alias:example.com");
    }

    #[test]
    fn test_dismiss_error() {
        let mut state = State::default();
        state.error = Some("An error occurred".to_string());
        let _ = state.update(Message::DismissError, &None);
        assert_eq!(state.error, None);
    }

    #[test]
    fn test_child_filter_changed() {
        let mut state = State::default();
        let _ = state.update(Message::ChildFilterChanged("test".to_string()), &None);
        assert_eq!(state.child_filter, "test");
    }
}
