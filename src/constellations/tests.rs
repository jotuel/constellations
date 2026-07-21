use super::subscriptions::get_room_data;
use super::*;
use crate::matrix;
use crate::settings;
use cosmic::Application;
use std::collections::HashMap;

fn create_test_app() -> Constellations {
    Constellations {
        core: cosmic::app::Core::default(),
        matrix: None,
        sync_status: matrix::SyncStatus::Disconnected,
        room_list: Vec::new(),
        filtered_room_list: Vec::new(),
        other_rooms: Vec::new(),
        filtered_other_rooms: Vec::new(),
        selected_room: None,
        pending_link: None,
        pending_event_focus: None,
        active_event_focus: None,
        open_link_dialog: None,
        pending_alias_op: None,
        timeline_items: eyeball_im::Vector::new(),
        composer_content: cosmic::widget::text_editor::Content::new(),
        composer_preview_events: Vec::new(),
        composer_preview_links: Vec::new(),
        composer_is_preview: false,
        composer_attachments: Vec::new(),
        user_id: None,
        media_cache: std::collections::HashMap::new(),
        creating_room: false,
        creating_space: false,
        new_room_name: String::new(),
        inviting_to_space: false,
        invite_to_space_id: String::new(),
        inviting_to_room: false,
        invite_to_room_id: String::new(),
        error: None,
        login_homeserver: String::new(),
        login_username: String::new(),
        login_password: String::new(),
        auth_flow: AuthFlow::Idle,
        qr_code_bytes: None,
        qr_check_code_sender: None,
        qr_user_code: None,
        qr_check_code_input: String::new(),
        is_registering_mode: false,
        is_registering: false,
        is_initializing: false,
        is_sync_indicator_active: false,
        search_query: String::new(),
        is_search_active: false,
        public_search_results: Vec::new(),
        is_searching_messages: false,
        search_has_more: false,
        is_searching_more_messages: false,
        message_search_results: Vec::new(),
        search_generation: 0,
        global_message_search_results: Vec::new(),
        is_searching_global_messages: false,
        global_search_scope: matrix::GlobalSearchScope::All,
        is_searching_public: false,
        new_room_is_video: false,
        active_reaction_picker: None,
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
        selected_space: None,
        current_settings_panel: None,
        user_settings: settings::user::State::default(),
        room_settings: settings::room::State::default(),
        space_settings: settings::space::State::default(),
        app_settings: settings::app::State::default(),
        active_thread_root: None,
        threaded_timeline_items: eyeball_im::Vector::new(),
        is_loading_more: false,
        replying_to: None,
        editing_item: None,
        call_participants: HashMap::new(),
        last_timeline_offset: Default::default(),
        last_threaded_timeline_offset: Default::default(),
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
    }
}

#[test]
fn test_update_filtered_rooms_no_search_no_space() {
    let mut app = create_test_app();
    app.room_list = vec![
        matrix::RoomData {
            id: std::sync::Arc::from("!room1:matrix.org"),
            name: Some("Room 1".to_string()),
            last_message: None,
            unread_count: 0,
            unread_count_str: None,
            avatar_url: None,
            room_type: None,
            is_space: false,
            parent_space_id: None,
            order: None,
            join_rule: None,
            allowed_spaces: Vec::new(),
            suggested: false,
        },
        matrix::RoomData {
            id: std::sync::Arc::from("!space1:matrix.org"),
            name: Some("Space 1".to_string()),
            last_message: None,
            unread_count: 0,
            unread_count_str: None,
            avatar_url: None,
            room_type: None,
            is_space: true,
            parent_space_id: None,
            order: None,
            join_rule: None,
            allowed_spaces: Vec::new(),
            suggested: false,
        },
    ];

    app.update_filtered_rooms();

    assert_eq!(app.filtered_room_list.len(), 1);
    assert_eq!(
        app.room_list[app.filtered_room_list[0]].id.as_ref(),
        "!room1:matrix.org"
    );
}

#[test]
fn test_update_filtered_rooms_search_by_name() {
    let mut app = create_test_app();
    app.room_list = vec![
        matrix::RoomData {
            id: std::sync::Arc::from("!room1:matrix.org"),
            name: Some("Alpha Room".to_string()),
            last_message: None,
            unread_count: 0,
            unread_count_str: None,
            avatar_url: None,
            room_type: None,
            is_space: false,
            parent_space_id: None,
            order: None,
            join_rule: None,
            allowed_spaces: Vec::new(),
            suggested: false,
        },
        matrix::RoomData {
            id: std::sync::Arc::from("!room2:matrix.org"),
            name: Some("Beta Room".to_string()),
            last_message: None,
            unread_count: 0,
            unread_count_str: None,
            avatar_url: None,
            room_type: None,
            is_space: false,
            parent_space_id: None,
            order: None,
            join_rule: None,
            allowed_spaces: Vec::new(),
            suggested: false,
        },
    ];

    app.search_query = "alpha".to_string();
    app.update_filtered_rooms();

    assert_eq!(app.filtered_room_list.len(), 1);
    assert_eq!(
        app.room_list[app.filtered_room_list[0]].id.as_ref(),
        "!room1:matrix.org"
    );
}

