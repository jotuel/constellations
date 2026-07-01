use crate::{
    Constellations, MenuAct, Message,
    view::{
        ALL_ROOMS, AVATAR_RADIUS, CANCEL, CREATE, CREATE_ROOM, CREATE_SPACE, ENTER_ROOM_NAME,
        ENTER_SPACE_NAME, JOIN, JOINED_ROOMS, OTHER_ROOMS, ROOM_AVATAR_HEIGHT, ROOM_AVATAR_WIDTH,
        ROOM_HAS_NO_AVATAR, ROOM_NAME, ROOM_SWITCHER_WIDTH, SPACE_AVATAR_HEIGHT,
        SPACE_AVATAR_WIDTH, SPACE_NAME, SUBSPACES, UNKNOWN_ROOM, UNKNOWN_SPACE,
    },
};
use cosmic::{
    Element,
    iced::Alignment,
    widget::{
        Column, RcElementWrapper, Row, button, container, divider, icon::Named, menu, scrollable,
        text, text_input, tooltip, tooltip::Position,
    },
};

fn clean_last_message(last_msg: &str) -> &str {
    let mut actual_line = None;
    let mut in_quote = false;
    for line in last_msg.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('>') {
            in_quote = true;
            continue;
        }
        if in_quote && trimmed.is_empty() {
            continue;
        }
        actual_line = Some(line);
        break;
    }

    if let Some(line) = actual_line {
        line.trim()
    } else {
        last_msg.split('\n').next().unwrap_or("").trim()
    }
}

