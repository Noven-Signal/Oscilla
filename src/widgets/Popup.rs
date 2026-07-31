use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Paragraph, Widget, Wrap};

use crate::app_state::app_state::AppStateContainer;
use crate::app::PopupObject;

#[derive(Default, Clone, Copy)]
pub struct Popup {}

impl StatefulWidget for Popup {
    type State = AppStateContainer;
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut AppStateContainer) {
        let Some(PopupObject {
            title,
            message,
            button_name,
        }) = &state.popup_object
        else {
            return;
        };
        Block::new().bg(Color::Magenta).render(area, buf);

        let [title_area, message_area, _, button_area, _] = area.layout(
            &Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Length(4),
                    Constraint::Fill(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                ]),
        );
        let [_, button_area, _] = button_area.layout(
            &Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Fill(1),
                    Constraint::Length(6),
                    Constraint::Fill(1),
                ]),
        );
        Line::from(title.as_str().bold())
            .centered()
            .render(title_area, buf);
        Paragraph::new(message.as_str())
            .wrap(Wrap { trim: true })
            .centered()
            .render(message_area, buf);
        Line::from(button_name.as_str())
            .centered()
            .bg(Color::DarkGray)
            .underlined()
            .render(button_area, buf);
    }
}

impl Popup {
    pub fn handle_key(app_state_container: &mut AppStateContainer, key_code: KeyCode) {
        match key_code {
            KeyCode::Enter => {
                app_state_container.popup_object = None;
            }
            _ => {}
        }
    }
}
