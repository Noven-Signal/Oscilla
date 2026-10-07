use crate::app::{AppContorlSignal};
use crate::extensions::rect::RectExtension;
use crate::key_guide::{KeyGuide, LineExt};
use crate::{get_decorated_border, rgb_color};
use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

use crate::app_state::app_state::{AppStateContainer, AreaHandler, Tabs};

use crate::shared::is_mcp_is_enabled;

#[derive(Default)]
pub struct McpServerArea {}

impl StatefulWidget for McpServerArea {
    type State = AppStateContainer;
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        get_decorated_border!(state.focus_state, Tabs::McpServerArea).render(area, buf);

        let indicator_emoji = if is_mcp_is_enabled(state) {
            "✅"
        } else {
            "❌"
        };
        Span::from(format!("MCP: {indicator_emoji}"))
            .bg(rgb_color::BUTTON_NORMAL_DISABLED)
            .render(area.margin(None), buf);
    }
}

impl AreaHandler for McpServerArea {
    fn handle_key(app_state_container: &mut AppStateContainer, key_code: KeyCode) {
        match key_code {
            KeyCode::Enter => {
                _ = app_state_container
                    .app_control_signal_sender
                    .send(AppContorlSignal::McpServerToggle)
            }
            _ => {}
        };
    }

    fn get_disp_bottom_line_text_area_selected<'a>(
        app_state_container: &mut AppStateContainer,
        available_width: usize,
    ) -> Line<'a> {
        Line::from_key_guide(
            [
                KeyGuide::ESC_DEFAULT,
                KeyGuide::new_mazenta(
                    "Enter",
                    if is_mcp_is_enabled(app_state_container) {
                        "Shutdown MCP server"
                    } else {
                        "turn on MCP server"
                    },
                ),
            ]
            .into_iter()
            .chain(KeyGuide::get_global_gudies(app_state_container)),
            available_width,
        )
    }
}