impl<'switcher> Constellations {
    pub fn view_space_switcher(&self) -> Element<'_, Message> {
        let mut content = Column::new().spacing(10).align_x(Alignment::Center);

        // Global icon (All Rooms)
        let is_global_selected = self.selected_space.is_none();

        let mut global_btn = button::icon(Named::new("web-browser")).selected(is_global_selected);
        if !is_global_selected {
            global_btn = global_btn.on_press(Message::SelectSpace(None));
        }

        let global_tooltip = tooltip(global_btn, text::body(ALL_ROOMS.as_str()), Position::Right);

        content = content.push(global_tooltip);

        for space in self.room_list.iter().filter(|r| r.is_space) {
            let space_id_str = space.id.clone();
            // Try to parse just for validity
            if matrix_sdk::ruma::RoomId::parse(&*space_id_str).is_err() {
                continue;
            }
            let is_selected =
                self.selected_space.as_ref().map(|s| s.as_str()) == Some(&*space_id_str);

            let has_avatar = space
                .avatar_url
                .as_ref()
                .map(|url| self.media_cache.contains_key(url))
                .unwrap_or(false);

            let avatar_element = self.view_avatar_space(space);

            let space_container = container(avatar_element)
                .padding(if has_avatar { 0 } else { 8 })
                .align_x(Alignment::Center)
                .align_y(Alignment::Center);

            let mut btn = button::custom(space_container).selected(is_selected);
            if !is_selected {
                btn = btn.on_press(Message::SelectSpace(Some(space_id_str)));
            }

            if has_avatar {
                btn = btn.padding(0);
            }

            let space_name = space.name.as_deref().unwrap_or("Unknown Space");
            let space_tooltip = tooltip(btn, text::body(space_name), Position::Right);

            content = content.push(space_tooltip);
        }

        let scrollable_spaces = scrollable(content).height(cosmic::iced::Length::Fill);

        let bottom_content = Column::new()
            .push(view_menu_create())
            .spacing(10)
            .align_x(Alignment::Center);

        let layout = Column::new()
            .push(scrollable_spaces)
            .push(bottom_content)
            .align_x(Alignment::Center);

        container(layout).width(60).padding(5).into()
    }

    fn view_avatar_space(&self, space: &crate::matrix::RoomData) -> Element<'switcher, Message> {
        let default_avatar = container(
            text::body(
                space
                    .name
                    .as_deref()
                    .unwrap_or("S")
                    .chars()
                    .next()
                    .unwrap_or('S')
                    .to_string(),
            )
            .size(ROOM_AVATAR_HEIGHT),
        )
        .width(SPACE_AVATAR_WIDTH)
        .height(SPACE_AVATAR_HEIGHT)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);

        let avatar_element: Element<'switcher, Message> = if let Some(url) = &space.avatar_url {
            if let Some(handle) = self.media_cache.get(url) {
                cosmic::widget::image(handle.clone())
                    .width(SPACE_AVATAR_WIDTH)
                    .height(SPACE_AVATAR_WIDTH)
                    .into()
            } else {
                default_avatar.into()
            }
        } else {
            default_avatar.into()
        };
        avatar_element
    }

    pub fn view_sidebar(&self) -> Element<'_, Message> {
        let mut room_list = Column::new().spacing(5);

        let mut subspaces = Vec::new();
        let mut subspace_ids = std::collections::HashSet::new();

        if let Some(selected_space) = &self.selected_space
            && let Some(matrix) = &self.matrix
            && let Ok(selected_space_id) = matrix_sdk::ruma::RoomId::parse(selected_space.as_str())
        {
            for room in &self.room_list {
                if room.is_space
                    && room.id.as_ref() != selected_space.as_str()
                    && let Ok(room_id) = matrix_sdk::ruma::RoomId::parse(room.id.as_ref())
                    && matrix.is_in_space_sync(&room_id, &selected_space_id)
                {
                    subspaces.push(room);
                    subspace_ids.insert(room.id.as_ref());
                }
            }
        }

        if self.inviting_to_space && self.selected_space.is_some() {
            let mut invite_input = text_input("@user:example.com", &self.invite_to_space_id)
                .on_input(Message::InviteToSpaceIdChanged);

            let is_empty = self.invite_to_space_id.trim().is_empty();

            let mut invite_btn = button::text(crate::fl!("invite"));
            if !is_empty {
                invite_input = invite_input.on_submit(|_| Message::InviteToSpace);
                invite_btn = invite_btn.on_press(Message::InviteToSpace);
            }

            let invite_btn_widget: Element<'_, Message> = if is_empty {
                tooltip(
                    invite_btn,
                    text::body(crate::fl!("enter-user-id-to-invite")),
                    Position::Top,
                )
                .into()
            } else {
                invite_btn.into()
            };

            let invite_ui = Column::new().spacing(5).push(invite_input).push(
                Row::new()
                    .spacing(5)
                    .push(invite_btn_widget)
                    .push(button::text(CANCEL.as_str()).on_press(Message::ToggleInviteToSpace)),
            );

            room_list = room_list.push(container(invite_ui).padding(5));
        }

        if let Some(selected_space) = &self.selected_space {
            let space_room = self
                .room_list
                .iter()
                .find(|r| r.id.as_ref() == selected_space.as_str());

            let space_name = space_room
                .and_then(|r| r.name.as_deref())
                .map(std::borrow::Cow::Borrowed)
                .unwrap_or_else(|| std::borrow::Cow::Borrowed(UNKNOWN_SPACE.as_str()));

            let avatar = if let Some(space) = space_room {
                let default_avatar = container(
                    text::body(
                        space
                            .name
                            .as_deref()
                            .unwrap_or("S")
                            .chars()
                            .next()
                            .unwrap_or('S')
                            .to_string(),
                    )
                    .size(14),
                )
                .width(ROOM_AVATAR_WIDTH)
                .height(ROOM_AVATAR_HEIGHT)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center);

                if let Some(url) = &space.avatar_url {
                    if let Some(handle) = self.media_cache.get(url) {
                        Element::from(
                            cosmic::widget::image(handle.clone())
                                .width(ROOM_AVATAR_WIDTH)
                                .height(ROOM_AVATAR_HEIGHT)
                                .border_radius(AVATAR_RADIUS),
                        )
                    } else {
                        Element::from(default_avatar)
                    }
                } else {
                    Element::from(default_avatar)
                }
            } else {
                Element::from(
                    container(text::body("S").size(14))
                        .width(ROOM_AVATAR_WIDTH)
                        .height(ROOM_AVATAR_HEIGHT)
                        .align_x(Alignment::Center)
                        .align_y(Alignment::Center),
                )
            };

            let space_header = Row::new()
                .align_y(Alignment::Center)
                .spacing(10)
                .width(cosmic::iced::Length::Fill)
                .push(avatar)
                .push(view_space_name_menu(&space_name));
            room_list = room_list.push(container(space_header).padding([5, 5, 15, 5]));
            room_list = room_list.push(divider::horizontal::default());
            if !subspaces.is_empty() {
                room_list = room_list.push(
                    container(text::title3(SUBSPACES.as_str()).size(14)).padding([10, 5, 5, 5]),
                );
                for subspace in &subspaces {
                    let mut room_content = Column::new().spacing(2);
                    let mut header = self.view_avatar_room(subspace);
                    if let Some(unread_str) = &subspace.unread_count_str {
                        header = header.push(text::body(unread_str.as_str()).size(12));
                    }
                    room_content = room_content.push(header);

                    if let Some(last_msg) = &subspace.last_message {
                        let first_line = clean_last_message(last_msg);
                        room_content = room_content.push(
                            text::body(first_line)
                                .size(12)
                                .width(cosmic::iced::Length::Fill),
                        );
                    }

                    let btn = button::custom(
                        container(room_content)
                            .padding(5)
                            .width(cosmic::iced::Length::Fill),
                    )
                    .selected(false)
                    .class(cosmic::theme::Button::ListItem(
                        self.core.system_theme().cosmic().corner_radii.radius_m,
                    ))
                    .on_press(Message::SelectSpace(Some(subspace.id.clone())));

                    room_list = room_list.push(btn.width(cosmic::iced::Fill));
                }
            }

            if !self.other_rooms.is_empty() {
                room_list = room_list.push(
                    container(text::title3(JOINED_ROOMS.as_str()).size(14)).padding([10, 5, 5, 5]),
                );
            }
        }

        for &room_idx in &self.filtered_room_list {
            let room = &self.room_list[room_idx];
            if subspace_ids.contains(room.id.as_ref()) {
                continue;
            }
            let room_id = room.id.clone();
            let mut room_content = Column::new().spacing(2);

            let mut header = self.view_avatar_room(room);

            if let Some(unread_str) = &room.unread_count_str {
                header = header.push(text::body(unread_str.as_str()).size(12));
            }

            room_content = room_content.push(header);

            if let Some(last_msg) = &room.last_message {
                let first_line = clean_last_message(last_msg);
                room_content = room_content.push(
                    text::body(first_line)
                        .size(12)
                        .width(cosmic::iced::Length::Fill),
                );
            }

            let is_selected = self.selected_room.as_ref() == Some(&room.id);
            let btn = button::custom(
                container(room_content)
                    .padding(5)
                    .width(cosmic::iced::Length::Fill),
            )
            .selected(is_selected)
            .class(cosmic::theme::Button::ListItem(
                self.core.system_theme().cosmic().corner_radii.radius_m,
            ))
            .on_press(Message::RoomSelected(room_id));

            room_list = room_list.push(btn.width(cosmic::iced::Fill));
        }

        if !self.filtered_other_rooms.is_empty() {
            room_list = room_list.push(divider::horizontal::default());
            room_list = room_list.push(
                container(text::title3(OTHER_ROOMS.as_str()).size(14)).padding([10, 5, 5, 5]),
            );

            for &idx in &self.filtered_other_rooms {
                let room = &self.other_rooms[idx];
                let mut room_content = Column::new().spacing(2);
                let mut header = self.view_avatar_room(room);

                if let Some(unread_str) = &room.unread_count_str {
                    header = header.push(text::body(unread_str.as_str()).size(12));
                }

                room_content = room_content.push(header);

                if let Some(last_msg) = &room.last_message {
                    let first_line = clean_last_message(last_msg);
                    room_content = room_content.push(
                        text::body(first_line)
                            .size(12)
                            .width(cosmic::iced::Length::Fill),
                    );
                }

                let btn = button::custom(
                    container(room_content)
                        .padding(5)
                        .width(cosmic::iced::Length::Fill),
                )
                .selected(false)
                .class(cosmic::theme::Button::ListItem(
                    self.core.system_theme().cosmic().corner_radii.radius_m,
                ))
                .width(cosmic::iced::Length::Fill);

                let join_btn =
                    button::text(JOIN.as_str()).on_press(Message::JoinRoom(room.id.clone()));

                room_list = room_list.push(
                    Row::new()
                        .align_y(Alignment::Center)
                        .push(btn)
                        .push(container(join_btn).padding([0, 5])),
                );
            }
        }

        container(scrollable(room_list))
            .width(ROOM_SWITCHER_WIDTH)
            .padding(10)
            .into()
    }

    pub(crate) fn view_create_form(&self) -> Element<'_, Message> {
        let is_room = self.creating_room;
        let (label, empty_hint) = if is_room {
            (ROOM_NAME.as_str(), ENTER_ROOM_NAME.as_str())
        } else {
            (SPACE_NAME.as_str(), ENTER_SPACE_NAME.as_str())
        };

        let mut name_input =
            text_input(label, &self.new_room_name).on_input(Message::NewRoomNameChanged);

        let is_empty = self.new_room_name.trim().is_empty();

        let mut create_btn = button::text(CREATE.as_str());
        if !is_empty {
            if is_room {
                name_input =
                    name_input.on_submit(|_| Message::CreateRoom(self.new_room_name.clone()));
                create_btn = create_btn.on_press(Message::CreateRoom(self.new_room_name.clone()));
            } else {
                name_input =
                    name_input.on_submit(|_| Message::CreateSpace(self.new_room_name.clone()));
                create_btn = create_btn.on_press(Message::CreateSpace(self.new_room_name.clone()));
            }
        }

        let create_btn_widget: Element<'_, Message> = if is_empty {
            tooltip(create_btn, text::body(empty_hint), Position::Top).into()
        } else {
            create_btn.into()
        };

        Column::new()
            .spacing(10)
            .push(name_input)
            .push(create_btn_widget)
            .into()
    }

    fn view_avatar_room(
        &self,
        room: &'switcher crate::matrix::RoomData,
    ) -> Row<'switcher, Message, cosmic::prelude::Theme> {
        let name_str = room.name.as_deref();
        let name = text::body(
            name_str
                .map(std::borrow::Cow::Borrowed)
                .unwrap_or_else(|| std::borrow::Cow::Borrowed(UNKNOWN_ROOM.as_str())),
        )
        .width(cosmic::iced::Length::Fill);
        let mut header = Row::new()
            .spacing(10)
            .align_y(Alignment::Center)
            .width(cosmic::iced::Length::Fill);

        let view_default_avatar = || {
            container(text::body(ROOM_HAS_NO_AVATAR.as_str()))
                .width(ROOM_AVATAR_WIDTH)
                .height(ROOM_AVATAR_HEIGHT)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
        };

        if let Some(avatar_url) = &room.avatar_url {
            if let Some(handle) = self.media_cache.get(avatar_url) {
                header = header.push(
                    cosmic::widget::image(handle.clone())
                        .width(ROOM_AVATAR_WIDTH)
                        .height(ROOM_AVATAR_HEIGHT)
                        .border_radius(AVATAR_RADIUS),
                );
            } else {
                header = header.push(view_default_avatar());
            }
        } else {
            header = header.push(view_default_avatar());
        }
        header.push(name)
    }
}

