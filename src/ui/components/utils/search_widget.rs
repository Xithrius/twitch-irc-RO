use std::{clone::Clone, convert::From, iter::Iterator, vec::Vec};

use color_eyre::Result;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use once_cell::sync::Lazy;
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
        user_input::events::{Event, Key},
    },
    terminal::TerminalAction,
    ui::components::{Component, ErrorWidget},
    utils::{
        styles::{NO_COLOR, SEARCH_STYLE, TITLE_STYLE},
        text::{title_line, TitleStyle},
    },
};

use super::{centered_rect, InputWidget};

static FUZZY_FINDER: Lazy<SkimMatcherV2> = Lazy::new(SkimMatcherV2::default);

pub trait SearchItemGetter<T>
where
    T: ToString,
{
    async fn get_items(&mut self) -> Result<Vec<T>>;
}

pub trait ToQueryString {
    fn to_query_string(&self) -> String;
}

// WARN:
// This prevents other traits from implementing `ToQueryString`, unless we use "specialization".
// See : https://rust-lang.github.io/rfcs/1210-impl-specialization.html
// impl<T: ToString> ToQueryString for T {
//     fn to_query_string(&self) -> String {
//         self.to_string()
//     }
// }

pub struct SearchWidget<T, U>
where
    T: ToString + Clone + ToQueryString,
    U: SearchItemGetter<T>,
{
    config: SharedCompleteConfig,
    focused: bool,

    item_getter: U,
    items: Result<Vec<T>>,
    filtered_items: Option<Vec<T>>,

    list_state: ListState,
    search_input: InputWidget<()>,
    vertical_scroll_state: ScrollbarState,
    vertical_scroll: usize,

    error_widget: ErrorWidget,
}

impl<T, U> SearchWidget<T, U>
where
    T: ToString + Clone + ToQueryString,
    U: SearchItemGetter<T>,
{
    pub fn new(
        config: SharedCompleteConfig,
        item_getter: U,
        error_message: Vec<&'static str>,
    ) -> Self {
        let search_input = InputWidget::new(config.clone(), "Search", None, None, None);
        let error_widget = ErrorWidget::new(error_message);

        Self {
            config,
            focused: false,
            item_getter,
            items: Ok(vec![]),
            filtered_items: None,
            list_state: ListState::default(),
            search_input,
            vertical_scroll_state: ScrollbarState::default(),
            vertical_scroll: 0,
            error_widget,
        }
    }

    fn next(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => {
                if let Some(filtered) = &self.filtered_items {
                    if i >= filtered.len().saturating_sub(1) {
                        filtered.len().saturating_sub(1)
                    } else {
                        i + 1
                    }
                } else if i >= self.items.as_ref().unwrap().len().saturating_sub(1) {
                    self.items.as_ref().unwrap().len().saturating_sub(1)
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

    pub async fn toggle_focus(&mut self) {
        if !self.focused {
            self.items = self.item_getter.get_items().await;
        }

        if self.items.is_err() {
            self.error_widget.toggle_focus();
        }

        self.focused = !self.focused;
    }
}

impl<T, U> Component<T> for SearchWidget<T, U>
where
    T: ToString + Clone + ToQueryString,
    U: SearchItemGetter<T>,
{
    fn draw(&mut self, f: &mut Frame, area: Option<Rect>) {
        let r = area.map_or_else(|| centered_rect(60, 60, 20, f.area()), |a| a);

        if self.error_widget.is_focused() {
            self.error_widget.draw(f, Some(r));

            return;
        }

        let mut items = vec![];
        let current_items = &self.items.as_ref().map_or(vec![], Clone::clone);
        let current_input = self.search_input.to_string();

        if current_input.is_empty() {
            for item in current_items {
                items.push(ListItem::new(item.to_string()));
            }

            self.filtered_items = None;
        } else {
            let item_filter = |c: String| -> Vec<usize> {
                FUZZY_FINDER
                    .fuzzy_indices(&c, &current_input)
                    .map(|(_, indices)| indices)
                    .unwrap_or_default()
            };

            let mut matched = vec![];

            for item in current_items.clone() {
                let matched_indices = item_filter(item.to_query_string());

                if matched_indices.is_empty() {
                    continue;
                }

                let line = item
                    .to_string()
                    .chars()
                    .enumerate()
                    .map(|(i, c)| {
                        if matched_indices.contains(&i) {
                            Span::styled(c.to_string(), *SEARCH_STYLE)
                        } else {
                            Span::raw(c.to_string())
                        }
                    })
                    .collect::<Vec<Span>>();

                items.push(ListItem::new(vec![Line::from(line)]));
                matched.push(item);
            }

            self.filtered_items = Some(matched);
        }

        let title_binding = [TitleStyle::Single("Following")];

        let list = List::new(items.clone())
            .block(
                Block::default()
                    .title(title_line(&title_binding, *TITLE_STYLE))
                    .borders(Borders::ALL)
                    .border_type(self.config.borrow().frontend.border_type.clone().into()),
            )
            .highlight_style(if *NO_COLOR {
                Style::default()
            } else {
                Style::default()
                    .bg(Color::LightGreen)
                    .add_modifier(Modifier::BOLD)
            });

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
            self.filtered_items
                .as_ref()
                .map_or(current_items.len(), Vec::len)
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

        let input_rect = Rect::new(rect.x, rect.bottom(), rect.width, 3);

        self.search_input.draw(f, Some(input_rect));
    }

    async fn event(&mut self, event: &Event) -> Option<TerminalAction<T>> {
        if self.error_widget.is_focused() && matches!(event, Event::Input(Key::Esc)) {
            self.error_widget.toggle_focus();
            self.toggle_focus().await;

            return None;
        }

        if let Event::Input(key) = event {
            match key {
                Key::Esc => {
                    if self.list_state.selected().is_some() {
                        self.unselect();
                    } else {
                        self.toggle_focus().await;
                    }
                }
                Key::ScrollDown | Key::Down => self.next(),
                Key::ScrollUp | Key::Up => self.previous(),
                Key::Enter => {
                    if let Some(i) = self.list_state.selected() {
                        let selected_item = if let Some(v) = self.filtered_items.clone() {
                            if v.is_empty() {
                                return None;
                            }

                            v.get(i).unwrap().clone()
                        } else {
                            self.items.as_ref().unwrap().get(i).unwrap().clone()
                        };

                        self.toggle_focus().await;

                        self.unselect();

                        return Some(TerminalAction::Enter(selected_item.clone()));
                    }
                }
                _ => {
                    self.search_input.event(event).await;

                    // Assuming that the user inputted something that modified the input
                    if let Some(v) = &self.filtered_items {
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
