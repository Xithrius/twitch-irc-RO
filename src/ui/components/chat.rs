use std::{collections::VecDeque, slice::Iter};

use chrono::Local;
use tui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{block::Position, Block, Borders, List, ListItem},
    Frame,
};

use crate::{
    emotes::SharedEmotes,
    handlers::{
        app::SharedMessages,
        config::SharedCompleteConfig,
        data::MessageData,
        filters::SharedFilters,
        state::State,
        storage::SharedStorage,
        user_input::{
            events::{Event, Key},
            scrolling::Scrolling,
        },
    },
    terminal::TerminalAction,
    twitch::TwitchAction,
    ui::components::{
        following::FollowingWidget, ChannelSwitcherWidget, ChatInputWidget, Component,
        MessageSearchWidget,
    },
    utils::{
        styles::{NO_COLOR, TEXT_DARK_STYLE, TITLE_STYLE},
        text::{title_line, TitleStyle},
    },
};

pub struct ChatWidget {
    config: SharedCompleteConfig,
    messages: SharedMessages,
    chat_input: ChatInputWidget,
    channel_input: ChannelSwitcherWidget,
    search_input: MessageSearchWidget,
    following: FollowingWidget,
    filters: SharedFilters,
    pub scroll_offset: Scrolling,
    // theme: Theme,
}

impl ChatWidget {
    pub fn new(
        config: SharedCompleteConfig,
        messages: SharedMessages,
        storage: &SharedStorage,
        emotes: &SharedEmotes,
        filters: SharedFilters,
    ) -> Self {
        let chat_input = ChatInputWidget::new(config.clone(), storage.clone(), emotes.clone());
        let channel_input = ChannelSwitcherWidget::new(config.clone(), storage.clone());
        let search_input = MessageSearchWidget::new(config.clone());
        let following = FollowingWidget::new(config.clone());

        let scroll_offset = Scrolling::new(config.borrow().frontend.inverted_scrolling);

        Self {
            config,
            messages,
            chat_input,
            channel_input,
            search_input,
            following,
            filters,
            scroll_offset,
        }
    }

    pub fn open_in_browser(&self) {
        webbrowser::open(format!(
            "https://player.twitch.tv/?channel={}&enableExtensions=true&parent=twitch.tv&quality=chunked",
            self.config.borrow().twitch.channel).as_str()).unwrap();
    }

    pub fn get_messages<'a>(
        &self,
        frame: &Frame,
        area: Rect,
        messages_data: &'a VecDeque<MessageData>,
    ) -> VecDeque<Line<'a>> {
        // Accounting for not all heights of rows to be the same due to text wrapping,
        // so extra space needs to be used in order to scroll correctly.
        let mut total_row_height: usize = 0;

        let mut messages = VecDeque::new();

        let mut general_chunk_height = area.height as usize;
        if !self.config.borrow().frontend.hide_chat_border {
            general_chunk_height -= 2;
        }

        let mut scroll = self.scroll_offset.get_offset();

        // Horizontal chunks represents the list within the main chat window.
        let h_chunk = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1)])
            .split(frame.area());

        let message_chunk_width = h_chunk[0].width as usize;

        let config = self.config.borrow();

        'outer: for data in messages_data {
            if self
                .filters
                .borrow()
                .contaminated(data.payload.clone().as_str())
            {
                continue;
            }

            // Offsetting of messages for scrolling through said messages
            if scroll > 0 {
                scroll -= 1;

                continue;
            }

            let username_highlight: Option<&str> = if config.frontend.username_highlight {
                Some(config.twitch.username.as_str())
            } else {
                None
            };

            let search = self.search_input.to_string();

            let lines = data.to_vec(
                &self.config.borrow().frontend,
                message_chunk_width,
                if self.search_input.is_focused() {
                    Some(&search)
                } else {
                    None
                },
                username_highlight,
            );

            for span in lines.into_iter().rev() {
                if total_row_height < general_chunk_height {
                    messages.push_front(span);
                    total_row_height += 1;
                } else {
                    break 'outer;
                }
            }
        }

        // Padding with empty rows so chat can go from bottom to top.
        if general_chunk_height > total_row_height {
            for _ in 0..(general_chunk_height - total_row_height) {
                messages.push_front(Line::from(vec![Span::raw("")]));
            }
        }

        messages
    }
}