#[test]
fn test_update_filtered_rooms_search_by_id() {
    let mut app = create_test_app();
    app.room_list = vec![
        matrix::RoomData {
            id: std::sync::Arc::from("!room1:matrix.org"),
            name: Some("Alpha Room".to_string()),
            last_message: None,
            unread_count: 0,
            unread_count_str: None,
            avatar_url: None,
            room_type: None,
            is_space: false,
            parent_space_id: None,
            order: None,
            join_rule: None,
            allowed_spaces: Vec::new(),
            suggested: false,
        },
        matrix::RoomData {
            id: std::sync::Arc::from("!room2:matrix.org"),
            name: Some("Beta Room".to_string()),
            last_message: None,
            unread_count: 0,
            unread_count_str: None,
            avatar_url: None,
            room_type: None,
            is_space: false,
            parent_space_id: None,
            order: None,
            join_rule: None,
            allowed_spaces: Vec::new(),
            suggested: false,
        },
    ];

    app.search_query = "!ROOM2".to_string();
    app.update_filtered_rooms();

    assert_eq!(app.filtered_room_list.len(), 1);
    assert_eq!(
        app.room_list[app.filtered_room_list[0]].id.as_ref(),
        "!room2:matrix.org"
    );
}

#[test]
fn test_update_filtered_rooms_search_no_match() {
    let mut app = create_test_app();
    app.room_list = vec![matrix::RoomData {
        id: std::sync::Arc::from("!room1:matrix.org"),
        name: Some("Alpha Room".to_string()),
        last_message: None,
        unread_count: 0,
        unread_count_str: None,
        avatar_url: None,
        room_type: None,
        is_space: false,
        parent_space_id: None,
        order: None,
        join_rule: None,
        allowed_spaces: Vec::new(),
        suggested: false,
    }];

    app.search_query = "gamma".to_string();
    app.update_filtered_rooms();

    assert_eq!(app.filtered_room_list.len(), 0);
}

#[test]
fn test_update_filtered_rooms_with_selected_space_no_matrix() {
    let mut app = create_test_app();
    app.room_list = vec![matrix::RoomData {
        id: std::sync::Arc::from("!room1:matrix.org"),
        name: Some("Alpha Room".to_string()),
        last_message: None,
        unread_count: 0,
        unread_count_str: None,
        avatar_url: None,
        room_type: None,
        is_space: false,
        parent_space_id: None,
        order: None,
        join_rule: None,
        allowed_spaces: Vec::new(),
        suggested: false,
    }];

    app.selected_space = Some(matrix_sdk::ruma::RoomId::parse("!space1:matrix.org").unwrap());

    app.update_filtered_rooms();

    assert_eq!(app.filtered_room_list.len(), 0);
}

#[tokio::test]
async fn test_get_room_data_not_found() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let engine = match matrix::MatrixEngine::new(tmp_dir.path().to_path_buf()).await {
        Ok(e) => e,
        Err(e) => {
            tracing::info!(
                "Skipping test due to engine initialization failure (likely dbus/keyring): {}",
                e
            );
            return;
        }
    };

    let room_id = matrix_sdk::ruma::RoomId::parse("!nonexistent:example.com").unwrap();

    let result = get_room_data(&engine, &room_id).await;

    assert!(result.is_none());
}

#[test]
fn test_update_room_joined_error() {
    let mut app = create_test_app();
    let _ = app.update(Message::RoomJoined(
        Err("some connection error".to_string()),
    ));

    assert_eq!(
        app.error,
        Some(crate::fl!("error-failed-join-room", error = "some connection error").to_string())
    );
}

#[test]
fn test_room_name_cache() {
    let mut app = create_test_app();
    let room_id: std::sync::Arc<str> = std::sync::Arc::from("!room1:matrix.org");

    assert_eq!(app.get_room_name(&room_id), None);

    app.room_name_cache
        .insert(room_id.clone(), "Cached Room Name".to_string());
    assert_eq!(app.get_room_name(&room_id), Some("Cached Room Name"));

    app.room_list = vec![matrix::RoomData {
        id: room_id.clone(),
        name: Some("Active Room Name".to_string()),
        last_message: None,
        unread_count: 0,
        unread_count_str: None,
        avatar_url: None,
        room_type: None,
        is_space: false,
        parent_space_id: None,
        order: None,
        join_rule: None,
        allowed_spaces: Vec::new(),
        suggested: false,
    }];
    assert_eq!(app.get_room_name(&room_id), Some("Active Room Name"));
}

#[test]
fn test_search_query_changed_debounce() {
    let mut app = create_test_app();
    assert_eq!(app.search_generation, 0);

    // Typing a search query should increment the search generation (debounce tracking)
    let _ = app.update(Message::SearchQueryChanged("hello".to_string()));
    assert_eq!(app.search_query, "hello");
    assert_eq!(app.search_generation, 1);

    // Typing more should increment it further
    let _ = app.update(Message::SearchQueryChanged("hello world".to_string()));
    assert_eq!(app.search_query, "hello world");
    assert_eq!(app.search_generation, 2);

    // Clearing the search query should set flags to false and increment generation again
    app.is_searching_public = true;
    app.is_searching_messages = true;
    let _ = app.update(Message::SearchQueryChanged("".to_string()));
    assert_eq!(app.search_query, "");
    assert_eq!(app.search_generation, 3);
    assert!(!app.is_searching_public);
    assert!(!app.is_searching_messages);
}
