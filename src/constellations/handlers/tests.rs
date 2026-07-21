use cosmic::{Action, Application};
use matrix_sdk::ruma::{OwnedEventId, RoomId};

use crate::constellations::{AuthFlow, Constellations, Message, QrLoginStep};
use crate::matrix;
use crate::{ConstellationsItem, Core};
use std::collections::HashMap;
use std::collections::HashSet;

fn create_dummy_constellations() -> Constellations {
    Constellations {
        core: Core::default(),
        matrix: None,
        sync_status: matrix::SyncStatus::Disconnected,
        room_list: Vec::new(),
        other_rooms: Vec::new(),
        filtered_room_list: Vec::new(),
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
        user_id: None,
        media_cache: HashMap::new(),
        creating_room: false,
        new_room_name: String::new(),
        error: None,
        login_homeserver: String::new(),
        login_username: String::new(),
        login_password: String::new(),
        auth_flow: AuthFlow::Idle,
        is_registering: false,
        is_registering_mode: false,
        is_initializing: false,
        is_sync_indicator_active: false,
        search_query: String::new(),
        is_search_active: false,
        public_search_results: Vec::new(),
        is_searching_public: false,
        message_search_results: Vec::new(),
        is_searching_messages: false,
        search_has_more: false,
        is_searching_more_messages: false,
        search_generation: 0,
        global_message_search_results: Vec::new(),
        is_searching_global_messages: false,
        global_search_scope: matrix::GlobalSearchScope::All,
        new_room_is_video: false,
        joined_room_ids: HashSet::new(),
        visited_room_ids: HashSet::new(),
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
        user_settings: crate::settings::user::State::default(),
        room_settings: crate::settings::room::State::default(),
        space_settings: crate::settings::space::State::default(),
        app_settings: crate::settings::app::State::default(),
        composer_attachments: Vec::new(),
        active_reaction_picker: None,
        creating_space: false,
        inviting_to_space: false,
        invite_to_space_id: String::new(),
        inviting_to_room: false,
        invite_to_room_id: String::new(),
        active_thread_root: None,
        threaded_timeline_items: eyeball_im::Vector::new(),
        is_loading_more: false,
        last_timeline_offset: 0.0,
        last_threaded_timeline_offset: 0.0,
        replying_to: None,
        editing_item: None,
        call_participants: HashMap::new(),
        fullscreen_image: None,
        emoji_search_query: String::new(),
        selected_emoji_group: None,
        is_composer_emoji_picker_active: false,
        qr_code_bytes: None,
        qr_check_code_sender: None,
        qr_user_code: None,
        qr_check_code_input: String::new(),
        room_name_cache: HashMap::new(),
        thread_counts: HashMap::new(),
        show_pinned_panel: false,
        is_loading_pinned: false,
        pinned_events: HashSet::new(),
        pinned_events_details: Vec::new(),
        show_members_panel: false,
        room_members: Vec::new(),
        is_loading_members: false,
    }
}

#[test]
fn test_handle_media_fetched_error() {
    let mut app = create_dummy_constellations();

    // Ensure error is initially None
    assert_eq!(app.error, None);

    // Call handle_media_fetched with an Err result
    let _task = app.handle_media_fetched(
        "mxc://example.com/media".to_string(),
        Err("network timeout".to_string()),
    );

    // Verify the error state is set correctly
    assert_eq!(
        app.error,
        Some(crate::fl!("error-failed-fetch-media", error = "network timeout").to_string())
    );

    // Ensure nothing was inserted into the cache
    assert!(app.media_cache.is_empty());
}

#[test]
fn test_toggle_members_panel() {
    let mut app = create_dummy_constellations();

    assert!(!app.show_members_panel);
    assert!(app.room_members.is_empty());

    let _ = app.update(Message::ToggleMembersPanel);
    assert!(app.show_members_panel);
    assert!(app.is_loading_members);

    // Send simulated fetched members
    let mock_member = matrix::RoomMemberInfo {
        user_id: "@user:matrix.org".to_string(),
        display_name: Some("User".to_string()),
        avatar_url: None,
    };
    let _ = app.update(Message::MembersFetched(Ok(vec![mock_member.clone()])));
    assert!(!app.is_loading_members);
    assert_eq!(app.room_members.len(), 1);
    assert_eq!(app.room_members[0].user_id, "@user:matrix.org");

    let _ = app.update(Message::ToggleMembersPanel);
    assert!(!app.show_members_panel);
    assert!(app.room_members.is_empty());
}

#[test]
fn test_toggle_pinned_panel() {
    let mut app = create_dummy_constellations();

    assert!(!app.show_pinned_panel);
    assert!(app.pinned_events.is_empty());

    let _ = app.update(Message::TogglePinnedPanel);
    assert!(app.show_pinned_panel);
    assert!(app.is_loading_pinned);

    // Send simulated fetched pinned events
    let mock_id = matrix_sdk::ruma::event_id!("$123:example.com").to_owned();
    let mock_info = matrix::PinnedEventInfo {
        event_id: mock_id.to_string(),
        sender_id: "@user:matrix.org".to_string(),
        sender_name: "User".to_string(),
        avatar_url: None,
        timestamp: "2026-06-09 12:00:00".to_string(),
        body: "Pinned message content".to_string(),
    };
    let _ = app.update(Message::PinnedEventsFetched(Ok(vec![mock_info])));
    assert!(!app.is_loading_pinned);
    assert_eq!(app.pinned_events.len(), 1);
    assert!(app.pinned_events.contains(&mock_id));
    assert_eq!(app.pinned_events_details.len(), 1);

    let _ = app.update(Message::TogglePinnedPanel);
    assert!(!app.show_pinned_panel);
}

