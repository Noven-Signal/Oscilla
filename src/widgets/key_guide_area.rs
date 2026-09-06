use ratatui::widgets::StatefulWidget;
use std::cmp;

use ratatui::prelude::*;

use crate::app_state::app_state::{AppStateContainer, PlayState, TabState};

use crate::get_area_handler_fn;
use crate::key_guide::{KeyGuide, LineExt};

#[derive(Default)]
pub struct KeyGuideArea {}

impl StatefulWidget for KeyGuideArea {
    type State = AppStateContainer;
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        match state.focus_state {
            TabState::Focused(_) => Line::from_key_guide(
                [
                    KeyGuide::new_mazenta("Esc", "Cancel"),
                    KeyGuide::new_mazenta("Enter", "Select"),
                    KeyGuide::new_mazenta("↑/↓/→/←", "Move"),
                ]
                .into_iter()
                .chain(KeyGuide::get_global_gudies(state)),
                buf.area.width as usize,
            )
            .render(area, buf),
            TabState::Selected(tabs) => {
                let render_bottom_line_text_area_selected =
                    get_area_handler_fn!(tabs, render_bottom_line_text_area_selected);
                render_bottom_line_text_area_selected(buf, area, state);
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
                    (area.width as f32 * 0.6) as usize,
                ) as u16;
                let [keygides_area, now_palying_area] = area.layout(
                    &Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Fill(1), Constraint::Max(min_now_playing_width)])
                        .spacing(1),
                );

                let key_guides = Line::from_key_guide(
                    [KeyGuide::new_mazenta("↑/↓/→/←", "Move Focus")]
                        .into_iter()
                        .chain(KeyGuide::get_global_gudies(state)),
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
