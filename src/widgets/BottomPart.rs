use ratatui::prelude::*;
use ratatui::widgets::{Block, LineGauge, Widget};

use crate::AppState::AppState::{TabState, Tabs, focus_state};
use crate::components::home::{RectExtension, SelectedBlock};
use crate::extensions::OnceLock::OnceLock_ext;
use crate::get_decorated_border;
use crate::widgets::ButtonArea::ButtonsArea;
use crate::widgets::VolArea::VolArea;

pub struct BottomPart {
    track_duration: f64,
    current_time: f64,
}
impl BottomPart {
    pub fn default() -> Self {
        Self {
            track_duration: 101 as f64,
            current_time: 10 as f64,
        }
    }
}

impl Widget for BottomPart {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let focus_state_mutex = focus_state.get_mutex_guard();

        let [progressbar_area, buttons_and_volume_area] = area.layout(
            &Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Length(3)])
                .spacing(-1),
        );

        get_decorated_border!(focus_state_mutex,Tabs::DurationBarArea).render(progressbar_area, buf);

        let [progressbar_area, remaining_time_area] = progressbar_area.margin(None).layout(
            &Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Fill(1), Constraint::Length(3)]),
        );

        let duration_bar = LineGauge::default()
            .filled_style(Style::new().white().on_red().bold())
            .unfilled_style(Style::new().gray().on_black())
            .label(self.current_time.to_string())
            .ratio(self.current_time / self.track_duration)
            .filled_symbol(symbols::line::THICK_HORIZONTAL)
            .unfilled_symbol(symbols::line::THICK_HORIZONTAL);

        let remaining_time = (self.track_duration - self.current_time).to_string();
        let remaining_time = Text::from(remaining_time);

        remaining_time.render(remaining_time_area, buf);
        duration_bar.render(progressbar_area, buf);

        let [buttons_area, _fill, volume_area] = buttons_and_volume_area.layout(
            &Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(50),
                    Constraint::Fill(1),
                    Constraint::Length(20),
                ]),
        );
        drop(focus_state_mutex);
        ButtonsArea::default().render(buttons_area, buf);

        VolArea::default().render(volume_area, buf);
    }
}
