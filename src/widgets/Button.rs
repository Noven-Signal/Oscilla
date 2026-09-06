use crossterm::event::KeyCode;
use ratatui::buffer::Buffer;

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{StatefulWidget, Widget};

use crate::app_state::app_state::AppStateContainer;

/// A custom widget that renders a button with a label, theme and state.
#[derive(Debug, Clone)]
pub struct Button<'a> {
    label: Line<'a>,
    state: ButtonState,
    enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    Normal,
    Focused,
}

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub enum ButtonIdent {
    PlayOrPauseOrResume(PlayButtonState),
    Prev,
    Next,
    Stop,
}

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub enum PlayButtonState {
    Playing,
    Paused,
    Stopped,
}

impl ButtonIdent {
    pub const fn get_disp_name(&self) -> &'static str {
        match self {
            ButtonIdent::PlayOrPauseOrResume(PlayButtonState::Playing) => "|| Pause",
            ButtonIdent::PlayOrPauseOrResume(PlayButtonState::Paused) => "||> Resume",
            ButtonIdent::PlayOrPauseOrResume(PlayButtonState::Stopped) => "| > Play",
            ButtonIdent::Prev => "< prev",
            ButtonIdent::Next => "next >",
            ButtonIdent::Stop => "■ Stop",
        }
    }

    pub fn get_next_focus(
        &self,
        app_state_container: &AppStateContainer,
        key_code: KeyCode,
    ) -> Option<Self> {
        use ButtonIdent::*;
        use KeyCode::*;
        match self {
            PlayOrPauseOrResume(_) => match key_code {
                Left => None,
                Right => Some(Prev),
                _ => None,
            },
            Prev => match key_code {
                Left => Some(Self::PlayOrPauseOrResume(
                    app_state_container.play_state.to_play_button_state(),
                )),
                Right => Some(Next),
                _ => None,
            },
            Next => match key_code {
                Left => Some(Prev),
                Right => Some(Stop),
                _ => None,
            },
            Stop => match key_code {
                Left => Some(Next),
                Right => None,
                _ => None,
            },
        }
    }
}

impl<'a> Button<'a> {
    pub fn new(ident: ButtonIdent, state: ButtonState, enabled: bool) -> Self {
        Button {
            label: ident.get_disp_name().into(),
            state: state,
            enabled,
        }
    }
}

impl StatefulWidget for Button<'_> {
    type State = AppStateContainer;

    fn render(self, area: Rect, buf: &mut Buffer, _state: &mut Self::State) {
        let (background_color, text_color) = match (&self.state, self.enabled) {
            (ButtonState::Normal, true) => (Color::Rgb(0, 100, 0), Color::White),
            (ButtonState::Normal, false) => (Color::Rgb(68, 83, 64), Color::White),
            (ButtonState::Focused, true) => (Color::Magenta, Color::White),
            (ButtonState::Focused, false) => (Color::Rgb(174, 140, 179), Color::White),
        };

        Line::from(self.label)
            .centered()
            .style(Style::new().bg(background_color).fg(text_color))
            .render(area, buf);
    }
}
