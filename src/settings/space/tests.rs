use super::*;

#[test]
fn test_name_changed() {
    let mut state = State::default();
    let _ = state.update(Message::NameChanged("New Space Name".to_string()), &None);
    assert_eq!(state.name, "New Space Name");
}

#[test]
fn test_topic_changed() {
    let mut state = State::default();
    let _ = state.update(Message::TopicChanged("New Topic".to_string()), &None);
    assert_eq!(state.topic, "New Topic");
}

#[test]
fn test_canonical_alias_changed() {
    let mut state = State::default();
    let _ = state.update(
        Message::CanonicalAliasChanged("#new_alias:example.com".to_string()),
        &None,
    );
    assert_eq!(state.canonical_alias, "#new_alias:example.com");
}

#[test]
fn test_dismiss_error() {
    let mut state = State {
        error: Some("An error occurred".to_string()),
        ..Default::default()
    };
    let _ = state.update(Message::DismissError, &None);
    assert_eq!(state.error, None);
}

#[test]
fn test_child_filter_changed() {
    let mut state = State::default();
    let _ = state.update(Message::ChildFilterChanged("test".to_string()), &None);
    assert_eq!(state.child_filter, "test");
}
