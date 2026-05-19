use cosmic::Theme as CosmicTheme;
use cosmic::iced::advanced::layout::{self, Layout};
use cosmic::iced::advanced::renderer;
use cosmic::iced::advanced::widget::{self, Widget};
use cosmic::iced::advanced::{Clipboard, Shell, text};
use cosmic::iced::mouse;
use cosmic::iced::{
    Border, Color, Element, Event, Length, Pixels, Point, Rectangle, Shadow, Size, alignment,
};
use cosmic_text::{Attrs, Buffer, Edit, Editor, Family, FontSystem, Metrics, Shaping, Weight};
use std::ops::Range;
use std::sync::{Arc, LazyLock, Mutex};

static FONT_SYSTEM: LazyLock<Mutex<FontSystem>> = LazyLock::new(|| Mutex::new(FontSystem::new()));

pub struct RichSelectableText<'a, Message> {
    content: Vec<crate::PreviewEvent>,
    on_link_click: Arc<dyn Fn(String) -> Message + 'a>,
}

impl<'a, Message> RichSelectableText<'a, Message> {
    pub fn new(
        content: Vec<crate::PreviewEvent>,
        on_link_click: impl Fn(String) -> Message + 'a,
    ) -> Self {
        Self {
            content,
            on_link_click: Arc::new(on_link_click),
        }
    }
}

pub struct State {
    pub editor: Editor<'static>,
    pub links: Vec<(Range<usize>, String)>,
    pub is_dragging: bool,
    pub content: Vec<crate::PreviewEvent>,
}

impl State {
    pub fn new(content: &[crate::PreviewEvent]) -> Self {
        let mut font_system = FONT_SYSTEM.lock().unwrap();
        let mut buffer = Buffer::new(&mut font_system, Metrics::new(14.0, 20.0));
        let (links, spans) = Self::parse_content(content);

        let iter = spans.iter().map(|(s, a)| (s.as_str(), a.clone()));
        buffer.set_rich_text(iter, &Attrs::new(), Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);

        Self {
            editor: Editor::new(buffer),
            links,
            is_dragging: false,
            content: content.to_vec(),
        }
    }

    pub fn update(&mut self, content: &[crate::PreviewEvent]) {
        if self.content == content {
            return;
        }

        let mut font_system = FONT_SYSTEM.lock().unwrap();
        let (links, spans) = Self::parse_content(content);

        self.editor.with_buffer_mut(|buffer| {
            let iter = spans.iter().map(|(s, a)| (s.as_str(), a.clone()));
            buffer.set_rich_text(iter, &Attrs::new(), Shaping::Advanced, None);
            buffer.shape_until_scroll(&mut font_system, false);
        });

        self.links = links;
        self.content = content.to_vec();
    }

    fn parse_content(
        content: &[crate::PreviewEvent],
    ) -> (Vec<(Range<usize>, String)>, Vec<(String, Attrs<'static>)>) {
        let mut current_link = None;
        let mut is_heading = false;

        let mut byte_pos = 0;
        let mut links = Vec::new();
        let mut spans = Vec::new();

        for event in content {
            match event {
                crate::PreviewEvent::StartHeading => {
                    is_heading = true;
                }
                crate::PreviewEvent::EndBlock => {
                    is_heading = false;
                    current_link = None;
                    spans.push(("\n".to_string(), Attrs::new()));
                    byte_pos += 1;
                }
                crate::PreviewEvent::StartLink(url) => {
                    current_link = Some(url.clone());
                }
                crate::PreviewEvent::EndLink => {
                    current_link = None;
                }
                crate::PreviewEvent::Text(t) => {
                    let mut attrs = Attrs::new();
                    if is_heading {
                        attrs = attrs.weight(Weight::BOLD);
                    }
                    if current_link.is_some() {
                        attrs = attrs.color(cosmic_text::Color::rgb(0, 0, 255));
                    }
                    let start = byte_pos;
                    byte_pos += t.len();

                    if let Some(url) = &current_link {
                        links.push((start..byte_pos, url.clone()));
                    }
                    spans.push((t.clone(), attrs));
                }
                crate::PreviewEvent::Code(c) => {
                    let attrs = Attrs::new().family(Family::Monospace);
                    byte_pos += c.len();
                    spans.push((c.clone(), attrs));
                }
                crate::PreviewEvent::Break => {
                    byte_pos += 1;
                    spans.push(("\n".to_string(), Attrs::new()));
                }
            }
        }

        if let Some((s, _)) = spans.last()
            && s == "\n" {
                spans.pop();
            }

        (links, spans)
    }
}

impl<'a, Message, Renderer> Widget<Message, CosmicTheme, Renderer>
    for RichSelectableText<'a, Message>
where
    Renderer: renderer::Renderer + text::Renderer<Font = cosmic::iced::Font>,
{
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::new(&self.content))
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<State>();
        state.update(&self.content);

