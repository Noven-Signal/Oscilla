use std::sync::atomic::Ordering;

use crate::extensions::Rect::RectExtension;
use crate::extensions::SelectBlock::SelectedBlock;
use crate::get_decorated_border;
use crate::manipulation::PlayerControlSignal;
use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::{Block, LineGauge};
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

use crate::AppState::AppState::{AppStateContainer, AreaHandler, TabState, Tabs};
use crate::extensions::OnceLock::OnceLock_ext;

#[derive(Default)]
pub struct VolArea {}

impl StatefulWidget for VolArea {
    type State = AppStateContainer;
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let [guarge_area, digit_area] = area.margin(None).layout(
            &Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Fill(1), Constraint::Length(3)]),
        );

        get_decorated_border!(state.focus_state, Tabs::VolArea).render(area, buf);
        let vol = state.vol_state;
        let volume_bar = LineGauge::default()
            .filled_style(Style::new().white().on_magenta().bold())
            .unfilled_style(Style::new().gray().on_black())
            .label("vol ")
            .ratio(state.vol_state as f64 / 100.0)
            .filled_symbol(symbols::line::HORIZONTAL)
            .unfilled_symbol(symbols::line::LIGHT_TRIPLE_DASH_HORIZONTAL);

        volume_bar.render(guarge_area, buf);
        Span::from(format!("{vol}")).render(digit_area, buf);
    }
}

impl AreaHandler for VolArea {
    fn handle_key(app_state_continer: &mut AppStateContainer, key_code: KeyCode) {
        let move_quantity: i16 = match key_code {
            KeyCode::Up | KeyCode::Right => 1,
            KeyCode::Down | KeyCode::Left => -1,
            _ => return
        };
        let after = app_state_continer.vol_state as i16 + move_quantity;
        let after = match after {
            ..=0 => 0,
            100.. => 100,
            x => x,
        };
        app_state_continer.vol_state = after as u16;
        if let Some(ref mut sender) = app_state_continer.player_control_singnal_sender {
            sender.send(PlayerControlSignal::SetVol(after as u16));
        }
    }
}
