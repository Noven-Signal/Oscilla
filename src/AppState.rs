pub mod AppState {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use std::sync::OnceLock;
    use std::sync::RwLock;
    use std::sync::atomic::AtomicI32;
    use std::sync::atomic::AtomicU16;

    use color_eyre::eyre::Ok;
    use crossterm::event::KeyCode;
    use futures::future::ok;

    use crate::AppState;
    use crate::extensions::OnceLock::OnceLock_ext;
    use crate::widgets::Button::ButtonIdent;
    use crate::widgets::ButtonArea;
    use crate::widgets::ButtonArea::ButtonsArea;
    use crate::widgets::VolArea::VolArea;

    #[derive(Clone, Copy, Debug)]
    pub enum TabState {
        Focused(Tabs),
        Selected(Tabs),
        None,
    }

    pub trait AreaHandler {
        fn handle_key(app_state_container: &mut AppStateContainer,key_code: KeyCode);
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
                Tabs::ListArea => todo!(),
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

        pub fn handle_key(&self,app_state_container: &mut AppStateContainer, key_code: KeyCode) {
            let func = get_area_handler_fn!(self, handle_key);
            func(app_state_container ,key_code);
        }
        pub fn get_tab_selected_handler(&self,app_state_container: &mut AppStateContainer,) {
            let func = get_area_handler_fn!(self, get_tab_selected_handler);
            func(app_state_container);
        }
        pub fn lost_tab_selection_handler(&self,app_state_container: &mut AppStateContainer,) {
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

    type ButtonIdentToHandler = HashMap<ButtonIdent, Box<dyn Fn() + Send>>;
    pub struct AppStateContainer {
        pub focus_state: TabState,
        pub button_focus_state: ButtonIdent,
        pub button_handler_func_dic: ButtonIdentToHandler,
        pub vol_state: u16,
        pub play_list: Vec<String>,
    }

    impl AppStateContainer {
        pub fn new(list: Vec<String>) -> Self {
            //          type OLM<T> = OnceLock<Mutex<T>>;
            // type ButtonIdentToHandler = HashMap<ButtonIdent, Box<dyn Fn() + Send>>;
            Self {
                focus_state: TabState::None,
                button_focus_state: ButtonIdent::Play,
                button_handler_func_dic: ButtonIdentToHandler::new(),
                play_list: list,
                vol_state: 100,
            }
        }
    }
    // type OLM<T> = OnceLock<Mutex<T>>;
    // type ButtonIdentToHandler = HashMap<ButtonIdent, Box<dyn Fn() + Send>>;

    // focus_state: OLM<TabState> = OnceLock::new();
    // button_focus_state: OLM<ButtonIdent> = OnceLock::new();

    // button_handler_func_dic: OLM<ButtonIdentToHandler> = OnceLock::new();

    // vol_state: AtomicU16 = AtomicU16::new(100);

    // play_list: OLM<Vec<String>> = OnceLock::new();

    // fn init(list: Vec<String>) {
    //     focus_state.get_or_init(|| Mutex::new(TabState::None));
    //     button_focus_state.get_or_init(|| Mutex::new(ButtonIdent::Play));

    //     button_handler_func_dic.get_or_init(|| Mutex::new(ButtonIdentToHandler::new()));

    //     play_list.get_or_init(|| Mutex::new(list));
    // }
}
