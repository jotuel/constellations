// ⚡ Bolt Optimization:
// We cache the parsed Markdown structure in `PreviewEvent`s to avoid running
// `pulldown_cmark::Parser` on every single render frame inside `view_preview()`.
#[derive(Clone, Debug, PartialEq)]
pub enum PreviewEvent {
    StartHeading,
    EndBlock,
    Text(String),
    Code(String),
    Break,
    StartLink(String),
    EndLink,
}

fn split_text_by_urls(text: &str, events: &mut Vec<PreviewEvent>) {
    let mut current_idx = 0;
    
    while current_idx < text.len() {
        let remaining = &text[current_idx..];
        if let Some(pos) = remaining.find("http://").or_else(|| remaining.find("https://")) {
            let start_of_url = current_idx + pos;
            
            if start_of_url > current_idx {
                events.push(PreviewEvent::Text(text[current_idx..start_of_url].to_string()));
            }
            
            let mut end_of_url = start_of_url;
            while end_of_url < text.len() {
                let c = text.as_bytes()[end_of_url];
                if c.is_ascii_whitespace() {
                    break;
                }
                end_of_url += 1;
            }
            
            while end_of_url > start_of_url {
                let last_char = text.as_bytes()[end_of_url - 1];
                if matches!(last_char, b'.' | b',' | b'?' | b'!' | b':' | b';' | b')' | b']' | b'>') {
                    end_of_url -= 1;
                } else {
                    break;
                }
            }
            
            let url = &text[start_of_url..end_of_url];
            if !url.is_empty() {
                events.push(PreviewEvent::StartLink(url.to_string()));
                events.push(PreviewEvent::Text(url.to_string()));
                events.push(PreviewEvent::EndLink);
            }
            
            current_idx = end_of_url;
        } else {
            events.push(PreviewEvent::Text(remaining.to_string()));
            break;
        }
    }
}

pub fn parse_markdown(text: &str, skip_first_blockquote: bool) -> Vec<PreviewEvent> {
    let mut events = Vec::new();
    let mut options = pulldown_cmark::Options::empty();
    options.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    options.insert(pulldown_cmark::Options::ENABLE_TASKLISTS);

    let parser = pulldown_cmark::Parser::new_ext(text, options);
    let mut in_blockquote = 0;
    let mut is_first_blockquote = true;
    let mut in_link = 0;

    for event in parser {
        match event {
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::BlockQuote(_)) => {
                in_blockquote += 1;
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::BlockQuote(_)) => {
                if in_blockquote > 0 {
                    in_blockquote -= 1;
                    if in_blockquote == 0 {
                        is_first_blockquote = false;
                    }
                }
            }
            _ => {
                if in_blockquote > 0 && skip_first_blockquote && is_first_blockquote {
                    continue;
                }
                match event {
                    pulldown_cmark::Event::Start(pulldown_cmark::Tag::Heading { .. }) => {
                        events.push(PreviewEvent::StartHeading)
                    }
                    pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link {
                        dest_url, ..
                    }) => {
                        in_link += 1;
                        events.push(PreviewEvent::StartLink(dest_url.to_string()));
                    }
                    pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Link) => {
                        if in_link > 0 {
                            in_link -= 1;
                        }
                        events.push(PreviewEvent::EndLink);
                    }
                    pulldown_cmark::Event::End(
                        pulldown_cmark::TagEnd::Paragraph | pulldown_cmark::TagEnd::Heading(_),
                    ) => events.push(PreviewEvent::EndBlock),
                    pulldown_cmark::Event::Text(t) => {
                        if in_link > 0 {
                            events.push(PreviewEvent::Text(t.to_string()));
                        } else {
                            split_text_by_urls(&t, &mut events);
                        }
                    }
                    pulldown_cmark::Event::Code(c) => {
                        events.push(PreviewEvent::Code(c.to_string()))
                    }
                    pulldown_cmark::Event::SoftBreak | pulldown_cmark::Event::HardBreak => {
                        events.push(PreviewEvent::Break)
                    }
                    _ => {}
                }
            }
        }
    }
    events
}