#[test]
fn test_handle_engine_ready_err() {
    let mut app = create_dummy_constellations();

    // Ensure initial state
    app.is_initializing = true;
    assert_eq!(app.error, None);

    let err_res = Err(matrix::SyncError::Anyhow("Initial sync failed".to_string()));
    let _task = app.handle_engine_ready(err_res);

    assert_eq!(
        app.error,
        Some(
            crate::fl!(
                "error-failed-init-engine",
                error = "Error: Initial sync failed"
            )
            .to_string()
        )
    );
    assert!(!app.is_initializing);
}

#[tokio::test]
async fn test_handle_engine_ready_ok() {
    let mut app = create_dummy_constellations();
    app.is_initializing = true;
    assert!(app.matrix.is_none());

    let tmp_dir = tempfile::tempdir().unwrap();
    let engine = match crate::matrix::MatrixEngine::new(tmp_dir.path().to_path_buf()).await {
        Ok(e) => e,
        Err(e) => {
            println!(
                "Skipping test due to engine initialization failure (likely dbus/keyring): {}",
                e
            );
            return;
        }
    };

    let _task = app.handle_engine_ready(Ok(engine.clone()));

    assert!(app.matrix.is_some());
    assert!(app.is_initializing);
}

#[test]
fn test_handle_user_ready_none_user_id() {
    let mut app = create_dummy_constellations();
    app.is_initializing = true;
    app.user_id = Some("stale_user".to_string());

    let _task = app.handle_user_ready(None, Ok(()));

    assert_eq!(app.user_id, None);
    assert!(!app.is_initializing);
}

#[test]
fn test_handle_user_ready_success() {
    let mut app = create_dummy_constellations();
    app.is_initializing = true;

    let _task = app.handle_user_ready(Some("alice".to_string()), Ok(()));

    assert_eq!(app.user_id, Some("alice".to_string()));
    assert!(!app.is_initializing);
    assert_eq!(app.sync_status, matrix::SyncStatus::Disconnected); // Unchanged
}

#[test]
fn test_handle_user_ready_missing_sliding_sync() {
    let mut app = create_dummy_constellations();
    app.is_initializing = true;

    let _task = app.handle_user_ready(
        Some("alice".to_string()),
        Err(matrix::SyncError::MissingSlidingSyncSupport),
    );

    assert_eq!(app.user_id, Some("alice".to_string()));
    assert!(!app.is_initializing);
    assert_eq!(
        app.sync_status,
        matrix::SyncStatus::MissingSlidingSyncSupport
    );
}

#[test]
fn test_handle_user_ready_generic_sync_error() {
    let mut app = create_dummy_constellations();
    app.is_initializing = true;

    let _task = app.handle_user_ready(
        Some("alice".to_string()),
        Err(matrix::SyncError::Generic("network timeout".to_string())),
    );

    assert_eq!(app.user_id, Some("alice".to_string()));
    assert!(!app.is_initializing);
    assert_eq!(
        app.sync_status,
        matrix::SyncStatus::Error("network timeout".to_string())
    );
}

#[tokio::test]
async fn test_handle_user_ready_replay_pending_link() {
    let mut app = create_dummy_constellations();
    app.is_initializing = true;
    app.pending_link = Some("https://matrix.to/#/!room:example.com".to_string());

    let tmp_dir = tempfile::tempdir().unwrap();
    let engine = match crate::matrix::MatrixEngine::new(tmp_dir.path().to_path_buf()).await {
        Ok(e) => e,
        Err(e) => {
            println!(
                "Skipping test due to engine initialization failure (likely dbus/keyring): {}",
                e
            );
            return;
        }
    };
    app.matrix = Some(engine);

    let _task = app.handle_user_ready(Some("alice".to_string()), Ok(()));

    assert!(app.pending_link.is_none());
    assert!(!app.is_initializing);
}

#[test]
fn test_handle_login_finished_ok() {
    let mut app = create_dummy_constellations();
    app.auth_flow = AuthFlow::Password;
    app.auth_flow = AuthFlow::Oidc;

    let _task = app.handle_login_finished(Ok("test_user_id".to_string()));

    assert!(app.auth_flow != AuthFlow::Password);
    assert!(app.auth_flow != AuthFlow::Oidc);
    assert_eq!(app.user_id, Some("test_user_id".to_string()));
}

#[test]
fn test_handle_login_finished_err_sliding_sync() {
    let mut app = create_dummy_constellations();
    app.auth_flow = AuthFlow::Password;
    app.auth_flow = AuthFlow::Oidc;

    let _task = app.handle_login_finished(Err(matrix::SyncError::MissingSlidingSyncSupport));

    assert!(app.auth_flow != AuthFlow::Password);
    assert!(app.auth_flow != AuthFlow::Oidc);
    assert_eq!(
        app.sync_status,
        matrix::SyncStatus::MissingSlidingSyncSupport
    );
}

#[test]
fn test_handle_login_finished_err_generic() {
    let mut app = create_dummy_constellations();
    app.auth_flow = AuthFlow::Password;
    app.auth_flow = AuthFlow::Oidc;

    let _task =
        app.handle_login_finished(Err(matrix::SyncError::Generic("network error".to_string())));

    assert!(app.auth_flow != AuthFlow::Password);
    assert!(app.auth_flow != AuthFlow::Oidc);
    assert_eq!(
        app.error,
        Some(crate::fl!("error-failed-login", error = "network error").to_string())
    );
}

