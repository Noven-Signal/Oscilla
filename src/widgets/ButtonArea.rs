use crate::app::App;
use crate::extensions::Rect::RectExtension;
use crate::manipulation::PlayerControlSignal;
use crate::widgets::Button::{ButtonState, PlayButtonState};
use crate::{app, get_decorated_border, manipulation};
use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::{Block, LineGauge};
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};
use tokio::sync::mpsc::unbounded_channel;

use crate::AppState::AppState::{
    AppStateContainer, AreaHandler, PlayState, PlayerThread, TabState, Tabs,
};
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
            ButtonIdent::PlayOrPause(player_button_state) => 'play_arm: {
                let Some(PlayerThread {
                    player_control_singnal_sender: sender,
                    ..
                }) = &mut app_state_container.player_thread
                else {
                    break 'play_arm;
                };

                let play_state = &mut app_state_container.play_state;
                let player_control_signal = match play_state {
                    PlayState::Playing(_) => PlayerControlSignal::Pause,
                    PlayState::Paused(_) => PlayerControlSignal::Resume,
                    _ => panic!(),
                };

                _ = sender.send(player_control_signal);

                use crate::AppState::AppState::PlayState::*;
                *play_state = match play_state {
                    Playing(idx) => Paused(*idx),
                    Paused(idx) => Playing(*idx),
                    Stopped => todo!(),
                };
            }
            ButtonIdent::Prev => App::play_previous(app_state_container),
            ButtonIdent::Next => App::play_next(app_state_container),
        }
    }
}
