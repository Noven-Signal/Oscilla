pub mod app_state {
    use crate::app::{AppContorlSignal, PlayerRequestState, PopupObject, Ves};
    use crate::audio_file_info::AudioFileInfo;
    use crate::manipulation::{PlayerControlSignal, PlayerExecutorError, VESharedBuffer};
    use crate::widgets::button::{ButtonIdent, PlayButtonState};

    use crate::get_area_handler_fn;
    use crossterm::event::KeyCode;
    use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
    use ratatui::widgets::{ListState, Widget};
    use std::fmt::Debug;
    use std::pin::Pin;
    use std::slice::Iter;
    use std::time::Duration;
    use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
    use tokio::task::JoinHandle;

    #[derive(Clone, Copy, Debug)]
    pub enum TabState {
        Focused(Tabs),
        Selected(Tabs),
        None,
    }

    pub trait AreaHandler {
        fn handle_key(app_state_container: &mut AppStateContainer, key_code: KeyCode);
        fn get_tab_selected_handler(_app_state_container: &mut AppStateContainer) {}
        fn lost_tab_selection_handler(_app_state_container: &mut AppStateContainer) {}

        fn get_disp_bottom_line_text_area_selected<'a>(_app_state_container: &mut AppStateContainer, _available_width:usize) -> Line<'a>{
            Line::default()
        }

        fn render_bottom_line_text_area_selected(buf: &mut Buffer,area: Rect,app_state_container: &mut AppStateContainer){
            let line = Self::get_disp_bottom_line_text_area_selected(app_state_container, area.width.into());
            line.render(area, buf);
        }
        
    }

    #[derive(Clone, Copy, Debug)]
    pub enum Tabs {
        ListArea,
        EffectArea,
        DurationBarArea,
        ButtonsArea,
        VolArea,
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
        #[allow(dead_code)]
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
                seeking_duration: None,
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
        pub handle: JoinHandle<Result<(), PlayerExecutorError>>,
        pub player_control_singnal_sender: UnboundedSender<PlayerControlSignal>,
    }

    //type ButtonIdentToHandler = HashMap<ButtonIdent, Box<dyn Fn() + Send>>;
    pub struct AppStateContainer {
        pub focus_state: TabState,
        pub button_focus_state: ButtonIdent,
        pub vol_state: u16,
        pub play_list: Vec<AudioFileInfo>,
        pub play_list_selected: ListState,
        pub play_state: PlayState,
        pub playing_track_info: Option<PlayingTrackInfo>,
        pub player_thread: Option<PlayerThread>,
        pub ve_shared_buffer: Option<Pin<Box<VESharedBuffer>>>,
        pub ve_selected: VeSelectedTab,
        pub ve_channel: Option<Ves>,
        pub ve_switcher_request_signal_sender: UnboundedSender<VeSwitcherRequestSignal>,
        pub ve_switcher_request_signal_recv: UnboundedReceiver<VeSwitcherRequestSignal>,
        pub wait_next_tack_idx: Option<usize>,
        pub app_control_signal_sender: UnboundedSender<AppContorlSignal>,
        pub seek_no: u64,
        pub played_frame_buffer: Option<u32>,
        pub player_request_state: Option<PlayerRequestState>,
        pub popup_object: Option<PopupObject>,
        pub popup_queue_signal_sender: UnboundedSender<PopupObject>,
    }

    impl AppStateContainer {
        pub fn new(
            app_control_signal_sender: UnboundedSender<AppContorlSignal>,
            popup_queue_signal_sender: UnboundedSender<PopupObject>,
            list: Vec<String>,
        ) -> Self {
            let (ve_switcher_request_signal_sender, ve_switcher_request_signal_recv) =
                unbounded_channel();

            let play_list = Self::validate_audio_file_and_create_audio_file_info_list(
                list,
                popup_queue_signal_sender.clone(),
            );

            Self {
                focus_state: TabState::None,
                button_focus_state: ButtonIdent::PlayOrPause(PlayButtonState::Playing),
                play_list,
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
                player_request_state: None,
                popup_object: None,
                popup_queue_signal_sender,
            }
        }

        pub fn validate_audio_file_and_create_audio_file_info_list(
            list: Vec<String>,
            popup_queue_signal_sender: UnboundedSender<PopupObject>,
        ) -> Vec<AudioFileInfo> {
            let (play_list, error_list) = Self::categorize_into_two::<AudioFileInfo, String>(
                list.iter(),
                |path, play_list, error_list| {
                    match AudioFileInfo::new(path) {
                        Ok(info) => play_list.push(info),
                        Err(_) => error_list.push(path.clone()),
                    };
                },
            );

            if !error_list.is_empty() {
                let error_paths = error_list.join("\n");
                let signal = PopupObject {
                    title: "error".into(),
                    message: format!("an error occurred during loading file: \n{error_paths}"),
                    button_name: "OK".into(),
                };
                _ = popup_queue_signal_sender.send(signal);
            }

            play_list
        }

        fn categorize_into_two<T: Debug, U: Debug>(
            input: Iter<'_, String>,
            mut f: impl FnMut(&String, &mut Vec<T>, &mut Vec<U>),
        ) -> (Vec<T>, Vec<U>) {
            let mut x_0 = Vec::<T>::new();
            let mut x_1 = Vec::<U>::new();

            for ele in input {
                f(ele, &mut x_0, &mut x_1)
            }
            (x_0, x_1)
        }
    }
}
