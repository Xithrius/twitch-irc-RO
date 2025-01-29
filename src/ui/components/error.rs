use tui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::{
    ui::components::Component,
    utils::styles::{NO_COLOR, TEXT_DARK_STYLE},
};

#[derive(Debug, Clone)]
pub struct ErrorWidget {
    message: Vec<&'static str>,
    focused: bool,
}

impl ErrorWidget {
    pub const fn new(message: Vec<&'static str>) -> Self {
        Self {
            message,
            focused: false,
        }
    }

    pub const fn is_focused(&self) -> bool {
        self.focused
    }

    pub fn toggle_focus(&mut self) {
        self.focused = !self.focused;
    }
}

impl Component<()> for ErrorWidget {
    fn draw(&mut self, f: &mut Frame, area: Option<Rect>) {
        let r = area.map_or_else(|| f.area(), |a| a);

        let paragraph = Paragraph::new(
            self.message
                .iter()
                .map(|&s| Line::from(vec![Span::raw(s)]))
                .collect::<Vec<Line>>(),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(if *NO_COLOR {
                    Style::default()
                } else {
                    Style::default().fg(Color::Red)
                })
                .title_top(Line::from("[ ERROR ]").centered()),
        )
        .style(*TEXT_DARK_STYLE)
        .alignment(Alignment::Center);

        f.render_widget(Clear, r);
        f.render_widget(paragraph, r);
    }
}
