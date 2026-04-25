use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::{Block, List, ListDirection, ListItem, ListState, Widget};

use crate::AppState::AppState::{AppStateContainer, AreaHandler};
use crate::extensions::OnceLock::OnceLock_ext;
use crate::extensions::Rect::RectExtension;
use crate::{AppState, get_decorated_border};

#[derive(Default)]
pub struct ListArea {}

impl StatefulWidget for ListArea {
    type State = AppStateContainer;
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut AppStateContainer) {
        use AppState::AppState::*;
        

        let block = get_decorated_border!(state.focus_state, Tabs::ListArea);

        // let list = List::new(*play_list_mutex.iter().map(|s| ListItem::new(s.as_str())));
        let items = state.play_list.iter().map(|x| x.as_str());

        let list = List::new(items)
            .style(Color::White)
            .highlight_style(Style::new().yellow().italic())
            .highlight_symbol("> ".red())
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
            _ => {}
        }
    }
}
