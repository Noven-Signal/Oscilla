use std::ops::Div;
use std::time::Duration;

use clap::builder::Str;
use ratatui::prelude::*;
use ratatui::widgets::{Block, LineGauge, Widget};

use crate::AppState::AppState::{AppStateContainer, TabState, Tabs};
use crate::extensions::Rect::RectExtension;
use crate::get_decorated_border;
use crate::widgets::ButtonArea::ButtonsArea;
use crate::widgets::VolArea::VolArea;

pub struct BottomPart {}
impl BottomPart {
    pub fn default() -> Self {
        Self {}
    }
}

impl StatefulWidget for BottomPart {
    type State = AppStateContainer;
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let [progressbar_area, buttons_and_volume_area] = area.layout(
            &Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Length(3)])
                .spacing(-1),
        );

        get_decorated_border!(state.focus_state, Tabs::DurationBarArea)
            .render(progressbar_area, buf);

        let [progressbar_area, remaining_time_area] = progressbar_area.margin(None).layout(
            &Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Fill(1), Constraint::Length(5)]),
        );

        let (label, ratio, remaining_time_str);
        if let Some(track_info) = &state.playing_track_info {
            trait DurationExt {
                fn format_to_min_sec(&self) -> String;
            }
            impl DurationExt for Duration {
                fn format_to_min_sec(&self) -> String {
                    let total_secs = self.as_secs();
                    let min = total_secs / 60;
                    let sec = total_secs % 60;
                    format!("{:02}:{:02}", min, sec)
                }
            }
            label = track_info.current_played_duration.format_to_min_sec();
            ratio = {
                let current = track_info.current_played_duration.as_secs_f64();
                let total = track_info.track_duraion.as_secs_f64();
                let ratio = current / total;
                if ratio > 1f64 { 1f64 } else { ratio }
            };
            remaining_time_str = Duration::saturating_sub(
                track_info.track_duraion,
                track_info.current_played_duration,
            )
            .format_to_min_sec();
        } else {
            label = "--".to_string();
            ratio = 0f64;
            remaining_time_str = "--".to_string();
        }

        let duration_bar = LineGauge::default()
            .filled_style(Style::new().white().on_red().bold())
            .unfilled_style(Style::new().gray().on_black())
            .label(label)
            .ratio(ratio)
            .filled_symbol(symbols::line::THICK_HORIZONTAL)
            .unfilled_symbol(symbols::line::THICK_HORIZONTAL);

        let remaining_time = Text::from(remaining_time_str);

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

        ButtonsArea::default().render(buttons_area, buf, state);

        VolArea::default().render(volume_area, buf, state);
    }
}
