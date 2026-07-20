use cosmic::Element;
use cosmic::iced::Alignment;
use cosmic::widget::{Column, Row, button, settings, text, text_input, tooltip, tooltip::Position};

use super::message::Message;
use super::state::State;

impl State {
    pub fn view(&self) -> Element<'_, Message> {
        if self.is_loading {
            return settings::view_column(vec![
                text::body(crate::fl!("loading-space-data")).into(),
            ])
            .into();
        }

        let mut col = settings::view_column(vec![self.view_profile(), self.view_discovery()]);

        if let Some(error_view) = self.view_error() {
            col = col.push(error_view);
        }

        if let Some(save_btn) = self.view_save_button() {
            col = col.push(save_btn);
        }

        col.into()
    }

    pub fn view_manage(&self) -> Element<'_, Message> {
        let mut col = settings::view_column(vec![self.view_hierarchy(), self.view_add_child()]);

        if let Some(error_view) = self.view_error() {
            col = col.push(error_view);
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

                section = section.add(settings::item_row(vec![self.view_child(child, name)]));
            }
        }
        section.into()
    }

    fn view_child<'a>(
        &'a self,
        child: &'a crate::matrix::RoomData,
        name: &'a str,
    ) -> Element<'a, Message> {
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
            layout = layout.push(self.view_child_join_rules(child));
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
        layout.into()
    }

    fn view_child_join_rules<'a>(
        &'a self,
        child: &'a crate::matrix::RoomData,
    ) -> Element<'a, Message> {
        use matrix_sdk::ruma::events::room::join_rules::{AllowRule, JoinRule, Restricted};

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
            && let Ok(space_id_parsed) = matrix_sdk::ruma::RoomId::parse(space_id.as_ref())
        {
            let mut restricted =
                Restricted::new(vec![AllowRule::room_membership(space_id_parsed.to_owned())]);
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

        Row::new()
            .spacing(5)
            .push(invite_btn)
            .push(restricted_btn)
            .into()
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
