use crate::app::App;
use crate::extensions::rect::RectExtension;
use crate::key_guide::{KeyGuide, LineExt};
use crate::{get_decorated_border, rgb_color};
use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::LineGauge;
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

use crate::app_state::app_state::{AppStateContainer, AreaHandler, Tabs};

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
            .filled_style(
                Style::new()
                    .fg(rgb_color::WHITE)
                    .bg(rgb_color::MAGENTA)
                    .bold(),
            )
            .unfilled_style(Style::new().fg(rgb_color::GRAY).bg(rgb_color::BLACK))
            .label("vol ")
            .ratio(state.vol_state as f64 / 100.0)
            .filled_symbol(symbols::line::HORIZONTAL)
            .unfilled_symbol(symbols::line::LIGHT_TRIPLE_DASH_HORIZONTAL);

        volume_bar.render(guarge_area, buf);
        Span::from(format!("{vol}")).render(digit_area, buf);
    }
}

impl AreaHandler for VolArea {
    fn handle_key(app_state_container: &mut AppStateContainer, key_code: KeyCode) {
        let move_quantity: i16 = match key_code {
            KeyCode::Up => 10,
            KeyCode::Right => 1,
            KeyCode::Down => -10,
            KeyCode::Left => -1,
            _ => return,
        };
        App::move_vol(app_state_container, move_quantity);
    }

    fn get_disp_bottom_line_text_area_selected<'a>(
        app_state_container: &mut AppStateContainer,
        available_width: usize,
    ) -> Line<'a> {
        Line::from_key_guide(
            [
                KeyGuide::ESC_DEFAULT,
                KeyGuide::new_mazenta("↑/↓", "Volume ±10"),
                KeyGuide::new_mazenta("←/→", "Volume ±1"),
            ]
            .into_iter()
            .chain(KeyGuide::get_global_gudies(app_state_container)),
            available_width,
        )
    }
}
