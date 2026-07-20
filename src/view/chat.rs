use chrono::{DateTime, DurationRound, TimeDelta};
use cosmic::{
    Element, Theme,
    iced::{
        Alignment,
        widget::{scrollable, tooltip},
    },
    widget::{
        Column, Row, button, container, divider, icon::Named, text, text_editor, text_input,
        tooltip::Position,
    },
};
use matrix_sdk::ruma::events::room::{MediaSource, message::MessageType};
use matrix_sdk_ui::timeline::{TimelineDetails, TimelineEventItemId};

use crate::{
    Constellations, Message, PreviewEvent, matrix,
    view::{
        ADD_REACTION, CLOSE_THREAD, DOWNLOAD_FILE, DOWNLOAD_IMAGE, DOWNLOADED, IGNORE, OPEN_THREAD,
        REPLIES, REPLY, TOOLTIP_ATTACH, TOOLTIP_COPY_LINK, TOOLTIP_COPY_ROOM_LINK, TOOLTIP_DELETE,
        TOOLTIP_EDIT, TOOLTIP_EMOJIS, TOOLTIP_FIND, TOOLTIP_LOCATION, TOOLTIP_REPLY,
        TOOLTIP_THREAD, UNIGNORE_USER, switcher::view_room_name_menu,
    },
};

