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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    Normal,
    Focused,
}

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub enum ButtonIdent {
    PlayOrPause(PlayButtonState),
    Prev,
    Next,
}

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub enum PlayButtonState {
    Playing,
    Paused,
    Stopped
}

impl ButtonIdent {
    const fn get_disp_name(&self) -> &'static str {
        match self {
            ButtonIdent::PlayOrPause(PlayButtonState::Playing) => "Pause",
            ButtonIdent::PlayOrPause(PlayButtonState::Paused) => "Resume",
            ButtonIdent::PlayOrPause(PlayButtonState::Stopped) => "Pause",
            ButtonIdent::Prev => "prev",
            ButtonIdent::Next => "next",
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
            PlayOrPause(_) => match key_code {
                Left => None,
                Right => Some(Prev),
                _ => None,
            },
            Prev => match key_code {
                KeyCode::Left => Some(Self::PlayOrPause(
                    app_state_container.play_state.to_play_button_state(),
                )),
                KeyCode::Right => Some(Next),
                _ => None,
            },
            Next => match key_code {
                Left => Some(Prev),
                Right => None,
                _ => None,
            },
        }
    }
}

impl<'a> Button<'a> {
    pub fn new(ident: ButtonIdent, state: ButtonState) -> Self {
        Button {
            label: ident.get_disp_name().into(),
            state: state,
        }
    }
}

impl StatefulWidget for Button<'_> {
    type State = AppStateContainer;
    fn render(self, area: Rect, buf: &mut Buffer, _state: &mut Self::State) {
        let (background_color, text_color) = match &self.state {
            ButtonState::Normal => (Color::Rgb(0, 100, 0), Color::White),
            ButtonState::Focused => (Color::Magenta, Color::White),
        };

        Line::from(self.label)
            .centered()
            .style(Style::new().bg(background_color).fg(text_color))
            .render(area, buf);
    }
}
