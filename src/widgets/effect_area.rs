use crate::{
    app::Ves,
    app_state::app_state::{
        AppStateContainer, AreaHandler, PlayingTrackInfo, VeSelectedTab, VeSwitcherRequestSignal,
    },
    extensions::rect::RectExtension,
    get_decorated_border,
    key_guide::{KeyGuide, LineExt},
};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    prelude::{Buffer, Rect},
    style::{Color, Style},
    symbols::Marker,
    text::{Line, Span},
    widgets::{Axis, Chart, Dataset, GraphType, StatefulWidget, Tabs, Widget},
};
#[derive(Default)]
pub struct EffectArea {}

impl StatefulWidget for EffectArea {
    type State = AppStateContainer;
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        get_decorated_border!(
            state.focus_state,
            crate::app_state::app_state::Tabs::EffectArea
        )
        .render(area, buf);

        let selected_idnex = state.ve_selected.enumerate_arr_idnex();
        let list = VeSelectedTab::enumate_case().map(|x| {
            if state.ve_selected == x {
                x.nameof().into()
            } else {
                Line::from(x.nameof()).style(Color::White)
            }
        });

        let tabs = Tabs::new(list)
            .highlight_style(Style::default().magenta().on_black().bold())
            .select(selected_idnex)
            .divider(Span::from("|").style(Color::White))
            .padding(" ", " ");

        tabs.render(area, buf);

        let Some(ref ve_shared_buffer) = state.ve_shared_buffer else {
            return;
        };

        if let Some(_) = state.player_request_state {
            return;
        }

        let [left_ch_area, right_ch_area] = area.margin(None).layout(
            &Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Fill(1), Constraint::Fill(1)]),
        );

        let AppStateContainer {
            playing_track_info:
                Some(PlayingTrackInfo {
                    audio_device_sample_rate: sample_rate,
                    ..
                }),
            ..
        } = state
        else {
            return;
        };

        let Some(Ves {
            ve_read_exclusive: Some(ve_read_exclusive),
            ..
        }) = state.ve_channel
        else {
            return;
        };

        struct RenderChannelInfo<'a> {
            pub area: Rect,
            pub data: &'a [(f64, f64)],
        }
        let mut render_channel_wave = |render_channel_info: RenderChannelInfo| {
            let dataset = Dataset::default()
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Color::Yellow)
                .data(render_channel_info.data);

            let x_axis = Axis::default().bounds([0.0, *sample_rate as f64 / 60f64]);

            let y_axis = Axis::default().bounds([-1.1, 1.1]);

            let chart = Chart::new(vec![dataset]).x_axis(x_axis).y_axis(y_axis);

            Widget::render(chart, render_channel_info.area, buf);
        };

        let info = {
            let get_data_wave_data_slice =
                |i: usize| ve_shared_buffer[ve_read_exclusive][i].0.as_slice();
            [
                RenderChannelInfo {
                    area: left_ch_area,
                    data: get_data_wave_data_slice(0),
                },
                RenderChannelInfo {
                    area: right_ch_area,
                    data: get_data_wave_data_slice(1),
                },
            ]
        };

        for ele in info {
            render_channel_wave(ele);
        }
    }
}

impl AreaHandler for EffectArea {
    fn handle_key(
        app_state_container: &mut AppStateContainer,
        key_code: crossterm::event::KeyCode,
    ) {
        let ve_selected = &mut app_state_container.ve_selected;
        let target_tab = *&ve_selected.get_focus_tab(key_code);

        if *ve_selected == target_tab {
            return;
        }
        *ve_selected = target_tab;

        let Some(_) = app_state_container.playing_track_info else {
            return;
        };
        _ = app_state_container
            .ve_switcher_request_signal_sender
            .send(VeSwitcherRequestSignal {
                request_tab: target_tab,
            });
    }

    fn get_disp_bottom_line_text_area_selected<'a>(
        app_state_container: &mut AppStateContainer,
        available_width: usize,
    ) -> Line<'a> {
        Line::from_key_guide(
            [
                KeyGuide::ESC_DEFAULT,
                KeyGuide::new_mazenta("←/→", "Select Visual Effect"),
            ].into_iter()
            .chain(KeyGuide::get_global_gudies(app_state_container))
            ,
            available_width,
        )
    }
}
