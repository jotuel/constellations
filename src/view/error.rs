use crate::Message;
use cosmic::Element;
use cosmic::iced::Alignment;
use cosmic::widget::{Row, button, container, icon, text};

pub fn view_error(error: impl Into<String>) -> Element<'static, Message> {
    let error_card = container(
        Row::new()
            .spacing(12)
            .align_y(Alignment::Center)
            .push(
                icon::from_name("dialog-error-symbolic")
                    .symbolic(true)
                    .size(20),
            )
            .push(text::body(error.into()))
            .push(
                button::icon(icon::from_name("window-close-symbolic").symbolic(true))
                    .tooltip(crate::fl!("dismiss"))
                    .on_press(Message::DismissError),
            ),
    )
    .style(|theme: &cosmic::Theme| {
        use cosmic::iced::widget::container::Catalog;
        let cosmic = theme.cosmic();
        let mut style = theme.style(&cosmic::theme::Container::Dialog);
        style.border.color = cosmic.destructive.base.into();
        style.border.width = 1.0;
        style
    })
    .padding(16)
    .max_width(500);

    container(error_card)
        .width(cosmic::iced::Length::Fill)
        .height(cosmic::iced::Length::Fill)
        .padding(20)
        .align_x(Alignment::Center)
        .align_y(Alignment::Start)
        .into()
}
