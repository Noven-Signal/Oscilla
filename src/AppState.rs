pub mod AppState {
    use std::ops::Index;
    use std::sync::Arc;
    use std::time::{Duration, SystemTime};

    use crossterm::event::KeyCode;
    use ratatui::widgets::ListState;
    use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
    use tokio::task::JoinHandle;

    use crate::AppState::AppState::VeSelectedTab::Oscilloscope;
    use crate::AudioFileInfo::AudioFileInfo;
    use crate::app::{AppContorlSignal, PlayerRequestState, Ves};
    use crate::manipulation::{PlayerControlSignal, UiVEThreadSyncSignal, VESharedBuffer};
    use crate::widgets::Button::{ButtonIdent, PlayButtonState};
    use crate::widgets::ButtonArea::ButtonsArea;
    use crate::widgets::DurationBarArea::DurationBarArea;
    use crate::widgets::EffectArea::EffectArea;
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
                Tabs::EffectArea => handle_key_via_trait!(EffectArea, $ident),
                Tabs::DurationBarArea => handle_key_via_trait!(DurationBarArea, $ident),
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
        pub file_sample_rate: usize,
        pub audio_device_sample_rate: usize,
        pub track_duraion: Duration,
        pub seek_completed_recieved_seek_no: u64,
        pub seeking_duration: Option<Duration>,
        current_played_duration: Duration,
        audio_device_buffered_duration: Duration,
    }

    impl PlayingTrackInfo {
        pub fn new(
            file_sample_rate: usize,
            audio_device_sample_rate: usize,
            track_duraion: Duration,
        ) -> Self {
            Self {
                file_sample_rate,
                audio_device_sample_rate,
                track_duraion,
                current_played_duration: Duration::ZERO,
                audio_device_buffered_duration: Duration::ZERO,
                seek_completed_recieved_seek_no: 0,
                seeking_duration: None
            }
        }

        pub fn get_carib_duration(&self) -> Duration {
            self.current_played_duration
                .saturating_sub(self.audio_device_buffered_duration)
        }

        pub fn set_played_duration(&mut self, add_frames: u32, buffered_frames: u32) {
            let add_duration =
                Duration::from_secs_f64(add_frames as f64 / self.audio_device_sample_rate as f64);

            self.current_played_duration =
                self.current_played_duration.saturating_add(add_duration);
            self.audio_device_buffered_duration = Duration::from_secs_f64(
                (buffered_frames + add_frames) as f64 / self.audio_device_sample_rate as f64,
            );
        }
        pub fn set_played_duration_direct(
            &mut self,
            current_played_duration: Duration,
            audio_device_buffered_duration: Duration,
        ) {
            *self = Self {
                current_played_duration,
                audio_device_buffered_duration,
                ..*self
            };
        }
    }

    pub enum PlayState {
        Playing(usize),
        Paused(usize),
        Stopped,
    }

    impl PlayState {
        pub fn to_play_button_state(&self) -> PlayButtonState {
            match self {
                PlayState::Playing(_) => PlayButtonState::Playing,
                PlayState::Paused(_) => PlayButtonState::Paused,
                PlayState::Stopped => PlayButtonState::Stopped,
            }
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum VeSelectedTab {
        Off,
        Oscilloscope,
    }

    impl VeSelectedTab {
        pub const fn enumate_case() -> [Self; 2] {
            use VeSelectedTab::*;
            [Off, Oscilloscope]
        }
        pub fn list_str() -> [String; 2] {
            use VeSelectedTab::*;

            [
                stringify!(Off).to_string(),
                stringify!(Oscilloscope).to_string(),
            ]
        }
        pub fn nameof(&self) -> String {
            use VeSelectedTab::*;
            match self {
                Off => stringify!(Off),
                Oscilloscope => stringify!(Oscilloscope),
            }
            .to_string()
        }

        pub fn enumerate_arr_idnex(&self) -> usize {
            Self::enumate_case()
                .iter()
                .position(|x| x == self)
                .expect("bug")
        }

        pub fn get_focus_tab(&self, key_code: KeyCode) -> VeSelectedTab {
            use VeSelectedTab::*;
            let arr = Self::enumate_case();
            let current_index = self.enumerate_arr_idnex();

            let slide = match key_code {
                KeyCode::Left => -1,
                KeyCode::Right => 1,
                _ => 0,
            };

            let target_index = (current_index as i32 + slide + arr.len() as i32) % arr.len() as i32;

            arr[target_index as usize]
        }
    }

    #[derive(Clone, Copy)]
    pub struct VeSwitcherRequestSignal {
        pub request_tab: VeSelectedTab,
    }

    pub struct PlayerThread {
        pub handle: JoinHandle<()>,
        pub player_control_singnal_sender: UnboundedSender<PlayerControlSignal>,
    }

    //type ButtonIdentToHandler = HashMap<ButtonIdent, Box<dyn Fn() + Send>>;
    pub struct AppStateContainer {
        pub focus_state: TabState,
        pub button_focus_state: ButtonIdent,
        pub vol_state: u16,
        pub play_list: Arc<Vec<AudioFileInfo>>,
        pub play_list_selected: ListState,
        pub play_state: PlayState,
        pub playing_track_info: Option<PlayingTrackInfo>,
        pub player_thread: Option<PlayerThread>,
        pub ve_shared_buffer: Option<VESharedBuffer>,
        pub ve_selected: VeSelectedTab,
        pub ve_channel: Option<Ves>,
        pub ve_switcher_request_signal_sender: UnboundedSender<VeSwitcherRequestSignal>,
        pub ve_switcher_request_signal_recv: UnboundedReceiver<VeSwitcherRequestSignal>,
        pub wait_next_tack_idx: Option<usize>,
        pub app_control_signal_sender: UnboundedSender<AppContorlSignal>,
        pub seek_no: u64,
        pub played_frame_buffer: Option<u32>,
        pub player_request_state: Option<PlayerRequestState>,
    }

    impl AppStateContainer {
        pub fn new(
            app_control_signal_sender: UnboundedSender<AppContorlSignal>,
            list: Vec<String>,
        ) -> Self {
            let (ve_switcher_request_signal_sender, ve_switcher_request_signal_recv) =
                unbounded_channel();
            let play_list = list
                .iter()
                .map(|path| AudioFileInfo::new(path)) // comment to prevent formatter to single liner
                .collect();

            Self {
                focus_state: TabState::None,
                button_focus_state: ButtonIdent::PlayOrPause(PlayButtonState::Playing),
                play_list: Arc::new(play_list),
                vol_state: 100,
                play_list_selected: ListState::default(),
                play_state: PlayState::Stopped,
                player_thread: None,
                playing_track_info: None,
                ve_shared_buffer: None,
                ve_selected: VeSelectedTab::Off,
                ve_channel: None,
                ve_switcher_request_signal_sender,
                ve_switcher_request_signal_recv,
                wait_next_tack_idx: None,
                app_control_signal_sender,
                seek_no: 0,
                played_frame_buffer: None,
                player_request_state: None
            }
        }
    }
}
