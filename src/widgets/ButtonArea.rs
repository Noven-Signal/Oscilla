use crate::extensions::Rect::RectExtension;
use crate::get_decorated_border;
use crate::widgets::Button::ButtonState;
use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::{Block, LineGauge};
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

use crate::AppState::AppState::{AppStateContainer, AreaHandler, TabState, Tabs};
use crate::extensions::OnceLock::OnceLock_ext;

use crate::widgets::Button::{Button, ButtonIdent};

#[derive(Default)]
pub struct ButtonsArea {}

impl StatefulWidget for ButtonsArea {
    type State = AppStateContainer;
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        get_decorated_border!(state.focus_state, Tabs::ButtonsArea).render(area, buf);

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
                match (state.focus_state, state.button_focus_state) {
                    (TabState::Selected(Tabs::ButtonsArea), $button_ident) => ButtonState::Focused,
                    _ => ButtonState::Normal,
                }
            };
        }
        Button::new(ButtonIdent::Play, match_state!(ButtonIdent::Play), || {}).render(
            play_button_area,
            buf,
            state,
        );
        Button::new(ButtonIdent::Prev, match_state!(ButtonIdent::Prev), || {}).render(
            prev_button_area,
            buf,
            state,
        );
        Button::new(ButtonIdent::Next, match_state!(ButtonIdent::Next), || {}).render(
            next_button_area,
            buf,
            state,
        );
    }
}

impl AreaHandler for ButtonsArea {
    fn get_tab_selected_handler(app_state_container: &mut AppStateContainer) {
        app_state_container.button_focus_state = ButtonIdent::Play;
    }

    fn handle_key(app_state_container: &mut AppStateContainer, key_code: KeyCode) {
        match app_state_container
            .button_focus_state
            .get_next_focus(key_code)
        {
            Some(next) => app_state_container.button_focus_state = next,
            None => {}
        }
    }
}
