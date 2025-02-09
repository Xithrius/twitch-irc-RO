use rustyline::{
    line_buffer::{ChangeListener, DeleteListener, LineBuffer},
    At, Word,
};
use std::fmt::Display;
use tui::{
    layout::{Position as LayoutPosition, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{block::Position as BlockPosition, Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::{
    handlers::{
        config::SharedCompleteConfig,
        user_input::events::{Event, Key},
    },
    terminal::TerminalAction,
    ui::{components::Component, statics::LINE_BUFFER_CAPACITY},
    utils::{
        styles::NO_COLOR,
        text::{get_cursor_position, title_line, TitleStyle},
    },
};

use super::centered_rect;

pub type InputValidator<T> = Box<dyn Fn(T, String) -> bool>;
pub type VisualValidator = Box<dyn Fn(String) -> String>;
pub type InputSuggester<T> = Box<dyn Fn(T, String) -> Option<String>>;

#[derive(Debug)]
pub struct InputListener;

#[allow(dead_code, unused_variables)]
impl ChangeListener for InputListener {
    fn insert_char(&mut self, idx: usize, c: char) {}

    fn insert_str(&mut self, idx: usize, string: &str) {}

    fn replace(&mut self, idx: usize, old: &str, new: &str) {}
}

#[allow(dead_code, unused_variables)]
impl DeleteListener for InputListener {
    fn delete(&mut self, idx: usize, string: &str, dir: rustyline::line_buffer::Direction) {}
}

pub struct InputWidget<T: Clone> {
    config: SharedCompleteConfig,
    input: LineBuffer,
    title: String,
    focused: bool,
    input_listener: InputListener,
    input_validator: Option<(T, InputValidator<T>)>,
    visual_indicator: Option<VisualValidator>,
    input_suggester: Option<(T, InputSuggester<T>)>,
    suggestion: Option<String>,
}

impl<T: Clone> InputWidget<T> {
    pub fn new(
        config: SharedCompleteConfig,
        title: &str,
        input_validator: Option<(T, InputValidator<T>)>,
        visual_indicator: Option<VisualValidator>,
        input_suggester: Option<(T, InputSuggester<T>)>,
    ) -> Self {
        Self {
            config,
            input: LineBuffer::with_capacity(LINE_BUFFER_CAPACITY),
            title: title.to_string(),
            focused: false,
            input_listener: InputListener,
            input_validator,
            visual_indicator,
            input_suggester,
            suggestion: None,
        }
    }

    #[allow(dead_code)]
    pub fn update(&mut self, s: &str) {
        self.input.update(s, 0, &mut self.input_listener);
    }

    pub fn clear(&mut self) {
        self.input.update("", 0, &mut self.input_listener);
    }

    pub const fn is_focused(&self) -> bool {
        self.focused
    }

    pub fn toggle_focus(&mut self) {
        self.focused = !self.focused;
    }

    pub fn toggle_focus_with(&mut self, s: &str) {
        self.focused = !self.focused;
        self.input.update(s, 1, &mut self.input_listener);
    }

    pub fn is_valid(&self) -> bool {
        self.input_validator
            .as_ref()
            .map_or(true, |(items, validator)| {
                validator(items.clone(), self.input.to_string())
            })
    }

    pub fn accept_suggestion(&mut self) {
        if let Some(suggestion) = &self.suggestion {
            self.input.update(suggestion, 0, &mut self.input_listener);
        }
    }

    pub fn insert(&mut self, s: &str) {
        self.input
            .insert_str(self.input.pos(), s, &mut self.input_listener);
        self.input.set_pos(self.input.pos() + s.len());
    }
}

impl<T: Clone> Display for InputWidget<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.input.as_str())
    }
}

