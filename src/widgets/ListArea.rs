use ratatui::prelude::*;
use ratatui::widgets::{Block, List, ListDirection, ListItem, ListState, Widget};

use crate::AppState::AppState::AppStateContainer;
use crate::extensions::OnceLock::OnceLock_ext;
use crate::extensions::Rect::RectExtension;
use crate::{AppState, get_decorated_border};

#[derive(Default)]
pub struct ListArea {}

impl StatefulWidget for ListArea {
    type State = AppStateContainer;
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut AppStateContainer) {
        use AppState::AppState::*;
        let mut list_state = ListState::default();
        list_state.select_first();

        let block = get_decorated_border!(state.focus_state, Tabs::ListArea);


        let items = [
            "[Gusteau]: With enough passion, yes.",
            "[Remy]: But can anyone build a TUI in Rust?",
            "[Gusteau]: Anyone can cook!",
            &format!("focus_state_mutex_ref: {:?}", state.play_list),
        ];

       // let list = List::new(*play_list_mutex.iter().map(|s| ListItem::new(s.as_str())));

        let list = List::new(items.clone())
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

        StatefulWidget::render(list, area.margin(None), buf, &mut list_state);
    }
}