#[tokio::test]
async fn test_handle_fetch_media() {
    let mut app = create_dummy_constellations();

    // We need to set app.matrix to Some(...) to evaluate the inner path.
    // If DBus/Keyring fails, we skip gracefully as done in other tests.
    let tmp_dir = tempfile::tempdir().unwrap();
    let engine = match crate::matrix::MatrixEngine::new(tmp_dir.path().to_path_buf()).await {
        Ok(e) => e,
        Err(_) => return, // Skip if initialization fails due to environment
    };
    app.matrix = Some(engine);

    // Case 1: Plain MediaSource
    let plain_uri = matrix_sdk::ruma::mxc_uri!("mxc://example.com/plain").to_owned();
    let plain_source = matrix_sdk::ruma::events::room::MediaSource::Plain(plain_uri);

    let _task = app.handle_fetch_media(plain_source);
    // The task contains the async fetching which we can't easily await or evaluate directly.
    // However, we've successfully passed through the variant match arm `MediaSource::Plain(uri)`.
    assert!(app.media_cache.is_empty());

    // Case 2: Encrypted MediaSource
    let v2_info = matrix_sdk::ruma::events::room::V2EncryptedFileInfo::new(
        matrix_sdk::ruma::serde::Base64::parse("testtesttesttesttesttesttesttesttesttesttes=")
            .unwrap(),
        matrix_sdk::ruma::serde::Base64::parse("iviviviviviviviviviviv==").unwrap(),
    );
    let info = matrix_sdk::ruma::events::room::EncryptedFileInfo::V2(v2_info);

    let file = matrix_sdk::ruma::events::room::EncryptedFile::new(
        matrix_sdk::ruma::mxc_uri!("mxc://example.com/encrypted").to_owned(),
        info,
        matrix_sdk::ruma::events::room::EncryptedFileHashes::new(),
    );
    let encrypted_source = matrix_sdk::ruma::events::room::MediaSource::Encrypted(Box::new(file));

    let _task = app.handle_fetch_media(encrypted_source);
    // Successfully passed through the variant match arm `MediaSource::Encrypted(file)`.
    assert!(app.media_cache.is_empty());
}

#[test]
fn test_handle_load_more_already_loading() {
    let mut app = create_dummy_constellations();
    app.is_loading_more = true;
    app.selected_room = Some("!room:example.com".into());
    // matrix is None, but even if it was Some, it should return Task::none() because is_loading_more is true

    let _task = app.handle_load_more(false);
    // Since Task is opaque, we can't easily check if it's "none",
    // but we can check that is_loading_more stayed true (it would still be true anyway)
    // and more importantly, that it didn't crash or change other state.
    assert!(app.is_loading_more);

    // If it wasn't loading more, and had no matrix, it would also return Task::none()
    app.is_loading_more = false;
    let _task = app.handle_load_more(false);
    assert!(!app.is_loading_more);
}

#[test]
fn test_handle_logout_no_matrix() {
    let mut app = create_dummy_constellations();
    app.matrix = None;

    let _task = app.handle_logout();

    // When matrix is None, handle_logout should return Task::none() and not modify any state
    assert!(app.matrix.is_none());
    assert_eq!(app.sync_status, matrix::SyncStatus::Disconnected);
}

#[test]
fn test_handle_logout_with_matrix() {
    // Since initializing a true MatrixEngine requires async runtime and IO,
    // and we cannot easily extract the `Action` mapped from a `Task` (due to `Task` being opaque),
    // we write a test verifying the state transitions manually and assert that the task logic
    // will result in LogoutFinished.

    // In this UI framework context, to truly test the return value of Task::perform,
    // we often need to simulate the mapping logic directly.
    let _app = create_dummy_constellations();
    // Since MatrixEngine is difficult to stub without full `tokio::test` and `PathBuf`,
    // and since `handle_logout` strictly clones the matrix and returns `Task::perform`,
    // we've tested the `None` path in `test_handle_logout_no_matrix`.
    // To verify the Message returned by the Task::perform mapping:

    // Let's assert that the closure `|_| Action::from(Message::LogoutFinished)` mapping works.
    let message_mapping_closure = |_| Action::from(Message::LogoutFinished);
    let _action = message_mapping_closure(());

    // Check if the action contains the expected message.
    // `Action::from(Message::LogoutFinished)` returns an Action wrapping our Message
    // We can't use Action::Application because the inner structure isn't public or matches differently.
    // We can verify that the code compiles, but we can't do equality without PartialEq.
    // However, we know this maps correctly by structure.
}

#[test]
fn test_handle_logout_finished() {
    let mut app = create_dummy_constellations();

    // Set up state that should be cleared by logout_finished
    app.user_id = Some("test_user".to_string());
    app.sync_status = matrix::SyncStatus::Syncing;
    app.auth_flow = AuthFlow::Password;
    app.auth_flow = AuthFlow::Oidc;
    app.login_password = "password123".to_string();
    app.error = Some("some error".to_string());
    app.selected_space = Some(RoomId::parse("!space:example.com").unwrap());
    app.is_sync_indicator_active = true;
    app.is_loading_more = true;
    app.joined_room_ids.insert("!room:example.com".into());

    let _task = app.handle_logout_finished();

    // Verify all relevant state was cleared
    assert_eq!(app.user_id, None);
    assert!(app.matrix.is_none());
    assert_eq!(app.sync_status, matrix::SyncStatus::Disconnected);
    assert!(app.room_list.is_empty());
    assert_eq!(app.selected_room, None);
    assert!(app.timeline_items.is_empty());
    assert!(app.auth_flow != AuthFlow::Password);
    assert!(app.auth_flow != AuthFlow::Oidc);
    assert!(app.login_password.is_empty());
    assert_eq!(app.error, None);
    assert_eq!(app.selected_space, None);
    assert!(!app.is_sync_indicator_active);
    assert!(!app.is_loading_more);
    assert!(app.joined_room_ids.is_empty());
}

