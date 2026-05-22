use chrono::{DateTime, DurationRound, TimeDelta};
use cosmic::{
    Element, Theme,
    iced::{
        Alignment,
        widget::{scrollable, tooltip},
    },
    widget::{
        Column, Row, button, container, divider, icon::Named, text, text_input, tooltip::Position,
    },
};
use matrix_sdk::ruma::events::room::{MediaSource, message::MessageType};

use crate::{Constellations, Message, PreviewEvent, matrix};

impl<'chat> Constellations {
    pub fn view_timeline(&self) -> Element<'_, Message> {
        let mut timeline = Column::new().spacing(10).width(cosmic::iced::Length::Fill);

        let is_filtering = self.is_search_active
            && !self.search_query.is_empty()
            && self.current_settings_panel.is_none();

        let filter_is_ascii = self.search_query.is_ascii();
        let filter_lower_fallback =
            (is_filtering && !filter_is_ascii).then(|| self.search_query.to_lowercase());

        for item in &self.timeline_items {
            if let Some(event) = item.item.as_event() {
                // View-side thread filtering
                if self.app_settings.hide_threaded_messages
                    && event.content().thread_root().is_some()
                {
                    continue;
                }

                if is_filtering {
                    let body = event
                        .content()
                        .as_message()
                        .map(|m| m.body())
                        .unwrap_or_default();
                    if !crate::contains_ignore_ascii_case(
                        body,
                        &self.search_query,
                        filter_lower_fallback.as_deref(),
                    ) {
                        continue;
                    }
                }
                timeline = timeline.push(self.view_item(item));
            } else if let Some(matrix::VirtualTimelineItem::DateDivider(date)) =
                item.item.as_virtual()
            {
                timeline = timeline.push(
                    Row::new()
                        .push(divider::horizontal::default())
                        .push(text::body(
                            DateTime::from_timestamp_secs(date.as_secs().into())
                                .unwrap_or_default()
                                .duration_trunc(TimeDelta::try_days(1).unwrap_or_default())
                                .unwrap_or_default()
                                .to_rfc2822()
                                .trim_end_matches(" 00:00:00 +0000")
                                .to_owned(),
                        ))
                        .push(divider::horizontal::default())
                        .align_y(Alignment::Center),
                )
            }
        }

        scrollable(timeline)
            .id(crate::TIMELINE_ID.clone())
            .height(cosmic::iced::Length::Fill)
            .on_scroll(|viewport| Message::TimelineScrolled(viewport, false))
            .into()
    }

    fn view_reactions<'a>(
        &'a self,
        event: &'a matrix_sdk_ui::timeline::EventTimelineItem,
    ) -> Row<'a, Message, cosmic::Theme> {
        let mut reaction_row = Row::new().spacing(5).align_y(Alignment::Center);
        let reactions = event.content().reactions();
        let item_id = event.identifier();

        if let Some(reaction) = reactions {
            for key in reaction.keys() {
                let people = reaction.get_key_value(key);
                let count = people.iter().count();

                let is_me_reacted = people.iter().any(|(user_id, _)| {
                    if let Some(me) = &self.user_id {
                        user_id.as_str() == me
                    } else {
                        false
                    }
                });

                let btn_content =
                    container(text::body(format!("{} {}", key, count)).size(10)).padding([2, 4]);

                // We can differentiate style if reacted, but for now we just wrap in button.
                let btn = button::custom(btn_content)
                    .on_press(Message::ToggleReaction(item_id.clone(), key.clone()));

                // If `is_me_reacted` is true, we could style it differently
                if is_me_reacted {
                    // Use standard button to give it some background highlight, or specific style
                    // But custom button with standard background works nicely if we could pass theme
                }

                reaction_row = reaction_row.push(btn);
            }
        }

        reaction_row
    }

    fn view_emoji_picker<'a>(
        &'a self,
        item_id: Option<matrix::TimelineEventItemId>,
    ) -> Element<'a, Message> {
        let search_input = text_input(crate::fl!("search-emojis"), &self.emoji_search_query)
            .on_input(Message::EmojiSearchQueryChanged)
            .width(cosmic::iced::Length::Fill);

        let close_btn = button::icon(cosmic::widget::icon::from_name("window-close-symbolic"))
            .on_press(if item_id.is_some() {
                Message::OpenReactionPicker(None)
            } else {
                Message::ToggleComposerEmojiPicker
            });

        let top_row = Row::new()
            .spacing(5)
            .align_y(Alignment::Center)
            .push(search_input)
            .push(close_btn);

        let mut picker_col = Column::new().spacing(8);
        picker_col = picker_col.push(top_row);

        if self.emoji_search_query.is_empty() {
            let categories = [
                (emojis::Group::SmileysAndEmotion, "😄"),
                (emojis::Group::PeopleAndBody, "👋"),
                (emojis::Group::AnimalsAndNature, "🌲"),
                (emojis::Group::FoodAndDrink, "🍔"),
                (emojis::Group::TravelAndPlaces, "✈️"),
                (emojis::Group::Activities, "⚽"),
                (emojis::Group::Objects, "💡"),
                (emojis::Group::Symbols, "🔣"),
                (emojis::Group::Flags, "🏁"),
            ];

            let mut cat_row = Row::new().spacing(4).align_y(Alignment::Center);
            for (group, symbol) in categories {
                let is_selected = self.selected_emoji_group == Some(group);

                if is_selected {
                    let btn =
                        button::suggested(symbol).on_press(Message::SelectEmojiGroup(Some(group)));
                    cat_row = cat_row.push(btn);
                } else {
                    let btn_content = container(text::body(symbol).size(16))
                        .padding([2, 4])
                        .align_x(Alignment::Center)
                        .align_y(Alignment::Center);
                    let btn = button::custom(btn_content)
                        .on_press(Message::SelectEmojiGroup(Some(group)));
                    cat_row = cat_row.push(btn);
                }
            }
            picker_col = picker_col.push(cat_row);
        }

        let mut emoji_grid = Row::new().spacing(4);
        let mut has_elements = false;
        let mut no_results = false;

        if self.emoji_search_query.is_empty() {
            if let Some(group) = self.selected_emoji_group {
                for emoji in group.emojis() {
                    let emoji_str = emoji.as_str();
                    let btn = button::custom(
                        container(text::body(emoji.as_str()).size(18))
                            .padding(4)
                            .align_x(Alignment::Center)
                            .align_y(Alignment::Center),
                    )
                    .on_press(Message::EmojiPickerSelected(emoji_str));
                    emoji_grid = emoji_grid.push(btn);
                    has_elements = true;
                }
            }
        } else {
            let filter_is_ascii = self.emoji_search_query.is_ascii();
            let filter_lower_fallback = (!filter_is_ascii).then(|| self.emoji_search_query.to_lowercase());
            let mut count = 0;
            for emoji in emojis::iter() {
                if crate::contains_ignore_ascii_case(
                    emoji.name(),
                    &self.emoji_search_query,
                    filter_lower_fallback.as_deref(),
                ) || emoji.shortcodes().any(|s| {
                    crate::contains_ignore_ascii_case(s, &self.emoji_search_query, filter_lower_fallback.as_deref())
                })
                {
                    let emoji_str = emoji.as_str();
                    let btn = button::custom(
                        container(text::body(emoji.as_str()).size(18))
                            .padding(4)
                            .align_x(Alignment::Center)
                            .align_y(Alignment::Center),
                    )
                    .on_press(Message::EmojiPickerSelected(emoji_str));
                    emoji_grid = emoji_grid.push(btn);
                    count += 1;
                    has_elements = true;
                    if count >= 100 {
                        break;
                    }
                }
            }
            if count == 0 {
                no_results = true;
            }
        }

        let scroll_grid = if no_results {
            let no_found = text::body(crate::fl!("no-results-found")).size(12);
            scrollable(no_found)
                .height(200)
                .width(cosmic::iced::Length::Fill)
        } else if has_elements {
            scrollable(emoji_grid.wrap())
                .height(200)
                .width(cosmic::iced::Length::Fill)
        } else {
            scrollable(cosmic::widget::space().height(0))
                .height(200)
                .width(cosmic::iced::Length::Fill)
        };

        picker_col = picker_col.push(scroll_grid);

        container(picker_col)
            .padding(10)
            .width(cosmic::iced::Length::Fill)
            .max_width(320)
            .into()
    }

    fn view_sender_info<'a>(
        &'a self,
        avatar_url: Option<&'a str>,
        sender_name: &'a str,
        timestamp: &'a str,
        sender_id: Option<&'a matrix_sdk::ruma::UserId>,
        is_ignored: bool,
        is_me: bool,
    ) -> Row<'a, Message, cosmic::Theme> {
        let mut sender_info = Row::new().spacing(5).align_y(Alignment::Center);

        if let Some(mxc_url) = avatar_url {
            if let Some(handle) = self.media_cache.get(mxc_url) {
                sender_info =
                    sender_info.push(cosmic::widget::image(handle.clone()).width(20).height(20));
            } else {
                sender_info = sender_info.push(
                    container(cosmic::widget::icon::from_name("avatar-default-symbolic").size(12))
                        .padding(2),
                );
            }
        } else {
            sender_info = sender_info.push(
                container(cosmic::widget::icon::from_name("avatar-default-symbolic").size(12))
                    .padding(2),
            );
        }

        sender_info = sender_info.push(text::body(sender_name).size(10));

        if !is_me && let Some(id) = sender_id {
            if is_ignored {
                sender_info = sender_info.push(
                    button::icon(Named::new("dialog-error-symbolic"))
                        .on_press(Message::UserSettings(
                            crate::settings::user::Message::UnignoreUserById(id.to_owned()),
                        ))
                        .tooltip(crate::fl!("unignore-user")),
                );
            } else {
                sender_info = sender_info.push(
                    button::icon(Named::new("dialog-error-symbolic"))
                        .on_press(Message::UserSettings(
                            crate::settings::user::Message::IgnoreUserById(id.to_owned()),
                        ))
                        .tooltip(crate::fl!("ignore")),
                );
            }
        }

        sender_info = sender_info.push(text::body(timestamp).size(10));

        sender_info
    }

    fn view_message_image<'a>(
        &'a self,
        image: &'a matrix_sdk::ruma::events::room::message::ImageMessageEventContent,
    ) -> Column<'a, Message, cosmic::Theme> {
        let mut bubble_col = Column::new();
        let mxc_url = match &image.source {
            MediaSource::Plain(uri) => uri.to_string(),
            MediaSource::Encrypted(file) => file.url.to_string(),
        };
        bubble_col = bubble_col.push(text::body(format!("📷 Image: {}", image.body)).size(
            if self.app_settings.compact_mode {
                12
            } else {
                14
            },
        ));

        if self.user_settings.media_previews_display_policy {
            if let Some(handle) = self.media_cache.get(&mxc_url) {
                bubble_col = bubble_col.push(
                    button::custom(cosmic::widget::image(handle.clone()).width(
                        if self.app_settings.compact_mode {
                            150
                        } else {
                            300
                        },
                    ))
                    .padding(0)
                    .on_press(Message::OpenImage(handle.clone())),
                );
            } else {
                bubble_col = bubble_col.push(
                    button::text(crate::fl!("download-image"))
                        .on_press(Message::FetchMedia(image.source.clone())),
                );
            }
        }
        bubble_col
    }

    fn view_message_file<'a>(
        &'a self,
        file: &'a matrix_sdk::ruma::events::room::message::FileMessageEventContent,
    ) -> Column<'a, Message, cosmic::Theme> {
        let mut bubble_col = Column::new();
        let mxc_url = match &file.source {
            MediaSource::Plain(uri) => uri.to_string(),
            MediaSource::Encrypted(file) => file.url.to_string(),
        };
        bubble_col = bubble_col.push(text::body(format!("📁 File: {}", file.body)).size(
            if self.app_settings.compact_mode {
                12
            } else {
                14
            },
        ));
        if self.media_cache.contains_key(&mxc_url) {
            bubble_col = bubble_col.push(text::body(crate::fl!("downloaded")));
        } else {
            bubble_col = bubble_col.push(
                button::text(crate::fl!("download-file"))
                    .on_press(Message::FetchMedia(file.source.clone())),
            );
        }
        bubble_col
    }

    fn view_message_text<'a>(
        &'a self,
        message: &'a matrix_sdk::ruma::events::room::message::MessageType,
        markdown: &'a [PreviewEvent],
    ) -> Column<'a, Message, Theme> {
        let mut bubble_col: Column<'a, Message, Theme> = Column::new();
        if self.app_settings.render_markdown {
            bubble_col = bubble_col.push(
                crate::rich_text::RichSelectableText::new(markdown.to_vec(), |url| {
                    Message::OpenUrl(url)
                })
                .into_element(),
            );
        } else {
            bubble_col = bubble_col.push(
                crate::rich_text::RichSelectableText::new(
                    vec![crate::PreviewEvent::Text(message.body().to_string())],
                    Message::OpenUrl,
                )
                .into_element(),
            );
        }
        bubble_col
    }

    pub fn view_threaded_timeline(&self) -> Element<'_, Message> {
        let mut timeline_col = Column::new().spacing(10).width(cosmic::iced::Length::Fill);

        let is_filtering = self.is_search_active
            && !self.search_query.is_empty()
            && self.current_settings_panel.is_none();

        let filter_is_ascii = self.search_query.is_ascii();
        let filter_lower_fallback =
            (is_filtering && !filter_is_ascii).then(|| self.search_query.to_lowercase());

        let room_name = if let Some(room_id) = &self.selected_room {
            self.room_list
                .iter()
                .find(|r| &r.id == room_id)
                .and_then(|r| r.name.as_deref())
                .unwrap_or("Room")
        } else {
            "Room"
        };

        let header = Row::new()
            .spacing(10)
            .align_y(Alignment::Center)
            .push(text::title3(format!(
                "{}: {}",
                crate::fl!("thread"),
                room_name
            )))
            .push(cosmic::widget::space().width(cosmic::iced::Length::Fill))
            .push(button::text(crate::fl!("close-thread")).on_press(Message::CloseThread));

        // In a real application, you might want to find and show the root message first.
        // For simplicity, we assume it's part of the threaded timeline from the SDK.

        for item in &self.threaded_timeline_items {
            if let Some(event) = item.item.as_event() {
                if is_filtering {
                    let body = event
                        .content()
                        .as_message()
                        .map(|m| m.body())
                        .unwrap_or_default();
                    if !crate::contains_ignore_ascii_case(
                        body,
                        &self.search_query,
                        filter_lower_fallback.as_deref(),
                    ) {
                        continue;
                    }
                }
                timeline_col = timeline_col.push(self.view_item(item));
            }
        }

        let scrollable_timeline = scrollable(timeline_col)
            .id(crate::THREADED_TIMELINE_ID.clone())
            .height(cosmic::iced::Length::Fill)
            .on_scroll(|viewport| Message::TimelineScrolled(viewport, true));

        Column::new()
            .spacing(10)
            .push(header)
            .push(scrollable_timeline)
            .push(self.view_composer())
            .into()
    }

    pub fn view_preview(&self) -> Element<'_, Message> {
        container(
            scrollable(
                crate::rich_text::RichSelectableText::new(
                    self.composer_preview_events.clone(),
                    Message::OpenUrl,
                )
                .into_element(),
            )
            .height(100),
        )
        .padding(10)
        .into()
    }

    fn view_item<'a>(&'a self, item: &'a crate::ConstellationsItem) -> Element<'a, Message> {
        if let Some(event) = item.item.as_event()
            && let Some(message) = event.content().as_message()
        {
            let is_me = item.is_me;

            let reaction_row = self.view_reactions(event);
            let is_ignored = self.user_settings.ignored_users.contains(&item.sender_id);
            let sender_info = self.view_sender_info(
                item.avatar_url.as_deref(),
                item.sender_name.as_str(),
                item.timestamp.as_str(),
                Some(&item.sender_id),
                is_ignored,
                is_me,
            );

            let mut bubble_col = Column::new()
                .spacing(if self.app_settings.compact_mode { 0 } else { 2 })
                .push(sender_info);

            if let Some(in_reply_to) = event.content().in_reply_to() {
                let mut reply_sender = "";
                let mut reply_body = "";

                if let matrix_sdk_ui::timeline::TimelineDetails::Ready(replied_ev) =
                    &in_reply_to.event
                {
                    reply_sender = replied_ev.sender.as_str();
                    if let Some(msg) = replied_ev.content.as_message() {
                        reply_body = msg.body();
                    }
                }

                let mut reply_snippet = String::with_capacity(64);
                if !reply_sender.is_empty() {
                    reply_snippet.push_str(reply_sender);
                    reply_snippet.push_str(": ");
                }

                if reply_body.len() <= 50 {
                    if !reply_body.is_empty() {
                        reply_snippet.push_str(reply_body);
                    } else {
                        reply_snippet.push_str("Replying...");
                    }
                } else {
                    let mut char_indices = reply_body.char_indices();
                    if let Some((idx_47, _)) = char_indices.nth(47) {
                        if char_indices.nth(2).is_some() { // 50th char
                            reply_snippet.push_str(&reply_body[..idx_47]);
                            reply_snippet.push_str("...");
                        } else {
                            reply_snippet.push_str(reply_body);
                        }
                    } else {
                        reply_snippet.push_str(reply_body);
                    }
                }

                let reply_indicator = Row::new().spacing(5).push(text::body("⤴").size(10)).push(
                    text::body(reply_snippet).size(10),
                );

                bubble_col = bubble_col.push(container(reply_indicator).padding([0, 0, 5, 10]));
            }

            match message.msgtype() {
                MessageType::Image(image) => {
                    bubble_col = bubble_col.push(self.view_message_image(image));
                }
                MessageType::File(file) => {
                    bubble_col = bubble_col.push(self.view_message_file(file));
                }
                _ => {
                    bubble_col =
                        bubble_col.push(self.view_message_text(message.msgtype(), &item.markdown));
                }
            }

            let mut action_row = Row::new().spacing(5).align_y(Alignment::Center);

            // "Add reaction" button
            let is_picker_open = self.active_reaction_picker.as_ref() == Some(&event.identifier());
            let btn = button::text(crate::fl!("reaction")).on_press(if is_picker_open {
                Message::OpenReactionPicker(None)
            } else {
                Message::OpenReactionPicker(Some(event.identifier().clone()))
            });
            let btn_tooltip = tooltip(
                btn,
                text::body(crate::fl!("add-reaction")),
                Position::Bottom,
            );
            action_row = action_row.push(btn_tooltip);

            // Start a thread
            let root_id = event.identifier();
            let start_thread_btn =
                button::text(crate::fl!("open-thread")).on_press(match root_id {
                    matrix::TimelineEventItemId::EventId(id) => Message::OpenThread(id.to_owned()),
                    _ => Message::NoOp,
                });
            let action_tooltip = tooltip(
                start_thread_btn,
                text::body(crate::fl!("tooltip-thread")),
                Position::Bottom,
            );
            action_row = action_row.push(action_tooltip);

            let reply_btn = button::text(crate::fl!("reply"))
                .on_press(Message::StartReply(event.identifier().clone()));
            let reply_tooltip = tooltip(
                reply_btn,
                text::body(crate::fl!("tooltip-reply")),
                Position::Bottom,
            );
            action_row = action_row.push(reply_tooltip);

            if is_me {
                let edit_btn = button::text(crate::fl!("edit"))
                    .on_press(Message::StartEdit(event.identifier().clone()));
                let edit_tooltip = tooltip(
                    edit_btn,
                    text::body(crate::fl!("tooltip-edit")),
                    Position::Bottom,
                );
                action_row = action_row.push(edit_tooltip);

                let delete_btn = button::text(crate::fl!("delete"))
                    .on_press(Message::RedactMessage(event.identifier().clone()));
                let delete_tooltip = tooltip(
                    delete_btn,
                    text::body(crate::fl!("tooltip-delete")),
                    Position::Bottom,
                );
                action_row = action_row.push(delete_tooltip);
            }

            bubble_col = bubble_col.push(reaction_row);

            if self.active_reaction_picker.as_ref() == Some(&event.identifier()) {
                bubble_col =
                    bubble_col.push(self.view_emoji_picker(Some(event.identifier().clone())));
            }

            action_row = action_row.push(self.view_thread_summary(event));
            bubble_col = bubble_col.push(action_row);

            let bubble = container(bubble_col)
                .padding(if self.app_settings.compact_mode {
                    5
                } else {
                    10
                })
                .max_width(600);

            let bubble_wrap =
                container(bubble)
                    .width(cosmic::iced::Length::Fill)
                    .align_x(if is_me {
                        Alignment::End
                    } else {
                        Alignment::Start
                    });

            return bubble_wrap.into();
        }
        cosmic::widget::space().height(0).into()
    }

    fn view_thread_summary(
        &'chat self,
        event: &matrix_sdk_ui::timeline::EventTimelineItem,
    ) -> cosmic::widget::Container<'chat, Message, cosmic::Theme> {
        let has_thread_root = event.content().thread_root().is_some();

        let mut num_replies = event
            .content()
            .thread_summary()
            .map(|s| s.num_replies)
            .unwrap_or_default();

        // Manual count if summary is missing or incomplete (due to view-side filtering)
        if let Some(event_id) = event.event_id() {
            let manual_count = self
                .timeline_items
                .iter()
                .filter(|i| {
                    i.item
                        .as_event()
                        .and_then(|e| e.content().thread_root())
                        .map(|r| r == event_id)
                        .unwrap_or(false)
                })
                .count() as u32;
            if manual_count > num_replies {
                num_replies = manual_count;
            }
        }

        if num_replies > 0 && !has_thread_root && self.active_thread_root.is_none() {
            let mut latest_sender = None;
            let mut latest_body = None;
            // Buffers to hold strings from thread_summary to avoid borrow checker issues with `summary` dropping
            let mut fallback_sender_buf = String::new();
            let mut fallback_body_buf = String::new();

            if let Some(summary) = event.content().thread_summary()
                && let matrix_sdk_ui::timeline::TimelineDetails::Ready(latest_ev) =
                    &summary.latest_event
                {
                    // Try to find in timeline_items for better profile info
                    if let Some(item) = self.timeline_items.iter().rfind(|i| {
                        i.item
                            .as_event()
                            .and_then(|e| e.event_id())
                            .map(|id| match &latest_ev.identifier {
                                matrix::TimelineEventItemId::EventId(eid) => id == eid,
                                _ => false,
                            })
                            .unwrap_or(false)
                    }) {
                        latest_sender = Some(item.sender_name.as_str());
                        if let Some(ev) = item.item.as_event()
                            && let Some(msg) = ev.content().as_message() {
                                latest_body = Some(msg.body());
                            }
                    } else {
                        // Use info from embedded event
                        fallback_sender_buf.push_str(latest_ev.sender.as_str());
                        if let Some(msg) = latest_ev.content.as_message() {
                            fallback_body_buf.push_str(msg.body());
                        }
                    }
                }

            // If we still don't have it (e.g. summary was missing or not ready),
            // try manual search by thread root
            if latest_body.is_none() && fallback_body_buf.is_empty()
                && let Some(event_id) = event.event_id()
                    && let Some(item) = self.timeline_items.iter().rfind(|i| {
                        i.item
                            .as_event()
                            .and_then(|e| e.content().thread_root())
                            .map(|r| r == event_id)
                            .unwrap_or(false)
                    }) {
                        latest_sender = Some(item.sender_name.as_str());
                        if let Some(ev) = item.item.as_event()
                            && let Some(msg) = ev.content().as_message() {
                                latest_body = Some(msg.body());
                            }
                    }

            let mut summary_row = Row::new()
                .spacing(5)
                .align_y(Alignment::Center)
                .push(
                    text::body(format!(
                        "{} {}",
                        num_replies,
                        if num_replies == 1 {
                            crate::fl!("reply")
                        } else {
                            crate::fl!("replies")
                        }
                    ))
                    .size(12),
                )
                .push(cosmic::widget::icon::from_name("chat-bubble-symbolic").size(14));

            let final_body = latest_body.unwrap_or({
                if !fallback_body_buf.is_empty() {
                    fallback_body_buf.as_str()
                } else {
                    ""
                }
            });

            if !final_body.is_empty() {
                let sender = latest_sender.unwrap_or({
                    if !fallback_sender_buf.is_empty() {
                        fallback_sender_buf.as_str()
                    } else {
                        "Unknown"
                    }
                });
                let mut text_str = String::with_capacity(64);
                text_str.push_str(sender);
                text_str.push_str(": ");

                if final_body.len() <= 30 {
                    text_str.push_str(final_body);
                } else {
                    let mut char_indices = final_body.char_indices();
                    if let Some((idx_27, _)) = char_indices.nth(27) {
                        if char_indices.nth(2).is_some() { // 30th char
                            text_str.push_str(&final_body[..idx_27]);
                            text_str.push_str("...");
                        } else {
                            text_str.push_str(final_body);
                        }
                    } else {
                        text_str.push_str(final_body);
                    }
                }
                summary_row = summary_row.push(text::body(text_str).size(12));
            }

            let summary_btn = button::custom(container(summary_row).padding([0, 5])).on_press(
                match event.identifier() {
                    matrix::TimelineEventItemId::EventId(id) => Message::OpenThread(id.to_owned()),
                    _ => Message::NoOp,
                },
            );

            container(summary_btn).padding([5, 0])
        } else {
            container(text(""))
        }
    }

    #[rust_analyzer::skip]
    pub fn view_main_content(&self, status_text: String) -> Element<'_, Message> {
        let mut content = Column::new()
            .spacing(20)
            .padding(20)
            .width(cosmic::iced::Length::Fill);

        if matches!(
            self.sync_status,
            matrix::SyncStatus::Error(_) | matrix::SyncStatus::MissingSlidingSyncSupport
        ) {
            content = content.push(text::body(status_text).size(14));
        }

        if let Some(room_id) = &self.selected_room {
            let room_name = self
                .room_list
                .iter()
                .find(|r| &r.id == room_id)
                .and_then(|r| r.name.as_deref())
                .unwrap_or("Room");
            let is_in_call = self.user_id.as_ref().is_some_and(|uid| {
                if let Ok(user_id) = matrix_sdk::ruma::UserId::parse(uid) {
                    self.call_participants
                        .get(room_id)
                        .is_some_and(|p| p.contains(&user_id))
                } else {
                    false
                }
            });

            let call_participants = self.call_participants.get(room_id);
            let participant_count = call_participants.map_or(0, |p| p.len());

            let mut room_header = Row::new()
                .spacing(10)
                .align_y(Alignment::Center)
                .push(text::title3(room_name));

            if participant_count > 0 {
                room_header = room_header.push(
                    container(
                        Row::new()
                            .spacing(5)
                            .align_y(Alignment::Center)
                            .push(cosmic::widget::icon::from_name("camera-video-symbolic").size(16))
                            .push(text::body(participant_count.to_string()).size(12)),
                    )
                    .padding([2, 5]),
                );
            }

            room_header = room_header
                .push(cosmic::widget::space().width(cosmic::iced::Length::Fill))
                .push(if is_in_call {
                    button::icon(Named::new("call-stop"))
                        .on_press(Message::LeaveCall)
                        .tooltip(crate::fl!("call-leave"))
                } else {
                    button::icon(Named::new("camera-web"))
                        .on_press(Message::JoinCall)
                        .tooltip(crate::fl!("call-join"))
                })
                .push(
                    button::icon(Named::new("emblem-system"))
                        .tooltip(crate::fl!("room-settings"))
                        .on_press(Message::OpenSettings(crate::SettingsPanel::Room)),
                );

            if self.active_thread_root.is_some() {
                content = content.push(self.view_threaded_timeline());
            } else {
                content = content.push(room_header);

                content = content.push(self.view_timeline());

                content = content.push(self.view_composer());
            }
        } else {
            let empty_state = container(
                Column::new()
                    .spacing(10)
                    .align_x(Alignment::Center)
                    .push(cosmic::widget::icon::from_name("chat-bubble-symbolic").size(64))
                    .push(text::title1(crate::fl!("no-room-selected")))
                    .push(text::body(crate::fl!("select-room-to-start"))),
            )
            .width(cosmic::iced::Length::Fill)
            .height(cosmic::iced::Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);

            content = content.push(empty_state);
        }

        if let Some(error) = &self.error {
            let error_bar = container(
                Row::new()
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .push(text::body(error))
                    .push(button::text(crate::fl!("dismiss")).on_press(Message::DismissError)),
            )
            .padding(10);
            content = content.push(error_bar);
        }

        content.into()
    }

    pub fn view_composer(&self) -> Element<'_, Message> {
        let mut content = Column::new().spacing(10);

        if self.is_composer_emoji_picker_active {
            content = content.push(self.view_emoji_picker(None));
        }

        if let Some(replying_to) = &self.replying_to {
            let mut snippet = replying_to
                .item
                .as_event()
                .and_then(|ev| ev.content().as_message())
                .map(|msg| msg.body().to_string())
                .unwrap_or_default();
            if snippet.len() > 100 {
                snippet.truncate(97);
                snippet.push_str("...");
            }

            let reply_bar = view_reply_bar(snippet, replying_to);
            content = content.push(container(reply_bar).padding(10));
        }

        if let Some(editing_item) = &self.editing_item {
            let mut snippet = editing_item
                .item
                .as_event()
                .and_then(|ev| ev.content().as_message())
                .map(|msg| msg.body().to_string())
                .unwrap_or_default();
            if snippet.len() > 100 {
                snippet.truncate(97);
                snippet.push_str("...");
            }

            let edit_bar = Row::new()
                .spacing(10)
                .align_y(Alignment::Center)
                .push(text::body(crate::fl!("editing")).size(12))
                .push(text::body(snippet).size(12))
                .push(cosmic::widget::space().width(cosmic::iced::Length::Fill))
                .push(button::text(crate::fl!("cancel")).on_press(Message::CancelEdit));

            content = content.push(container(edit_bar).padding(10));
        }

        let composer = if self.composer_is_preview {
            self.view_preview()
        } else {
            container(
                text_input(crate::fl!("type-message"), &self.composer_text)
                    .on_input(Message::ComposerChanged)
                    .on_submit(|_| Message::SendMessage),
            )
            .padding(10)
            .into()
        };

        let mut attachments_view = Column::new().spacing(5);
        if !self.composer_attachments.is_empty() {
            attachments_view =
                attachments_view.push(text::body(crate::fl!("attachments")).size(12));
            for (i, path) in self.composer_attachments.iter().enumerate() {
                let filename = path.file_name().unwrap_or_default().to_string_lossy();
                let attachment_row = Row::new()
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .push(text::body(filename).size(12))
                    .push(
                        button::destructive(crate::fl!("remove-attachment"))
                            .on_press(Message::RemoveAttachment(i)),
                    );
                attachments_view = attachments_view.push(attachment_row);
            }
        }

        let is_empty = self.composer_text.trim().is_empty() && self.composer_attachments.is_empty();

        let mut send_btn = button::text(if self.editing_item.is_some() {
            crate::fl!("save")
        } else if self.active_thread_root.is_some() {
            crate::fl!("reply")
        } else {
            crate::fl!("send")
        });
        if !is_empty {
            send_btn = send_btn
                .on_press(Message::SendMessage)
                .tooltip(crate::fl!("tooltip-send"));
        }

        let send_btn_widget: Element<'_, Message> = if is_empty {
            tooltip(
                send_btn,
                text::body(crate::fl!("type-message-or-attach")),
                Position::Top,
            )
            .into()
        } else {
            send_btn.into()
        };

        let controls = Row::new()
            .spacing(10)
            .push(
                button::text(crate::fl!("attach"))
                    .on_press(Message::AddAttachment)
                    .tooltip(crate::fl!("tooltip-attach")),
            )
            .push(
                button::text("😀")
                    .on_press(Message::ToggleComposerEmojiPicker)
                    .tooltip(crate::fl!("tooltip-emojis")),
            )
            .push(if self.composer_is_preview {
                button::text(crate::fl!("edit"))
                    .on_press(Message::TogglePreview)
                    .tooltip(crate::fl!("tooltip-edit"))
            } else {
                button::text(crate::fl!("preview"))
                    .on_press(Message::TogglePreview)
                    .tooltip(crate::fl!("tooltip-preview"))
            })
            .push(send_btn_widget);

        content
            .push(attachments_view)
            .push(composer)
            .push(controls)
            .into()
    }
}

#[rust_analyzer::skip]
fn view_reply_bar(
    snippet: String,
    replying_to: &crate::ConstellationsItem,
) -> Row<'_, Message, Theme> {
    Row::new()
        .spacing(10)
        .align_y(Alignment::Center)
        .push(
            text::body(crate::fl!(
                "replying-to",
                user = replying_to.sender_name.as_str()
            ))
            .size(12),
        )
        .push(text::body(snippet).size(12))
        .push(cosmic::widget::space().width(cosmic::iced::Length::Fill))
        .push(button::text(crate::fl!("cancel")).on_press(Message::CancelReply))
}