impl<T: Clone> Component<()> for InputWidget<T> {
    fn draw(&mut self, f: &mut Frame, area: Option<Rect>) {
        let r = area.map_or_else(|| centered_rect(60, 60, 20, f.area()), |a| a);

        let cursor_pos = get_cursor_position(&self.input);

        f.set_cursor_position(LayoutPosition::new(
            (r.x + cursor_pos as u16 + 1).min(r.x + r.width.saturating_sub(2)),
            r.y + 1,
        ));

        let current_input = self.input.as_str();

        let binding = [TitleStyle::Single(&self.title)];

        let status_color = if self.is_valid() {
            Color::Green
        } else {
            Color::Red
        };

        self.suggestion = self
            .config
            .borrow()
            .storage
            .channels
            .then(|| {
                self.input_suggester
                    .as_ref()
                    .and_then(|(items, suggester)| suggester(items.clone(), self.input.to_string()))
            })
            .flatten();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(self.config.borrow().frontend.border_type.clone().into())
            .border_style(Style::default().fg(status_color))
            .title(title_line(
                &binding,
                if *NO_COLOR {
                    Style::default()
                } else {
                    Style::default()
                        .fg(status_color)
                        .add_modifier(Modifier::BOLD)
                },
            ));

        let paragraph_lines = Line::from(vec![
            Span::raw(current_input),
            Span::styled(
                self.suggestion
                    .as_ref()
                    .map_or_else(String::new, |suggestion_buffer| {
                        if suggestion_buffer.len() > current_input.len() {
                            suggestion_buffer[current_input.len()..].to_string()
                        } else {
                            String::new()
                        }
                    }),
                Style::default().add_modifier(Modifier::DIM),
            ),
        ]);

        let paragraph = Paragraph::new(paragraph_lines)
            .block(block)
            .scroll((0, ((cursor_pos + 3) as u16).saturating_sub(r.width)));

        f.render_widget(Clear, r);
        f.render_widget(paragraph, r);

        if let Some(visual) = &self.visual_indicator {
            let contents = visual(self.input.to_string());

            let title = [TitleStyle::Single(&contents)];

            let bottom_block = Block::default()
                .title(title_line(
                    &title,
                    if *NO_COLOR {
                        Style::default()
                    } else {
                        Style::default()
                            .fg(status_color)
                            .add_modifier(Modifier::BOLD)
                    },
                ))
                .title_position(BlockPosition::Bottom)
                .borders(Borders::BOTTOM | Borders::LEFT | Borders::RIGHT)
                .border_type(self.config.borrow().frontend.border_type.clone().into());

            // This is only supposed to render on the very bottom line of the area.
            // If some rendering breaks for input boxes, this is a possible source.
            let rect = Rect::new(r.x, r.bottom() - 1, r.width, 1);
            f.render_widget(bottom_block, rect);
        }
    }

    async fn event(&mut self, event: &Event) -> Option<TerminalAction<()>> {
        if let Event::Input(key) = event {
            match key {
                Key::Ctrl('f') | Key::Right => {
                    if self.input.next_pos(1).is_none() {
                        self.accept_suggestion();
                        self.input.move_end();
                    } else {
                        self.input.move_forward(1);
                    }
                }
                Key::Ctrl('b') | Key::Left => {
                    self.input.move_backward(1);
                }
                Key::Ctrl('a') | Key::Home => {
                    self.input.move_home();
                }
                Key::Ctrl('e') | Key::End => {
                    self.input.move_end();
                }
                Key::Alt('f') => {
                    self.input.move_to_next_word(At::AfterEnd, Word::Emacs, 1);
                }
                Key::Alt('b') => {
                    self.input.move_to_prev_word(Word::Emacs, 1);
                }
                Key::Ctrl('t') => {
                    self.input.transpose_chars(&mut self.input_listener);
                }
                Key::Alt('t') => {
                    self.input.transpose_words(1, &mut self.input_listener);
                }
                Key::Ctrl('u') => {
                    self.input.discard_line(&mut self.input_listener);
                }
                Key::Ctrl('k') => {
                    self.input.kill_line(&mut self.input_listener);
                }
                Key::Ctrl('w') => {
                    self.input
                        .delete_prev_word(Word::Emacs, 1, &mut self.input_listener);
                }
                Key::Ctrl('d') => {
                    self.input.delete(1, &mut self.input_listener);
                }
                Key::Backspace | Key::Delete => {
                    self.input.backspace(1, &mut self.input_listener);
                }
                Key::Tab => {
                    if self.config.borrow().storage.channels {
                        if let Some(suggestion) = &self.suggestion {
                            self.input.update(
                                suggestion,
                                suggestion.len(),
                                &mut self.input_listener,
                            );
                        }
                    }
                }
                Key::Ctrl('p') => panic!("Manual panic triggered by user."),
                Key::Ctrl('q') => return Some(TerminalAction::Quit),
                Key::Char(c) => {
                    self.input.insert(*c, 1, &mut self.input_listener);
                }
                _ => {}
            }
        }

        None
    }
}
