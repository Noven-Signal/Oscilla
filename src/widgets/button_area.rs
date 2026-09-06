use crate::app::App;
use crate::extensions::rect::RectExtension;
use crate::get_decorated_border;
use crate::key_guide::{KeyGuide, LineExt};

use crate::widgets::button::ButtonState;

use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

use crate::app_state::app_state::{
    AppStateContainer, AreaHandler, TabState, Tabs,
};

use crate::widgets::button::{Button, ButtonIdent};

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
        Button::new(
            ButtonIdent::PlayOrPause(state.play_state.to_play_button_state()),
            match_state!(ButtonIdent::PlayOrPause(_)),
        )
        .render(play_button_area, buf, state);

        Button::new(ButtonIdent::Prev, match_state!(ButtonIdent::Prev)).render(
            prev_button_area,
            buf,
            state,
        );
        Button::new(ButtonIdent::Next, match_state!(ButtonIdent::Next)).render(
            next_button_area,
            buf,
            state,
        );

        // ListArea
    }
}

impl AreaHandler for ButtonsArea {
    fn get_tab_selected_handler(app_state_container: &mut AppStateContainer) {
        app_state_container.button_focus_state =
            ButtonIdent::PlayOrPause(app_state_container.play_state.to_play_button_state());
    }

    fn handle_key(app_state_container: &mut AppStateContainer, key_code: KeyCode) {
        let focused_button_ident = app_state_container.button_focus_state;
        if let Some(next) = focused_button_ident.get_next_focus(app_state_container, key_code) {
            app_state_container.button_focus_state = next
        }

        if key_code != KeyCode::Enter {
            return;
        }

        match focused_button_ident {
            ButtonIdent::PlayOrPause(_) => {
               App::toggle_play_pause(app_state_container);
            }
            ButtonIdent::Prev => App::play_previous(app_state_container),
            ButtonIdent::Next => App::play_next(app_state_container),
        }
    }

    fn get_disp_bottom_line_text_area_selected<'a>(
        app_state_container: &mut AppStateContainer,
        available_width: usize,
    ) -> Line<'a> {
        let key_guides = match app_state_container.button_focus_state {
            ButtonIdent::PlayOrPause(_) => [
                KeyGuide::ESC_DEFAULT,
                KeyGuide::new_mazenta("Enter", "Play"),
                KeyGuide::new_mazenta("→", "Move"),
            ],
            ButtonIdent::Prev => [
                KeyGuide::ESC_DEFAULT,
                KeyGuide::new_mazenta("Enter", "Play Prev Item"),
                KeyGuide::new_mazenta("←/→", "Move"),
            ],
            ButtonIdent::Next => [
                KeyGuide::ESC_DEFAULT,
                KeyGuide::new_mazenta("Enter", "Play Next Item"),
                KeyGuide::new_mazenta("←", "Move"),
            ],
        };
        Line::from_key_guide(
            key_guides
                .into_iter()
                .chain(KeyGuide::get_global_gudies(app_state_container)),
            available_width,
        )
    }
}
