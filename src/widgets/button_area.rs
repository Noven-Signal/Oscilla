use crate::app::App;
use crate::extensions::rect::RectExtension;
use crate::get_decorated_border;
use crate::key_guide::{KeyGuide, LineExt};

use crate::widgets::button::ButtonState;

use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

use crate::app_state::app_state::{AppStateContainer, AreaHandler, PlayState, TabState, Tabs};

use crate::widgets::button::{Button, ButtonIdent};

#[derive(Default)]
pub struct ButtonsArea {}

impl StatefulWidget for ButtonsArea {
    type State = AppStateContainer;
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        get_decorated_border!(state.focus_state, Tabs::ButtonsArea).render(area, buf);

        let [
            _,
            play_button_area,
            prev_button_area,
            next_button_area,
            stop_button_area,
            _,
        ] = area.margin(None).layout(
            &Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Max(0),
                    Constraint::Length(12),
                    Constraint::Length(12),
                    Constraint::Length(12),
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
            ButtonIdent::PlayOrPauseOrResume(state.play_state.to_play_button_state()),
            match_state!(ButtonIdent::PlayOrPauseOrResume(_)),
            is_play_button_enabled(state),
        )
        .render(play_button_area, buf, state);

        Button::new(
            ButtonIdent::Prev,
            match_state!(ButtonIdent::Prev),
            is_prev_button_enabled(state),
        )
        .render(prev_button_area, buf, state);
        Button::new(
            ButtonIdent::Next,
            match_state!(ButtonIdent::Next),
            is_next_button_enabled(state),
        )
        .render(next_button_area, buf, state);

        Button::new(
            ButtonIdent::Stop,
            match_state!(ButtonIdent::Stop),
            is_stop_button_enabled(state),
        )
        .render(stop_button_area, buf, state);

        // ListArea
    }
}

impl AreaHandler for ButtonsArea {
    fn get_tab_selected_handler(app_state_container: &mut AppStateContainer) {
        app_state_container.button_focus_state =
            ButtonIdent::PlayOrPauseOrResume(app_state_container.play_state.to_play_button_state());
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
            ButtonIdent::PlayOrPauseOrResume(_) => match app_state_container.play_state {
                PlayState::Stopped if app_state_container.play_list.first().is_some() => {
                    _ = app_state_container
                        .app_control_signal_sender
                        .send(crate::app::AppContorlSignal::StartPlayer(0))
                }
                PlayState::Playing(_) | PlayState::Paused(_) => {
                    App::toggle_play_pause(app_state_container)
                }
                _ => {}
            },
            ButtonIdent::Prev => App::play_previous(app_state_container),
            ButtonIdent::Next => App::play_next(app_state_container),
            ButtonIdent::Stop => App::stop_player(app_state_container),
        }
    }

    fn get_disp_bottom_line_text_area_selected<'a>(
        app_state_container: &mut AppStateContainer,
        available_width: usize,
    ) -> Line<'a> {
        let key_guides = match app_state_container.button_focus_state {
            ButtonIdent::PlayOrPauseOrResume(_) => [
                Some(KeyGuide::ESC_DEFAULT),
                is_play_button_enabled(app_state_container).then_some(KeyGuide::new_mazenta(
                    "Enter",
                    match app_state_container.play_state{
                        PlayState::Playing(_) => "Pause",
                        PlayState::Paused(_) => "Resume",
                        PlayState::Stopped => "Play"
                    },
                )),
                Some(KeyGuide::new_mazenta("→", "Move")),
            ],
            ButtonIdent::Prev => [
                Some(KeyGuide::ESC_DEFAULT),
                is_prev_button_enabled(app_state_container)
                    .then_some(KeyGuide::new_mazenta("Enter", "Play Prev Item")),
                Some(KeyGuide::new_mazenta("←/→", "Move")),
            ],
            ButtonIdent::Next => [
                Some(KeyGuide::ESC_DEFAULT),
                is_next_button_enabled(app_state_container)
                    .then_some(KeyGuide::new_mazenta("Enter", "Play Next Item")),
                Some(KeyGuide::new_mazenta("←/→", "Move")),
            ],
            ButtonIdent::Stop => [
                Some(KeyGuide::ESC_DEFAULT),
                is_stop_button_enabled(app_state_container)
                    .then_some(KeyGuide::new_mazenta("Enter", "Stop")),
                Some(KeyGuide::new_mazenta("←", "Move")),
            ],
        };
        Line::from_key_guide_optioanl(
            key_guides.into_iter().chain(
                KeyGuide::get_global_gudies(app_state_container)
                    .iter()
                    .map(|x| Some(*x)),
            ),
            available_width,
        )
    }
}

fn is_play_button_enabled(state: &AppStateContainer) -> bool {
    state.play_list.first().is_some()
}

fn is_prev_button_enabled(state: &AppStateContainer) -> bool {
    state
        .play_state
        .get_now_playing_idx()
        .is_some_and(|idx| idx != 0)
}

fn is_next_button_enabled(state: &AppStateContainer) -> bool {
    state
        .play_state
        .get_now_playing_idx()
        .is_some_and(|idx| state.play_list.get(idx + 1).is_some())
}

fn is_stop_button_enabled(state: &AppStateContainer) -> bool {
    state.play_state.get_now_playing_idx().is_some()
}
