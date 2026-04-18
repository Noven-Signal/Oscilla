use std::collections::HashMap;

use crossterm::event::KeyCode;
use ratatui::buffer::Buffer;

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::Widget;

use crate::AppState::AppState::button_handler_func_dic;
use crate::extensions::OnceLock::OnceLock_ext;

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

#[derive(PartialEq, Eq, Hash, Clone,Copy)]
pub enum ButtonIdent {
    Play,
    Prev,
    Next,
}
impl ButtonIdent {
    const fn nameof(&self) -> &'static str {
        match self {
            ButtonIdent::Play => "play",
            ButtonIdent::Prev => "prev",
            ButtonIdent::Next => "next",
        }
    }

    pub const fn get_next_focus(&self, key_code: KeyCode) -> Option<Self> {
        use ButtonIdent::*;
        use KeyCode::*;
        match (self, key_code) {
            (Play, code) => match code {
                Left => None,
                Right => Some(Prev),
                _ => None,
            },
            (Prev, code) => match code {
                KeyCode::Left => Some(Play),
                KeyCode::Right => Some(Next),
                _ => None,
            },
            (Next, code) => match code {
                Left => Some(Prev),
                Right => None,
                _ => None,
            },
        }
    }
}

/// A button with a label that can be themed.
impl<'a> Button<'a> {
    pub fn new(ident: ButtonIdent, state: ButtonState, on_pushed: impl Fn() + Send + 'static) -> Self {
        let mut button_focus_state_mutex_guard = button_handler_func_dic.get_mutex_guard();
        button_focus_state_mutex_guard.insert(ident.clone(), Box::new(on_pushed));
        Button {
            label: ident.nameof().into(),
            state: state,
        }
    }
}

impl Widget for Button<'_> {
    #[expect(clippy::cast_possible_truncation)]
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (background_color, text_color) = match &self.state {
            ButtonState::Normal => (Color::Rgb(0, 100, 0), Color::White),
            ButtonState::Focused => (Color::Magenta, Color::White),
        };
        // let background_color = Color::Rgb(0, 100, 0);
        // let text_color = Color::White;

        Line::from(self.label)
            .centered()
            .style(Style::new().bg(background_color).fg(text_color))
            .render(area, buf);

        // buf.set_style(area, Style::new().bg(background_color));

        // // render label centered
        // buf.set_line(
        //     area.x + (area.width.saturating_sub(self.label.width() as u16)) / 2,
        //     area.y + (area.height.saturating_sub(1)) / 2,
        //     &self.label,
        //     area.width,
        // );
    }
}