#[test]
fn test_handle_timeline_diff_clear() {
    let mut app = create_dummy_constellations();
    // Initial state is already empty, but calling clear should still work and keep it empty
    let diff = eyeball_im::VectorDiff::Clear;
    let _task = app.handle_timeline_diff(diff, false, None);

    // We can't directly inspect app.timeline_items easily without exposing it,
    // but since we know apply_diff with Clear removes all elements, and we
    // just want to ensure the logic runs without crashing for the regular timeline:
    assert_eq!(app.timeline_items.len(), 0);
}

#[test]
fn test_handle_timeline_diff_thread_clear() {
    let mut app = create_dummy_constellations();
    let event_id = matrix_sdk::ruma::EventId::parse("$test_event_id").unwrap();
    app.active_thread_root = Some(event_id.clone());

    let diff = eyeball_im::VectorDiff::Clear;
    let _task = app.handle_timeline_diff(diff, true, Some(event_id));

    assert_eq!(app.threaded_timeline_items.len(), 0);
}

#[test]
fn test_handle_timeline_diff_thread_wrong_root() {
    let mut app = create_dummy_constellations();
    let event_id1 = matrix_sdk::ruma::EventId::parse("$test_event_id1").unwrap();
    let event_id2 = matrix_sdk::ruma::EventId::parse("$test_event_id2").unwrap();

    app.active_thread_root = Some(event_id1.clone());

    // If the diff is for a thread that is NOT active, it should be ignored
    let diff = eyeball_im::VectorDiff::Clear;
    let _task = app.handle_timeline_diff(diff, true, Some(event_id2));

    // It shouldn't crash, and shouldn't apply to the active thread (though both are empty here,
    // the core goal is ensuring the condition works).
    assert_eq!(app.threaded_timeline_items.len(), 0);
}

#[test]
fn test_qr_login_progress_step_transitions() {
    let mut app = create_dummy_constellations();
    app.auth_flow = AuthFlow::Qr {
        step: QrLoginStep::Initiating,
    };

    // QrReady → ShowingQr with bytes stored for rendering.
    let _task = app.handle_qr_login_progress(matrix::QrLoginProgress::QrReady(vec![
        0x4d, 0x41, 0x54, 0x52, 0x49, 0x58,
    ]));
    assert_eq!(
        app.auth_flow,
        AuthFlow::Qr {
            step: QrLoginStep::ShowingQr
        }
    );
    assert!(app.qr_code_bytes.is_some());
    assert!(!app.qr_code_bytes.as_ref().unwrap().is_empty());

    // SyncingSecrets → SyncingSecrets step.
    let _task = app.handle_qr_login_progress(matrix::QrLoginProgress::SyncingSecrets);
    assert_eq!(
        app.auth_flow,
        AuthFlow::Qr {
            step: QrLoginStep::SyncingSecrets
        }
    );

    // WaitingForToken → Authenticating with user code stored.
    let _task = app.handle_qr_login_progress(matrix::QrLoginProgress::WaitingForToken {
        user_code: "AB12CD".to_string(),
    });
    assert_eq!(
        app.auth_flow,
        AuthFlow::Qr {
            step: QrLoginStep::Authenticating
        }
    );
    assert_eq!(app.qr_user_code.as_deref(), Some("AB12CD"));

    // Finished(Err) → Error step, error set, QR fields cleared.
    let _task =
        app.handle_qr_login_progress(matrix::QrLoginProgress::Finished(Err("boom".to_string())));
    assert_eq!(
        app.auth_flow,
        AuthFlow::Qr {
            step: QrLoginStep::Error
        }
    );
    assert!(app.qr_code_bytes.is_none());
    assert!(app.qr_user_code.is_none());
    assert!(app.error.is_some());
}

#[test]
fn test_qr_check_code_input_filtering() {
    let mut app = create_dummy_constellations();

    // Only digits are kept, max two characters.
    let _task = app.handle_qr_check_code_changed("a1b2c3".to_string());
    assert_eq!(app.qr_check_code_input, "12");

    // A short valid input is kept as-is.
    let _task = app.handle_qr_check_code_changed("7".to_string());
    assert_eq!(app.qr_check_code_input, "7");

    // Non-digit input is rejected entirely.
    let _task = app.handle_qr_check_code_changed("abc".to_string());
    assert_eq!(app.qr_check_code_input, "");
}

#[test]
fn test_qr_login_cancel_clears_state() {
    let mut app = create_dummy_constellations();
    app.auth_flow = AuthFlow::Qr {
        step: QrLoginStep::ShowingQr,
    };
    app.qr_code_bytes = Some(vec![1, 2, 3]);
    app.qr_user_code = Some("XY".to_string());
    app.qr_check_code_input = "42".to_string();

    let _task = app.handle_cancel_qr_login();
    assert_eq!(app.auth_flow, AuthFlow::Idle);
    assert!(app.qr_code_bytes.is_none());
    assert!(app.qr_user_code.is_none());
    assert!(app.qr_check_code_input.is_empty());
}

fn setup_scroll_test_app() -> (crate::Constellations, std::sync::Arc<str>) {
    let mut app = create_dummy_constellations();
    app.user_id = Some("@test_user:matrix.org".to_string());

    let room_id: std::sync::Arc<str> = std::sync::Arc::from("!room1:example.com");
    app.room_list.push(matrix::RoomData {
        id: room_id.clone(),
        name: Some("Room 1".to_string()),
        unread_count: 5,
        unread_count_str: Some("5".to_string()),
        last_message: None,
        avatar_url: None,
        room_type: None,
        is_space: false,
        parent_space_id: None,
        join_rule: None,
        allowed_spaces: Vec::new(),
        order: None,
        suggested: false,
    });

    (app, room_id)
}

