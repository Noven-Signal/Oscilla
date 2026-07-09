#[macro_export]
macro_rules! get_decorated_border {
    ($focus_state: expr,$tab: pat) => {
        match $focus_state {
            crate::AppState::AppState::TabState::Focused($tab) => Some(<ratatui::widgets::Block as $crate::extensions::SelectBlock::SelectedBlock>::focused_block()),
            crate::AppState::AppState::TabState::Selected($tab) => Some(<ratatui::widgets::Block as $crate::extensions::SelectBlock::SelectedBlock>::selected_block()),
            _ => None,
        }
    };
}