impl<'chat> Constellations {
    pub fn view_timeline(&self) -> Element<'_, Message> {
        let selected_room_data = self
            .selected_room
            .as_ref()
            .and_then(|id| self.room_list.iter().find(|r| r.id.as_ref() == id.as_ref()));

        let is_video_room = selected_room_data
            .map(|r| {
                r.room_type
                    .as_ref()
                    .is_some_and(|t| t.to_string() == "org.matrix.msc3401.call.room")
            })
            .unwrap_or(false);

        if is_video_room && let Some(room) = selected_room_data {
            return self.view_video_room(room);
        }

        let mut timeline = Column::new().spacing(10).width(cosmic::iced::Length::Fill);

        let is_filtering = self.is_search_active
            && !self.search_query.is_empty()
            && self.current_settings_panel.is_none();

        if is_filtering {
            return self.view_search_results();
        }
        let filter_is_ascii = self.search_query.is_ascii();
        let filter_lower_fallback =
            (is_filtering && !filter_is_ascii).then(|| self.search_query.to_lowercase());

        let mut pending_date_divider: Option<matrix_sdk::ruma::MilliSecondsSinceUnixEpoch> = None;

        for item in &self.timeline_items {
            if item.item.is_none() {
                // Render simulated/mock items!
                timeline = timeline.push(self.view_item(item, &self.thread_counts));
                continue;
            }

            if let Some(timeline_item) = &item.item
                && let Some(event) = timeline_item.as_event()
                && event.content().as_message().is_some()
            {
                // View-side thread filtering
                if self.app_settings.hide_threaded_messages && item.thread_root_id.is_some() {
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

                if let Some(date) = pending_date_divider.take() {
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
                    );
                }

                timeline = timeline.push(self.view_item(item, &self.thread_counts));
            } else if let Some(timeline_item) = &item.item
                && let Some(matrix::VirtualTimelineItem::DateDivider(date)) =
                    timeline_item.as_virtual()
            {
                pending_date_divider = Some(*date);
            }
        }

        scrollable(timeline)
            .id(crate::TIMELINE_ID.clone())
            .height(cosmic::iced::Length::Fill)
            .on_scroll(|viewport| Message::TimelineScrolled(viewport, false))
            .into()
    }

    /// Persistent banner shown while the room is viewed through an
    /// event-focused (permalink context) timeline. Explains that the user is
    /// not at the newest messages and offers a one-click return to live.
    fn view_older_messages_banner(&self) -> Element<'_, Message> {
        let label = text::body(crate::fl!("viewing-older-messages"));
        let jump_btn = button::text(crate::fl!("jump-to-newest")).on_press(Message::ReturnToLive);
        // Wrap in a container so the banner reads as a distinct, themed strip
        // (uses the default theme surface rather than a hardcoded color).
        container(
            Row::new()
                .spacing(10)
                .align_y(Alignment::Center)
                .push(label)
                .push(jump_btn),
        )
        .padding([6, 10])
        .width(cosmic::iced::Length::Fill)
        .into()
    }

    fn view_reactions<'reaction>(
        &'reaction self,
        event: &'reaction matrix_sdk_ui::timeline::EventTimelineItem,
        item_id: &matrix::TimelineEventItemId,
    ) -> Row<'reaction, Message, cosmic::Theme> {
        let mut reaction_row = Row::new().spacing(5).align_y(Alignment::Center);
        let reactions = event.content().reactions();

        if let Some(reaction) = reactions {
            for (key, senders) in reaction.iter() {
                let count = senders.len();

                let is_me_reacted = if let Some(me) = &self.user_id {
                    senders.keys().any(|user_id| user_id.as_str() == me)
                } else {
                    false
                };

                let btn_content =
                    container(text::body(format!("{} {}", key, count)).size(10)).padding([2, 4]);

                // We can differentiate style if reacted, but for now we just wrap in button.
                let btn = button::custom(btn_content)
                    .on_press(Message::ToggleReaction(item_id.to_owned(), key.clone()));

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

    fn view_emoji_picker<'emoji>(
        &'emoji self,
        item_id: Option<matrix::TimelineEventItemId>,
    ) -> Element<'emoji, Message> {
        let search_input = text_input(crate::fl!("search-emojis"), &self.emoji_search_query)
            .on_input(Message::EmojiSearchQueryChanged)
            .width(cosmic::iced::Length::Fill);

        let close_btn = button::icon(cosmic::widget::icon::from_name("window-close-symbolic"))
            .on_press(if item_id.is_some() {
                Message::OpenReactionPicker(None)
            } else {
                Message::ToggleComposerEmojiPicker
            });

        let close_btn_tooltip = tooltip(
            close_btn,
            text::body(crate::fl!("close-picker")),
            Position::Bottom,
        );

        let top_row = Row::new()
            .spacing(5)
            .align_y(Alignment::Center)
            .push(search_input)
            .push(close_btn_tooltip);

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
            let filter_lower_fallback =
                (!filter_is_ascii).then(|| self.emoji_search_query.to_lowercase());
            let mut count = 0;
            for emoji in emojis::iter() {
                if crate::contains_ignore_ascii_case(
                    emoji.name(),
                    &self.emoji_search_query,
                    filter_lower_fallback.as_deref(),
                ) || emoji.shortcodes().any(|s| {
                    crate::contains_ignore_ascii_case(
                        s,
                        &self.emoji_search_query,
                        filter_lower_fallback.as_deref(),
                    )
                }) {
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
            let no_found = container(
                Column::new()
                    .spacing(10)
                    .align_x(Alignment::Center)
                    .push(cosmic::widget::icon::from_name("edit-find-symbolic").size(64))
                    .push(text::body(crate::fl!("no-results-found")).size(16)),
            )
            .width(cosmic::iced::Length::Fill)
            .align_x(Alignment::Center)
            .padding(20);

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

    fn view_sender_info<'sender>(
        &'sender self,
        avatar_url: Option<&'sender str>,
        sender_name: &'sender str,
        timestamp: &'sender str,
        is_pinned: bool,
    ) -> Row<'sender, Message, cosmic::Theme> {
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

        sender_info = sender_info.push(text::body(timestamp).size(10));

        if is_pinned {
            sender_info =
                sender_info.push(cosmic::widget::icon::from_name("pin-symbolic").size(12));
        }

        sender_info
    }

    fn view_message_image<'image>(
        &'image self,
        image: &'image matrix_sdk::ruma::events::room::message::ImageMessageEventContent,
    ) -> Column<'image, Message, cosmic::Theme> {
        let mut bubble_col = Column::new();
        let mxc_url = match &image.source {
            MediaSource::Plain(uri) => uri.as_str(),
            MediaSource::Encrypted(file) => file.url.as_str(),
        };
        bubble_col = bubble_col.push(
            text::body(crate::fl!("image-message", body = image.body.clone())).size(
                if self.app_settings.compact_mode {
                    12
                } else {
                    14
                },
            ),
        );

        if self.user_settings.media_previews_display_policy {
            if let Some(handle) = self.media_cache.get(mxc_url) {
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
                    button::text(DOWNLOAD_IMAGE.as_str())
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
            MediaSource::Plain(uri) => uri.as_str(),
            MediaSource::Encrypted(file) => file.url.as_str(),
        };
        bubble_col = bubble_col.push(
            text::body(crate::fl!("file-message", body = file.body.clone())).size(
                if self.app_settings.compact_mode {
                    12
                } else {
                    14
                },
            ),
        );
        if self.media_cache.contains_key(mxc_url) {
            bubble_col = bubble_col.push(text::body(DOWNLOADED.as_str()));
        } else {
            bubble_col = bubble_col.push(
                button::text(DOWNLOAD_FILE.as_str())
                    .on_press(Message::FetchMedia(file.source.clone())),
            );
        }
        bubble_col
    }

    fn view_message_text<'message>(
        &'message self,
        events: &'message [PreviewEvent],
        _links: &'message [(String, String)],
    ) -> Column<'message, Message, Theme> {
        let mut bubble_col: Column<'message, Message, Theme> = Column::new();
        // `RichSelectableText` borrows `[PreviewEvent]` slices to avoid allocations
        // on every frame. Links are styled with the theme accent color and get an
        // underline on hover — matching the cosmic Link widget appearance.
        bubble_col = bubble_col.push(
            crate::rich_text::RichSelectableText::new(events, Message::OpenMatrixLink)
                .into_element(),
        );
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

        let room_name = self
            .selected_room
            .as_ref()
            .and_then(|room_id| self.get_room_name(room_id))
            .map(str::to_string)
            .unwrap_or_else(|| crate::fl!("room-fallback"));

        let header = Row::new()
            .spacing(10)
            .align_y(Alignment::Center)
            .push(text::title3(format!(
                "{}: {}",
                OPEN_THREAD.as_str(),
                room_name
            )))
            .push(cosmic::widget::space().width(cosmic::iced::Length::Fill))
            .push(tooltip(
                button::icon(cosmic::widget::icon::from_name("window-close-symbolic"))
                    .on_press(Message::CloseThread),
                text::body(CLOSE_THREAD.as_str()),
                Position::Bottom,
            ));

        for item in &self.threaded_timeline_items {
            if item.item.is_none() {
                timeline_col = timeline_col.push(self.view_item(item, &self.thread_counts));
                continue;
            }

            if let Some(timeline_item) = &item.item
                && let Some(event) = timeline_item.as_event()
                && event.content().as_message().is_some()
            {
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
                timeline_col = timeline_col.push(self.view_item(item, &self.thread_counts));
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
                self.view_message_text(&self.composer_preview_events, &self.composer_preview_links),
            )
            .height(100),
        )
        .padding(0)
        .into()
    }

    fn view_item<'item>(
        &'item self,
        item: &'item crate::ConstellationsItem,
        thread_counts: &std::collections::HashMap<matrix_sdk::ruma::OwnedEventId, u32>,
    ) -> Element<'item, Message> {
        if let Some(timeline_item) = &item.item
            && let Some(event) = timeline_item.as_event()
            && let Some(message) = event.content().as_message()
        {
            self.view_message_item(item, event, message, thread_counts)
        } else {
            self.view_state_item(item)
        }
    }

    fn view_message_item<'item>(
        &'item self,
        item: &'item crate::ConstellationsItem,
        event: &'item matrix_sdk_ui::timeline::EventTimelineItem,
        message: &'item matrix_sdk_ui::timeline::Message,
        thread_counts: &std::collections::HashMap<matrix_sdk::ruma::OwnedEventId, u32>,
    ) -> Element<'item, Message> {
        let is_me = item.is_me;

        let fallback_id;
        let item_id = if let Some(id) = item.item_id.as_ref() {
            id
        } else {
            fallback_id = event.identifier();
            &fallback_id
        };
        let reaction_row = self.view_reactions(event, item_id);
        let is_ignored = self.user_settings.ignored_users.contains(&item.sender_id);
        let is_pinned = if let Some(TimelineEventItemId::EventId(id)) = &item.item_id {
            self.pinned_events.contains(id)
        } else {
            false
        };
        let sender_info = self.view_sender_info(
            item.avatar_url.as_deref(),
            item.sender_name.as_str(),
            item.timestamp.as_str(),
            is_pinned,
        );

        let sender_info_wrap = container(sender_info)
            .width(cosmic::iced::Length::Fill)
            .align_x(if is_me {
                Alignment::End
            } else {
                Alignment::Start
            });

        let mut bubble_col = Column::new()
            .spacing(if self.app_settings.compact_mode { 0 } else { 2 })
            .push(sender_info_wrap);

        if let Some(in_reply_to) = event.content().in_reply_to() {
            let mut reply_sender = "";
            let mut reply_body = "";

            if let TimelineDetails::Ready(replied_ev) = &in_reply_to.event {
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
                    reply_snippet.push_str(&crate::fl!("replying"));
                }
            } else {
                let mut char_indices = reply_body.char_indices();
                if let Some((idx_47, _)) = char_indices.nth(47) {
                    if char_indices.nth(2).is_some() {
                        // 50th char
                        reply_snippet.push_str(&reply_body[..idx_47]);
                        reply_snippet.push_str("...");
                    } else {
                        reply_snippet.push_str(reply_body);
                    }
                } else {
                    reply_snippet.push_str(reply_body);
                }
            }

            let reply_event_id = in_reply_to.event_id.clone();
            let reply_indicator = Row::new()
                .spacing(5)
                .push(text::body("⤴").size(10))
                .push(text::body(reply_snippet).size(10));

            let reply_btn = button::custom(reply_indicator)
                .on_press(Message::JumpToMessage(reply_event_id))
                .class(cosmic::theme::Button::ListItem(
                    self.core.system_theme().cosmic().corner_radii.radius_m,
                ));

            let reply_indicator_wrap = container(reply_btn)
                .width(cosmic::iced::Length::Fill)
                .align_x(if is_me {
                    Alignment::End
                } else {
                    Alignment::Start
                })
                .padding([0, 0, 5, 10]);

            bubble_col = bubble_col.push(reply_indicator_wrap);
        }

        match message.msgtype() {
            MessageType::Image(image) => {
                bubble_col = bubble_col.push(self.view_message_image(image));
            }
            MessageType::File(file) => {
                bubble_col = bubble_col.push(self.view_message_file(file));
            }
            _ => {
                let (events, links) = if self.app_settings.render_markdown {
                    (&item.markdown, &item.markdown_links)
                } else {
                    (&item.plain_text, &item.plain_links)
                };
                bubble_col = bubble_col.push(self.view_message_text(events, links));
            }
        }

        let mut action_row = Row::new().spacing(5).align_y(Alignment::Center);

        // "Add reaction" button
        let is_picker_open = self.active_reaction_picker.as_ref() == Some(item_id);
        let btn = button::icon(cosmic::widget::icon::from_name("face-smile-symbolic")).on_press(
            if is_picker_open {
                Message::OpenReactionPicker(None)
            } else {
                Message::OpenReactionPicker(Some(item_id.clone()))
            },
        );
        let btn_tooltip = tooltip(btn, text::body(ADD_REACTION.as_str()), Position::Bottom);
        action_row = action_row.push(btn_tooltip);

        // Reply button
        let reply_btn = button::icon(cosmic::widget::icon::from_name("mail-replied-symbolic"))
            .on_press(Message::StartReply(item_id.clone()));
        let reply_tooltip = tooltip(
            reply_btn,
            text::body(TOOLTIP_REPLY.as_str()),
            Position::Bottom,
        );
        action_row = action_row.push(reply_tooltip);

        // Thread button (either summary or start thread button)
        let has_thread_root = item.thread_root_id.is_some();
        let mut num_replies = event
            .content()
            .thread_summary()
            .map(|s| s.num_replies)
            .unwrap_or_default();

        if let Some(event_id) = event.event_id() {
            let manual_count = thread_counts.get(event_id).copied().unwrap_or(0);
            if manual_count > num_replies {
                num_replies = manual_count;
            }
        }

        let has_thread_summary =
            num_replies > 0 && !has_thread_root && self.active_thread_root.is_none();

        if has_thread_summary {
            action_row = action_row.push(self.view_thread_summary(item, event, thread_counts));
        } else {
            let root_id = item_id.clone();
            let start_thread_btn = button::icon(cosmic::widget::icon::from_name(
                "view-list-symbolic",
            ))
            .on_press(match root_id {
                TimelineEventItemId::EventId(id) => Message::OpenThread(id.to_owned()),
                _ => Message::NoOp,
            });
            let action_tooltip = tooltip(
                start_thread_btn,
                text::body(TOOLTIP_THREAD.as_str()),
                Position::Bottom,
            );
            action_row = action_row.push(action_tooltip);
        }

        if matches!(item_id, TimelineEventItemId::EventId(_)) {
            let copy_btn = button::icon(cosmic::widget::icon::from_name("edit-copy-symbolic"))
                .on_press(Message::CopyMessageLink(item_id.clone()));
            let copy_tooltip = tooltip(
                copy_btn,
                text::body(TOOLTIP_COPY_LINK.as_str()),
                Position::Bottom,
            );
            action_row = action_row.push(copy_tooltip);
        }

        if is_me {
            let edit_btn = button::icon(cosmic::widget::icon::from_name("edit-symbolic"))
                .on_press(Message::StartEdit(item_id.clone()));
            let edit_tooltip = tooltip(
                edit_btn,
                text::body(TOOLTIP_EDIT.as_str()),
                Position::Bottom,
            );
            action_row = action_row.push(edit_tooltip);

            let delete_btn = button::custom(cosmic::widget::icon::from_name("user-trash-symbolic"))
                .class(cosmic::theme::Button::Destructive)
                .on_press(Message::RedactMessage(item_id.clone()));
            let delete_tooltip = tooltip(
                delete_btn,
                text::body(TOOLTIP_DELETE.as_str()),
                Position::Bottom,
            );
            action_row = action_row.push(delete_tooltip);
        } else {
            if is_ignored {
                let ignore_btn = button::icon(Named::new("dialog-error-symbolic"))
                    .on_press(Message::UserSettings(
                        crate::settings::user::Message::UnignoreUserById(item.sender_id.to_owned()),
                    ))
                    .tooltip(UNIGNORE_USER.as_str());
                action_row = action_row.push(ignore_btn);
            } else {
                let ignore_btn = button::icon(Named::new("dialog-error-symbolic"))
                    .on_press(Message::UserSettings(
                        crate::settings::user::Message::IgnoreUserById(item.sender_id.to_owned()),
                    ))
                    .tooltip(IGNORE.as_str());
                action_row = action_row.push(ignore_btn);
            }
        }

        let reaction_row_wrap = container(reaction_row)
            .width(cosmic::iced::Length::Fill)
            .align_x(if is_me {
                Alignment::End
            } else {
                Alignment::Start
            });
        bubble_col = bubble_col.push(reaction_row_wrap);

        if self.active_reaction_picker.as_ref() == Some(item_id) {
            bubble_col = bubble_col.push(self.view_emoji_picker(Some(item_id.clone())));
        }

        let action_row_wrap = container(action_row)
            .width(cosmic::iced::Length::Fill)
            .align_x(if is_me {
                Alignment::End
            } else {
                Alignment::Start
            });
        bubble_col = bubble_col.push(action_row_wrap);

        let bubble = container(bubble_col)
            .style(move |theme: &cosmic::Theme| {
                use cosmic::iced::widget::container::Catalog;
                let cosmic = theme.cosmic();
                let mut style = theme.style(&cosmic::theme::Container::Card);
                if is_me {
                    style.border.color = cosmic.accent.base.into();
                    style.border.width = 1.0;
                }
                style
            })
            .padding(if self.app_settings.compact_mode {
                5
            } else {
                10
            })
            .max_width(600);

        let bubble_wrap = container(bubble)
            .width(cosmic::iced::Length::Fill)
            .align_x(if is_me {
                Alignment::End
            } else {
                Alignment::Start
            });

        bubble_wrap.into()
    }

    fn view_state_item<'item>(
        &'item self,
        item: &'item crate::ConstellationsItem,
    ) -> Element<'item, Message> {
        let is_me = item.is_me;
        let is_pinned = if let Some(TimelineEventItemId::EventId(id)) = &item.item_id {
            self.pinned_events.contains(id)
        } else {
            false
        };

        let sender_info = self.view_sender_info(
            item.avatar_url.as_deref(),
            item.sender_name.as_str(),
            item.timestamp.as_str(),
            is_pinned,
        );

        let sender_info_wrap = container(sender_info)
            .width(cosmic::iced::Length::Fill)
            .align_x(if is_me {
                Alignment::End
            } else {
                Alignment::Start
            });

        let mut bubble_col = Column::new()
            .spacing(if self.app_settings.compact_mode { 0 } else { 2 })
            .push(sender_info_wrap);

        let (events, links) = if self.app_settings.render_markdown {
            (&item.markdown, &item.markdown_links)
        } else {
            (&item.plain_text, &item.plain_links)
        };
        bubble_col = bubble_col.push(self.view_message_text(events, links));

        let bubble = container(bubble_col)
            .style(move |theme: &cosmic::Theme| {
                use cosmic::iced::widget::container::Catalog;
                let cosmic = theme.cosmic();
                let mut style = theme.style(&cosmic::theme::Container::Card);
                if is_me {
                    style.border.color = cosmic.accent.base.into();
                    style.border.width = 1.0;
                }
                style
            })
            .padding(if self.app_settings.compact_mode {
                5
            } else {
                10
            })
            .max_width(600);

        let bubble_wrap = container(bubble)
            .width(cosmic::iced::Length::Fill)
            .align_x(if is_me {
                Alignment::End
            } else {
                Alignment::Start
            });

        bubble_wrap.into()
    }

    fn view_thread_summary(
        &'chat self,
        item: &crate::ConstellationsItem,
        event: &matrix_sdk_ui::timeline::EventTimelineItem,
        thread_counts: &std::collections::HashMap<matrix_sdk::ruma::OwnedEventId, u32>,
    ) -> cosmic::widget::Container<'chat, Message, cosmic::Theme> {
        let has_thread_root = item.thread_root_id.is_some();

        let mut num_replies = event
            .content()
            .thread_summary()
            .map(|s| s.num_replies)
            .unwrap_or_default();

        // Manual count if summary is missing or incomplete (due to view-side filtering)
        if let Some(event_id) = event.event_id() {
            let manual_count = thread_counts.get(event_id).copied().unwrap_or(0);
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
                && let TimelineDetails::Ready(latest_ev) = &summary.latest_event
            {
                // Try to find in timeline_items for better profile info
                if let Some(item) = self.timeline_items.iter().rfind(|i| {
                    i.item
                        .as_ref()
                        .and_then(|timeline_item| timeline_item.as_event())
                        .and_then(|e| e.event_id())
                        .map(|id| match &latest_ev.identifier {
                            TimelineEventItemId::EventId(eid) => id == eid,
                            _ => false,
                        })
                        .unwrap_or(false)
                }) {
                    latest_sender = Some(item.sender_name.as_str());
                    if let Some(timeline_item) = &item.item
                        && let Some(ev) = timeline_item.as_event()
                        && let Some(msg) = ev.content().as_message()
                    {
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
            if latest_body.is_none()
                && fallback_body_buf.is_empty()
                && let Some(event_id) = event.event_id()
                && let Some(item) = self.timeline_items.iter().rfind(|i| {
                    i.item
                        .as_ref()
                        .and_then(|timeline_item| timeline_item.as_event())
                        .and_then(|_e| i.thread_root_id.clone())
                        .map(|r| r == event_id)
                        .unwrap_or(false)
                })
            {
                latest_sender = Some(item.sender_name.as_str());
                if let Some(timeline_item) = &item.item
                    && let Some(ev) = timeline_item.as_event()
                    && let Some(msg) = ev.content().as_message()
                {
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
                            REPLY.as_str()
                        } else {
                            REPLIES.as_str()
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
                let unknown_sender = crate::fl!("unknown-sender");
                let sender = latest_sender.unwrap_or({
                    if !fallback_sender_buf.is_empty() {
                        fallback_sender_buf.as_str()
                    } else {
                        unknown_sender.as_str()
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
                        if char_indices.nth(2).is_some() {
                            // 30th char
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

            let summary_btn = button::custom(container(summary_row).padding([0, 5])).on_press({
                let fallback_id;
                let id_to_use = if let Some(id) = item.item_id.as_ref() {
                    id
                } else {
                    fallback_id = event.identifier();
                    &fallback_id
                };
                match id_to_use {
                    TimelineEventItemId::EventId(id) => Message::OpenThread(id.to_owned()),
                    _ => Message::NoOp,
                }
            });

            container(summary_btn).padding([5, 0])
        } else {
            container(text(""))
        }
    }

    #[rust_analyzer::skip]
    pub fn view_main_content(&self) -> Element<'_, Message> {
        let mut content = Column::new()
            .spacing(20)
            .padding(20)
            .width(cosmic::iced::Length::Fill)
            .height(cosmic::iced::Length::Fill);

        if let Some(room_id) = &self.selected_room {
            let fallback = crate::fl!("room-fallback");
            let room_name = self.get_room_name(room_id).unwrap_or(&fallback);

            // ⚡ Bolt Optimization: Avoid parsing UserId per frame
            let is_in_call = self.user_id.as_ref().is_some_and(|uid| {
                self.call_participants
                    .get(room_id)
                    .is_some_and(|p| p.iter().any(|participant| participant.as_str() == uid))
            });

            let call_participants = self.call_participants.get(room_id);
            let participant_count = call_participants.map_or(0, |p| p.len());

            let mut room_header = Row::new()
                .spacing(10)
                .align_y(Alignment::Center)
                .push(view_room_name_menu(room_name));

            if participant_count > 0 {
                room_header = room_header.push(
                    container(
                        Row::new()
                            .spacing(5)
                            .align_y(Alignment::Center)
                            .push(cosmic::widget::icon::from_name("camera-video-symbolic").size(16))
                            .push(text::body(format!("{participant_count}")).size(12)),
                    )
                    .padding([2, 5]),
                );
            }

            let call_button: Element<'_, Message> = if is_in_call {
                tooltip(
                    button::custom(cosmic::widget::icon::from_name("call-stop"))
                        .class(cosmic::theme::Button::Destructive)
                        .on_press(Message::LeaveCall),
                    text::body(crate::fl!("call-leave")),
                    Position::Bottom,
                )
                .into()
            } else {
                button::icon(Named::new("camera-web"))
                    .on_press(Message::JoinCall)
                    .tooltip(crate::fl!("call-join"))
                    .into()
            };

            room_header = room_header
                .push(cosmic::widget::space().width(cosmic::iced::Length::Fill))
                .push(call_button)
                .push({
                    let count = self.pinned_events.len();
                    if count > 0 {
                        let btn_content = Row::new()
                            .spacing(6)
                            .align_y(Alignment::Center)
                            .push(cosmic::widget::icon::from_name("pin-symbolic").size(16))
                            .push(text::body(count.to_string()).size(12));
                        Element::from(tooltip(
                            button::custom(btn_content)
                                .selected(
                                    self.current_settings_panel
                                        == Some(crate::SettingsPanel::Pinned),
                                )
                                .on_press(Message::TogglePinnedPanel),
                            text::body(crate::fl!("pinned-messages")),
                            Position::Bottom,
                        ))
                    } else {
                        Element::from(
                            button::icon(Named::new("pin-symbolic"))
                                .tooltip(crate::fl!("pinned-messages"))
                                .selected(
                                    self.current_settings_panel
                                        == Some(crate::SettingsPanel::Pinned),
                                )
                                .on_press(Message::TogglePinnedPanel),
                        )
                    }
                })
                .push(
                    button::icon(Named::new("system-users-symbolic"))
                        .tooltip(crate::fl!("room-members"))
                        .selected(
                            self.current_settings_panel == Some(crate::SettingsPanel::Members),
                        )
                        .on_press(Message::ToggleMembersPanel),
                )
                .push(
                    button::icon(Named::new("link-symbolic"))
                        .tooltip(TOOLTIP_COPY_ROOM_LINK.as_str())
                        .on_press(Message::CopyRoomLink(room_id.clone())),
                );

            if self.active_thread_root.is_some() {
                content = content.push(self.view_threaded_timeline());
            } else {
                content = content.push(room_header);
                if self.inviting_to_room {
                    let mut invite_input = text_input("@user:example.com", &self.invite_to_room_id)
                        .on_input(Message::InviteToRoomIdChanged);
                    let is_empty = self.invite_to_room_id.trim().is_empty();
                    let mut invite_btn = button::text(crate::fl!("invite"));
                    if !is_empty {
                        invite_input = invite_input.on_submit(|_| Message::InviteToRoom);
                        invite_btn = invite_btn.on_press(Message::InviteToRoom);
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
                        Row::new().spacing(5).push(invite_btn_widget).push(
                            button::text(crate::fl!("cancel"))
                                .on_press(Message::ToggleInviteToRoom),
                        ),
                    );
                    content = content.push(container(invite_ui).padding(5));
                }

                let selected_room_data = self
                    .selected_room
                    .as_ref()
                    .and_then(|id| self.room_list.iter().find(|r| r.id.as_ref() == id.as_ref()));

                let is_video_room = selected_room_data
                    .map(|r| {
                        r.room_type
                            .as_ref()
                            .is_some_and(|t| t.to_string() == "org.matrix.msc3401.call.room")
                    })
                    .unwrap_or(false);

                let mut chat_area = Column::new()
                    .spacing(10)
                    .width(cosmic::iced::Length::Fill)
                    .height(cosmic::iced::Length::Fill);
                // When viewing an event-focused (permalink context) timeline,
                // show a persistent banner offering to return to live.
                if self.active_event_focus.is_some() {
                    chat_area = chat_area.push(self.view_older_messages_banner());
                }
                chat_area = chat_area.push(self.view_timeline());
                if !is_video_room {
                    chat_area = chat_area.push(self.view_composer());
                }
                content = content.push(chat_area);
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

        content.into()
    }

    pub fn view_composer(&self) -> Element<'_, Message> {
        let mut content = Column::new().spacing(10);

        if self.is_composer_emoji_picker_active {
            content = content.push(self.view_emoji_picker(None));
        }

        if let Some(replying_to) = &self.replying_to {
            let body = replying_to.body_text();
            let snippet = truncate_snippet(body);
            let reply_bar = view_reply_bar(snippet, replying_to);
            content = content.push(container(reply_bar).padding(10));
        }

        if let Some(editing_item) = &self.editing_item {
            let body = editing_item.body_text();
            content = content.push(Self::view_editing_bar(body));
        }

        let composer = self.view_composer_editor();
        let attachments_view = self.view_composer_attachments();
        let controls = self.view_composer_controls();

        let composer_card = container(Column::new().spacing(5).push(composer).push(controls))
            .style(|theme: &cosmic::Theme| {
                use cosmic::iced::widget::container::Catalog;
                theme.style(&cosmic::theme::Container::Card)
            })
            .padding(10);

        content.push(attachments_view).push(composer_card).into()
    }


    fn view_editing_bar(body: String) -> Element<'static, Message> {
        let snippet = truncate_snippet(body);

        let edit_bar = Row::new()
            .spacing(10)
            .align_y(Alignment::Center)
            .push(text::body(crate::fl!("editing")).size(12))
            .push(text::body(snippet).size(12))
            .push(cosmic::widget::space().width(cosmic::iced::Length::Fill))
            .push(tooltip(
                button::icon(cosmic::widget::icon::from_name("window-close-symbolic"))
                    .on_press(Message::CancelEdit),
                text::body(crate::fl!("cancel")),
                Position::Bottom,
            ));

        container(edit_bar).padding(10).into()
    }

    fn view_composer_editor(&self) -> Element<'_, Message> {
        if self.composer_is_preview {
            self.view_preview()
        } else {
            container(
                text_editor(&self.composer_content)
                    .placeholder(crate::fl!("type-message"))
                    .on_action(Message::ComposerAction)
                    .key_binding(|keypress| match keypress.key.as_ref() {
                        cosmic::iced::keyboard::Key::Named(
                            cosmic::iced::keyboard::key::Named::Enter,
                        ) => {
                            if keypress.modifiers.shift() {
                                Some(cosmic::widget::text_editor::Binding::Enter)
                            } else {
                                Some(cosmic::widget::text_editor::Binding::Custom(
                                    Message::SendMessage,
                                ))
                            }
                        }
                        _ => cosmic::widget::text_editor::Binding::from_key_press(keypress),
                    })
                    .height(80),
            )
            .padding(0)
            .into()
        }
    }

    fn view_composer_attachments(&self) -> Element<'_, Message> {
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
        attachments_view.into()
    }

    fn view_composer_controls(&self) -> Element<'_, Message> {
        let is_empty =
            self.composer_content.text().trim().is_empty() && self.composer_attachments.is_empty();

        let mut send_btn = button::icon(if self.editing_item.is_some() {
            Named::new("mail-send-symbolic")
        } else if self.active_thread_root.is_some() {
            Named::new("mail-reply-all-symbolic")
        } else {
            Named::new("mail-send-symbolic")
        });
        if !is_empty {
            send_btn = send_btn
                .on_press(Message::SendMessage)
                .tooltip(crate::fl!("tooltip-send"))
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

        Row::new()
            .spacing(10)
            .push(
                button::icon(Named::new("mail-attachment-symbolic"))
                    .on_press(Message::AddAttachment)
                    .tooltip(TOOLTIP_ATTACH.as_str()),
            )
            .push(
                button::icon(Named::new("face-smile-symbolic"))
                    .on_press(Message::ToggleComposerEmojiPicker)
                    .tooltip(TOOLTIP_EMOJIS.as_str()),
            )
            .push(
                button::icon(Named::new("mark-location-symbolic"))
                    .on_press(Message::ShareLocation)
                    .tooltip(TOOLTIP_LOCATION.as_str()),
            )
            .push(if self.composer_is_preview {
                button::icon(Named::new("edit-symbolic"))
                    .on_press(Message::TogglePreview)
                    .tooltip(TOOLTIP_EDIT.as_str())
            } else {
                button::icon(Named::new("edit-find-symbolic"))
                    .on_press(Message::TogglePreview)
                    .tooltip(TOOLTIP_FIND.as_str())
            })
            .push(cosmic::widget::space().width(cosmic::iced::Length::Fill))
            .push(send_btn_widget)
            .into()
    }

    pub fn view_search_results(&self) -> Element<'_, Message> {
        let mut results_col = Column::new().spacing(15).width(cosmic::iced::Length::Fill);

        // Section: Messages in this Room (server-side full-text search).
        // Only shown when a room is selected — without one there is no room to
        // search and `message_search_results` stays empty.
        if self.selected_room.is_some() {
            results_col =
                results_col.push(text::title3(crate::fl!("search-messages-in-room")).size(14));

            if self.is_searching_messages {
                results_col = results_col.push(
                    container(cosmic::widget::progress_bar::indeterminate_circular().size(24.0))
                        .width(cosmic::iced::Length::Fill)
                        .align_x(Alignment::Center)
                        .padding(20),
                );
            } else if !self.message_search_results.is_empty() {
                let mut message_list = Column::new().spacing(10).width(cosmic::iced::Length::Fill);
                for result in &self.message_search_results {
                    let event_id = result.event_id.clone();

                    let mut card_content = Column::new().spacing(5);
                    card_content = card_content.push(
                        Row::new()
                            .spacing(8)
                            .align_y(Alignment::Center)
                            .push(
                                text::body(result.sender_id.as_str())
                                    .font(cosmic::iced::Font {
                                        weight: cosmic::iced::font::Weight::Bold,
                                        ..Default::default()
                                    })
                                    .size(13),
                            )
                            .push(text::body(result.timestamp.as_str()).size(10)),
                    );
                    card_content = card_content
                        .push(self.view_message_text(&result.plain_text, &result.links));

                    let event_id_for_jump = event_id.clone();
                    card_content =
                        card_content.push(
                            Row::new()
                                .push(cosmic::widget::space().width(cosmic::iced::Length::Fill))
                                .push(button::text(crate::fl!("jump-to-message")).on_press(
                                    Message::JumpToMessageOrLoadContext(event_id_for_jump),
                                )),
                        );

                    message_list = message_list.push(
                        container(card_content)
                            .style(|theme: &cosmic::Theme| {
                                use cosmic::iced::widget::container::Catalog;
                                let cosmic = theme.cosmic();
                                let mut style = theme.style(&cosmic::theme::Container::Card);
                                style.border.color = cosmic.accent.base.into();
                                style.border.width = 1.0;
                                style
                            })
                            .padding(10)
                            .width(cosmic::iced::Length::Fill),
                    );
                }
                results_col = results_col.push(message_list);
            } else {
                results_col = results_col.push(
                    container(
                        Column::new()
                            .spacing(10)
                            .align_x(Alignment::Center)
                            .push(cosmic::widget::icon::from_name("edit-find-symbolic").size(32))
                            .push(text::body(crate::fl!("search-no-room-matches")).size(14)),
                    )
                    .width(cosmic::iced::Length::Fill)
                    .align_x(Alignment::Center)
                    .padding(20),
                );
            }
        }

        results_col = results_col.push(divider::horizontal::default());

        // Section: Public Spaces & Rooms
        results_col = results_col.push(text::title3(crate::fl!("public-rooms-spaces")).size(14));

        if self.is_searching_public {
            results_col = results_col.push(
                container(cosmic::widget::progress_bar::indeterminate_circular().size(24.0))
                    .width(cosmic::iced::Length::Fill)
                    .align_x(Alignment::Center)
                    .padding(20),
            );
        } else if self.public_search_results.is_empty() {
            results_col = results_col.push(
                container(text::body(crate::fl!("no-public-rooms")).size(14))
                    .width(cosmic::iced::Length::Fill)
                    .align_x(Alignment::Center)
                    .padding(20),
            );
        } else {
            let mut public_list = Column::new().spacing(10).width(cosmic::iced::Length::Fill);
            for room in &self.public_search_results {
                let name = room
                    .name
                    .as_deref()
                    .or(room.canonical_alias.as_deref())
                    .map(str::to_string)
                    .unwrap_or_else(|| crate::fl!("unnamed-room"));
                let is_joined = self.joined_room_ids.contains(room.id.as_str());

                // Avatar
                let default_avatar = || {
                    let initials: String = name.chars().next().unwrap_or('R').to_string();
                    container(text::body(initials).size(14))
                        .width(40)
                        .height(40)
                        .align_x(Alignment::Center)
                        .align_y(Alignment::Center)
                        .style(|theme: &cosmic::Theme| {
                            use cosmic::iced::widget::container::Catalog;
                            let cosmic = theme.cosmic();
                            let mut style = theme.style(&cosmic::theme::Container::Card);
                            style.border.radius = cosmic.corner_radii.radius_xs.into();
                            style
                        })
                };
                let avatar_widget: Element<'_, Message> = if let Some(url) = &room.avatar_url {
                    if let Some(handle) = self.media_cache.get(url) {
                        cosmic::widget::image(handle.clone())
                            .width(40)
                            .height(40)
                            .border_radius(4.0)
                            .into()
                    } else {
                        default_avatar().into()
                    }
                } else {
                    default_avatar().into()
                };

                let mut details_col = Column::new().spacing(4).push(
                    Row::new()
                        .spacing(8)
                        .align_y(Alignment::Center)
                        .push(text::body(name.to_string()).font(cosmic::iced::Font {
                            weight: cosmic::iced::font::Weight::Bold,
                            ..Default::default()
                        }))
                        .push(
                            text::body(crate::fl!(
                                "room-member-count",
                                count = room.num_joined_members
                            ))
                            .size(10),
                        ),
                );
                if let Some(topic) = &room.topic {
                    details_col = details_col.push(text::body(topic.to_string()).size(11));
                }
                details_col = details_col.push(text::body(room.id.to_string()).size(9));

                let action_btn = if is_joined {
                    button::text(crate::fl!("joined-button"))
                } else {
                    let id_arc = std::sync::Arc::from(room.id.as_str());
                    button::suggested(crate::fl!("join")).on_press(Message::JoinRoom(id_arc))
                };

                let card_row = Row::new()
                    .spacing(15)
                    .align_y(Alignment::Center)
                    .push(avatar_widget)
                    .push(details_col.width(cosmic::iced::Length::Fill))
                    .push(action_btn);

                public_list = public_list.push(
                    container(card_row)
                        .style(|theme: &cosmic::Theme| {
                            use cosmic::iced::widget::container::Catalog;
                            theme.style(&cosmic::theme::Container::Card)
                        })
                        .padding(10)
                        .width(cosmic::iced::Length::Fill),
                );
            }
            results_col = results_col.push(public_list);
        }

        scrollable(results_col)
            .id(crate::TIMELINE_ID.clone())
            .height(cosmic::iced::Length::Fill)
            .into()
    }

    pub fn view_video_room<'a>(
        &'a self,
        room_data: &'a crate::matrix::RoomData,
    ) -> Element<'a, Message> {
        let room_id = room_data.id.as_ref();
        let is_in_call = self.user_id.as_ref().is_some_and(|uid| {
            self.call_participants
                .get(room_id)
                .is_some_and(|p| p.iter().any(|participant| participant.as_str() == uid))
        });

        let mut content = Column::new()
            .spacing(20)
            .align_x(Alignment::Center)
            .width(cosmic::iced::Length::Fill);

        // Icon
        content = content.push(cosmic::widget::icon::from_name("camera-video-symbolic").size(96));

        // Room Name
        let room_name = room_data
            .name
            .as_deref()
            .map(str::to_string)
            .unwrap_or_else(|| crate::fl!("unnamed-video-room"));
        content = content.push(text::title1(room_name).size(24));

        // Call status
        if is_in_call {
            content = content.push(text::body(crate::fl!("call-status-connected")).size(16));
            let leave_btn =
                button::destructive(crate::fl!("call-leave")).on_press(Message::LeaveCall);
            content = content.push(leave_btn);
        } else {
            content = content.push(text::body(crate::fl!("call-status-not-connected")).size(16));
            let join_btn = button::suggested(crate::fl!("call-join")).on_press(Message::JoinCall);
            content = content.push(join_btn);
        }

        // Participants list
        let call_participants = self.call_participants.get(room_id);
        let participant_count = call_participants.map_or(0, |p| p.len());

        let mut participants_col = Column::new().spacing(10).align_x(Alignment::Center);
        participants_col = participants_col
            .push(text::title3(crate::fl!("participants", count = participant_count)).size(16));

        if let Some(participants) = call_participants
            && !participants.is_empty()
        {
            for participant in participants {
                participants_col = participants_col.push(text::body(participant.as_str()).size(14));
            }
        } else {
            participants_col =
                participants_col.push(text::body(crate::fl!("no-participants")).size(14));
        }

        content = content.push(
            container(participants_col)
                .style(|theme: &cosmic::Theme| {
                    use cosmic::iced::widget::container::Catalog;
                    theme.style(&cosmic::theme::Container::Card)
                })
                .padding(15)
                .width(300.0),
        );

        container(content)
            .width(cosmic::iced::Length::Fill)
            .height(cosmic::iced::Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
    }

    pub fn view_members_panel(&self) -> Element<'_, Message> {
        if self.is_loading_members {
            container(cosmic::widget::progress_bar::indeterminate_circular().size(24.0))
                .width(cosmic::iced::Length::Fill)
                .height(cosmic::iced::Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .into()
        } else {
            let mut member_list = Column::new().spacing(5);
            for member in &self.room_members {
                let avatar = if let Some(avatar_url) = &member.avatar_url
                    && let Some(handle) = self.media_cache.get(avatar_url)
                {
                    Element::from(cosmic::widget::image(handle.clone()).width(24).height(24))
                } else {
                    container(cosmic::widget::icon::from_name("avatar-default-symbolic").size(16))
                        .padding(4)
                        .into()
                };

                let name_text = member.display_name.as_deref().unwrap_or(&member.user_id);
                let mut member_info_col = Column::new().spacing(2);
                member_info_col = member_info_col.push(text::body(name_text).size(14));
                if member.display_name.is_some() {
                    member_info_col = member_info_col.push(text::body(&member.user_id).size(10));
                }

                let member_row = Row::new()
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .push(avatar)
                    .push(member_info_col);

                let member_container = container(member_row)
                    .padding(8)
                    .width(cosmic::iced::Length::Fill);

                member_list = member_list.push(member_container);
            }

            scrollable(member_list)
                .height(cosmic::iced::Length::Fill)
                .into()
        }
    }

    pub fn view_pinned_panel(&self) -> Element<'_, Message> {
        if self.is_loading_pinned {
            container(cosmic::widget::progress_bar::indeterminate_circular().size(24.0))
                .width(cosmic::iced::Length::Fill)
                .height(cosmic::iced::Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .into()
        } else {
            let mut pinned_list = Column::new().spacing(10);
            let mut found_any = false;

            for item in &self.pinned_events_details {
                found_any = true;

                let avatar = if let Some(avatar_url) = &item.avatar_url
                    && let Some(handle) = self.media_cache.get(avatar_url)
                {
                    Element::from(cosmic::widget::image(handle.clone()).width(20).height(20))
                } else {
                    container(cosmic::widget::icon::from_name("avatar-default-symbolic").size(12))
                        .padding(2)
                        .into()
                };

                let message_row = Row::new()
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .push(avatar)
                    .push(
                        Column::new()
                            .spacing(2)
                            .push(
                                Row::new()
                                    .spacing(5)
                                    .align_y(Alignment::Center)
                                    .push(text::title3(&item.sender_name).size(12))
                                    .push(text::body(&item.timestamp).size(10)),
                            )
                            .push(text::body(&item.body).size(12)),
                    );

                // Card is a jump-to-message link, with a sibling unpin button
                // (siblings, not nested, to keep iced happy).
                let card_row = match matrix_sdk::ruma::EventId::parse(&item.event_id) {
                    Ok(event_id) => {
                        let jump_btn = button::custom(
                            container(message_row)
                                .padding(5)
                                .width(cosmic::iced::Length::Fill),
                        )
                        .width(cosmic::iced::Length::Fill)
                        .class(cosmic::theme::Button::ListItem(
                            self.core.system_theme().cosmic().corner_radii.radius_m,
                        ))
                        .on_press(Message::JumpToMessage(event_id.clone()));

                        let unpin_btn = tooltip(
                            button::icon(cosmic::widget::icon::from_name("pin-symbolic"))
                                .on_press(Message::UnpinMessage(event_id)),
                            text::body(crate::fl!("unpin-message")),
                            Position::Bottom,
                        );

                        Row::new()
                            .spacing(8)
                            .align_y(Alignment::Center)
                            .push(jump_btn)
                            .push(unpin_btn)
                    }
                    Err(_) => Row::new().push(message_row),
                };

                let card = container(card_row)
                    .style(move |theme: &cosmic::Theme| {
                        use cosmic::iced::widget::container::Catalog;
                        theme.style(&cosmic::theme::Container::Card)
                    })
                    .width(cosmic::iced::Length::Fill);

                pinned_list = pinned_list.push(card);
            }

            if !found_any {
                container(
                    Column::new()
                        .spacing(10)
                        .align_x(Alignment::Center)
                        .push(cosmic::widget::icon::from_name("pin-symbolic").size(48))
                        .push(text::body(crate::fl!("no-pinned-messages")).size(14)),
                )
                .width(cosmic::iced::Length::Fill)
                .padding(20)
                .align_x(Alignment::Center)
                .into()
            } else {
                scrollable(pinned_list)
                    .height(cosmic::iced::Length::Fill)
                    .into()
            }
        }
    }
}

