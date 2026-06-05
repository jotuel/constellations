use cosmic::Element;
use cosmic::iced::Alignment;
use cosmic::iced::widget::image;
use cosmic::widget::{Column, Row, Widget, button, container, divider, text};

use crate::{CONSTELLATIONS_ICON, Constellations, Message, matrix};

impl Constellations {
    pub fn view_app(&self) -> Element<'_, Message> {
        if self.is_initializing {
            let content = Column::new()
                .push(
                    cosmic::widget::svg(cosmic::widget::svg::Handle::from_memory(
                        CONSTELLATIONS_ICON,
                    ))
                    .width(cosmic::iced::Length::Fixed(128.0))
                    .height(cosmic::iced::Length::Fixed(128.0)),
                )
                .push(cosmic::widget::progress_bar::indeterminate_circular())
                .spacing(32)
                .align_x(Alignment::Center);

            return container(content)
                .width(cosmic::iced::Length::Fill)
                .height(cosmic::iced::Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .into();
        }

        if self.user_id.is_none() {
            return self.view_login();
        }

        let sidebar = self.view_sidebar();
        let content = self.view_main_content();

        let main_view = Row::new()
            .push(self.view_space_switcher())
            .push(divider::vertical::default())
            .push(sidebar)
            .push(divider::vertical::default())
            .push(content);

        let mut final_view: Element<'_, Message> = main_view.into();
        if self.app_settings.show_sync_indicator && self.is_sync_indicator_active {
            let sync_widget: Element<'_, Message> = match self.sync_status {
                matrix::SyncStatus::Syncing => {
                    container(cosmic::widget::progress_bar::indeterminate_circular().size(24.0))
                        .into()
                }
                matrix::SyncStatus::Connected => {
                    container(cosmic::widget::icon::from_name("network-idle-symbolic").size(24))
                        .into()
                }
                matrix::SyncStatus::Disconnected => {
                    container(cosmic::widget::icon::from_name("network-offline-symbolic").size(24))
                        .into()
                }
                matrix::SyncStatus::Error(_) | matrix::SyncStatus::MissingSlidingSyncSupport => {
                    container(cosmic::widget::icon::from_name("network-error-symbolic").size(24))
                        .into()
                }
            };

            let sync_overlay = container(sync_widget)
                .padding(20)
                .width(cosmic::iced::Length::Fill)
                .height(cosmic::iced::Length::Fill)
                .align_x(Alignment::End)
                .align_y(Alignment::End);

            final_view = cosmic::iced::widget::stack![final_view, sync_overlay].into();
        }

        if let Some(handle) = &self.fullscreen_image {
            let image: image::Image<'_> = cosmic::widget::image(handle.clone())
                .width(cosmic::iced::Length::Fill)
                .height(cosmic::iced::Length::Fill)
                .content_fit(cosmic::iced::ContentFit::Contain);
            let image_viewer = container(image)
                .width(cosmic::iced::Length::Fill)
                .height(cosmic::iced::Length::Fill)
                .padding(40)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center);

            let close_button = container(cosmic::widget::tooltip(
                button::icon(cosmic::widget::icon::from_name("window-close-symbolic"))
                    .on_press(Message::CloseImage),
                text::body(crate::fl!("close-image")),
                cosmic::widget::tooltip::Position::Bottom,
            ))
            .width(cosmic::iced::Length::Fill)
            .height(cosmic::iced::Length::Fill)
            .padding(10)
            .align_right(image_viewer.size_hint().width)
            .align_top(image_viewer.size_hint().height);

            // Overlay that closes on click
            let dismiss_overlay = button::custom(
                container(cosmic::iced::widget::Space::new())
                    .width(cosmic::iced::Length::Fill)
                    .height(cosmic::iced::Length::Fill),
            )
            .on_press(Message::CloseImage)
            .padding(0);

            final_view = cosmic::iced::widget::stack![
                final_view,
                dismiss_overlay,
                image_viewer,
                close_button
            ]
            .into();
        }

        let active_error = self.error.as_deref().or(match &self.sync_status {
            matrix::SyncStatus::Error(e) => Some(e.as_str()),
            matrix::SyncStatus::MissingSlidingSyncSupport => Some("Error: Your homeserver does not support Sliding Sync (MSC4186), which is required by Constellations."),
            _ => None,
        });

        if let Some(error) = active_error {
            let error_overlay = crate::view::error::view_error(error);
            final_view = cosmic::iced::widget::stack![final_view, error_overlay].into();
        }

        final_view
    }
}
