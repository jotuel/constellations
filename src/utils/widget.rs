//! Shared widget helpers.
//!
//! Centralises the recurring "button + tooltip" construction patterns so the
//! codebase doesn't mix the `tooltip(btn, text::body(..), Position::..)` free
//! function with the `button.tooltip(..)` builder method. Everything here routes
//! through [`cosmic::widget::tooltip`] so the `Container::Tooltip` style, padding,
//! and gap are applied consistently (importing the tooltip widget from
//! `cosmic::iced::widget::tooltip` instead silently skips them).

use std::borrow::Cow;

use cosmic::Element;
use cosmic::widget::{text, tooltip::Position};

/// Wrap any element in a tooltip at [`Position::Top`] (the iced default).
///
/// Replaces both `tooltip(content, text::body(tip), Position::Top)` and
/// `button.tooltip(tip)`.
pub fn tooltip_button<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    tip: impl Into<Cow<'a, str>> + 'a,
) -> Element<'a, Message> {
    tooltip_button_at(content, tip, Position::Top)
}

/// Wrap any element in a tooltip at the given [`Position`] (e.g. `Bottom`,
/// `Right`).
pub fn tooltip_button_at<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    tip: impl Into<Cow<'a, str>> + 'a,
    position: Position,
) -> Element<'a, Message> {
    cosmic::widget::tooltip(content, text::body(tip), position).into()
}

/// The "disabled button shows a hint tooltip, otherwise fires `on_press`" idiom.
///
/// Every existing call site of this shape used [`Position::Top`], so `Top` is
/// baked in rather than threaded through as a parameter. `on_press` is attached
/// only when `enabled`, so callers should pass a button that has not already
/// had `on_press` set.
pub fn disabled_or_tooltip<'a, Btn, Message>(
    btn: Btn,
    enabled: bool,
    on_press: Message,
    tip: impl Into<Cow<'a, str>> + 'a,
) -> Element<'a, Message>
where
    Btn: OnPress<Message> + Into<Element<'a, Message>>,
    Message: Clone + 'a,
{
    if enabled {
        btn.on_press(on_press).into()
    } else {
        tooltip_button(btn, tip)
    }
}

/// Adapter over the libcosmic button builders' inherent `on_press` method.
///
/// libcosmic models each button variant (`text`, `icon`, `custom`, …) as a
/// distinct `Builder<'a, Message, Variant>` type, so we can't name a single
/// `Button` type in a generic signature. Every variant shares the same
/// `on_press(self, Message) -> Self` inherent method though, so we lift it into
/// a tiny trait to use as a bound.
pub trait OnPress<Message> {
    fn on_press(self, on_press: Message) -> Self;
}

impl<Message, Variant> OnPress<Message> for cosmic::widget::button::Builder<'_, Message, Variant> {
    fn on_press(self, on_press: Message) -> Self {
        cosmic::widget::button::Builder::on_press(self, on_press)
    }
}