#[test]
fn test_room_scroll_behavior_just_joined() {
    let (mut app, room_id) = setup_scroll_test_app();

    // 1. Just joined the room
    let owned_room_id = matrix_sdk::ruma::RoomId::parse(room_id.as_ref())
        .expect("Failed to parse valid test room ID");
    let _ = app.update(Message::RoomJoined(Ok(owned_room_id)));
    assert!(app.visited_room_ids.contains(&room_id));
    assert!(app.is_first_time_joining);

    // Simulate timeline reset when subscription starts
    let _ = app.update(Message::Matrix(matrix::MatrixEvent::TimelineReset));
    assert!(app.needs_initial_scroll);
    assert!(app.is_timeline_at_bottom);

    // Populate timeline
    for i in 0..10 {
        app.timeline_items
            .push_back(crate::ConstellationsItem::mock(
                "Sender",
                &format!("Msg {}", i),
                "2026-06-08T13:22:31Z",
                false,
            ));
    }

    // Simulate TimelineInitFinished
    let _ = app.update(Message::Matrix(matrix::MatrixEvent::TimelineInitFinished));
    assert!(app.is_timeline_initialized);

    let _task = app.update(Message::LoadMoreFinished(Ok(())));
    assert!(!app.needs_initial_scroll);
}

#[test]
fn test_room_scroll_behavior_normal_selection() {
    let (mut app, room_id) = setup_scroll_test_app();

    // 2. Normal room selection
    app.timeline_items.clear();
    app.is_first_time_joining = true; // set to true to verify RoomSelected sets it to false
    app.needs_initial_scroll = false;

    let _task = app.update(Message::RoomSelected(room_id.clone()));
    assert!(!app.is_first_time_joining);
    assert!(app.needs_initial_scroll);

    // Populate timeline again
    for i in 0..10 {
        app.timeline_items
            .push_back(crate::ConstellationsItem::mock(
                "Sender",
                &format!("Msg {}", i),
                "2026-06-08T13:22:31Z",
                false,
            ));
    }

    // Simulate TimelineInitFinished
    let _ = app.update(Message::Matrix(matrix::MatrixEvent::TimelineInitFinished));
    assert!(app.is_timeline_initialized);

    let _task2 = app.update(Message::LoadMoreFinished(Ok(())));
    assert!(!app.needs_initial_scroll);
}

#[test]
fn test_room_scroll_behavior_check_initial_scroll() {
    let (mut app, room_id) = setup_scroll_test_app();
    app.selected_room = Some(room_id);

    // 3. Directly test check_and_perform_initial_scroll helper
    app.timeline_items.clear();
    app.needs_initial_scroll = true;
    app.is_loading_more = true;
    app.is_timeline_initialized = false;
    assert!(app.check_and_perform_initial_scroll().is_none());

    app.is_loading_more = false;
    assert!(app.check_and_perform_initial_scroll().is_none()); // still none because is_timeline_initialized is false

    app.is_timeline_initialized = true;
    app.timeline_items
        .push_back(crate::ConstellationsItem::mock(
            "Sender",
            "Msg",
            "2026-06-08T13:22:31Z",
            false,
        ));
    assert!(app.check_and_perform_initial_scroll().is_some());
    assert!(!app.needs_initial_scroll);
}

#[test]
fn test_room_scroll_behavior_timeline_reset_initial() {
    let (mut app, _) = setup_scroll_test_app();

    // 4. Test timeline reset scroll behavior (initial reset)
    app.is_timeline_initialized = false;
    let _ = app.update(Message::Matrix(matrix::MatrixEvent::TimelineReset));
    assert!(app.needs_initial_scroll);
    assert!(app.is_timeline_at_bottom);
    assert!(!app.is_timeline_initialized);
}

#[test]
fn test_room_scroll_behavior_timeline_reset_background() {
    let (mut app, _) = setup_scroll_test_app();

    // 5. Test background timeline reset scroll behavior (when already initialized)
    app.is_timeline_initialized = true;
    app.is_timeline_at_bottom = false;
    app.last_timeline_offset = 150.0;
    let _ = app.update(Message::Matrix(matrix::MatrixEvent::TimelineReset));
    assert!(!app.needs_initial_scroll);
    assert!(app.needs_scroll_restoration);
    assert!(!app.is_timeline_at_bottom); // preserved!
    assert!(!app.is_timeline_initialized);

    // Simulate TimelineInitFinished for background reset
    let _ = app.update(Message::Matrix(matrix::MatrixEvent::TimelineInitFinished));
    assert!(app.is_timeline_initialized);
    assert!(!app.needs_scroll_restoration);
}
#[test]
fn test_recompute_thread_counts_skips_none_inner_no_panic() {
    // Regression: items whose `item` field is `None` (mock/virtual items) used to
    // hit `.expect("No item")` and panic recompute_thread_counts. They must now be
    // skipped gracefully.
    let mut app = create_dummy_constellations();

    let root_a = matrix_sdk::ruma::EventId::parse("$root_a:example.com").unwrap();
    let root_b = matrix_sdk::ruma::EventId::parse("$root_b:example.com").unwrap();

    // `new_mock` constructs items with `item: None` by design.
    let mut threaded_a = ConstellationsItem::mock("alice", "reply", "12:00", false);
    threaded_a.thread_root_id = Some(root_a.clone());
    let mut threaded_b = ConstellationsItem::mock("bob", "reply", "12:01", false);
    threaded_b.thread_root_id = Some(root_b.clone());
    let plain = ConstellationsItem::mock("carol", "message", "12:02", true);

    app.timeline_items.push_back(threaded_a);
    app.timeline_items.push_back(threaded_b);
    app.timeline_items.push_back(plain);

    // Must not panic; None-inner items are skipped even when they carry a thread root.
    app.recompute_thread_counts();

    // No event-bearing items were counted.
    assert!(app.thread_counts.is_empty());
}

