use crate::{
    Constellations, MenuAct, Message,
    view::{
        AVATAR_RADIUS, ROOM_AVATAR_HEIGHT, ROOM_AVATAR_WIDTH, SPACE_AVATAR_HEIGHT,
        SPACE_AVATAR_WIDTH,
    },
};
use cosmic::{
    Element,
    iced::Alignment,
    widget::{
        Column, RcElementWrapper, Row, button, container, icon::Named, menu, scrollable, text,
        text_input, tooltip, tooltip::Position,
    },
};

impl<'switcher> Constellations {
    pub fn view_space_switcher(&self) -> Element<'_, Message> {
        let mut content = Column::new().spacing(10).align_x(Alignment::Center);

        // Global icon (All Rooms)
        let is_global_selected = self.selected_space.is_none();

        let global_btn = if is_global_selected {
            button::icon(Named::new("web-browser"))
        } else {
            button::icon(Named::new("web-browser")).on_press(Message::SelectSpace(None))
        };

        let global_tooltip = tooltip(
            global_btn,
            text::body(crate::fl!("all-rooms")),
            Position::Right,
        );

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

            let mut btn = if is_selected {
                button::custom(space_container)
            } else {
                button::custom(space_container).on_press(Message::SelectSpace(Some(space_id_str)))
            };

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
                    .width(32)
                    .height(32)
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

        if self.creating_room || self.creating_space {
            let label = if self.creating_room {
                crate::fl!("room-name")
            } else {
                crate::fl!("space-name")
            };

            let mut name_input =
                text_input(label, &self.new_room_name).on_input(Message::NewRoomNameChanged);

            let is_empty = self.new_room_name.trim().is_empty();

            let mut create_btn = button::text(crate::fl!("create"));
            if !is_empty {
                if self.creating_room {
                    name_input =
                        name_input.on_submit(|_| Message::CreateRoom(self.new_room_name.clone()));
                    create_btn =
                        create_btn.on_press(Message::CreateRoom(self.new_room_name.clone()));
                } else {
                    name_input =
                        name_input.on_submit(|_| Message::CreateSpace(self.new_room_name.clone()));
                    create_btn =
                        create_btn.on_press(Message::CreateSpace(self.new_room_name.clone()));
                }
            }

            let create_btn_widget: Element<'_, Message> = if is_empty {
                tooltip(
                    create_btn,
                    text::body(if self.creating_room {
                        crate::fl!("enter-room-name")
                    } else {
                        crate::fl!("enter-space-name")
                    }),
                    Position::Top,
                )
                .into()
            } else {
                create_btn.into()
            };

            let cancel_msg = if self.creating_room {
                Message::ToggleCreateRoom
            } else {
                Message::ToggleCreateSpace
            };

            let create_ui = Column::new().spacing(5).push(name_input).push(
                Row::new()
                    .spacing(5)
                    .push(create_btn_widget)
                    .push(button::text(crate::fl!("cancel")).on_press(cancel_msg)),
            );

            room_list = room_list.push(container(create_ui).padding(5));
        }

        if let Some(selected_space) = &self.selected_space {
            let space_name = self
                .room_list
                .iter()
                .find(|r| r.id.as_ref() == selected_space.as_str())
                .and_then(|r| r.name.as_deref())
                .map(std::borrow::Cow::Borrowed)
                .unwrap_or_else(|| std::borrow::Cow::Owned(crate::fl!("unknown-space")));

            let space_header = Row::new()
                .align_y(Alignment::Center)
                .push(text::title3(space_name))
                .push(cosmic::widget::space().width(cosmic::iced::Length::Fill))
                .push(
                    button::icon(Named::new("emblem-system"))
                        .tooltip(crate::fl!("space-settings"))
                        .on_press(Message::OpenSettings(crate::SettingsPanel::Space)),
                );
            room_list = room_list.push(container(space_header).padding(5));

            if !self.other_rooms.is_empty() {
                room_list = room_list.push(
                    container(text::title3(crate::fl!("joined-rooms")).size(14))
                        .padding([10, 5, 5, 5]),
                );
            }
        }

        for &room_idx in &self.filtered_room_list {
            let room = &self.room_list[room_idx];
            let room_id = room.id.clone();
            let mut room_content = Column::new().spacing(2);

            let mut header = self.view_avatar_room(room);

            if let Some(unread_str) = &room.unread_count_str {
                header = header.push(text::body(unread_str.as_str()).size(12));
            }

            room_content = room_content.push(header);

            if let Some(last_msg) = &room.last_message {
                let first_line = last_msg.split('\n').next().unwrap_or("");
                room_content = room_content.push(text::body(first_line).size(12));
            }

            let btn = button::custom(
                container(room_content)
                    .padding(5)
                    .width(cosmic::iced::Length::Fill),
            )
            .on_press(Message::RoomSelected(room_id));

            room_list = room_list.push(btn.width(cosmic::iced::Fill));
        }

        if !self.filtered_other_rooms.is_empty() {
            room_list = room_list.push(
                container(text::title3(crate::fl!("other-rooms")).size(14)).padding([10, 5, 5, 5]),
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
                    let first_line = last_msg.split('\n').next().unwrap_or("");
                    room_content = room_content.push(text::body(first_line).size(12));
                }

                let btn = button::custom(
                    container(room_content)
                        .padding(5)
                        .width(cosmic::iced::Length::Fill),
                );

                let join_btn =
                    button::text(crate::fl!("join")).on_press(Message::JoinRoom(room.id.clone()));

                room_list = room_list.push(
                    Row::new()
                        .align_y(Alignment::Center)
                        .push(btn)
                        .push(container(join_btn).padding([0, 5])),
                );
            }
        }

        container(scrollable(room_list))
            .width(250)
            .padding(10)
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
                .unwrap_or_else(|| std::borrow::Cow::Owned(crate::fl!("unknown-room"))),
        );
        let mut header = Row::new().spacing(10).align_y(Alignment::Center);

        let view_default_avatar = || {
            container(text::body(crate::fl!("room-has-no-avatar")))
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
    let plus_tooltip = tooltip(plus_btn, text::body(crate::fl!("create")), Position::Right);
    let key_binds = std::collections::HashMap::new();

    let menu_tree = menu::Tree::with_children(
        RcElementWrapper::new(Element::from(plus_tooltip)),
        menu::items(
            &key_binds,
            vec![
                menu::Item::Button(
                    crate::fl!("create-room"),
                    Some(cosmic::widget::icon::Handle::from(Named::new(
                        "chat-symbolic",
                    ))),
                    MenuAct::CreateRoom,
                ),
                menu::Item::Button(
                    crate::fl!("create-space"),
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
