use crate::{
    Constellations, MenuAct, Message,
    utils::widget::{disabled_or_tooltip, tooltip_button, tooltip_button_at},
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
        text, text_input, tooltip::Position,
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

        let global_tooltip = tooltip_button_at(global_btn, ALL_ROOMS.as_str(), Position::Right);

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

            let space_name = space
                .name
                .as_deref()
                .map(str::to_string)
                .unwrap_or_else(|| crate::fl!("unknown-space"));
            let space_tooltip = tooltip_button_at(btn, space_name, Position::Right);

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

        if let Some(invite_form) = self.view_sidebar_invite_form() {
            room_list = room_list.push(invite_form);
        }

        if let Some(selected_space) = &self.selected_space {
            let space_room = self
                .room_list
                .iter()
                .find(|r| r.id.as_ref() == selected_space.as_str());

            let space_header = self.view_sidebar_space_header(space_room);
            room_list = room_list.push(container(space_header).padding([5, 5, 15, 5]));
            room_list = room_list.push(divider::horizontal::default());

            if !subspaces.is_empty() {
                room_list = room_list.push(
                    container(text::title3(SUBSPACES.as_str()).size(14)).padding([10, 5, 5, 5]),
                );
                for subspace in &subspaces {
                    let btn = self
                        .view_sidebar_room_button(subspace, false)
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
            let is_selected = self.selected_room.as_ref() == Some(&room.id);
            let btn = self
                .view_sidebar_room_button(room, is_selected)
                .on_press(Message::RoomSelected(room_id));

            room_list = room_list.push(btn.width(cosmic::iced::Fill));
        }

        let filtered_suggested_rooms: Vec<usize> = self
            .filtered_other_rooms
            .iter()
            .copied()
            .filter(|&idx| self.other_rooms[idx].suggested)
            .collect();

        let filtered_non_suggested_rooms: Vec<usize> = self
            .filtered_other_rooms
            .iter()
            .copied()
            .filter(|&idx| !self.other_rooms[idx].suggested)
            .collect();

        for item in
            self.view_sidebar_other_rooms(&filtered_suggested_rooms, &crate::fl!("suggested"))
        {
            room_list = room_list.push(item);
        }

        for item in
            self.view_sidebar_other_rooms(&filtered_non_suggested_rooms, OTHER_ROOMS.as_str())
        {
            room_list = room_list.push(item);
        }

        container(scrollable(room_list))
            .width(ROOM_SWITCHER_WIDTH)
            .padding(10)
            .into()
    }

    fn view_sidebar_room_button(
        &self,
        room: &'switcher crate::matrix::RoomData,
        is_selected: bool,
    ) -> cosmic::widget::Button<'switcher, Message> {
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

        button::custom(
            container(room_content)
                .padding(5)
                .width(cosmic::iced::Length::Fill),
        )
        .selected(is_selected)
        .class(cosmic::theme::Button::ListItem(
            self.core.system_theme().cosmic().corner_radii.radius_m,
        ))
    }

    fn view_sidebar_invite_form<'a>(&'a self) -> Option<Element<'a, Message>> {
        if self.inviting_to_space && self.selected_space.is_some() {
            let mut invite_input = text_input("@user:example.com", &self.invite_to_space_id)
                .on_input(Message::InviteToSpaceIdChanged);

            let is_empty = self.invite_to_space_id.trim().is_empty();

            let invite_btn = button::text(crate::fl!("invite"));
            if !is_empty {
                invite_input = invite_input.on_submit(|_| Message::InviteToSpace);
            }

            let invite_btn_widget: Element<'_, Message> = disabled_or_tooltip(
                invite_btn,
                !is_empty,
                Message::InviteToSpace,
                crate::fl!("enter-user-id-to-invite"),
            );

            let invite_ui = Column::new().spacing(5).push(invite_input).push(
                Row::new()
                    .spacing(5)
                    .push(invite_btn_widget)
                    .push(button::text(CANCEL.as_str()).on_press(Message::ToggleInviteToSpace)),
            );

            Some(container(invite_ui).padding(5).into())
        } else {
            None
        }
    }

    fn view_sidebar_space_header(
        &self,
        space_room: Option<&'switcher crate::matrix::RoomData>,
    ) -> Element<'switcher, Message> {
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

        Row::new()
            .align_y(Alignment::Center)
            .spacing(10)
            .width(cosmic::iced::Length::Fill)
            .push(avatar)
            .push(view_space_name_menu(&space_name))
            .into()
    }

    fn view_sidebar_other_rooms<'a>(
        &'a self,
        filtered_indices: &[usize],
        title: &str,
    ) -> Vec<Element<'a, Message>> {
        let mut items = Vec::new();
        if !filtered_indices.is_empty() {
            items.push(divider::horizontal::default().into());
            items.push(
                container(text::title3(title.to_string()).size(14))
                    .padding([10, 5, 5, 5])
                    .into(),
            );

            for &idx in filtered_indices {
                let room = &self.other_rooms[idx];
                let btn = self.view_sidebar_room_button(room, false);
                let join_btn =
                    button::text(JOIN.as_str()).on_press(Message::JoinRoom(room.id.clone()));

                items.push(
                    Row::new()
                        .align_y(Alignment::Center)
                        .push(btn.width(cosmic::iced::Length::Fill))
                        .push(container(join_btn).padding([0, 5]))
                        .into(),
                );
            }
        }
        items
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
            tooltip_button(create_btn, empty_hint)
        } else {
            create_btn.into()
        };

        let mut content = Column::new().spacing(10).push(name_input);

        if is_room {
            let video_toggler = Row::new()
                .align_y(Alignment::Center)
                .spacing(10)
                .push(text::body(crate::fl!("video-room")))
                .push(
                    cosmic::widget::toggler(self.new_room_is_video)
                        .on_toggle(Message::NewRoomIsVideoChanged),
                );
            content = content.push(video_toggler);
        }

        content.push(create_btn_widget).into()
    }

    /// In-app "Open link…" paste dialog. A single text input bound to
    /// [`Message::OpenLinkTextChanged`], submitting (Enter or the button)
    /// routes the link through the shared `open_matrix_link` path.
    pub(crate) fn view_open_link_form(&self) -> Element<'_, Message> {
        // Borrow from self for display so the returned Element borrows from
        // self (matching the other view helpers); clone only into messages.
        let value: &str = self.open_link_dialog.as_deref().unwrap_or_default();
        let is_empty = value.trim().is_empty();

        let mut link_input = text_input(crate::fl!("open-link-placeholder"), value)
            .on_input(Message::OpenLinkTextChanged);

        let mut open_btn = button::text(crate::fl!("open-link"));
        if !is_empty {
            // `on_submit` is `Fn` (may fire repeatedly), so it must clone on
            // each invocation; `on_press` fires once and can move.
            let for_submit = value.to_string();
            let for_press = value.to_string();
            link_input = link_input.on_submit(move |_| Message::SubmitOpenLink(for_submit.clone()));
            open_btn = open_btn.on_press(Message::SubmitOpenLink(for_press));
        }

        let open_btn_widget: Element<'_, Message> = if is_empty {
            tooltip_button(open_btn, crate::fl!("open-link-placeholder"))
        } else {
            open_btn.into()
        };

        Column::new()
            .spacing(10)
            .push(link_input)
            .push(
                Row::new()
                    .spacing(5)
                    .push(open_btn_widget)
                    .push(button::text(crate::fl!("cancel")).on_press(Message::ToggleOpenLink)),
            )
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
    let plus_tooltip = tooltip_button_at(plus_btn, CREATE.as_str(), Position::Right);
    let key_binds = std::collections::HashMap::new();

    let menu_tree = menu::Tree::with_children(
        RcElementWrapper::new(plus_tooltip),
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
                    crate::fl!("manage-spaces-users"),
                    Some(cosmic::widget::icon::Handle::from(Named::new(
                        "network-workgroup-symbolic",
                    ))),
                    crate::MenuAct::ManageSpaceRooms,
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
        .item_width(menu::ItemWidth::Uniform(180))
        .spacing(4.0)
}
pub(crate) fn view_room_name_menu(name: &str) -> menu::MenuBar<Message> {
    let key_binds = std::collections::HashMap::new();
    let menu_tree = menu::Tree::with_children(
        RcElementWrapper::new(Element::from(menu::root(name.to_owned()))),
        menu::items(
            &key_binds,
            vec![
                menu::Item::Button(
                    crate::fl!("room-settings"),
                    Some(cosmic::widget::icon::Handle::from(Named::new(
                        "emblem-system",
                    ))),
                    crate::MenuAct::RoomSettings,
                ),
                menu::Item::Button(
                    crate::fl!("manage-members"),
                    Some(cosmic::widget::icon::Handle::from(Named::new(
                        "avatar-default-symbolic",
                    ))),
                    crate::MenuAct::ManageRoomMembers,
                ),
                menu::Item::Button(
                    crate::fl!("invite"),
                    Some(cosmic::widget::icon::Handle::from(Named::new(
                        "contact-new-symbolic",
                    ))),
                    crate::MenuAct::RoomInvite,
                ),
            ],
        ),
    );
    menu::bar(vec![menu_tree])
        .item_height(menu::ItemHeight::Dynamic(40))
        .item_width(menu::ItemWidth::Uniform(180))
        .spacing(4.0)
}