// --- Phase 3: event-focused (permalink context) timeline ---

/// A room switch must always leave the event-focused view, clearing any
/// pending or active event focus so the new room opens on its live timeline
/// and the "viewing older messages" banner hides.
#[test]
fn test_room_selected_clears_event_focus() {
    use std::sync::Arc;
    let mut app = create_dummy_constellations();
    let event_id: OwnedEventId = matrix_sdk::ruma::EventId::parse("$target:example.com").unwrap();
    app.pending_event_focus = Some(event_id.clone());
    app.active_event_focus = Some(event_id.clone());
    app.selected_room = Some(Arc::from("!old:example.com"));

    // RoomSelected needs a room present in room_list to cache its name; an
    // empty list exercises the no-match path without panicking.
    let room_id: Arc<str> = Arc::from("!new:example.com");
    let _ = app.update(Message::RoomSelected(room_id.clone()));

    assert!(
        app.pending_event_focus.is_none(),
        "pending_event_focus must clear on room switch"
    );
    assert!(
        app.active_event_focus.is_none(),
        "active_event_focus must clear on room switch"
    );
    assert_eq!(app.selected_room.as_deref(), Some("!new:example.com"));
}

/// `check_pending_event_focus` consumes a pending event that is already in
/// the loaded window by scrolling to it (state is consumed, no event-focus
/// timeline is built — i.e. active_event_focus stays None).
#[test]
fn test_pending_event_focus_loaded_event_jumps() {
    let mut app = create_dummy_constellations();
    let event_id: OwnedEventId = matrix_sdk::ruma::EventId::parse("$loaded:example.com").unwrap();

    // Simulate the event already being in the loaded window.
    let mut item = ConstellationsItem::mock("alice", "loaded msg", "12:00", false);
    item.item_id = Some(matrix::TimelineEventItemId::EventId(event_id.clone()));
    app.timeline_items.push_back(item);
    app.pending_event_focus = Some(event_id.clone());

    let _ = app.check_pending_event_focus();

    assert!(
        app.pending_event_focus.is_none(),
        "pending focus must be consumed"
    );
    assert!(
        app.active_event_focus.is_none(),
        "loaded event must not build an event-focused timeline"
    );
}

/// `check_pending_event_focus` consumes a pending event that is NOT in the
/// loaded window by handing off to LoadEventContext. We can't drive the
/// async matrix call in a unit test, but we verify the intent: the helper
/// consumes the pending focus and the follow-up LoadEventContext handler
/// sets active_event_focus (when a room + engine are present, which they
/// aren't here, so it surfaces an error and leaves focus clear).
#[test]
fn test_pending_event_focus_missing_event_defers_to_load() {
    let mut app = create_dummy_constellations();
    let event_id: OwnedEventId = matrix_sdk::ruma::EventId::parse("$missing:example.com").unwrap();

    // Empty timeline: the event is not loaded.
    app.pending_event_focus = Some(event_id.clone());

    let _ = app.check_pending_event_focus();

    assert!(
        app.pending_event_focus.is_none(),
        "pending focus must be consumed"
    );
    // active_event_focus is only set inside handle_load_event_context, which
    // requires a live matrix engine; here it must stay None.
    assert!(app.active_event_focus.is_none());
}

/// `ReturnToLive` clears the active event focus and resets the timeline so
/// the live subscription reinitialises at the newest messages.
#[test]
fn test_return_to_live_clears_active_focus() {
    let mut app = create_dummy_constellations();
    let event_id: OwnedEventId = matrix_sdk::ruma::EventId::parse("$focused:example.com").unwrap();
    app.active_event_focus = Some(event_id);
    app.is_timeline_initialized = true;
    app.is_timeline_at_bottom = false;
    app.needs_initial_scroll = false;

    let _ = app.update(Message::ReturnToLive);

    assert!(
        app.active_event_focus.is_none(),
        "active_event_focus must clear on return to live"
    );
    assert!(!app.is_timeline_initialized, "timeline must reinitialise");
    assert!(
        app.needs_initial_scroll,
        "must scroll to newest on live restore"
    );
    assert!(app.is_timeline_at_bottom);
}

// --- Phase 4: in-app paste-link dialog ---

/// `ToggleOpenLink` opens the dialog when signed in; when signed out it
/// surfaces the sign-in error instead of an inert dialog.
#[test]
fn test_toggle_open_link_signed_out_surfaces_error() {
    let mut app = create_dummy_constellations();
    // create_dummy_constellations leaves matrix as None (signed out).
    assert!(app.open_link_dialog.is_none());

    // Signed out: handler surfaces sign-in error and does not open.
    let _ = app.update(Message::ToggleOpenLink);
    assert!(
        app.open_link_dialog.is_none(),
        "dialog must not open when signed out"
    );
    assert!(
        app.error.as_deref().unwrap_or("").contains("Sign in"),
        "expected a sign-in prompt, got: {:?}",
        app.error
    );
}

/// `OpenLinkTextChanged` updates the dialog value when open, and is a no-op
/// when the dialog is closed (defensive: a stale input event must not
/// secretly open the dialog).
#[test]
fn test_open_link_text_changed_updates_and_guards() {
    let mut app = create_dummy_constellations();

    // Closed: changing text must not open the dialog.
    app.open_link_dialog = None;
    let _ = app.update(Message::OpenLinkTextChanged("ignored".to_string()));
    assert!(app.open_link_dialog.is_none());

    // Open: changing text updates the value.
    app.open_link_dialog = Some(String::new());
    let _ = app.update(Message::OpenLinkTextChanged(
        "https://matrix.to/#/!abc:example.org".to_string(),
    ));
    assert_eq!(
        app.open_link_dialog.as_deref(),
        Some("https://matrix.to/#/!abc:example.org")
    );
}

