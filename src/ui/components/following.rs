use once_cell::sync::Lazy;

use tui::{layout::Rect, Frame};

use crate::{
    handlers::{config::SharedCompleteConfig, user_input::events::Event},
    terminal::TerminalAction,
    twitch::{
        channels::{Following, FollowingStreaming, FollowingUser, StreamingUser},
        TwitchAction,
    },
    ui::components::Component,
};

use super::utils::SearchWidget;

static INCORRECT_SCOPES_ERROR_MESSAGE: Lazy<Vec<&'static str>> = Lazy::new(|| {
    vec![
        "Failed to get the list of streamers you currently follow.",
        "Either you have incorrect scopes in your token, or the API is down.",
        "To get the correct scopes, see the default config at the link below:",
        "https://github.com/Xithrius/twitch-tui/blob/main/default-config.toml#L8-L13",
        "",
        "Hit ESC to dismiss this error.",
    ]
});

type SearchFollowingStreamingWidget = SearchWidget<StreamingUser, FollowingStreaming>;
type SearchFollowingWidget = SearchWidget<FollowingUser, Following>;

pub enum SearchWidgetType {
    All(SearchFollowingWidget),
    Live(SearchFollowingStreamingWidget),
}

pub struct FollowingWidget {
    #[allow(dead_code)]
    config: SharedCompleteConfig,
    pub search_widget: SearchWidgetType,
    // pub search_widget: SearchWidget<String, Following>,
}

impl FollowingWidget {
    pub fn new(config: SharedCompleteConfig) -> Self {
        let search_widget = if config.borrow().frontend.only_show_live_channels {
            SearchWidgetType::Live(SearchWidget::new(
                config.clone(),
                FollowingStreaming::new(config.borrow().twitch.clone()),
                INCORRECT_SCOPES_ERROR_MESSAGE.to_vec(),
            ))
        } else {
            SearchWidgetType::All(SearchWidget::new(
                config.clone(),
                Following::new(config.borrow().twitch.clone()),
                INCORRECT_SCOPES_ERROR_MESSAGE.to_vec(),
            ))
        };

        Self {
            config,
            search_widget,
        }
    }

    pub const fn is_focused(&self) -> bool {
        match &self.search_widget {
            SearchWidgetType::All(w) => w.is_focused(),
            SearchWidgetType::Live(w) => w.is_focused(),
        }
    }

    pub async fn toggle_focus(&mut self) {
        match &mut self.search_widget {
            SearchWidgetType::All(w) => w.toggle_focus().await,
            SearchWidgetType::Live(w) => w.toggle_focus().await,
        }
    }
}

impl Component<TwitchAction> for FollowingWidget {
    fn draw(&mut self, f: &mut Frame, area: Option<Rect>) {
        match &mut self.search_widget {
            SearchWidgetType::All(w) => w.draw(f, area),
            SearchWidgetType::Live(w) => w.draw(f, area),
        }
    }

    async fn event(&mut self, event: &Event) -> Option<TerminalAction<TwitchAction>> {
        let w_event = match &mut self.search_widget {
            SearchWidgetType::All(w) => w
                .event(event)
                .await
                .map(|ta| ta.map_enter(|a| a.broadcaster_name.clone())),
            SearchWidgetType::Live(w) => w
                .event(event)
                .await
                .map(|ta| ta.map_enter(|a| a.user_login.clone())),
        };

        let action = w_event.map(|ta| match ta {
            TerminalAction::Enter(s) => TerminalAction::Enter(TwitchAction::Join(s)),
            TerminalAction::Quit => TerminalAction::Quit,
            TerminalAction::BackOneLayer => TerminalAction::BackOneLayer,
            TerminalAction::SwitchState(s) => TerminalAction::SwitchState(s),
            TerminalAction::ClearMessages => TerminalAction::ClearMessages,
        });

        if let Some(TerminalAction::Enter(TwitchAction::Join(channel))) = &action {
            self.config.borrow_mut().twitch.channel.clone_from(&channel);
        }

        action
    }
}
