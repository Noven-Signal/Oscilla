use ratatui::prelude::*;
use ratatui::widgets::{Block, List, ListDirection, ListState, Widget};

use crate::AppState::AppState::{AppStateContainer, TabState, Tabs};
use crate::extensions::OnceLock::OnceLock_ext;
use crate::extensions::Rect::RectExtension;
use crate::extensions::SelectBlock::SelectedBlock;
use crate::widgets::BottomPart::BottomPart;

use crate::widgets::{EffectArea::*, ListArea::*};
use crate::{AppState, get_decorated_border};

#[derive(Default, Clone, Copy)]
pub struct AppRoot {}

impl StatefulWidget for AppRoot {
    type State = AppStateContainer;
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut AppStateContainer) {
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

        ListArea::default().render(list_area, buf, state);
        EffectArea::default().render(effect_area, buf, state);
        BottomPart::default().render(bottom, buf, state);
    }
}
