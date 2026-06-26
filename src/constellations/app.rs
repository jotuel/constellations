use super::{AuthFlow, Constellations, MenuAct, Message, SettingsPanel};
use crate::matrix;
use crate::settings;

use cosmic::iced::widget::tooltip;
use cosmic::iced::{Alignment, Subscription};
use cosmic::widget::icon::Named;
use cosmic::widget::tooltip::Position;
use cosmic::widget::{RcElementWrapper, Row, button, menu, text, text_input};
use cosmic::{Action, Application, Core, Element, Task};
use eyeball_im::Vector;
use std::collections::HashMap;
use url::Url;

impl Application for Constellations {
    type Executor = cosmic::executor::Default;
    type Message = Message;
    type Flags = Option<String>;
    const APP_ID: &'static str = "fi.joonastuomi.CosmicExtConstellations";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn header_start(&self) -> Vec<Element<'_, Self::Message>> {
        let mut start = Vec::new();

        if self.user_id.is_none() {
            return start;
        }

        if self.is_search_active {
            let search_btn =
                button::icon(Named::new("edit-find-symbolic")).on_press(Message::ToggleSearch);
            let search_tooltip = tooltip(
                search_btn,
                text::body(crate::fl!("close-search")),
                Position::Bottom,
            );
            let row = Row::new()
                .align_y(Alignment::Center)
                .push(search_tooltip)
                .push(
                    text_input(crate::fl!("search-placeholder"), &self.search_query)
                        .on_input(Message::SearchQueryChanged)
                        .width(200.0),
                );
            start.push(row.into());
        } else {
            let search_btn =
                button::icon(Named::new("edit-find-symbolic")).on_press(Message::ToggleSearch);
            let search_tooltip = tooltip(
                search_btn,
                text::body(crate::fl!("search")),
                Position::Bottom,
            );
            start.push(search_tooltip.into());
        }

        start
    }

    fn header_end(&self) -> Vec<Element<'_, Self::Message>> {
        let mut end = Vec::new();

        if self.user_id.is_some() {
            let user_btn = button::icon(Named::new("user-available-symbolic"));
            let user_tooltip = tooltip(
                user_btn,
                text::body(crate::fl!("user-menu")),
                Position::Bottom,
            );
            let key_binds = std::collections::HashMap::new();

            let menu_tree = menu::Tree::with_children(
                RcElementWrapper::new(Element::from(user_tooltip)),
                menu::items(
                    &key_binds,
                    vec![
                        menu::Item::Button(
                            crate::fl!("app-settings"),
                            Some(cosmic::widget::icon::Handle::from(
                                cosmic::widget::icon::Named::new("applications-system"),
                            )),
                            MenuAct::AppSettings,
                        ),
                        menu::Item::Button(
                            crate::fl!("user-settings"),
                            Some(cosmic::widget::icon::Handle::from(
                                cosmic::widget::icon::Named::new("preferences-system-and-accounts"),
                            )),
                            MenuAct::UserSettings,
                        ),
                        menu::Item::Button(
                            crate::fl!("logout"),
                            Some(cosmic::widget::icon::Handle::from(
                                cosmic::widget::icon::Named::new("system-log-out"),
                            )),
                            MenuAct::Logout,
                        ),
                    ],
                ),
            );

            let user_menu = menu::bar(vec![menu_tree])
                .item_height(menu::ItemHeight::Dynamic(40))
                .item_width(menu::ItemWidth::Uniform(160))
                .spacing(4.0);

            end.push(user_menu.into());
        }

