use crate::app_state::app_state::AppStateContainer;
use crate::widgets::bottom_part::BottomPart;
use crate::widgets::popup::Popup;
use crate::widgets::{effect_area::*, list_area::*};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Clear, Widget};

#[derive(Default, Clone, Copy)]
pub struct AppRoot {}

impl StatefulWidget for AppRoot {
    type State = AppStateContainer;
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut AppStateContainer) {
        Block::default()
            .bg(Color::Rgb(20, 20, 28))
            .render(area, buf);

        let [top, bottom] = area.layout(
            &Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Fill(1), Constraint::Length(7)]),
        );

        let [list_area, effect_area] = top.layout(
            &Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(25), Constraint::Percentage(75)]),
        );

        ListArea::default().render(list_area, buf, state);
        EffectArea::default().render(effect_area, buf, state);
        BottomPart::default().render(bottom, buf, state);

        if state.popup_object.is_some() {
            let [_, horizonal_center, _] = area.layout(
                &Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Fill(1),
                        Constraint::Length(40),
                        Constraint::Fill(1),
                    ]),
            );
            let [_, popup, _] = horizonal_center.layout(
                &Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Fill(1),
                        Constraint::Length(7),
                        Constraint::Fill(1),
                    ]),
            );
            Clear::default().render(popup, buf);
            Popup::default().render(popup, buf, state);
        }
    }
}
