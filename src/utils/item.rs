use crate::matrix;
use crate::preview::{PreviewEvent, extract_links, parse_markdown, parse_plain_text};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct ConstellationsItem {
    pub item: Option<Arc<matrix::TimelineItem>>,
    pub sender_id: matrix_sdk::ruma::OwnedUserId,
    pub sender_name: String,
    pub avatar_url: Option<String>,
    pub timestamp: String,
    pub is_me: bool,
    pub markdown: Vec<PreviewEvent>,
    pub plain_text: Vec<PreviewEvent>,
    pub markdown_links: Vec<(String, String)>,
    pub plain_links: Vec<(String, String)>,
    pub thread_root_id: Option<matrix_sdk::ruma::OwnedEventId>,
    pub item_id: Option<matrix::TimelineEventItemId>,
}

impl ConstellationsItem {
    pub fn new(item: Arc<matrix::TimelineItem>, user_id: Option<&str>) -> Self {
        let mut sender_id = matrix_sdk::ruma::user_id!("@unknown:example.com").to_owned();
        let mut sender_name = String::new();
        let mut avatar_url = None;
        let mut timestamp = String::new();
        let mut is_me = false;
        let mut markdown = Vec::new();
        let mut plain_text = Vec::new();
        let mut markdown_links = Vec::new();
        let mut plain_links = Vec::new();
        let mut thread_root_id = None;
        let mut item_id = None;
        // ⚡ Bolt Optimization: Pre-compute plain_text representation here
        // to avoid allocating new Strings and Vecs inside the UI render loop (`view_message_text`).

        if let Some(event) = item.as_event() {
            item_id = Some(event.identifier());
            sender_id = event.sender().to_owned();
            if let Some(msg) = event.content().as_message() {
                let is_reply = event.content().in_reply_to().is_some();
                markdown = parse_markdown(msg.body(), is_reply);
                plain_text = parse_plain_text(msg.body());
                markdown_links = extract_links(&markdown);
                plain_links = extract_links(&plain_text);
            }
            let (name, url) = match event.sender_profile() {
                matrix_sdk_ui::timeline::TimelineDetails::Ready(profile) => (
                    profile
                        .display_name
                        .as_deref()
                        .unwrap_or(event.sender().as_ref())
                        .to_string(),
                    profile.avatar_url.as_ref().map(|uri| uri.to_string()),
                ),
                _ => (event.sender().to_string(), None),
            };
            sender_name = name;
            avatar_url = url;

            let ts_millis = u64::from(event.timestamp().0);
            let datetime =
                chrono::DateTime::from_timestamp_millis(ts_millis as i64).unwrap_or_default();
            timestamp = datetime
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string();

            is_me = user_id == Some(event.sender().as_str());
            thread_root_id = event.content().thread_root();
        }

        Self {
            item: Some(item),
            sender_id,
            sender_name,
            avatar_url,
            timestamp,
            is_me,
            markdown,
            plain_text,
            markdown_links,
            plain_links,
            thread_root_id,
            item_id,
        }
    }

    pub fn body_text(&self) -> String {
        self.item
            .as_ref()
            .and_then(|i| i.as_event())
            .and_then(|ev| ev.content().as_message())
            .map(|msg| msg.body().to_string())
            .unwrap_or_else(|| {
                self.plain_text
                    .iter()
                    .filter_map(|p| {
                        if let PreviewEvent::Text(txt) = p {
                            Some(txt.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("")
            })
    }
}

#[cfg(test)]
impl ConstellationsItem {
    pub fn mock(sender_name: &str, text: &str, timestamp: &str, is_me: bool) -> Self {
        let sender_id = matrix_sdk::ruma::user_id!("@unknown:example.com").to_owned();
        let markdown = parse_markdown(text, false);
        let plain_text = parse_plain_text(text);
        let markdown_links = extract_links(&markdown);
        let plain_links = extract_links(&plain_text);
        Self {
            item: None,
            sender_id,
            sender_name: sender_name.to_string(),
            avatar_url: None,
            timestamp: timestamp.to_string(),
            is_me,
            markdown,
            plain_text,
            markdown_links,
            plain_links,
            thread_root_id: None,
            item_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constellations_item_links_precomputation() {
        let item = ConstellationsItem::mock(
            "Alice",
            "Check out [Google](https://google.com) and https://rust-lang.org!",
            "2026-07-10 12:00:00",
            false,
        );

        assert_eq!(
            item.markdown_links,
            vec![
                ("Google".to_string(), "https://google.com".to_string()),
                (
                    "https://rust-lang.org".to_string(),
                    "https://rust-lang.org".to_string()
                ),
            ]
        );

        assert_eq!(
            item.plain_links,
            vec![
                ("Google".to_string(), "https://google.com".to_string()),
                (
                    "https://rust-lang.org".to_string(),
                    "https://rust-lang.org".to_string()
                ),
            ]
        );
    }
}