        end
    }

    fn init(core: Core, flags: Self::Flags) -> (Self, Task<Action<Self::Message>>) {
        let data_dir = dirs::data_dir().map(|d| d.join("fi.joonastuomi.Constellations"));

        let mut tasks = Vec::new();
        tasks.push(Task::perform(
            async move {
                let dir = data_dir.ok_or_else(|| {
                    matrix::SyncError::from(anyhow::anyhow!("No standard data directory found"))
                })?;
                matrix::MatrixEngine::new(dir)
                    .await
                    .map_err(matrix::SyncError::from)
            },
            |res| Action::from(Message::EngineReady(res)),
        ));

        if let Some(uri) = flags
            && let Ok(url) = Url::parse(&uri)
        {
            tasks.push(Task::done(Action::from(Message::OidcCallback(url))));
        }

        let config = settings::config::Config::load();

        let mut app = Constellations {
            core: core.clone(),
            matrix: None,
            sync_status: matrix::SyncStatus::Disconnected,
            room_list: Vec::new(),
            filtered_room_list: Vec::new(),
            other_rooms: Vec::new(),
            filtered_other_rooms: Vec::new(),
            selected_room: None,
            timeline_items: Vector::new(),
            composer_content: cosmic::widget::text_editor::Content::new(),
            composer_preview_events: Vec::new(),
            composer_is_preview: false,
            composer_attachments: Vec::new(),
            user_id: None,
            media_cache: HashMap::new(),
            creating_room: false,
            creating_space: false,
            new_room_name: String::new(),
            error: None,
            login_homeserver: "https://matrix.org".to_string(),
            login_username: String::new(),
            login_password: String::new(),
            auth_flow: AuthFlow::Idle,
            qr_rendezvous_url: None,
            is_registering_mode: false,
            is_registering: false,
            is_initializing: true,
            is_sync_indicator_active: false,
            is_loading_more: false,
            last_timeline_offset: 0.0,
            last_threaded_timeline_offset: 0.0,
            search_query: String::new(),
            is_search_active: false,
            active_reaction_picker: None,
            active_thread_root: None,
            threaded_timeline_items: Vector::new(),
            joined_room_ids: std::collections::HashSet::new(),
            visited_room_ids: std::collections::HashSet::new(),
            is_first_time_joining: false,
            needs_initial_scroll: false,
            needs_scroll_restoration: false,
            needs_threaded_scroll_restoration: false,
            is_timeline_at_bottom: true,
            is_threaded_timeline_at_bottom: true,
            is_timeline_initialized: false,
            is_threaded_timeline_initialized: false,
            last_content_height: 0.0,
            last_threaded_content_height: 0.0,
            last_viewport_width: 0.0,
            last_viewport_height: 0.0,
            last_threaded_viewport_width: 0.0,
            last_threaded_viewport_height: 0.0,
            needs_layout_scroll_restoration: false,
            needs_threaded_layout_scroll_restoration: false,
            needs_scroll_adjustment: false,
            needs_threaded_scroll_adjustment: false,
            replying_to: None,
            editing_item: None,
            selected_space: None,
            current_settings_panel: None,
            user_settings: settings::user::State::from_config(&config),
            room_settings: Default::default(),
            space_settings: Default::default(),
            app_settings: settings::app::State::from_config(&config),
            call_participants: HashMap::new(),
            fullscreen_image: None,
            emoji_search_query: String::new(),
            selected_emoji_group: None,
            is_composer_emoji_picker_active: false,
            room_name_cache: std::collections::HashMap::new(),
            thread_counts: std::collections::HashMap::new(),
            show_pinned_panel: false,
            is_loading_pinned: false,
            pinned_events: std::collections::HashSet::new(),
            pinned_events_details: Vec::new(),
            show_members_panel: false,
            room_members: Vec::new(),
            is_loading_members: false,
        };

        let title_task = app.update_title();
        tasks.push(title_task);

        (app, Task::batch(tasks))
    }

    fn context_drawer(
        &self,
    ) -> Option<cosmic::app::context_drawer::ContextDrawer<'_, Self::Message>> {
        if let Some(panel) = &self.current_settings_panel {
            let title = match panel {
                SettingsPanel::App => crate::fl!("app-settings"),
                SettingsPanel::User => crate::fl!("user-settings"),
                SettingsPanel::Room => crate::fl!("room-settings"),
                SettingsPanel::Space => crate::fl!("space-settings"),
                SettingsPanel::Members => crate::fl!("room-members"),
                SettingsPanel::Pinned => crate::fl!("pinned-messages"),
            };

            let panel_content = match panel {
                SettingsPanel::User => self.user_settings.view().map(Message::UserSettings),
                SettingsPanel::Room => self.room_settings.view().map(Message::RoomSettings),
                SettingsPanel::Space => self.space_settings.view().map(Message::SpaceSettings),
                SettingsPanel::App => self.app_settings.view().map(Message::AppSettings),
                SettingsPanel::Members => self.view_members_panel(),
                SettingsPanel::Pinned => self.view_pinned_panel(),
            };

            Some(
                cosmic::app::context_drawer::context_drawer(panel_content, Message::CloseSettings)
                    .title(title.to_string()),
            )
        } else {
            None
        }
    }

    fn update(&mut self, message: Message) -> Task<Action<Self::Message>> {
        self.handle_update(message)
    }

    fn view(&self) -> Element<'_, Message> {
        self.view_app()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let ipc_sub = self.ipc_subscription();

        let matrix = match &self.matrix {
            Some(m) => m,
            None => return ipc_sub,
        };

        let sync_sub = self.sync_subscription(matrix);

        let mut subs = vec![ipc_sub, sync_sub];

        if let Some(room_id) = self.selected_room.clone() {
            subs.push(self.timeline_subscription(matrix, room_id));
        }

        if let (Some(room_id), Some(root_id)) =
            (self.selected_room.clone(), self.active_thread_root.clone())
        {
            subs.push(self.threaded_timeline_subscription(matrix, room_id, root_id));
        }

        Subscription::batch(subs)
    }
}
