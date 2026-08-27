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

#[macro_export]
macro_rules! handle_key_via_trait {
    ($ty:ty,$ident:ident) => {
        <$ty as $crate::app_state::app_state::AreaHandler>::$ident
    };
}
#[macro_export]
macro_rules! get_area_handler_fn {
    ($self: ident,$ident:ident) => {
        match $self {
            $crate::app_state::app_state::Tabs::ListArea => {
                $crate::handle_key_via_trait!($crate::widgets::list_area::ListArea, $ident)
            }
            $crate::app_state::app_state::Tabs::EffectArea => {
                $crate::handle_key_via_trait!($crate::widgets::effect_area::EffectArea, $ident)
            }
            $crate::app_state::app_state::Tabs::DurationBarArea => {
                $crate::handle_key_via_trait!(
                    $crate::widgets::duration_bar_area::DurationBarArea,
                    $ident
                )
            }
            $crate::app_state::app_state::Tabs::ButtonsArea => {
                $crate::handle_key_via_trait!($crate::widgets::button_area::ButtonsArea, $ident)
            }
            $crate::app_state::app_state::Tabs::VolArea => {
                $crate::handle_key_via_trait!($crate::widgets::vol_area::VolArea, $ident)
            }
        }
    };
}