#[rust_analyzer::skip]
fn view_reply_bar<'a>(
    snippet: impl Into<std::borrow::Cow<'a, str>> + 'a,
    replying_to: &'a crate::ConstellationsItem,
) -> Row<'a, Message, Theme> {
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
        .push(tooltip(
            button::icon(cosmic::widget::icon::from_name("window-close-symbolic"))
                .on_press(Message::CancelReply),
            text::body(crate::fl!("cancel")),
            Position::Top,
        ))
}

#[cfg(test)]
mod tests {
    use crate::constellations::Constellations;

    #[test]
    fn test_view_timeline_renders_without_panicking() {
        let constellations = Constellations::mock();
        let _element = constellations.view_timeline();
    }

    #[test]
    fn test_view_threaded_timeline_renders_without_panicking() {
        let constellations = Constellations::mock();
        let _element = constellations.view_threaded_timeline();
    }

    #[test]
    fn test_view_main_content_renders_without_panicking() {
        let constellations = Constellations::mock();
        let _element = constellations.view_main_content();
    }

    #[test]
    fn test_view_composer_renders_without_panicking() {
        let constellations = Constellations::mock();
        let _element = constellations.view_composer();
    }

    #[test]
    fn test_view_search_results_renders_without_panicking() {
        let constellations = Constellations::mock();
        let _element = constellations.view_search_results();
    }

    #[test]
    fn test_view_members_panel_renders_without_panicking() {
        let constellations = Constellations::mock();
        let _element = constellations.view_members_panel();
    }

    #[test]
    fn test_view_pinned_panel_renders_without_panicking() {
        let constellations = Constellations::mock();
        let _element = constellations.view_pinned_panel();
    }
}


fn truncate_snippet(body: String) -> std::borrow::Cow<'static, str> {
    let mut char_indices = body.char_indices();
    if let Some((idx_97, _)) = char_indices.nth(97) {
        if char_indices.nth(2).is_some() {
            let mut s = String::with_capacity(100);
            s.push_str(&body[..idx_97]);
            s.push_str("...");
            std::borrow::Cow::Owned(s)
        } else {
            std::borrow::Cow::Owned(body)
        }
    } else {
        std::borrow::Cow::Owned(body)
    }
}
