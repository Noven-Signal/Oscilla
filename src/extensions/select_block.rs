use ratatui::{style::Style, symbols::border, widgets::Block};

use crate::rgb_color;

pub trait SelectedBlock {
    fn focused_block() -> Self;
    fn selected_block() -> Self;
}
impl SelectedBlock for Block<'_> {
    fn focused_block() -> Self {
        Block::bordered()
            .border_style(Style::new().fg(rgb_color::AREA_FOCUSED))
            .border_set(border::LIGHT_DOUBLE_DASHED)
    }

    fn selected_block() -> Self {
        Block::bordered()
            .border_style(Style::new().fg(rgb_color::RED))
            .border_set(border::ROUNDED)
    }
}
