use ratatui::prelude::*;

use crate::app_state::app_state::AppStateContainer;
use crate::extensions::rect::{Margin, RectExtension};

use crate::widgets::button_area::ButtonsArea;
use crate::widgets::duration_bar_area::DurationBarArea;
use crate::widgets::key_guide_area::KeyGuideArea;
#[cfg(feature = "mcp")]
use crate::widgets::mcp_server_area::McpServerArea;
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

        let mcp_area_width = if cfg!(feature = "mcp") { 9 } else { 0 };

        let [buttons_area, _fill, mcp_area, volume_area] = buttons_and_volume_area.layout(
            &Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(65),
                    Constraint::Fill(1),
                    Constraint::Length(mcp_area_width),
                    Constraint::Length(20),
                ]),
        );

        DurationBarArea::default().render(progressbar_area, buf, state);

        ButtonsArea::default().render(buttons_area, buf, state);

        #[cfg(feature = "mcp")]
        McpServerArea::default().render(mcp_area, buf, state);
        #[cfg(not(feature = "mcp"))]
        let _ = mcp_area;

        VolArea::default().render(volume_area, buf, state);

        let bottom_text_area = bottom_text_area.margin(Some(Margin {
            top: 1,
            ..Default::default()
        }));

        KeyGuideArea::default().render(bottom_text_area, buf, state);
    }
}
