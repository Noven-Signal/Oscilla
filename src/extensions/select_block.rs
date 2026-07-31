use ratatui::{style::{Color, Style}, symbols::border, widgets::Block};


pub trait SelectedBlock {
    fn focused_block() -> Self;
    fn selected_block() -> Self;
}
impl SelectedBlock for Block<'_> {
    fn focused_block() -> Self {
        Block::bordered()
            .border_style(Style::new().fg(Color::Rgb(180, 120, 120)))
            .border_set(border::LIGHT_DOUBLE_DASHED)
    }

    fn selected_block() -> Self {
        Block::bordered()
            .border_style(Style::new().fg(Color::Red))
            .border_set(border::ROUNDED)
    }
}