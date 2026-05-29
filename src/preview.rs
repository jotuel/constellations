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

pub fn parse_markdown(text: &str, skip_first_blockquote: bool) -> Vec<PreviewEvent> {
    let mut events = Vec::new();
    let mut options = pulldown_cmark::Options::empty();
    options.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    options.insert(pulldown_cmark::Options::ENABLE_TASKLISTS);

    let parser = pulldown_cmark::Parser::new_ext(text, options);
    let mut in_blockquote = 0;
    let mut is_first_blockquote = true;

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
                    }) => events.push(PreviewEvent::StartLink(dest_url.to_string())),
                    pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Link) => {
                        events.push(PreviewEvent::EndLink)
                    }
                    pulldown_cmark::Event::End(
                        pulldown_cmark::TagEnd::Paragraph | pulldown_cmark::TagEnd::Heading(_),
                    ) => events.push(PreviewEvent::EndBlock),
                    pulldown_cmark::Event::Text(t) => {
                        events.push(PreviewEvent::Text(t.to_string()))
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
}