impl Component<TwitchAction> for ChatWidget {
    fn draw(&mut self, f: &mut Frame, area: Option<Rect>) {
        let r = area.map_or_else(|| f.area(), |a| a);

        let config = self.config.borrow();

        let mut v_constraints = vec![Constraint::Min(1)];

        if self.chat_input.is_focused() || self.search_input.is_focused() {
            v_constraints.push(Constraint::Length(3));
        }

        let v_chunks_binding = Layout::default()
            .direction(Direction::Vertical)
            .margin(self.config.borrow().frontend.margin)
            .constraints(v_constraints)
            .split(r);

        let mut v_chunks: Iter<Rect> = v_chunks_binding.iter();

        let first_v_chunk = v_chunks.next().unwrap();

        if self.messages.borrow().len() > self.config.borrow().terminal.maximum_messages {
            self.messages
                .borrow_mut()
                .truncate(self.config.borrow().terminal.maximum_messages);
        }

        let messages_data = self.messages.borrow();

        let messages = self.get_messages(f, *first_v_chunk, &messages_data);

        let current_time = Local::now()
            .format(&config.frontend.datetime_format)
            .to_string();

        let spans = [
            TitleStyle::Combined("Time", &current_time),
            TitleStyle::Combined("Channel", config.twitch.channel.as_str()),
            TitleStyle::Custom(Span::styled(
                if self.filters.borrow().reversed() {
                    "retliF"
                } else {
                    "Filter"
                },
                if *NO_COLOR {
                    Style::default()
                } else {
                    Style::default().add_modifier(Modifier::BOLD).fg(
                        if self.filters.borrow().enabled() {
                            Color::Green
                        } else {
                            Color::Red
                        },
                    )
                },
            )),
        ];

        let chat_title = if self.config.borrow().frontend.title_shown {
            Line::from(title_line(&spans, *TITLE_STYLE))
        } else {
            Line::default()
        };

        let mut final_messages = vec![];

        for item in messages {
            final_messages.push(ListItem::new(Text::from(item)));
        }

        let list = if self.config.borrow().frontend.hide_chat_border {
            List::new(final_messages)
        } else {
            List::new(final_messages).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(self.config.borrow().frontend.border_type.clone().into())
                    .title(chat_title),
            )
        }
        .style(*TEXT_DARK_STYLE);

        f.render_widget(list, *first_v_chunk);

        if self.config.borrow().frontend.show_scroll_offset {
            // Cannot scroll past the first message
            let message_amount = messages_data.len().saturating_sub(1);

            let title_binding = format!(
                "{} / {}",
                self.scroll_offset.get_offset(),
                message_amount.to_string().as_str()
            );

            let title = [TitleStyle::Single(&title_binding)];

            let bottom_block = Block::default()
                .borders(Borders::BOTTOM | Borders::LEFT | Borders::RIGHT)
                .border_type(self.config.borrow().frontend.border_type.clone().into())
                .title(title_line(&title, Style::default()))
                .title_position(Position::Bottom)
                .title_alignment(Alignment::Right);

            let rect = Rect::new(
                first_v_chunk.x,
                first_v_chunk.bottom() - 1,
                first_v_chunk.width,
                1,
            );

            f.render_widget(bottom_block, rect);
        }

        if self.chat_input.is_focused() {
            self.chat_input.draw(f, v_chunks.next().copied());
        } else if self.channel_input.is_focused() {
            self.channel_input.draw(f, None);
        } else if self.search_input.is_focused() {
            self.search_input.draw(f, v_chunks.next().copied());
        } else if self.following.is_focused() {
            self.following.draw(f, None);
        }
    }

    async fn event(&mut self, event: &Event) -> Option<TerminalAction<TwitchAction>> {
        if let Event::Input(key) = event {
            let limit =
                self.scroll_offset.get_offset() < self.messages.borrow().len().saturating_sub(1);

            if self.chat_input.is_focused() {
                self.chat_input.event(event).await
            } else if self.channel_input.is_focused() {
                self.channel_input.event(event).await
            } else if self.search_input.is_focused() {
                // WARN:
                // Currently `search_input` will never return a TwitchAction.
                // If for some reason it does, it will be forced to a `Join("")`
                self.search_input
                    .event(event)
                    .await
                    .map(|ta| ta.map_enter(|_| TwitchAction::Join("".into())))
            } else if self.following.is_focused() {
                self.following.event(event).await
            } else {
                match key {
                    Key::Char('i' | 'c') => self.chat_input.toggle_focus(),
                    Key::Char('@') => self.chat_input.toggle_focus_with("@"),
                    Key::Char('/') => self.chat_input.toggle_focus_with("/"),
                    Key::Char('s') => self.channel_input.toggle_focus(),
                    Key::Ctrl('f') => self.search_input.toggle_focus(),
                    Key::Char('f') => self.following.toggle_focus().await,
                    Key::Ctrl('t') => self.filters.borrow_mut().toggle(),
                    Key::Ctrl('r') => self.filters.borrow_mut().reverse(),
                    Key::Char('S') => return Some(TerminalAction::SwitchState(State::Dashboard)),
                    Key::Char('?' | 'h') => return Some(TerminalAction::SwitchState(State::Help)),
                    Key::Char('q') => return Some(TerminalAction::Quit),
                    Key::Char('o') => self.open_in_browser(),
                    Key::Char('G') => {
                        self.scroll_offset.jump_to(0);
                    }
                    Key::Char('g') => {
                        // TODO: Make this not jump to nothingness
                        self.scroll_offset.jump_to(self.messages.borrow().len());
                    }
                    Key::Esc => {
                        if self.scroll_offset.get_offset() == 0 {
                            return Some(TerminalAction::BackOneLayer);
                        }

                        self.scroll_offset.jump_to(0);
                    }
                    Key::Ctrl('p') => panic!("Manual panic triggered by user."),
                    Key::ScrollUp => {
                        if limit {
                            self.scroll_offset.up();
                        } else if self.scroll_offset.is_inverted() {
                            self.scroll_offset.down();
                        }
                    }
                    Key::ScrollDown => {
                        if self.scroll_offset.is_inverted() {
                            if limit {
                                self.scroll_offset.up();
                            }
                        } else {
                            self.scroll_offset.down();
                        }
                    }
                    _ => {}
                }

                None
            }
        } else {
            None
        }
    }
}
