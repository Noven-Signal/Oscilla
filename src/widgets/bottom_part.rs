use std::cmp;

use ratatui::prelude::*;

use crate::app_state::app_state::{AppStateContainer, PlayState, TabState};
use crate::extensions::rect::{Margin, RectExtension};
use crate::get_area_handler_fn;
use crate::key_guide::{KeyGuide, LineExt};
use crate::widgets::button_area::ButtonsArea;
use crate::widgets::duration_bar_area::DurationBarArea;
use crate::widgets::vol_area::VolArea;

pub struct BottomPart {}
impl BottomPart {
    pub fn default() -> Self {
        Self {}
    }
}

impl StatefulWidget for BottomPart {
    type State = AppStateContainer;
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let [progressbar_area, buttons_and_volume_area, bottom_text_area] = area.layout(
            &Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Length(3),
                ])
                .spacing(-1),
        );
        let [buttons_area, _fill, volume_area] = buttons_and_volume_area.layout(
            &Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(50),
                    Constraint::Fill(1),
                    Constraint::Length(20),
                ]),
        );

        DurationBarArea::default().render(progressbar_area, buf, state);

        ButtonsArea::default().render(buttons_area, buf, state);

        VolArea::default().render(volume_area, buf, state);

        let bottom_text_area = bottom_text_area.margin(Some(Margin {
            top: 1,
            ..Default::default()
        }));

        match state.focus_state {
            TabState::Focused(_) => Line::from_key_guide(
                [
                    KeyGuide::new_mazenta("Esc", "Cancel"),
                    KeyGuide::new_mazenta("Enter", "Select"),
                    KeyGuide::new_mazenta("↑/↓/→/←", "Move"),
                ]
                .into_iter()
                .chain(KeyGuide::GLOBAL_GUIDES),
                buf.area.width as usize,
            )
            .render(bottom_text_area, buf),
            TabState::Selected(tabs) => {
                let render_bottom_line_text_area_selected =
                    get_area_handler_fn!(tabs, render_bottom_line_text_area_selected);
                render_bottom_line_text_area_selected(buf, bottom_text_area, state);
            }
            TabState::None => {
                let now_playing_text = match state.play_state {
                    PlayState::Playing(idx) | PlayState::Paused(idx) => {
                        match state.play_list.get(idx) {
                            Some(audio_file_info) => {
                                let disp_name = audio_file_info.get_disp_name().to_string();
                                Span::raw("🎵 ") + Span::raw(disp_name)
                            }
                            None => Line::default(),
                        }
                    }
                    PlayState::Stopped => Line::default(),
                };

                let min_now_playing_width = cmp::min(
                    now_playing_text.width(),
                    (bottom_text_area.width as f32 * 0.6) as usize,
                ) as u16;
                let [keygides_area, now_palying_area] = bottom_text_area.layout(
                    &Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Fill(1), Constraint::Max(min_now_playing_width)])
                        .spacing(1),
                );

                let key_guides = Line::from_key_guide(
                    [KeyGuide::new_mazenta("↑/↓/→/←", "Move Focus")]
                        .into_iter()
                        .chain(KeyGuide::GLOBAL_GUIDES.into_iter()),
                    keygides_area.width as usize,
                );

                key_guides.render(keygides_area, buf);
                now_playing_text.render(now_palying_area, buf);

                // Line::from_iter(key_guides.spans.into_iter().chain(now_playing_text.spans))
                //     .render(bottom_text_area, buf);
            }
        };
    }
}