fn view_menu_create() -> menu::MenuBar<Message> {
    let plus_btn = button::icon(Named::new("list-add-symbolic"));
    let plus_tooltip = tooltip(plus_btn, text::body(CREATE.as_str()), Position::Right);
    let key_binds = std::collections::HashMap::new();

    let menu_tree = menu::Tree::with_children(
        RcElementWrapper::new(Element::from(plus_tooltip)),
        menu::items(
            &key_binds,
            vec![
                menu::Item::Button(
                    CREATE_ROOM.as_str().to_string(),
                    Some(cosmic::widget::icon::Handle::from(Named::new(
                        "chat-symbolic",
                    ))),
                    MenuAct::CreateRoom,
                ),
                menu::Item::Button(
                    CREATE_SPACE.as_str().to_string(),
                    Some(cosmic::widget::icon::Handle::from(Named::new(
                        "network-workgroup-symbolic",
                    ))),
                    MenuAct::CreateSpace,
                ),
            ],
        ),
    );

    menu::bar(vec![menu_tree])
        .item_height(menu::ItemHeight::Dynamic(40))
        .item_width(menu::ItemWidth::Uniform(160))
        .spacing(4.0)
}

fn view_space_name_menu(name: &str) -> menu::MenuBar<Message> {
    let key_binds = std::collections::HashMap::new();
    let menu_tree = menu::Tree::with_children(
        RcElementWrapper::new(Element::from(menu::root(name.to_string()))),
        menu::items(
            &key_binds,
            vec![
                menu::Item::Button(
                    crate::fl!("space-settings"),
                    Some(cosmic::widget::icon::Handle::from(Named::new(
                        "emblem-system",
                    ))),
                    crate::MenuAct::SpaceSettings,
                ),
                menu::Item::Button(
                    crate::fl!("invite"),
                    Some(cosmic::widget::icon::Handle::from(Named::new(
                        "contact-new-symbolic",
                    ))),
                    crate::MenuAct::SpaceInvite,
                ),
            ],
        ),
    );
    menu::bar(vec![menu_tree])
        .item_height(menu::ItemHeight::Dynamic(40))
        .item_width(menu::ItemWidth::Uniform(160))
        .spacing(4.0)
}

/// A clickable title that opens settings on press.
pub(crate) fn view_settings_name_button(
    name: &str,
    panel: crate::SettingsPanel,
) -> Element<'static, Message> {
    button::custom(text::title3(name.to_string()))
        .class(cosmic::theme::Button::MenuRoot)
        .padding(0)
        .on_press(Message::OpenSettings(panel))
        .into()
}
