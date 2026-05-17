use cosmic::widget::button;
use cosmic::widget::settings;
use cosmic::{Action, Element, Task};

#[derive(Debug, Clone, Default)]
pub struct State {
    pub show_sync_indicator: bool,
    pub send_typing_notifications: bool,
    pub render_markdown: bool,
    pub compact_mode: bool,
    pub hide_threaded_messages: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    ToggleSyncIndicator(bool),
    ToggleTypingNotifications(bool),
    ToggleMarkdown(bool),
    ToggleCompactMode(bool),
    ToggleHideThreadedMessages(bool),
    ClearCache,
}

impl State {
    pub fn from_config(config: &super::config::Config) -> Self {
        Self {
            show_sync_indicator: config.show_sync_indicator,
            send_typing_notifications: config.send_typing_notifications,
            render_markdown: config.render_markdown,
            compact_mode: config.compact_mode,
            hide_threaded_messages: config.hide_threaded_messages,
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Action<crate::Message>> {
        match message {
            Message::ToggleSyncIndicator(show) => {
                self.show_sync_indicator = show;
                Task::done(Action::from(crate::Message::AppSettingChanged))
            }
            Message::ToggleTypingNotifications(send) => {
                self.send_typing_notifications = send;
                Task::done(Action::from(crate::Message::AppSettingChanged))
            }
            Message::ToggleMarkdown(render) => {
                self.render_markdown = render;
                Task::done(Action::from(crate::Message::AppSettingChanged))
            }
            Message::ToggleCompactMode(compact) => {
                self.compact_mode = compact;
                Task::done(Action::from(crate::Message::AppSettingChanged))
            }
            Message::ToggleHideThreadedMessages(hide) => {
                self.hide_threaded_messages = hide;
                Task::done(Action::from(crate::Message::AppSettingChanged))
            }
            Message::ClearCache => Task::done(Action::from(crate::Message::AppSettings(
                Message::ClearCache,
            ))),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        settings::view_column(vec![
            settings::section()
                .title(crate::fl!("general-settings"))
                .add(settings::item(
                    crate::fl!("show-sync-indicator"),
                    cosmic::widget::toggler(self.show_sync_indicator)
                        .on_toggle(Message::ToggleSyncIndicator),
                ))
                .add(settings::item(
                    crate::fl!("send-typing-notifications"),
                    cosmic::widget::toggler(self.send_typing_notifications)
                        .on_toggle(Message::ToggleTypingNotifications),
                ))
                .add(settings::item(
                    crate::fl!("render-markdown"),
                    cosmic::widget::toggler(self.render_markdown)
                        .on_toggle(Message::ToggleMarkdown),
                ))
                .add(settings::item(
                    crate::fl!("compact-mode"),
                    cosmic::widget::toggler(self.compact_mode)
                        .on_toggle(Message::ToggleCompactMode),
                ))
                .add(settings::item(
                    crate::fl!("hide-threaded-messages"),
                    cosmic::widget::toggler(self.hide_threaded_messages)
                        .on_toggle(Message::ToggleHideThreadedMessages),
                ))
                .into(),
            settings::section()
                .title(crate::fl!("maintenance"))
                .add(settings::item(
                    crate::fl!("media-cache"),
                    button::text(crate::fl!("clear-cache")).on_press(Message::ClearCache),
                ))
                .into(),
        ])
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_toggle_sync_indicator() {
        let mut state = State::default();
        assert!(!state.show_sync_indicator);

        let _ = state.update(Message::ToggleSyncIndicator(true));
        assert!(state.show_sync_indicator);

        let _ = state.update(Message::ToggleSyncIndicator(false));
        assert!(!state.show_sync_indicator);
    }

    #[test]
    fn test_update_toggle_typing_notifications() {
        let mut state = State::default();
        assert!(!state.send_typing_notifications);

        let _ = state.update(Message::ToggleTypingNotifications(true));
        assert!(state.send_typing_notifications);

        let _ = state.update(Message::ToggleTypingNotifications(false));
        assert!(!state.send_typing_notifications);
    }

    #[test]
    fn test_update_toggle_markdown() {
        let mut state = State::default();
        assert!(!state.render_markdown);

        let _ = state.update(Message::ToggleMarkdown(true));
        assert!(state.render_markdown);

        let _ = state.update(Message::ToggleMarkdown(false));
        assert!(!state.render_markdown);
    }

    #[test]
    fn test_update_toggle_compact_mode() {
        let mut state = State::default();
        assert!(!state.compact_mode);

        let _ = state.update(Message::ToggleCompactMode(true));
        assert!(state.compact_mode);

        let _ = state.update(Message::ToggleCompactMode(false));
        assert!(!state.compact_mode);
    }

    #[test]
    fn test_update_clear_cache() {
        let mut state = State::default();
        let _ = state.update(Message::ClearCache);
        // State doesn't change for ClearCache, just returns a task
        assert!(!state.show_sync_indicator);
        assert!(!state.send_typing_notifications);
        assert!(!state.render_markdown);
        assert!(!state.compact_mode);
    }
}