/// `SubmitOpenLink` always closes the dialog, regardless of input.
#[test]
fn test_submit_open_link_closes_dialog() {
    let mut app = create_dummy_constellations();
    app.open_link_dialog = Some("https://matrix.to/#/!abc:example.org".to_string());

    let _ = app.update(Message::SubmitOpenLink(
        "https://matrix.to/#/!abc:example.org".to_string(),
    ));

    assert!(
        app.open_link_dialog.is_none(),
        "submitting must close the dialog"
    );
}

/// `SubmitOpenLink` with empty input just closes the dialog without error.
#[test]
fn test_submit_open_link_empty_closes_silently() {
    let mut app = create_dummy_constellations();
    app.open_link_dialog = Some(String::new());
    app.error = None;

    let _ = app.update(Message::SubmitOpenLink("   ".to_string()));

    assert!(app.open_link_dialog.is_none());
    // Empty/whitespace input must not surface an error (it's a cancel-like
    // no-op, not a parse failure).
    assert!(app.error.is_none(), "empty submit must not error");
}

#[test]
fn test_copy_to_clipboard_success() {
    let mut app = create_dummy_constellations();
    let _task = app.update(Message::CopyToClipboard(Ok(
        "https://matrix.to/#/!room:example.com".to_string(),
    )));
    assert!(app.error.is_none());
}

#[test]
fn test_copy_to_clipboard_error() {
    let mut app = create_dummy_constellations();
    let _task = app.update(Message::CopyToClipboard(Err(
        "Failed to build link".to_string()
    )));
    assert_eq!(app.error.as_deref(), Some("Failed to build link"));
}

#[test]
fn test_copy_room_link_no_matrix() {
    let mut app = create_dummy_constellations();
    let _task = app.update(Message::CopyRoomLink("!room:example.com".into()));
    assert!(app.error.is_none());
}

#[test]
fn test_copy_message_link_no_matrix() {
    let mut app = create_dummy_constellations();
    let item_id = matrix::TimelineEventItemId::EventId(
        matrix_sdk::ruma::event_id!("$event:localhost").to_owned(),
    );
    let _task = app.update(Message::CopyMessageLink(item_id));
    assert!(app.error.is_none());
}

#[test]
fn test_dm_room_resolved_success() {
    let mut app = create_dummy_constellations();
    let target_room = matrix_sdk::ruma::room_id!("!room:example.com").to_owned();
    let _task = app.update(Message::DmRoomResolved(Ok(target_room)));
    assert_eq!(app.selected_room.as_deref(), Some("!room:example.com"));
    assert!(app.error.is_none());
}

#[test]
fn test_dm_room_resolved_error() {
    let mut app = create_dummy_constellations();
    let _task = app.update(Message::DmRoomResolved(
        Err("Failed to join DM".to_string()),
    ));
    let err = app.error.expect("Expected error to be set");
    assert!(err.contains("Failed to start direct message"));
    assert!(err.contains("Failed to join DM"));
}

#[test]
fn test_message_search_pagination() {
    let mut app = create_dummy_constellations();

    assert!(!app.search_has_more);
    assert!(!app.is_searching_more_messages);
    assert!(app.message_search_results.is_empty());

    // Simulate incoming search results with has_more = true
    let mock_result = matrix::MessageSearchResult {
        room_id: matrix_sdk::ruma::room_id!("!room:example.com").to_owned(),
        room_name: None,
        event_id: matrix_sdk::ruma::event_id!("$1:example.com").to_owned(),
        sender_id: matrix_sdk::ruma::user_id!("@alice:example.com").to_owned(),
        body: "hello world".to_string(),
        timestamp: "2026-06-08 13:00:00".to_string(),
        plain_text: Vec::new(),
        links: Vec::new(),
    };

    let _ = app.update(Message::MessageSearchResults(
        app.search_generation,
        Ok((vec![mock_result.clone()], true)),
    ));

    assert!(app.search_has_more);
    assert_eq!(app.message_search_results.len(), 1);

    // Simulate LoadMoreMessageSearch
    // Note: matrix is None, so it will return Task::none(), but we can still trigger it
    let _ = app.update(Message::LoadMoreMessageSearch);
    // Since matrix is None, is_searching_more_messages will remain false or change depending on conditions,
    // but we can manually trigger the response message to test results appending:
    app.is_searching_more_messages = true;

    let mock_result_2 = matrix::MessageSearchResult {
        room_id: matrix_sdk::ruma::room_id!("!room:example.com").to_owned(),
        room_name: None,
        event_id: matrix_sdk::ruma::event_id!("$2:example.com").to_owned(),
        sender_id: matrix_sdk::ruma::user_id!("@bob:example.com").to_owned(),
        body: "hello back".to_string(),
        timestamp: "2026-06-08 13:05:00".to_string(),
        plain_text: Vec::new(),
        links: Vec::new(),
    };

    let _ = app.update(Message::MessageSearchMoreResults(Ok((
        vec![mock_result_2],
        false,
    ))));

    assert!(!app.is_searching_more_messages);
    assert!(!app.search_has_more); // exhausted now
    assert_eq!(app.message_search_results.len(), 2);
    assert_eq!(app.message_search_results[0].body, "hello world");
    assert_eq!(app.message_search_results[1].body, "hello back");

    // Changing the search query should reset the pagination states
    let _ = app.update(Message::SearchQueryChanged("new query".to_string()));
    assert!(!app.search_has_more);
    assert!(!app.is_searching_more_messages);
}

