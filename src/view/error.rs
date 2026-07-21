use crate::Message;
use crate::utils::widget::tooltip_button;
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
            .push(tooltip_button(
                button::icon(icon::from_name("window-close-symbolic").symbolic(true))
                    .on_press(Message::DismissError),
                crate::fl!("dismiss"),
            )),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_view_error_renders_without_panicking_with_str() {
        // Smoke test for static str
        let _element = view_error("Test Error");
    }

    #[test]
    fn test_view_error_renders_without_panicking_with_string() {
        // Smoke test for owned String
        let _element = view_error(String::from("Another Test Error"));
    }

    #[test]
    fn test_view_error_renders_without_panicking_with_empty_string() {
        // Smoke test for empty string
        let _element = view_error("");
    }

    #[test]
    fn test_view_error_renders_without_panicking_with_long_string() {
        // Smoke test for long string
        let long_string = "a".repeat(1000);
        let _element = view_error(long_string);
    }
}
