use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use log::debug;
use once_cell::sync::Lazy;
use regex::Regex;
use std::fmt::Display;
use tui::{
    layout::Rect,
    prelude::{Alignment, Margin},
    style::{Color, Modifier, Style},
    symbols::scrollbar,
    text::{Line, Span},
    widgets::{
        block::Position, Block, Borders, Clear, List, ListItem, ListState, Scrollbar,
        ScrollbarOrientation, ScrollbarState,
    },
    Frame,
};

use crate::{
    handlers::{
        config::SharedCompleteConfig,
        storage::SharedStorage,
        user_input::events::{Event, Key},
    },
    terminal::TerminalAction,
    twitch::TwitchAction,
    ui::{
        components::{utils::InputWidget, Component},
        statics::{NAME_MAX_CHARACTERS, NAME_RESTRICTION_REGEX},
    },
    utils::{
        styles::TITLE_STYLE,
        text::{first_similarity, title_line, TitleStyle},
    },
};

use super::utils::centered_rect;

static FUZZY_FINDER: Lazy<SkimMatcherV2> = Lazy::new(SkimMatcherV2::default);

pub struct ChannelSwitcherWidget {
    config: SharedCompleteConfig,
    focused: bool,
    storage: SharedStorage,
    search_input: InputWidget<SharedStorage>,
    list_state: ListState,
    filtered_channels: Option<Vec<String>>,
    vertical_scroll_state: ScrollbarState,
    vertical_scroll: usize,
}

impl ChannelSwitcherWidget {
    pub fn new(config: SharedCompleteConfig, storage: SharedStorage) -> Self {
        let input_validator = Box::new(|_, s: String| -> bool {
            Regex::new(&NAME_RESTRICTION_REGEX)
                .unwrap()
                .is_match(s.as_str())
        });

        // Intuitively, a user will hit the username length limit rather than not hitting 4 characters.
        let visual_indicator =
            Box::new(|s: String| -> String { format!("{} / {}", s.len(), NAME_MAX_CHARACTERS) });

        let input_suggester = Box::new(|storage: SharedStorage, s: String| -> Option<String> {
            first_similarity(
                &storage
                    .borrow()
                    .get("channels")
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<String>>(),
                &s,
            )
        });

        let search_input = InputWidget::new(
            config.clone(),
            "Channel switcher",
            Some((storage.clone(), input_validator)),
            Some(visual_indicator),
            Some((storage.clone(), input_suggester)),
        );

        Self {
            config,
            focused: false,
            storage,
            search_input,
            list_state: ListState::default(),
            filtered_channels: None,
            vertical_scroll_state: ScrollbarState::default(),
            vertical_scroll: 0,
        }
    }

    fn next(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => {
                let items = self.storage.borrow().get("channels");

                if i >= items.len() - 1 {
                    items.len() - 1
                } else {
                    i + 1
                }
            }
            None => 0,
        };

        self.list_state.select(Some(i));

        self.vertical_scroll = self.vertical_scroll.saturating_add(1);
        self.vertical_scroll_state = self.vertical_scroll_state.position(self.vertical_scroll);
    }

    fn previous(&mut self) {
        let i = self
            .list_state
            .selected()
            .map_or(0, |i| if i == 0 { 0 } else { i - 1 });

        self.list_state.select(Some(i));

        self.vertical_scroll = self.vertical_scroll.saturating_sub(1);
        self.vertical_scroll_state = self.vertical_scroll_state.position(self.vertical_scroll);
    }

    fn unselect(&mut self) {
        self.list_state.select(None);
    }

    pub const fn is_focused(&self) -> bool {
        self.focused
    }

    pub fn toggle_focus(&mut self) {
        self.focused = !self.focused;
    }
}

impl Display for ChannelSwitcherWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.search_input)
    }
}

impl Component<TwitchAction> for ChannelSwitcherWidget {
    fn draw(&mut self, f: &mut Frame, area: Option<Rect>) {
        let mut r = area.map_or_else(|| centered_rect(60, 60, 23, f.area()), |a| a);
        // Make sure we have space for the input widget, which has a height of 3.
        r.height -= 3;

        let channels = self.storage.borrow().get("channels");

        let mut items = vec![];
        let current_input = self.search_input.to_string();

        if current_input.is_empty() {
            for channel in channels.clone() {
                items.push(ListItem::new(channel.clone()));
            }

            self.filtered_channels = None;
        } else {
            let channel_filter = |c: String| -> Vec<usize> {
                FUZZY_FINDER
                    .fuzzy_indices(&c, &current_input)
                    .map(|(_, indices)| indices)
                    .unwrap_or_default()
            };

            let mut matched = vec![];

            for channel in channels.clone() {
                let matched_indices = channel_filter(channel.clone());

                if matched_indices.is_empty() {
                    continue;
                }

                let line = channel
                    .chars()
                    .enumerate()
                    .map(|(i, c)| {
                        if matched_indices.contains(&i) {
                            Span::styled(c.to_string(), *TITLE_STYLE)
                        } else {
                            Span::raw(c.to_string())
                        }
                    })
                    .collect::<Vec<Span>>();

                items.push(ListItem::new(vec![Line::from(line)]));
                matched.push(channel);
            }

            self.filtered_channels = Some(matched);
        }

        let title_binding = [TitleStyle::Single("Channel switcher")];

        let list = List::new(items.clone())
            .block(
                Block::default()
                    .title(title_line(&title_binding, *TITLE_STYLE))
                    .borders(Borders::ALL)
                    .border_type(self.config.borrow().frontend.border_type.clone().into()),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::LightGreen)
                    .add_modifier(Modifier::BOLD),
            );

