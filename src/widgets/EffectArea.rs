use std::{
    sync::{Arc, atomic::AtomicPtr},
    time::Duration,
};

use ratatui::{
    prelude::{Buffer, Rect},
    style::{Color, Style, Stylize},
    symbols::{self, Marker},
    widgets::{Axis, Chart, Dataset, GraphType, StatefulWidget, Tabs, Widget},
};

use crate::{
    AppState::AppState::{AppStateContainer, PlayingTrackInfo, TabState},
    app::TrackInfo,
    extensions::Rect::RectExtension,
    get_decorated_border,
};

#[derive(Default)]
pub struct EffectArea {}
static mut count: i32 = 0;
impl StatefulWidget for EffectArea {
    type State = AppStateContainer;
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let Some(ref ve_shared_buffer) = state.ve_shared_buffer else {
            return;
        };

        get_decorated_border!(
            state.focus_state,
            crate::AppState::AppState::Tabs::EffectArea
        )
        .render(area, buf);

        let tabs = Tabs::new(vec!["Tab1", "Tab2", "Tab3"])
            .style(Color::White)
            .highlight_style(Style::default().magenta().on_black().bold())
            .select(0)
            .divider(symbols::DOT)
            .padding(" ", " ");

        tabs.render(area, buf);

        //let arr: Vec<(f64, f64)> = (0..400).map(|x| (x as f64, (x as f64).sin())).collect();
        let arr = ve_shared_buffer[state.ve_read_exclusive][0].0.as_slice();
        let arr = &arr
            .into_iter()
            .enumerate()
            .map(|(i, value)| (i as f64, *value as f64))
            .collect::<Vec<_>>();

        let dataset = Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Color::Yellow)
            .data(arr);

        let AppStateContainer {
            playing_track_info: Some(PlayingTrackInfo { sample_rate, .. }),
            ..
        } = state else { return;};

        let x_axis = Axis::default().bounds([0.0, *sample_rate as f64 / 60f64 ]);

        let y_axis = Axis::default().bounds([-1.0, 1.0]);

        let chart = Chart::new(vec![dataset]).x_axis(x_axis).y_axis(y_axis);

        Widget::render(chart, area.margin(None), buf);
    }
}
