#[macro_export]
macro_rules! get_decorated_border {
    ($focus_state_mutex: ident,$tab: pat) => {
        match *$focus_state_mutex {
            TabState::Focused($tab) => Some(<ratatui::widgets::Block as $crate::extensions::SelectBlock::SelectedBlock>::focused_block()),
            TabState::Selected($tab) => Some(<ratatui::widgets::Block as $crate::extensions::SelectBlock::SelectedBlock>::selected_block()),
            _ => None,
        }
    };
}