        let mut font_system = FONT_SYSTEM.lock().unwrap();

        let width = limits.max().width;
        state.editor.with_buffer_mut(|b| {
            b.set_size(Some(width), None);
            b.shape_until_scroll(&mut font_system, false);
        });

        let height = state
            .editor
            .with_buffer(|b| b.layout_runs().map(|r| r.line_height).sum::<f32>());

        layout::Node::new(Size::new(width, height))
    }

    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        let mut font_system = FONT_SYSTEM.lock().unwrap();

        // Ensure buffer is shaped for current width before calculating y_offset
        state.editor.with_buffer_mut(|b| {
            b.set_size(Some(bounds.width), None);
            b.shape_until_scroll(&mut font_system, false);
        });

        let text_height = state
            .editor
            .with_buffer(|b| b.layout_runs().map(|r| r.line_height).sum::<f32>());
        let y_offset = (bounds.height - text_height).max(0.0) / 2.0;

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(cursor_pos) = cursor.position_in(bounds) {
                    let buf_x = cursor_pos.x;
                    let buf_y = cursor_pos.y - y_offset;
                    state.is_dragging = true;

                    // Check for link click
                    if let Some(hit) = state.editor.with_buffer(|b| b.hit(buf_x, buf_y)) {
                        for (range, url) in &state.links {
                            if range.contains(&hit.index) {
                                shell.publish((self.on_link_click)(url.clone()));
                                shell.capture_event();
                                return;
                            }
                        }
                    }

                    state.editor.action(
                        &mut font_system,
                        cosmic_text::Action::Click {
                            x: buf_x as i32,
                            y: buf_y as i32,
                        },
                    );
                    shell.capture_event();
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.is_dragging = false;
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.is_dragging {
                    let cursor_pos = cursor.position().unwrap_or(Point::ORIGIN);
                    let buf_x = cursor_pos.x - bounds.x;
                    let buf_y = cursor_pos.y - bounds.y - y_offset;
                    state.editor.action(
                        &mut font_system,
                        cosmic_text::Action::Drag {
                            x: buf_x as i32,
                            y: buf_y as i32,
                        },
                    );
                    shell.capture_event();
                    shell.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();

        if let Some(cursor_pos) = cursor.position_in(bounds) {
            let text_height = state
                .editor
                .with_buffer(|b| b.layout_runs().map(|r| r.line_height).sum::<f32>());
            let y_offset = (bounds.height - text_height).max(0.0) / 2.0;

            let buf_x = cursor_pos.x;
            let buf_y = cursor_pos.y - y_offset;

            if let Some(hit) = state.editor.with_buffer(|b| b.hit(buf_x, buf_y)) {
                for (range, _) in &state.links {
                    if range.contains(&hit.index) {
                        return mouse::Interaction::Pointer;
                    }
                }
            }
            return mouse::Interaction::Text;
        }

        mouse::Interaction::default()
    }

    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut Renderer,
        theme: &CosmicTheme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();

        let text_height = state
            .editor
            .with_buffer(|b| b.layout_runs().map(|r| r.line_height).sum::<f32>());
        let y_offset = (bounds.height - text_height).max(0.0) / 2.0;

        let is_dark = theme.theme_type.is_dark();
        let text_color = if is_dark { Color::WHITE } else { Color::BLACK };

        let selection_color = if is_dark {
            Color::from_rgba8(60, 60, 180, 0.5)
        } else {
            Color::from_rgba8(180, 200, 255, 0.7)
        };

        // Draw selection
        if let Some((s, e)) = state.editor.selection_bounds() {
            let (start, end) = if s.line < e.line || (s.line == e.line && s.index < e.index) {
                (s, e)
            } else {
                (e, s)
            };

            state.editor.with_buffer(|buffer| {
                for run in buffer.layout_runs() {
                    if run.line_i >= start.line && run.line_i <= end.line {
                        for (x, width) in run.highlight(start, end) {
                            renderer.fill_quad(
                                renderer::Quad {
                                    bounds: Rectangle::new(
                                        Point::new(
                                            bounds.x + x,
                                            bounds.y + y_offset + run.line_top,
                                        ),
                                        Size::new(width, run.line_height),
                                    ),
                                    border: Border::default(),
                                    shadow: Shadow::default(),
                                    snap: false,
                                },
                                selection_color,
                            );
                        }
                    }
                }
            });
        }

        // Draw text
        state.editor.with_buffer(|buffer| {
            for run in buffer.layout_runs() {
                for glyph in run.glyphs {
                    let mut color = match glyph.color_opt {
                        Some(c) => Color::from_rgba8(c.r(), c.g(), c.b(), (c.a() as f32) / 255.0),
                        None => text_color,
                    };

                    // Color links
                    for (range, _) in &state.links {
                        if range.contains(&glyph.start) {
                            color = Color::from_rgba8(0, 0, 255, 1.0);
                        }
                    }

                    renderer.fill_text(
                        text::Text {
                            content: run.text[glyph.start..glyph.end].to_string(),
                            bounds: Size::new(glyph.w, run.line_height),
                            size: Pixels(14.0),
                            font: cosmic::iced::Font::default(),
                            align_x: alignment::Horizontal::Left.into(),
                            align_y: alignment::Vertical::Top,
                            line_height: text::LineHeight::Relative(1.0),
                            shaping: text::Shaping::Advanced,
                            wrapping: text::Wrapping::None,
                            ellipsize: text::Ellipsize::None,
                        },
                        Point::new(
                            bounds.x + glyph.x,
                            bounds.y + y_offset + run.line_top + glyph.y_offset,
                        ),
                        color,
                        *viewport,
                    );
                }
            }
        });

        // Draw cursor
        if state.is_dragging {
            let cursor = state.editor.cursor();
            state.editor.with_buffer(|buffer| {
                for run in buffer.layout_runs() {
                    if run.line_i == cursor.line {
                        let mut x = 0.0;
                        for glyph in run.glyphs {
                            if cursor.index >= glyph.start && cursor.index < glyph.end {
                                x = glyph.x;
                                break;
                            }
                            if cursor.index >= glyph.end {
                                x = glyph.x + glyph.w;
                            }
                        }

                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: Rectangle::new(
                                    Point::new(bounds.x + x, bounds.y + y_offset + run.line_top),
                                    Size::new(1.0, run.line_height),
                                ),
                                border: Border::default(),
                                shadow: Shadow::default(),
                                snap: false,
                            },
                            text_color,
                        );
                    }
                }
            });
        }
    }
}

impl<'a, Message> RichSelectableText<'a, Message>
where
    Message: 'a,
{
    pub fn into_element<Renderer>(self) -> Element<'a, Message, CosmicTheme, Renderer>
    where
        Renderer: renderer::Renderer + text::Renderer<Font = cosmic::iced::Font> + 'a,
    {
        Element::new(self)
    }
}
