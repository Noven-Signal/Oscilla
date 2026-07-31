#[macro_export]
macro_rules! get_decorated_border {
    ($focus_state: expr,$tab: pat) => {
        match $focus_state {
            crate::app_state::app_state::TabState::Focused($tab) => Some(<ratatui::widgets::Block as $crate::extensions::select_block::SelectedBlock>::focused_block()),
            crate::app_state::app_state::TabState::Selected($tab) => Some(<ratatui::widgets::Block as $crate::extensions::select_block::SelectedBlock>::selected_block()),
            _ => None,
        }
    };
}
