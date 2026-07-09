use ratatui::prelude::*;

use crate::AppState::AppState::AppStateContainer ;
use crate::widgets::ButtonArea::ButtonsArea;
use crate::widgets::DurationBarArea::DurationBarArea;
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
    }
}
