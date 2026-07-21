use crate::matrix;
use crate::preview::parse_markdown;
use crate::{Constellations, Message};
use cosmic::{Action, Application, Task};
use futures::FutureExt;
use futures::stream::StreamExt;

impl Constellations {
    pub fn handle_send_message(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        if let (Some(matrix), Some(room_id)) = (&self.matrix, &self.selected_room) {
            let body = self.composer_content.text();

            if let Some(editing_item) = self.editing_item.clone() {
                if body.is_empty() {
                    return Task::none();
                }

                let html_body = if self.app_settings.render_markdown {
                    Some(matrix::markdown_to_html(&body))
                } else {
                    None
                };
                let matrix_clone = matrix.clone();
                let room_id_clone = room_id.clone();

                return Task::perform(
                    async move {
                        let event = editing_item
                            .item
                            .as_ref()
                            .and_then(|i| i.as_event())
                            .ok_or("Not an event")?;
                        let item_id = event.identifier();
                        matrix_clone
                            .edit_message(&room_id_clone, &item_id, body, html_body)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    |res| Action::from(Message::MessageEdited(res)),
                );
            }

            let attachments = std::mem::take(&mut self.composer_attachments);

            if body.is_empty() && attachments.is_empty() {
                return Task::none();
            }

            let mut tasks = Vec::new();

            if self.active_thread_root.is_some() {
                self.is_threaded_timeline_at_bottom = true;
            } else {
                self.is_timeline_at_bottom = true;
            }

            if !body.is_empty() {
                let html_body = if self.app_settings.render_markdown {
                    Some(matrix::markdown_to_html(&body))
                } else {
                    None
                };
                let matrix_clone = matrix.clone();
                let room_id_clone = room_id.clone();

                if let Some(replying_to) = self.replying_to.clone() {
                    tasks.push(Task::perform(
                        async move {
                            let event = replying_to
                                .item
                                .as_ref()
                                .and_then(|i| i.as_event())
                                .ok_or("Not an event")?;
                            let event_id = event.event_id().ok_or("No event ID")?;
                            let sender = event.sender();

                            matrix_clone
                                .send_reply(&room_id_clone, event_id, sender, body, html_body)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| Action::from(Message::MessageSent(res)),
                    ));
                } else if let Some(root_id) = self.active_thread_root.clone() {
                    let user_id = self.user_id.clone();
                    tasks.push(Task::perform(
                        async move {
                            matrix_clone
                                .send_threaded_message(
                                    &room_id_clone,
                                    &root_id,
                                    user_id.as_ref(),
                                    body,
                                    html_body,
                                )
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| Action::from(Message::MessageSent(res)),
                    ));
                } else {
                    tasks.push(Task::perform(
                        async move {
                            matrix_clone
                                .send_message(&room_id_clone, body, html_body)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| Action::from(Message::MessageSent(res)),
                    ));
                }
            } else {
                // If only sending attachments, we clear the composer text state manually
                // because MessageSent clears it but might not run for empty body
                self.composer_content = cosmic::widget::text_editor::Content::new();
                self.composer_preview_events.clear();
                self.composer_preview_links.clear();
                self.composer_is_preview = false;
                self.replying_to = None;
                self.editing_item = None;
            }

            for path in attachments {
                let matrix_clone = matrix.clone();
                let room_id_clone = room_id.clone();

                tasks.push(Task::perform(
                    async move {
                        let res = matrix_clone
                            .send_attachment(&room_id_clone, &path)
                            .await
                            .map_err(|e| e.to_string());
                        (path, res)
                    },
                    move |(path, res)| Action::from(Message::AttachmentSent(path, res)),
                ));
            }

            Task::batch(tasks)
        } else {
            Task::none()
        }
    }

    pub fn handle_redact_message(
        &mut self,
        item_id: matrix::TimelineEventItemId,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        if let (Some(matrix), Some(room_id)) = (&self.matrix, &self.selected_room) {
            let matrix = matrix.clone();
            let room_id = room_id.clone();
            Task::perform(
                async move {
                    matrix
                        .redact_message(&room_id, &item_id, None)
                        .await
                        .map_err(|e| e.to_string())
                },
                |res| Action::from(Message::MessageRedacted(res)),
            )
        } else {
            Task::none()
        }
    }

    pub fn handle_add_attachment(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        Task::perform(
            async move {
                let dialog = rfd::AsyncFileDialog::new()
                    .set_title("Select files to attach")
                    .pick_files()
                    .await;

                let mut paths = Vec::new();
                if let Some(files) = dialog {
                    for file in files {
                        paths.push(file.path().to_path_buf());
                    }
                }
                paths
            },
            |paths| Action::from(Message::AttachmentsSelected(paths)),
        )
    }

    pub fn handle_share_location(
        &mut self,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        Task::perform(
            async move {
                let proxy = ashpd::desktop::location::LocationProxy::new()
                    .await
                    .map_err(|e| e.to_string())?;

                let session = proxy
                    .create_session(ashpd::desktop::location::CreateSessionOptions::default())
                    .await
                    .map_err(|e| e.to_string())?;

                let mut stream = proxy
                    .receive_location_updated()
                    .await
                    .map_err(|e| e.to_string())?;

                let (_, location_res) = futures::join!(
                    proxy
                        .start(
                            &session,
                            None,
                            ashpd::desktop::location::StartOptions::default()
                        )
                        .map(|e| e.map_err(|err| err.to_string())),
                    stream
                        .next()
                        .map(|opt| opt.ok_or_else(|| "Stream is exhausted".to_string()))
                );

                let _ = session.close().await;

                let location = location_res?;
                Ok((location.latitude(), location.longitude()))
            },
            |res| Action::from(Message::LocationRetrieved(res)),
        )
    }

    pub fn handle_location_retrieved(
        &mut self,
        res: Result<(f64, f64), String>,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        match res {
            Ok((lat, lon)) => {
                if let (Some(matrix), Some(room_id)) = (&self.matrix, &self.selected_room) {
                    let matrix_clone = matrix.clone();
                    let room_id_clone = room_id.clone();
                    let body = crate::fl!(
                        "location-message-body",
                        lat = lat.to_string(),
                        lon = lon.to_string()
                    )
                    .to_string();
                    let geo_uri = format!("geo:{lat},{lon}");
                    Task::perform(
                        async move {
                            matrix_clone
                                .send_location(&room_id_clone, body, geo_uri)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        |res| Action::from(Message::MessageSent(res)),
                    )
                } else {
                    Task::none()
                }
            }
            Err(e) => {
                self.set_error(
                    crate::fl!("error-failed-get-location", error = e.to_string()).to_string(),
                );
                Task::none()
            }
        }
    }

    pub(super) fn handle_composer_changed(&mut self, text: String) -> Task<Action<Message>> {
        self.composer_preview_events = parse_markdown(&text, false);
        self.composer_preview_links = crate::preview::extract_links(&self.composer_preview_events);
        self.composer_content = cosmic::widget::text_editor::Content::with_text(&text);

        if self.app_settings.send_typing_notifications
            && let Some(matrix) = &self.matrix
            && let Some(room_id) = &self.selected_room
        {
            let matrix = matrix.clone();
            let room_id = room_id.clone();
            let typing = !self.composer_content.is_empty();
            return Task::perform(
                async move {
                    let _ = matrix.typing_notice(&room_id, typing).await;
                },
                |_| Action::from(Message::NoOp),
            );
        }

        Task::none()
    }

    pub(super) fn handle_composer_action(
        &mut self,
        action: cosmic::widget::text_editor::Action,
    ) -> Task<Action<Message>> {
        self.composer_content.perform(action);
        let text = self.composer_content.text();
        self.composer_preview_events = parse_markdown(&text, false);
        self.composer_preview_links = crate::preview::extract_links(&self.composer_preview_events);

        if self.app_settings.send_typing_notifications
            && let Some(matrix) = &self.matrix
            && let Some(room_id) = &self.selected_room
        {
            let matrix = matrix.clone();
            let room_id = room_id.clone();
            let typing = !self.composer_content.is_empty();
            return Task::perform(
                async move {
                    let _ = matrix.typing_notice(&room_id, typing).await;
                },
                |_| Action::from(Message::NoOp),
            );
        }

        Task::none()
    }

    pub(super) fn handle_start_reply(
        &mut self,
        item_id: matrix::TimelineEventItemId,
    ) -> Task<Action<Message>> {
        let mut found_item = None;
        for item in self
            .timeline_items
            .iter()
            .chain(self.threaded_timeline_items.iter())
        {
            if let Some(timeline_item) = &item.item
                && let Some(event) = timeline_item.as_event()
                && event.identifier() == item_id
            {
                found_item = Some(item.clone());
                break;
            }
        }
        self.replying_to = found_item;
        Task::none()
    }

    pub(super) fn handle_start_edit(
        &mut self,
        item_id: matrix::TimelineEventItemId,
    ) -> Task<Action<Message>> {
        let mut found_item = None;
        for item in self
            .timeline_items
            .iter()
            .chain(self.threaded_timeline_items.iter())
        {
            if let Some(timeline_item) = &item.item
                && let Some(event) = timeline_item.as_event()
                && event.identifier() == item_id
            {
                found_item = Some(item.clone());
                break;
            }
        }
        if let Some(item) = found_item
            && let Some(timeline_item) = &item.item
            && let Some(event) = timeline_item.as_event()
            && let Some(msg) = event.content().as_message()
        {
            self.composer_content = cosmic::widget::text_editor::Content::with_text(msg.body());
            self.composer_preview_events = parse_markdown(&self.composer_content.text(), false);
            self.composer_preview_links =
                crate::preview::extract_links(&self.composer_preview_events);
            self.editing_item = Some(item);
            self.replying_to = None;
        }
        Task::none()
    }
}
