#[macro_export]
macro_rules! get_decorated_border {
    ($focus_state_mutex: ident,$tab: pat) => {
        match *$focus_state_mutex {
            TabState::Focused($tab) => Some(Block::focused_block()),
            TabState::Selected($tab) => Some(Block::selected_block()),
            _ => None,
        }
    };
}
