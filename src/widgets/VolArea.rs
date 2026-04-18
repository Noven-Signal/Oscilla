use std::sync::atomic::Ordering;

use crate::extensions::Rect::RectExtension;
use crate::extensions::SelectBlock::SelectedBlock;
use crate::get_decorated_border;
use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::{Block, LineGauge};
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

use crate::AppState::AppState::{
    AreaHandler, TabState, Tabs, button_focus_state, focus_state, vol_state,
};
use crate::extensions::OnceLock::OnceLock_ext;


#[derive(Default)]
pub struct VolArea {}

impl Widget for VolArea {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let focus_state_mutex = focus_state.get_mutex_guard();

        let [guarge_area, digit_area] = area.margin(None).layout(
            &Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Fill(1), Constraint::Length(3)]),
        );

        get_decorated_border!(focus_state_mutex, Tabs::VolArea).render(area, buf);

        let vol = vol_state.load(Ordering::Relaxed);
        let volume_bar = LineGauge::default()
            .filled_style(Style::new().white().on_magenta().bold())
            .unfilled_style(Style::new().gray().on_black())
            .label("vol ")
            .ratio(vol as f64 / 100.0)
            .filled_symbol(symbols::line::HORIZONTAL)
            .unfilled_symbol(symbols::line::LIGHT_TRIPLE_DASH_HORIZONTAL);

        volume_bar.render(guarge_area, buf);
        Span::from(format!("{vol}")).render(digit_area, buf);
    }
}

impl AreaHandler for VolArea {
    fn handle_key(key_code: KeyCode) {
        let move_quantity: i16 = match key_code {
            KeyCode::Up | KeyCode::Right => 1,
            KeyCode::Down | KeyCode::Left => -1,
            _ => 0,
        };
        let after = vol_state.load(Ordering::Relaxed) as i16 + move_quantity;
        let after = match after {
            ..=0 => 0,
            100.. => 100,
            x => x,
        };
        vol_state.store(after as u16, Ordering::Relaxed);
    }
}
