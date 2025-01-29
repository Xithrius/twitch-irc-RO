use std::fmt::Display;
use tui::{layout::Rect, Frame};

use crate::{
    handlers::{
        config::SharedCompleteConfig,
        user_input::events::{Event, Key},
    },
    terminal::TerminalAction,
    ui::{
        components::{utils::InputWidget, Component},
        statics::TWITCH_MESSAGE_LIMIT,
    },
};

pub struct MessageSearchWidget {
    _config: SharedCompleteConfig,
    input: InputWidget<()>,
}

impl MessageSearchWidget {
    pub fn new(config: SharedCompleteConfig) -> Self {
        let input_validator =
            Box::new(|(), s: String| -> bool { !s.is_empty() && s.len() <= TWITCH_MESSAGE_LIMIT });

        // Indication that user won't get any good results near the twitch message length limit.
        // TODO: In the future, this should be replaced with how many results have been found.
        let visual_indicator =
            Box::new(|s: String| -> String { format!("{} / {}", s.len(), TWITCH_MESSAGE_LIMIT) });

        let input = InputWidget::new(
            config.clone(),
            "Message search",
            Some(((), input_validator)),
            Some(visual_indicator),
            None,
        );

        Self {
            _config: config,
            input,
        }
    }

    pub const fn is_focused(&self) -> bool {
        self.input.is_focused()
    }

    pub fn toggle_focus(&mut self) {
        self.input.toggle_focus();
    }
}

impl Display for MessageSearchWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.input)
    }
}

impl Component<()> for MessageSearchWidget {
    fn draw(&mut self, f: &mut Frame, area: Option<Rect>) {
        self.input.draw(f, area);
    }

    async fn event(&mut self, event: &Event) -> Option<TerminalAction<()>> {
        if let Event::Input(key) = event {
            match key {
                Key::Esc => {
                    self.input.toggle_focus();
                }
                _ => {
                    self.input.event(event).await;
                }
            }
        }

        None
    }
}
