use ratatui::prelude::*;
use ratatui::widgets::{Block, List, ListDirection, ListState, Widget};

use crate::AppState::AppState::{TabState, Tabs};
use crate::extensions::OnceLock::OnceLock_ext;
use crate::extensions::Rect::RectExtension;
use crate::extensions::SelectBlock::SelectedBlock;
use crate::widgets::BottomPart::BottomPart;
use crate::widgets::ListArea::*;
use crate::{AppState, get_decorated_border};

#[derive(Default, Clone, Copy)]
pub struct AppRoot {}

impl Widget for AppRoot {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // fn draw(&mut self, f: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let [top, bottom] = area.layout(
            &Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Fill(1), Constraint::Length(6)]),
        );

        let [list_area, effect_area] = top.layout(
            &Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(25), Constraint::Percentage(75)]),
        );
        // use AppState::AppState::*;
        // let mut state = ListState::default();
        // state.select_first();

        // let focus_state_mutex = focus_state.get_mutex_guard();

        // let block = get_decorated_border!(focus_state_mutex, Tabs::ListArea);

        // let items = [
        //     "[Gusteau]: With enough passion, yes.",
        //     "[Remy]: But can anyone build a TUI in Rust?",
        //     "[Gusteau]: Anyone can cook!",
        //     &format!("focus_state_mutex_ref: {:?}", *focus_state_mutex),
        // ];
        // drop(focus_state_mutex);

        // let list = List::new(items)
        //     .style(Color::White)
        //     .highlight_style(Style::new().yellow().italic())
        //     .highlight_symbol("> ".red())
        //     .scroll_padding(1)
        //     .direction(ListDirection::TopToBottom)
        //     .repeat_highlight_symbol(true);

        // if let Some(block) = block {
        //     block.title("song list").render(list_area, buf);
        // }

       
        ListArea::default().render(list_area,buf);
        //f.render_stateful_widget(list, list_area.margin(None), &mut state);
        Block::default().render(effect_area, buf);
        //f.render_widget(Block::default(), effect_area);
        BottomPart::default().render(bottom, buf);
        //f.render_widget(BottomPart::default(), bottom);
    }
}
