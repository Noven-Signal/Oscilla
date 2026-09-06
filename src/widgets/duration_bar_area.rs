use std::time::Duration;

use ratatui::{
    layout::{Constraint, Direction, Layout},
    prelude::{Buffer, Rect},
    style::Style,
    symbols,
    text::{Line, Text},
    widgets::{LineGauge, StatefulWidget, Widget},
};

use crate::{
    app::App,
    app_state::app_state::{AppStateContainer, AreaHandler, TabState, Tabs},
    extensions::rect::RectExtension,
    get_decorated_border,
    key_guide::{KeyGuide, LineExt},
};

#[derive(Default)]
pub struct DurationBarArea {}

impl StatefulWidget for DurationBarArea {
    type State = AppStateContainer;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        get_decorated_border!(state.focus_state, Tabs::DurationBarArea).render(area, buf);

        let [progressbar_area, remaining_time_area] = area.margin(None).layout(
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
            label = track_info.get_carib_duration().format_to_min_sec();
            ratio = {
                let current = track_info.get_carib_duration().as_secs_f64();
                let total = track_info.track_duraion.as_secs_f64();
                let ratio = current / total;
                match ratio {
                    ..=0f64 => 0f64,
                    1f64.. => 1f64,
                    ratio if ratio.is_nan() || ratio.is_infinite() => 0f64,
                    ratio => ratio,
                }
            };
            remaining_time_str =
                Duration::saturating_sub(track_info.track_duraion, track_info.get_carib_duration())
                    .format_to_min_sec();
        } else {
            label = "--".to_string();
            ratio = 0f64;
            remaining_time_str = "   --".to_string();
        }

        macro_rules! filled_color {
            ($ident: ident) => {
                Style::new().white().$ident().bold()
            };
        }

        let filled_style = match state.focus_state {
            TabState::Selected(Tabs::DurationBarArea) => filled_color!(on_magenta),
            _ => filled_color!(on_red),
        };

        let duration_bar = LineGauge::default()
            .filled_style(filled_style)
            .unfilled_style(Style::new().gray().on_black())
            .label(label)
            .ratio(ratio)
            .filled_symbol(symbols::line::THICK_HORIZONTAL)
            .unfilled_symbol(symbols::line::THICK_HORIZONTAL);

        let remaining_time = Text::from(remaining_time_str);

        remaining_time.render(remaining_time_area, buf);
        duration_bar.render(progressbar_area, buf);
    }
}

impl AreaHandler for DurationBarArea {
    fn handle_key(
        app_state_container: &mut AppStateContainer,
        key_code: crossterm::event::KeyCode,
    ) {
        let Some(ref playing_track_info) = app_state_container.playing_track_info else {
            return;
        };
        let calc_seek_point =
            |key: u32| playing_track_info.track_duraion.as_secs_f32() * (key as f32 / 10f32);
        use crossterm::event::KeyCode::*;
        match key_code {
            Left => App::seek_prev(app_state_container, Duration::from_secs(5)),
            Right => App::seek_forward(app_state_container, Duration::from_secs(5)),
            Char(x) if matches!(x, '0'..='9') => {
                let seek_point = calc_seek_point(x.to_digit(10).unwrap());
                App::seek(app_state_container, Duration::from_secs_f32(seek_point));
            }
            _ => {}
        }
    }

    fn get_disp_bottom_line_text_area_selected<'a>(
        app_state_container: &mut AppStateContainer,
        available_width: usize,
    ) -> Line<'a> {
        Line::from_key_guide(
            [
                KeyGuide::ESC_DEFAULT,
                KeyGuide::new_mazenta("←", "Rewind 5sec"),
                KeyGuide::new_mazenta("→", "Fast forward 5sec"),
                KeyGuide::new_mazenta("0-9", "Go to [Number] / 10 position"),
            ]
            .into_iter()
            .chain(KeyGuide::get_global_gudies(app_state_container)),
            available_width,
        )
    }
}
