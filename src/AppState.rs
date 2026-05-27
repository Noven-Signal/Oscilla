pub mod AppState {
    use std::sync::Arc;
    use std::time::{Duration, SystemTime};

    use crossterm::event::KeyCode;
    use ratatui::widgets::ListState;
    use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

    use crate::manipulation::{PlayerControlSignal, UiVEThreadSyncSignal, VESharedBuffer};
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

    pub struct PlayingTrackInfo {
        pub sample_rate: u32,
        pub track_duraion: Duration,
        current_played_duration: Duration,
        audio_device_buffered_duration: Duration,
    }

    impl PlayingTrackInfo {
        pub fn new(sample_rate: u32, track_duraion: Duration) -> Self {
            Self {
                sample_rate,
                track_duraion,
                current_played_duration: Duration::ZERO,
                audio_device_buffered_duration: Duration::ZERO,
            }
        }

        pub fn get_carib_duration(&self) -> Duration {
            self.current_played_duration
                .saturating_sub(self.audio_device_buffered_duration)
        }

        pub fn set_played_duration(&mut self, add_frames: u32, buffered_frames: u32) {
            let add_duration = Duration::from_secs_f64(add_frames as f64 / self.sample_rate as f64);

            self.current_played_duration = self.current_played_duration.saturating_add(add_duration);
            self.audio_device_buffered_duration = Duration::from_secs_f64(
                (buffered_frames + add_frames) as f64 / self.sample_rate as f64,
            );
        }
    }

    pub enum PlayState {
        Playing(usize),
        Paused(usize),
        Stop,
    }

    impl PlayState {
        pub fn to_play_button_state(&self) -> PlayButtonState {
            match self {
                PlayState::Playing(_) => PlayButtonState::Playing,
                PlayState::Paused(_) => PlayButtonState::Paused,
                PlayState::Stop => todo!(),
            }
        }
    }

    //type ButtonIdentToHandler = HashMap<ButtonIdent, Box<dyn Fn() + Send>>;
    pub struct AppStateContainer {
        pub focus_state: TabState,
        pub button_focus_state: ButtonIdent,
        pub vol_state: u16,
        pub play_list: Arc<Vec<String>>,
        pub play_list_selected: ListState,
        pub play_state: PlayState,
        pub playing_track_info: Option<PlayingTrackInfo>,
        pub player_control_singnal_sender: Option<UnboundedSender<PlayerControlSignal>>,
        pub ve_shared_buffer: Option<VESharedBuffer>,
        // pub ui_to_ve_signal_sender: Option<UnboundedSender<UiVEThreadSyncSignal>>,
        // pub ve_to_ui_signal_recv: Option<UnboundedReceiver<UiVEThreadSyncSignal>>,
        pub ve_read_exclusive: usize,
    }

    impl AppStateContainer {
        pub fn new(list: Vec<String>) -> Self {
            //          type OLM<T> = OnceLock<Mutex<T>>;
            // type ButtonIdentToHandler = HashMap<ButtonIdent, Box<dyn Fn() + Send>>;
            Self {
                focus_state: TabState::None,
                button_focus_state: ButtonIdent::PlayOrPause(PlayButtonState::Playing),
                play_list: Arc::new(list),
                vol_state: 100,
                play_list_selected: ListState::default(),
                play_state: PlayState::Stop,
                player_control_singnal_sender: None,
                playing_track_info: None,
                ve_shared_buffer: None,
                // ui_to_ve_signal_sender: None,
                // ve_to_ui_signal_recv: None,
                ve_read_exclusive: 0,
            }
        }
    }
}