pub fn parse_plain_text(text: &str) -> Vec<PreviewEvent> {
    let mut events = Vec::new();
    split_text_by_urls(text, &mut events);
    events
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_markdown_paragraph() {
        let text = "This is a simple paragraph.";
        let events = parse_markdown(text, false);
        assert_eq!(
            events,
            vec![
                PreviewEvent::Text("This is a simple paragraph.".to_string()),
                PreviewEvent::EndBlock
            ]
        );
    }

    #[test]
    fn test_parse_markdown_heading() {
        let text = "# Heading 1\nSome text.";
        let events = parse_markdown(text, false);
        assert_eq!(
            events,
            vec![
                PreviewEvent::StartHeading,
                PreviewEvent::Text("Heading 1".to_string()),
                PreviewEvent::EndBlock,
                PreviewEvent::Text("Some text.".to_string()),
                PreviewEvent::EndBlock,
            ]
        );
    }

    #[test]
    fn test_parse_markdown_code() {
        let text = "Here is `some code` inline.";
        let events = parse_markdown(text, false);
        assert_eq!(
            events,
            vec![
                PreviewEvent::Text("Here is ".to_string()),
                PreviewEvent::Code("some code".to_string()),
                PreviewEvent::Text(" inline.".to_string()),
                PreviewEvent::EndBlock,
            ]
        );
    }

    #[test]
    fn test_parse_markdown_breaks() {
        let text = "Line 1\nLine 2  \nLine 3";
        let events = parse_markdown(text, false);
        assert_eq!(
            events,
            vec![
                PreviewEvent::Text("Line 1".to_string()),
                PreviewEvent::Break,
                PreviewEvent::Text("Line 2".to_string()),
                PreviewEvent::Break,
                PreviewEvent::Text("Line 3".to_string()),
                PreviewEvent::EndBlock,
            ]
        );
    }

    #[test]
    fn test_parse_markdown_ignored_formatting() {
        // Italics and bold should just emit Text events without wrapping them in special formatting events.
        let text = "Some **bold** and *italic* text.";
        let events = parse_markdown(text, false);
        assert_eq!(
            events,
            vec![
                PreviewEvent::Text("Some ".to_string()),
                PreviewEvent::Text("bold".to_string()),
                PreviewEvent::Text(" and ".to_string()),
                PreviewEvent::Text("italic".to_string()),
                PreviewEvent::Text(" text.".to_string()),
                PreviewEvent::EndBlock,
            ]
        );
    }

    #[test]
    fn test_parse_markdown_skip_fallback() {
        let text = "> <@alice:example.com> Hello\n\nHi!";
        let events = parse_markdown(text, true);
        assert_eq!(
            events,
            vec![
                PreviewEvent::Text("Hi!".to_string()),
                PreviewEvent::EndBlock
            ]
        );
    }

    #[test]
    fn test_parse_markdown_no_skip_normal_blockquote() {
        let text = "> This is a quote\n\nAnd this is text.";
        let events = parse_markdown(text, false);
        assert_eq!(
            events,
            vec![
                PreviewEvent::Text("This is a quote".to_string()),
                PreviewEvent::EndBlock,
                PreviewEvent::Text("And this is text.".to_string()),
                PreviewEvent::EndBlock,
            ]
        );
    }

    #[test]
    fn test_parse_plain_text_urls() {
        let text = "Check out https://google.com/search?q=test, it is awesome!";
        let events = parse_markdown(text, false);
        assert_eq!(
            events,
            vec![
                PreviewEvent::Text("Check out ".to_string()),
                PreviewEvent::StartLink("https://google.com/search?q=test".to_string()),
                PreviewEvent::Text("https://google.com/search?q=test".to_string()),
                PreviewEvent::EndLink,
                PreviewEvent::Text(", it is awesome!".to_string()),
                PreviewEvent::EndBlock,
            ]
        );
    }

    #[test]
    fn test_parse_plain_text_only_urls() {
        let text = "Check out https://google.com/search?q=test, it is awesome!";
        let events = parse_plain_text(text);
        assert_eq!(
            events,
            vec![
                PreviewEvent::Text("Check out ".to_string()),
                PreviewEvent::StartLink("https://google.com/search?q=test".to_string()),
                PreviewEvent::Text("https://google.com/search?q=test".to_string()),
                PreviewEvent::EndLink,
                PreviewEvent::Text(", it is awesome!".to_string()),
            ]
        );
    }
}