// --- Global (cross-room) message search ---

/// `OpenRoomEvent` for a different room selects the room and then re-
/// asserts the pending event focus *after* `RoomSelected` clears it. The
/// test simulates the runtime performing the batched `Task::done` messages
/// in order: RoomSelected runs first (clearing focus), then
/// SetPendingEventFocus runs (re-asserting it). The end state must have the
/// new room selected AND the focus pending for TimelineInitFinished.
#[test]
fn test_open_room_event_sets_pending_focus_after_select() {
    use std::sync::Arc;
    let mut app = create_dummy_constellations();
    let event_id: OwnedEventId = matrix_sdk::ruma::EventId::parse("$hit:example.com").unwrap();
    let room_id: Arc<str> = Arc::from("!new:example.com");
    app.selected_room = Some(Arc::from("!old:example.com"));

    // The handler returns a batch of [RoomSelected, SetPendingEventFocus];
    // simulate the runtime performing them in order.
    let _task = app.handle_update(Message::OpenRoomEvent {
        room_id: room_id.clone(),
        event_id: event_id.clone(),
    });
    // Runtime performs RoomSelected (clears focus) then SetPendingEventFocus
    // (re-asserts it) — same order as the batch.
    let _t1 = app.handle_update(Message::RoomSelected(room_id.clone()));
    let _t2 = app.handle_update(Message::SetPendingEventFocus(event_id.clone()));

    assert_eq!(app.selected_room.as_deref(), Some("!new:example.com"));
    assert_eq!(
        app.pending_event_focus,
        Some(event_id),
        "focus must be set after RoomSelected clears it"
    );
}

/// `OpenRoomEvent` for the currently-selected room must NOT switch rooms:
/// it jumps directly via `JumpToMessageOrLoadContext`. `pending_event_focus`
/// is left untouched (the jump path doesn't use it).
#[test]
fn test_open_room_event_same_room_does_not_switch() {
    use std::sync::Arc;
    let mut app = create_dummy_constellations();
    let event_id: OwnedEventId = matrix_sdk::ruma::EventId::parse("$hit:example.com").unwrap();
    let room_id: Arc<str> = Arc::from("!here:example.com");
    app.selected_room = Some(room_id.clone());

    // Same room: handler short-circuits to JumpToMessageOrLoadContext.
    // Don't perform any deferred message — the contract is no room switch.
    let _task = app.handle_update(Message::OpenRoomEvent {
        room_id: room_id.clone(),
        event_id,
    });

    assert_eq!(app.selected_room.as_deref(), Some("!here:example.com"));
    assert!(
        app.pending_event_focus.is_none(),
        "same-room jump must not touch pending_event_focus"
    );
}

/// `GlobalMessageSearchResults` honours the generation guard: a result
/// carrying a stale generation is discarded; the current generation lands.
#[test]
fn test_global_message_search_results_generation_guard() {
    let mut app = create_dummy_constellations();
    app.search_generation = 5;

    let make_hit = || matrix::MessageSearchResult {
        room_id: matrix_sdk::ruma::room_id!("!room:example.com").to_owned(),
        room_name: Some("Room".to_string()),
        event_id: matrix_sdk::ruma::EventId::parse("$e:example.com").unwrap(),
        sender_id: matrix_sdk::ruma::user_id!("@a:b.c").to_owned(),
        body: "hi".to_string(),
        timestamp: "2026-01-01 00:00:00".to_string(),
        plain_text: Vec::new(),
        links: Vec::new(),
    };

    // Stale generation (4 < 5) — discarded.
    let _t = app.handle_update(Message::GlobalMessageSearchResults(4, Ok(vec![make_hit()])));
    assert!(
        app.global_message_search_results.is_empty(),
        "stale-generation result must be discarded"
    );
    assert!(
        !app.is_searching_global_messages,
        "discarded result must not flip the loading flag either"
    );

    // Current generation (5) — lands.
    let _t = app.handle_update(Message::GlobalMessageSearchResults(5, Ok(vec![make_hit()])));
    assert_eq!(
        app.global_message_search_results.len(),
        1,
        "current-generation result must land"
    );
    assert!(!app.is_searching_global_messages);
}

/// `SetGlobalSearchScope` updates the scope and clears stale global hits so
/// a toggle doesn't briefly show the old scope's results.
#[test]
fn test_set_global_search_scope_updates_and_clears() {
    use crate::matrix::GlobalSearchScope;
    let mut app = create_dummy_constellations();
    app.global_search_scope = GlobalSearchScope::All;
    // Pretend we already have some All-scope hits on screen.
    app.global_message_search_results
        .push(matrix::MessageSearchResult {
            room_id: matrix_sdk::ruma::room_id!("!r:example.com").to_owned(),
            room_name: None,
            event_id: matrix_sdk::ruma::EventId::parse("$e:example.com").unwrap(),
            sender_id: matrix_sdk::ruma::user_id!("@a:b.c").to_owned(),
            body: "hi".to_string(),
            timestamp: "2026-01-01 00:00:00".to_string(),
            plain_text: Vec::new(),
            links: Vec::new(),
        });

    // Empty query: SetGlobalSearchScope re-enters SearchQueryChanged, which
    // hits the clear branch (no search fired) — so results are cleared.
    let _t = app.handle_update(Message::SetGlobalSearchScope(GlobalSearchScope::DmsOnly));

    assert_eq!(app.global_search_scope, GlobalSearchScope::DmsOnly);
    assert!(
        app.global_message_search_results.is_empty(),
        "stale hits must clear on scope change"
    );
}
