use std::{
    cell::RefCell,
    collections::VecDeque,
    process::{Child, Stdio},
    rc::Rc,
};

use chrono::{DateTime, Local};
use rustyline::line_buffer::LineBuffer;
use tui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};

use crate::{
    emotes::SharedEmotes,
    handlers::{
        config::{CompleteConfig, SharedCompleteConfig, Theme},
        data::MessageData,
        filters::{Filters, SharedFilters},
        state::State,
        storage::{SharedStorage, Storage},
        user_input::events::{Event, Key},
    },
    terminal::TerminalAction,
    twitch::TwitchAction,
    ui::{
        components::{Component, Components},
        statics::LINE_BUFFER_CAPACITY,
    },
};

pub type SharedMessages = Rc<RefCell<VecDeque<MessageData>>>;

#[allow(dead_code)]
pub struct App {
    /// All the available components.
    pub components: Components,
    /// A config for the app and components to share.
    pub config: SharedCompleteConfig,
    /// History of recorded messages (time, username, message, etc).
    pub messages: SharedMessages,
    /// Data loaded in from a JSON file.
    pub storage: SharedStorage,
    /// Messages to be filtered out.
    pub filters: SharedFilters,
    /// Which window the terminal is currently focused on.
    state: State,
    /// The previous state, if any.
    previous_state: Option<State>,
    /// What the user currently has inputted.
    pub input_buffer: LineBuffer,
    /// The current suggestion, if any.
    pub buffer_suggestion: Option<String>,
    /// The theme selected by the user.
    pub theme: Theme,
    /// Emotes
    pub emotes: SharedEmotes,
    /// Running stream.
    // TODO:
    // Review if this needs to be a `Rc<RefCell>`. I haven't bothered to check
    pub running_stream: Rc<RefCell<Option<Child>>>,
}

macro_rules! shared {
    ($expression:expr) => {
        Rc::new(RefCell::new($expression))
    };
}

impl App {
    pub fn new(config: CompleteConfig, startup_time: DateTime<Local>) -> Self {
        let shared_config = shared!(config.clone());

        let shared_config_borrow = shared_config.borrow();

        let storage = shared!(Storage::new("storage.json", &shared_config_borrow.storage));

        if !storage
            .borrow()
            .contains("channels", &config.twitch.channel)
        {
            storage.borrow_mut().add("channels", config.twitch.channel);
        }

        let filters = shared!(Filters::new("filters.txt", &shared_config_borrow.filters));

        let messages = shared!(VecDeque::with_capacity(
            shared_config_borrow.terminal.maximum_messages,
        ));

        let emotes = SharedEmotes::default();

        let components = Components::new(
            &shared_config,
            storage.clone(),
            filters.clone(),
            messages.clone(),
            &emotes,
            startup_time,
        );

        Self {
            components,
            config: shared_config.clone(),
            running_stream: shared!(None),
            messages,
            storage,
            filters,
            state: shared_config_borrow.terminal.first_state.clone(),
            previous_state: None,
            input_buffer: LineBuffer::with_capacity(LINE_BUFFER_CAPACITY),
            buffer_suggestion: None,
            theme: shared_config_borrow.frontend.theme.clone(),
            emotes,
        }
    }

    pub fn draw(&mut self, f: &mut Frame) {
        let mut size = f.area();

        if self.config.borrow().frontend.state_tabs {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(size.height - 1), Constraint::Length(1)])
                .split(f.area());

            size = layout[0];

            self.components.tabs.draw(f, Some(layout[1]), &self.state);
        }

        if (size.height < 10 || size.width < 60)
            && self.config.borrow().frontend.show_unsupported_screen_size
        {
            self.components.window_size_error.draw(f, Some(f.area()));
        } else {
            match self.state {
                State::Dashboard => self.components.dashboard.draw(f, None),
                State::Normal => self.components.chat.draw(f, None),
                State::Help => self.components.help.draw(f, None),
            }
        }

        if self.components.debug.is_focused() {
            let new_rect = Rect::new(size.x, size.y + 1, size.width - 1, size.height - 2);

            let rect = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(new_rect)[1];

            self.components.debug.draw(f, Some(rect));
        }
    }

    pub async fn event(&mut self, event: &Event) -> Option<TerminalAction<TwitchAction>> {
        if let Event::Input(key) = event {
            if self.components.debug.is_focused() {
                return self
                    .components
                    .debug
                    .event(event)
                    .await
                    .map(|ta| ta.map_enter(|()| TwitchAction::Join("".into())));
            }

            match key {
                // Global keybinds
                Key::Ctrl('d') => {
                    self.components.debug.toggle_focus();
                }
                _ => {
                    return match self.state {
                        State::Dashboard => self.components.dashboard.event(event).await,
                        State::Normal => self.components.chat.event(event).await,
                        State::Help => self
                            .components
                            .help
                            .event(event)
                            .await
                            .map(|ta| ta.map_enter(|()| TwitchAction::Join("".into()))),
                    };
                }
            }
        }

        None
    }

    // TODO:
    // Should Properly handle if a stream is not available.
    // WARN:
    // closes a previous stream if open. This is technically overloading this function, but
    // whatever.
    pub fn open_stream(&self, channel: &str) {
        let mut t = self.running_stream.borrow_mut();
        if let Some(c) = t.as_mut() {
            c.kill().unwrap();
        }
        *t = Some(
            std::process::Command::new("streamlink")
                .args([
                    (String::from("twitch.tv/") + channel).as_str(),
                    "--default-stream",
                    "720p, 720p60, best",
                    "--player",
                    "mpv",
                ])
                .stdout(Stdio::null())
                .spawn()
                .expect("Pog"),
        );
    }

    // TODO:
    // This probably sucks
    pub fn close_stream(&self) {
        let mut t = self.running_stream.borrow_mut();
        if let Some(c) = t.as_mut() {
            c.kill().unwrap();
        }
        *t = None;
    }

    pub fn cleanup(&self) {
        self.close_stream();
        self.storage.borrow().dump_data();
        self.emotes.unload();
    }

    pub fn clear_messages(&mut self) {
        self.messages.borrow_mut().clear();

        self.components.chat.scroll_offset.jump_to(0);
    }

    pub fn purge_user_messages(&self, user_id: &str) {
        let messages = self
            .messages
            .borrow_mut()
            .iter()
            .filter(|&m| m.user_id.clone().map_or(true, |user| user != user_id))
            .cloned()
            .collect::<VecDeque<MessageData>>();

        self.messages.replace(messages);
    }

    pub fn remove_message_with(&self, message_id: &str) {
        let index = self
            .messages
            .borrow_mut()
            .iter()
            .position(|f| f.message_id.clone().is_some_and(|id| id == message_id));

        if let Some(i) = index {
            self.messages.borrow_mut().remove(i).unwrap();
        }
    }

    pub fn get_previous_state(&self) -> Option<State> {
        self.previous_state.clone()
    }

    #[allow(dead_code)]
    pub fn get_state(&self) -> State {
        self.state.clone()
    }

    pub fn set_state(&mut self, other: State) {
        self.previous_state = Some(self.state.clone());
        self.state = other;
    }

    #[allow(dead_code)]
    pub fn rotate_theme(&self) {
        todo!("Rotate through different themes")
    }
}