        f.render_widget(Clear, r);
        f.render_stateful_widget(list, r, &mut self.list_state);

        self.vertical_scroll_state = self.vertical_scroll_state.content_length(items.len());

        f.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .symbols(scrollbar::VERTICAL)
                .begin_symbol(None)
                .end_symbol(None),
            r.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut self.vertical_scroll_state,
        );

        let title_binding = format!(
            "{} / {}",
            self.list_state.selected().map_or(1, |i| i + 1),
            self.filtered_channels
                .as_ref()
                .map_or(channels.len(), Vec::len)
        );

        let title = [TitleStyle::Single(&title_binding)];

        let bottom_block = Block::default()
            .borders(Borders::BOTTOM | Borders::LEFT | Borders::RIGHT)
            .border_type(self.config.borrow().frontend.border_type.clone().into())
            .title(title_line(&title, Style::default()))
            .title_position(Position::Bottom)
            .title_alignment(Alignment::Right);

        let rect = Rect::new(r.x, r.bottom() - 1, r.width, 1);

        f.render_widget(bottom_block, rect);

        let input_rect = Rect::new(r.x, r.bottom(), r.width, 3);

        self.search_input.draw(f, Some(input_rect));
    }

    async fn event(&mut self, event: &Event) -> Option<TerminalAction<TwitchAction>> {
        if let Event::Input(key) = event {
            match key {
                Key::Esc => {
                    if self.list_state.selected().is_some() {
                        self.unselect();
                    } else {
                        self.toggle_focus();
                        self.search_input.clear();
                    }
                }
                Key::Ctrl('p') => panic!("Manual panic triggered by user."),
                Key::ScrollDown | Key::Down => self.next(),
                Key::ScrollUp | Key::Up => self.previous(),
                Key::Ctrl('d') => {
                    if let Some(index) = self.list_state.selected() {
                        // TODO: Make this just two if lets
                        if let Some(filtered) = self.filtered_channels.clone() {
                            if let Some(value) = filtered.get(index) {
                                self.storage
                                    .borrow_mut()
                                    .remove_inner_with("channels", value);
                            }
                        } else if let Some(value) = self.storage.borrow().get("channels").get(index)
                        {
                            self.storage
                                .borrow_mut()
                                .remove_inner_with("channels", value);
                        }
                    }
                }
                Key::Enter => {
                    // TODO: Reduce code duplication
                    if let Some(i) = self.list_state.selected() {
                        self.toggle_focus();
                        self.unselect();

                        let channels = self.storage.borrow().get("channels");

                        let selected_channel = if let Some(v) = &self.filtered_channels {
                            if v.is_empty() {
                                let selected_channel = self.search_input.to_string();

                                if !selected_channel.is_empty() {
                                    if self.config.borrow().storage.channels {
                                        self.storage
                                            .borrow_mut()
                                            .add("channels", selected_channel.clone());
                                    }

                                    self.search_input.clear();

                                    self.config
                                        .borrow_mut()
                                        .twitch
                                        .channel
                                        .clone_from(&selected_channel);

                                    return Some(TerminalAction::Enter(TwitchAction::Join(
                                        selected_channel,
                                    )));
                                }
                            }

                            v.get(i).unwrap()
                        } else {
                            channels.get(i).unwrap()
                        };

                        if self.config.borrow().storage.channels {
                            self.storage
                                .borrow_mut()
                                .add("channels", selected_channel.clone());
                        }

                        self.search_input.clear();

                        self.config
                            .borrow_mut()
                            .twitch
                            .channel
                            .clone_from(selected_channel);

                        debug!(
                            "Joining previously joined channel {:?}",
                            selected_channel.to_string()
                        );

                        return Some(TerminalAction::Enter(TwitchAction::Join(
                            selected_channel.to_string(),
                        )));
                    } else if self.search_input.is_valid() {
                        self.toggle_focus();
                        self.unselect();

                        let selected_channel = self.search_input.to_string();

                        if self.config.borrow().storage.channels {
                            self.storage
                                .borrow_mut()
                                .add("channels", selected_channel.clone());
                        }

                        self.search_input.clear();

                        self.config
                            .borrow_mut()
                            .twitch
                            .channel
                            .clone_from(&selected_channel);

                        debug!("Joining new channel {:?}", selected_channel);

                        return Some(TerminalAction::Enter(TwitchAction::Join(selected_channel)));
                    }
                }
                _ => {
                    self.search_input.event(event).await;

                    // Assuming that the user inputted something that modified the input
                    if let Some(v) = &self.filtered_channels {
                        if !v.is_empty() {
                            self.list_state.select(Some(0));
                        }
                    }
                }
            }
        }

        None
    }
}
