pub mod AppState {
    use std::collections::HashMap;

    use crossterm::event::KeyCode;
    use ratatui::widgets::ListState;
    use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

    use crate::manipulation::PlayerControlSignal;
    use crate::tui::Event;
    use crate::widgets::Button::{ButtonIdent, PlayButtonState};
    use crate::widgets::ButtonArea::ButtonsArea;
    use crate::widgets::ListArea::ListArea;
    use crate::widgets::VolArea::VolArea;

    #[derive(Clone, Copy, Debug)]
    pub enum TabState {
        Focused(Tabs),
        Selected(Tabs),
        None,
    }

    pub trait AreaHandler {
        fn handle_key(app_state_container: &mut AppStateContainer, key_code: KeyCode);
        fn get_tab_selected_handler(app_state_container: &mut AppStateContainer) {}
        fn lost_tab_selection_handler(app_state_container: &mut AppStateContainer) {}
    }

    #[derive(Clone, Copy, Debug)]
    pub enum Tabs {
        ListArea,
        EffectArea,
        DurationBarArea,
        ButtonsArea,
        VolArea,
    }
    macro_rules! handle_key_via_trait {
        ($ty:ty,$ident:ident) => {
            <$ty as AreaHandler>::$ident
        };
    }

    macro_rules! get_area_handler_fn {
        ($self: ident,$ident:ident) => {
            match $self {
                Tabs::ListArea => handle_key_via_trait!(ListArea, $ident),
                Tabs::EffectArea => todo!(),
                Tabs::DurationBarArea => todo!(),
                Tabs::ButtonsArea => handle_key_via_trait!(ButtonsArea, $ident),
                Tabs::VolArea => handle_key_via_trait!(VolArea, $ident),
            }
        };
    }
    impl Tabs {
        const fn get_next_zone(&self) -> NextZone {
            use Tabs::*;
            match self {
                ListArea => NextZone {
                    up: None,
                    down: Some(DurationBarArea),
                    left: None,
                    right: Some(EffectArea),
                },
                EffectArea => NextZone {
                    up: None,
                    down: Some(DurationBarArea),
                    left: Some(ListArea),
                    right: None,
                },
                DurationBarArea => NextZone {
                    up: Some(ListArea),
                    down: Some(ButtonsArea),
                    left: None,
                    right: None,
                },
                ButtonsArea => NextZone {
                    up: Some(DurationBarArea),
                    down: None,
                    left: None,
                    right: Some(VolArea),
                },
                VolArea => NextZone {
                    up: Some(DurationBarArea),
                    down: None,
                    left: Some(ButtonsArea),
                    right: None,
                },
            }
        }

        pub fn handle_key(&self, app_state_container: &mut AppStateContainer, key_code: KeyCode) {
            let func = get_area_handler_fn!(self, handle_key);
            func(app_state_container, key_code);
        }
        pub fn get_tab_selected_handler(&self, app_state_container: &mut AppStateContainer) {
            let func = get_area_handler_fn!(self, get_tab_selected_handler);
            func(app_state_container);
        }
        pub fn lost_tab_selection_handler(&self, app_state_container: &mut AppStateContainer) {
            let func = get_area_handler_fn!(self, lost_tab_selection_handler);
            func(app_state_container);
        }
    }

    struct NextZone {
        up: Option<Tabs>,
        down: Option<Tabs>,
        left: Option<Tabs>,
        right: Option<Tabs>,
    }

    impl TabState {
        pub fn select(&mut self) {
            match *self {
                TabState::Focused(tab) => {
                    *self = Self::Selected(tab);
                }
                _ => {}
            }
        }
        pub const fn get_focus_tab(&self, key_code: KeyCode) -> Option<Tabs> {
            use Tabs::*;
            let n = match self {
                TabState::Focused(tabs) => tabs.get_next_zone(),
                TabState::Selected(_) => panic!(),
                TabState::None => NextZone {
                    up: Some(ButtonsArea),
                    down: Some(ListArea),
                    left: Some(EffectArea),
                    right: Some(ListArea),
                },
            };
            use KeyCode::*;
            match key_code {
                Up => n.up,
                Down => n.down,
                Left => n.left,
                Right => n.right,
                _ => None,
            }
        }
    }

    pub enum PlayState {
        Playing(usize),
        Paused(usize),
        Stop,
    }

    //type ButtonIdentToHandler = HashMap<ButtonIdent, Box<dyn Fn() + Send>>;
    pub struct AppStateContainer {
        pub focus_state: TabState,
        pub button_focus_state: ButtonIdent,
        pub vol_state: u16,
        pub play_list: Vec<String>,
        pub play_list_selected: ListState,
        pub play_list_playing: PlayState,
        pub player_control_singnal_sender: Option<UnboundedSender<PlayerControlSignal>>,
    }

    impl AppStateContainer {
        pub fn new(list: Vec<String>) -> Self {
            //          type OLM<T> = OnceLock<Mutex<T>>;
            // type ButtonIdentToHandler = HashMap<ButtonIdent, Box<dyn Fn() + Send>>;
            Self {
                focus_state: TabState::None,
                button_focus_state: ButtonIdent::PlayOrPause(PlayButtonState::Playing),
                play_list: list,
                vol_state: 100,
                play_list_selected: ListState::default(),
                play_list_playing: PlayState::Stop,
                player_control_singnal_sender: None,
            }
        }
    }
}
