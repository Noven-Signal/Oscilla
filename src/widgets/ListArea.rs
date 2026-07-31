use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, HighlightSpacing, List, ListDirection, ListItem, Widget};
use crate::AppState::AppState::{AppStateContainer, AreaHandler};
use crate::app::App;
use crate::extensions::Rect::RectExtension;
use crate::{AppState, get_decorated_border};

#[derive(Default)]
pub struct ListArea {}

impl StatefulWidget for ListArea {
    type State = AppStateContainer;
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut AppStateContainer) {
        use AppState::AppState::*;

        let block = get_decorated_border!(state.focus_state, Tabs::ListArea);

        let items = state.play_list.iter().enumerate().map(|(i, x)| {
            let name = x.get_disp_name();
            let is_selected_item = state.play_list_selected.selected().is_some_and(|x| x == i);
            use PlayState::*;
            match state.play_state {
                Playing(idx) | Paused(idx) if idx == i => ListItem::new(
                    Span::raw(format!("♬  {name}")).style(match is_selected_item {
                        true => Color::Yellow,
                        false => Color::Magenta,
                    }),
                ),
                _ if is_selected_item => ListItem::new(
                    Span::raw("> ").style(Color::Red) + Span::raw(name).style(Color::Yellow),
                ),
                _ => ListItem::new(name),
            }
        });

        let list = List::new(items)
            .style(Color::White)
            //.highlight_style(Style::new().yellow().italic())
            .highlight_spacing(HighlightSpacing::Never)
            //.highlight_symbol("> ".red())
            .scroll_padding(1)
            .direction(ListDirection::TopToBottom)
            .repeat_highlight_symbol(true);

        let block = match block {
            Some(block) => block,
            None => Block::bordered().border_style(Style::new().fg(Color::White)),
        };
        block.title("song list").render(area, buf);
        StatefulWidget::render(list, area.margin(None), buf, &mut state.play_list_selected);
    }
}

impl AreaHandler for ListArea {
    fn handle_key(app_state_container: &mut AppStateContainer, key_code: KeyCode) {
        match key_code {
            KeyCode::Up => app_state_container.play_list_selected.select_previous(),
            KeyCode::Down => app_state_container.play_list_selected.select_next(),
            KeyCode::Enter => 'b1: {
                let Some(idx) = app_state_container.play_list_selected.selected() else {
                    break 'b1;
                };
                App::play_track(app_state_container, idx);
            }
            _ => {}
        }
    }

    fn lost_tab_selection_handler(app_state_container: &mut AppStateContainer) {
        app_state_container.play_list_selected.select(None);
    }
}
