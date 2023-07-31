use std::ops::Index;

use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use once_cell::sync::Lazy;
use tui::{
    backend::Backend,
    layout::{Constraint, Rect},
    prelude::Alignment,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{block::Position, Block, Borders, Clear, Row, Table, TableState},
    Frame,
};

use crate::{
    emotes::Emotes,
    handlers::{
        config::SharedCompleteConfig,
        user_input::events::{Event, Key},
    },
    terminal::TerminalAction,
    twitch::{oauth::FollowingList, TwitchAction},
    ui::{components::Component, statics::NAME_MAX_CHARACTERS},
    utils::text::{title_line, TitleStyle},
};

use super::utils::InputWidget;

static FUZZY_FINDER: Lazy<SkimMatcherV2> = Lazy::new(SkimMatcherV2::default);

pub struct FollowingWidget {
    config: SharedCompleteConfig,
    focused: bool,
    following: FollowingList,
    filtered_following: Option<Vec<String>>,
    state: TableState,
    search_input: InputWidget,
}

impl FollowingWidget {
    pub fn new(config: SharedCompleteConfig, following: FollowingList) -> Self {
        let search_input = InputWidget::new(config.clone(), "Search", None, None, None);

        let table_state = TableState::default().with_selected(Some(0));

        Self {
            config,
            focused: false,
            following,
            state: table_state,
            filtered_following: None,
            search_input,
        }
    }

    fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.following.data.len() - 1 {
                    self.following.data.len() - 1
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn previous(&mut self) {
        let i = self
            .state
            .selected()
            .map_or(0, |i| if i == 0 { 0 } else { i - 1 });
        self.state.select(Some(i));
    }

    fn unselect(&mut self) {
        self.state.select(None);
    }

    pub const fn is_focused(&self) -> bool {
        self.focused
    }

    pub fn toggle_focus(&mut self) {
        self.focused = !self.focused;
    }
}

impl Component for FollowingWidget {
    fn draw<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect, _emotes: Option<&mut Emotes>) {
        let mut rows = vec![];
        let current_input = self.search_input.to_string();

        if current_input.is_empty() {
            for channel in self.following.clone().data {
                rows.push(Row::new(vec![channel.broadcaster_name.clone()]));
            }

            self.filtered_following = None;
        } else {
            let channel_filter = |c: String| -> Vec<usize> {
                FUZZY_FINDER
                    .fuzzy_indices(&c, &current_input)
                    .map(|(_, indices)| indices)
                    .unwrap_or_default()
            };

            let mut matched = vec![];

            for channel in self.following.clone().data {
                let matched_indices = channel_filter(channel.broadcaster_name.clone());

                if matched_indices.is_empty() {
                    continue;
                }

                let search_theme = Style::default().fg(Color::Red).add_modifier(Modifier::BOLD);

                let line = channel
                    .broadcaster_name
                    .chars()
                    .enumerate()
                    .map(|(i, c)| {
                        if matched_indices.contains(&i) {
                            Span::styled(c.to_string(), search_theme)
                        } else {
                            Span::raw(c.to_string())
                        }
                    })
                    .collect::<Vec<Span>>();

                rows.push(Row::new(vec![Line::from(line)]));
                matched.push(channel.broadcaster_name);
            }

            self.filtered_following = Some(matched);
        }

        let title_binding = [TitleStyle::Single("Following")];

        let constraint_binding = [Constraint::Length(NAME_MAX_CHARACTERS as u16)];

        let table = Table::new(rows)
            .block(
                Block::default()
                    .title(title_line(
                        &title_binding,
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    ))
                    .borders(Borders::ALL)
                    .border_type(self.config.borrow().frontend.border_type.clone().into()),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::LightGreen)
                    .add_modifier(Modifier::BOLD),
            )
            .widths(&constraint_binding);

        f.render_widget(Clear, area);
        f.render_stateful_widget(table, area, &mut self.state);

        let title_binding = format!(
            "{} / {}",
            self.state.selected().map_or(1, |i| i + 1),
            if let Some(v) = &self.filtered_following {
                v.len()
            } else {
                self.following.data.len()
            }
        );

        let title = [TitleStyle::Single(&title_binding)];

        let bottom_block = Block::default()
            .borders(Borders::BOTTOM | Borders::LEFT | Borders::RIGHT)
            .border_type(self.config.borrow().frontend.border_type.clone().into())
            .title(title_line(&title, Style::default()))
            .title_position(Position::Bottom)
            .title_alignment(Alignment::Right);

        let rect = Rect::new(area.x, area.bottom() - 1, area.width, 1);

        f.render_widget(bottom_block, rect);

        let input_rect = Rect::new(area.x, area.bottom(), area.width, 3);

        self.search_input.draw(f, input_rect, None);
    }

    fn event(&mut self, event: &Event) -> Option<TerminalAction> {
        if let Event::Input(key) = event {
            match key {
                Key::Esc => {
                    if self.state.selected().is_some() {
                        self.unselect();
                    } else {
                        self.toggle_focus();

                        return Some(TerminalAction::BackOneLayer);
                    }
                }
                Key::Ctrl('p') => panic!("Manual panic triggered by user."),
                Key::ScrollDown => self.next(),
                Key::ScrollUp => self.previous(),
                Key::Enter => {
                    if let Some(i) = self.state.selected() {
                        let selected_channel = if let Some(v) = self.filtered_following.clone() {
                            if v.is_empty() {
                                return None;
                            }

                            v.index(i).to_string()
                        } else {
                            self.following.data.index(i).broadcaster_name.to_string()
                        }
                        .to_lowercase();

                        self.toggle_focus();

                        self.unselect();

                        self.config.borrow_mut().twitch.channel = selected_channel.clone();

                        return Some(TerminalAction::Enter(TwitchAction::Join(selected_channel)));
                    }
                }
                _ => {
                    self.search_input.event(event);

                    // Assuming that the user inputted something that modified the input
                    if let Some(v) = &self.filtered_following {
                        if !v.is_empty() {
                            self.state.select(Some(0));
                        }
                    }
                }
            }
        }

        None
    }
}
