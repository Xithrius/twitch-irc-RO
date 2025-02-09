use tui::{
    layout::{Constraint, Rect},
    widgets::{Block, Borders, Cell, Row, Table},
    Frame,
};

use crate::{
    handlers::config::SharedCompleteConfig,
    ui::{
        components::Component,
        statics::{HELP_COLUMN_TITLES, HELP_KEYBINDS},
    },
    utils::styles::{BOLD_STYLE, COLUMN_TITLE_STYLE},
};

// Once a solution is found to calculate constraints, this will be removed.
const TABLE_CONSTRAINTS: [Constraint; 3] =
    [Constraint::Min(11), Constraint::Min(8), Constraint::Min(38)];

#[derive(Debug, Clone)]
pub struct HelpWidget {
    config: SharedCompleteConfig,
}

impl HelpWidget {
    pub const fn new(config: SharedCompleteConfig) -> Self {
        Self { config }
    }
}

impl Component<()> for HelpWidget {
    fn draw(&mut self, f: &mut Frame, area: Option<Rect>) {
        let r = area.map_or_else(|| f.area(), |a| a);

        let mut rows = vec![];

        for (s, v) in HELP_KEYBINDS.iter() {
            for (i, (key, desc)) in v.iter().enumerate() {
                rows.push(Row::new(vec![
                    if i == 0 {
                        Cell::from((*s).to_string())
                    } else {
                        Cell::from("")
                    }
                    .style(*BOLD_STYLE),
                    Cell::from((*key).to_string()),
                    Cell::from((*desc).to_string()),
                ]));
            }

            rows.push(Row::new(vec![Cell::from("")]));
        }

        let help_table = Table::new(rows, TABLE_CONSTRAINTS)
            .header(Row::new(HELP_COLUMN_TITLES.iter().copied()).style(*COLUMN_TITLE_STYLE))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("[ Keybinds ]")
                    .border_type(self.config.borrow().frontend.border_type.clone().into()),
            )
            .column_spacing(2);

        f.render_widget(help_table, r);
    }
}
