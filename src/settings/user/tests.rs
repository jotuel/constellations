use super::*;

use std::sync::Arc;

#[test]
fn test_display_name_changed() {
    let mut state = State::default();
    let _ = state.update(Message::DisplayNameChanged("John Doe".to_string()), &None);
    assert_eq!(state.display_name, "John Doe");
}

#[test]
fn test_dismiss_error() {
    let mut state = State {
        error: Some("Error".to_string()),
        ..Default::default()
    };
    let _ = state.update(Message::DismissError, &None);
    assert_eq!(state.error, None);
}

#[test]
fn test_devices_loaded() {
    let mut state = State {
        is_loading_devices: true,
        ..Default::default()
    };

    let devices = vec![DeviceInfo {
        device_id: Arc::from("DEV1"),
        display_name: Some("Test Device".to_string()),
        is_verified: true,
        is_current: true,
        is_renaming: false,
        edit_name: String::new(),
        is_deleting: false,
    }];

    let _ = state.update(Message::DevicesLoaded(Ok(devices.clone())), &None);
    assert!(!state.is_loading_devices);
    assert_eq!(state.devices.len(), 1);
    assert_eq!(state.devices[0].device_id.as_ref(), "DEV1");
    assert!(state.devices[0].is_verified);
    assert!(state.devices[0].is_current);

    state.is_loading_devices = true;
    let _ = state.update(
        Message::DevicesLoaded(Err("network error".to_string())),
        &None,
    );
    assert!(!state.is_loading_devices);
    assert_eq!(
        state.error,
        Some("Failed to load devices: network error".to_string())
    );
}
