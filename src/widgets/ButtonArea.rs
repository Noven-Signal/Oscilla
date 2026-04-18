use crate::get_decorated_border;
use crate::widgets::Button::ButtonState;
use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::{Block, LineGauge};
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

use crate::AppState::AppState::{AreaHandler, TabState, Tabs, button_focus_state, focus_state};
use crate::components::home::{RectExtension, SelectedBlock};
use crate::extensions::OnceLock::OnceLock_ext;

use crate::widgets::Button::{Button, ButtonIdent};

#[derive(Default)]
pub struct ButtonsArea {}

impl Widget for ButtonsArea {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let focus_state_mutex = focus_state.get_mutex_guard();
        let button_focus_state_mutex = button_focus_state.get_mutex_guard();


        get_decorated_border!(focus_state_mutex,Tabs::ButtonsArea).render(area, buf);

        let [_, play_button_area, prev_button_area, next_button_area, _] =
            area.margin(None).layout(
                &Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Max(0),
                        Constraint::Length(10),
                        Constraint::Length(10),
                        Constraint::Length(10),
                        Constraint::Max(0),
                    ])
                    .flex(layout::Flex::SpaceAround),
            );

        macro_rules! match_state {
            ($button_ident: pat) => {
                match (*focus_state_mutex, *button_focus_state_mutex) {
                    (TabState::Selected(Tabs::ButtonsArea), $button_ident) => ButtonState::Focused,
                    _ => ButtonState::Normal,
                }
            };
        }
        Button::new(ButtonIdent::Play, match_state!(ButtonIdent::Play), || {})
            .render(play_button_area, buf);
        Button::new(ButtonIdent::Prev, match_state!(ButtonIdent::Prev), || {})
            .render(prev_button_area, buf);
        Button::new(ButtonIdent::Next, match_state!(ButtonIdent::Next), || {})
            .render(next_button_area, buf);
    }
}

impl AreaHandler for ButtonsArea {
    fn get_tab_selected_handler() {
        let mut button_state_mutex = button_focus_state.get_mutex_guard();
        *button_state_mutex = ButtonIdent::Play;
    }

    fn handle_key(key_code: KeyCode) {
        let mut button_state_mutex = button_focus_state.get_mutex_guard();
        match (*button_state_mutex).get_next_focus(key_code) {
            Some(next) => *button_state_mutex = next,
            None => {}
        }
    }

}
