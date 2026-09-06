use crate::{
    app_state::{
        self,
        app_state::{
            AppStateContainer, PlayState, PlayerThread, PlayingTrackInfo, VeSelectedTab,
            VeSwitcherRequestSignal,
        },
    },
    manipulation::{
        self, NUM_OF_BLOCK_VE, OscilloscopeData, PlayerControlSignal, SeekCompleteFromVeSignal,
        UiToPlayerSeekSignal, UiVEThreadSyncSignal,
    },
    shared::{SUPPORTED_EXTENSIONS, filter_valid_extension},
    tui::Tui,
    utils::array_init,
    widgets::{app_root::AppRoot, popup::Popup},
};
use color_eyre::eyre::Ok;
use crossterm::event::Event as CrosstermEvent;
use crossterm::event::{EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::{FutureExt, StreamExt, future};
use ratatui::{prelude::Rect, widgets::StatefulWidget};
use windows::Win32::Foundation::{ERROR_CANCELLED, HWND};

use windows::Win32::UI::Shell::{
    Common::COMDLG_FILTERSPEC, FOS_ALLOWMULTISELECT, FileOpenDialog, IFileOpenDialog,
    SIGDN_FILESYSPATH,
};

use std::{ops::Add, sync::atomic::AtomicPtr, time::Duration, usize};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    time::Interval,
};

pub struct Ves {
    pub ui_to_ve_signal_sender: UnboundedSender<UiVEThreadSyncSignal>,
    pub ve_to_ui_signal_recv: UnboundedReceiver<UiVEThreadSyncSignal>,
    pub ve_buffer_duration_offset_sec: f64,
    pub interval: Interval,
    pub ve_frame_count: u32,
    pub ve_read_exclusive: Option<usize>,
}

#[derive(Clone, Copy)]
pub struct VeSwitcherSyncSignal();

pub enum AppContorlSignal {
    StartPlayer(usize),
    AddFilesDialogCallBack(windows::core::Result<Option<Vec<String>>>),
}

pub struct App {
    root_wiget: AppRoot,
    tui: Tui,
    app_state_container: AppStateContainer,
    event_loop_canceled: bool,
    ve_switcher_sync_signal_sender: UnboundedSender<VeSwitcherSyncSignal>,
    ve_switcher_sync_signal_recv: UnboundedReceiver<VeSwitcherSyncSignal>,
    player_to_ui_signal_recv: Option<UnboundedReceiver<PlayerToUISingnal>>,
    app_control_signal_recv: UnboundedReceiver<AppContorlSignal>,
    popup_queue_signal_recv: UnboundedReceiver<PopupObject>,
}

pub struct TrackInfo {
    pub file_sample_rate: usize,
    pub audio_device_sample_rate: usize,
    pub track_duration: Duration,
}
pub struct PlayedFrames {
    pub frames: u32,
    pub buffered_frames: u32,
    pub seek_no: u64,
}

pub struct SeekCompleteSignal {
    pub seek_ve_completed_signal: Option<SeekCompleteFromVeSignal>,
    pub actual_seek_duration_sec: f64,
    pub seek_no: u64,
}

pub enum PlayerToUISingnal {
    NoticeTrackInfo(TrackInfo),
    PlayedFrames(PlayedFrames),
    VeEnabled(PlayerToUISingnalVeEnabled),
    VeEnabledSync,
    VeDisabled,
    RendererPlayCompleted,
    EndOfStream,
    SeekComplete(SeekCompleteSignal),
}

pub enum PlayerRequestState {
    Seek(u64),
    VeSwitching,
}

pub struct PlayerToUISingnalVeEnabled {
    pub ui_to_ve_signal_sender: UnboundedSender<UiVEThreadSyncSignal>,
    pub ve_to_ui_signal_recv: UnboundedReceiver<UiVEThreadSyncSignal>,
    pub ve_buffer_duration_offset_sec: f64,
}

pub struct PopupObject {
    pub title: String,
    pub message: String,
    pub button_name: String,
}
impl PopupObject {
    pub fn new(message: String) -> Self {
        Self {
            title: "ERROR".to_string(),
            message,
            button_name: "OK".to_string(),
        }
    }
}

impl App {
    pub fn new(
        root_wiget: AppRoot,
        app_state_container: AppStateContainer,
        app_control_signal_recv: UnboundedReceiver<AppContorlSignal>,
        popup_queue_signal_recv: UnboundedReceiver<PopupObject>,
    ) -> color_eyre::Result<Self> {
        let (ve_switcher_request_signal_sender, ve_switcher_request_signal_recv) =
            unbounded_channel();
        _ = ve_switcher_request_signal_sender.send(VeSwitcherSyncSignal()); //for init sync
        Ok(Self {
            tui: Tui::new()?.mouse(true).paste(true),
            root_wiget: root_wiget,
            app_state_container: app_state_container,
            event_loop_canceled: false,
            ve_switcher_sync_signal_sender: ve_switcher_request_signal_sender,
            ve_switcher_sync_signal_recv: ve_switcher_request_signal_recv,
            player_to_ui_signal_recv: None,
            app_control_signal_recv,
            popup_queue_signal_recv,
        })
    }

    fn start_player(&mut self, idx: usize) {
        let app_state_container = &mut self.app_state_container;

        if let Some(_) = app_state_container.player_thread {
            return;
        }

        let Some(target_track) = app_state_container.play_list.get(idx) else {
            return;
        };

        let (player_to_ui_signal_sender, player_to_ui_signal_recv) = unbounded_channel();
        self.player_to_ui_signal_recv = Some(player_to_ui_signal_recv);

        app_state_container.play_state = PlayState::Playing(idx);
        let (player_control_signal_sender, mut player_control_signal_recv) = unbounded_channel();

        //app_state_container.player_control_singnal_sender = Some(player_control_signal_sender);
        let init_vol = app_state_container.vol_state;
        let target_track = target_track.clone();
        let player_handle = tokio::spawn(async move {
            manipulation::play_executor(
                &target_track.file_path,
                init_vol,
                &mut player_control_signal_recv,
                player_to_ui_signal_sender,
            )
            .await
        });

        app_state_container.player_thread = Some(PlayerThread {
            handle: player_handle,
            player_control_singnal_sender: player_control_signal_sender,
        });
    }

    async fn ve_sync(ve_read_exclusive: &mut Option<usize>) {
        const NUM_OF_BLOCK_VE_LAST_INDEX: usize = NUM_OF_BLOCK_VE - 1;
        *ve_read_exclusive = match *ve_read_exclusive {
            None | Some(NUM_OF_BLOCK_VE_LAST_INDEX) => Some(0),
            Some(x) => Some(x + 1),
        };
    }

    pub async fn run(&mut self) -> color_eyre::Result<()> {
        if let Some(_) = self.app_state_container.play_list.first() {
            self.start_player(0);
        }

        self.tui.enter()?;

        self.event_loop().await?;

        self.app_state_container.ve_channel = None;

        if let Some(PlayerThread {
            handle,
            player_control_singnal_sender,
        }) = &mut self.app_state_container.player_thread
        {
            _ = player_control_singnal_sender.send(PlayerControlSignal::Stop);
            let player_result = handle.await?;
            if let Err(err) = player_result {
                self.app_state_container.popup_object = Some(PopupObject::new(err.error_message));
            }
        }

        self.tui.exit()?;

        Ok(())
    }

    pub async fn event_loop(&mut self) -> color_eyre::Result<()> {
        let mut event_stream = EventStream::new();

        let mut ve_switcher_requst_signal: Option<VeSwitcherRequestSignal> = None;
        let mut ve_switcher_sync_signal: Option<VeSwitcherSyncSignal> = None;
        let mut prev_ve_switcher_requst = VeSelectedTab::Off;
        let mut ve_switcher_request_recv_buf = Vec::new();
        'l1: loop {
            if self.event_loop_canceled {
                break 'l1;
            }

            enum VeProcResult {
                SyncRange,
                VeIsTooForward,
            }

            let ve_proc = async |ves: &mut Ves, track_info: &PlayingTrackInfo| -> VeProcResult {
                let Ves {
                    ui_to_ve_signal_sender,
                    ve_to_ui_signal_recv,
                    ve_buffer_duration_offset_sec: init_offset,
                    ve_frame_count,
                    ve_read_exclusive,
                    ..
                } = ves;

                let carib_played_duration = track_info.get_carib_duration().as_secs_f64();

                let ve_position = (*ve_frame_count as f64 / 60f64) + *init_offset;

                const N1_60_DOBULE: f64 = 2f64 / 60f64;
                const M_N1_60_DOBULE: f64 = -N1_60_DOBULE;
                match carib_played_duration - ve_position {
                    ..=M_N1_60_DOBULE => VeProcResult::VeIsTooForward,
                    M_N1_60_DOBULE..=N1_60_DOBULE => {
                        ve_to_ui_signal_recv.recv().await;
                        VeProcResult::SyncRange
                    }
                    diff => {
                        let num_of_frame_forward = (diff / N1_60_DOBULE).abs().floor() as u32;
                        for _ in 0..num_of_frame_forward - 1 {
                            //dbg!(i);
                            ve_to_ui_signal_recv.recv().await;
                            Self::ve_sync(ve_read_exclusive).await;
                            _ = ui_to_ve_signal_sender.send(UiVEThreadSyncSignal());
                            *ve_frame_count = *ve_frame_count + 1;
                        }
                        ve_to_ui_signal_recv.recv().await;
                        VeProcResult::SyncRange
                    }
                }
            };

            macro_rules! before_start_player_clean_up_statement {
                () => {
                    self.player_to_ui_signal_recv = None;

                    async fn drain_all_signal<T>(reciever: &mut UnboundedReceiver<T>) {
                        while !reciever.is_empty() {
                            reciever.recv().await;
                        }
                    }
                    drain_all_signal(&mut self.app_state_container.ve_switcher_request_signal_recv)
                        .await;
                    drain_all_signal(&mut self.ve_switcher_sync_signal_recv).await;

                    ve_switcher_requst_signal = None;
                    ve_switcher_sync_signal = None;
                    prev_ve_switcher_requst = VeSelectedTab::Off;

                    self.app_state_container.player_request_state = None;

                    _ = self
                        .ve_switcher_sync_signal_sender
                        .send(VeSwitcherSyncSignal());
                };
            }

            tokio::select! {
                Some(signal) = self.popup_queue_signal_recv.recv() => {
                    self.app_state_container.popup_object = Some(signal);
                }
                Some(signal) = self.app_control_signal_recv.recv() => {
                    match signal {
                        AppContorlSignal::StartPlayer(idx) => {
                           before_start_player_clean_up_statement!();

                            self.start_player(idx);
                        }
                        AppContorlSignal::AddFilesDialogCallBack(res) => {
                            Self::add_newfiles_callback(&mut self.app_state_container, res);
                            self.render()?;
                        }
                    }
                }
                Some(signal) = async{
                    match self.player_to_ui_signal_recv {
                        Some(ref mut recv) => recv.recv().await,
                        None => future::pending().await,
                    }
                } => self.handle_player_to_ui_signal(signal).await?,
                    crossterm_event = event_stream.next().fuse() => match crossterm_event {
                        Some(Result::Ok(event)) => self.handle_crossterm_event(&event).await?,
                        _ => break 'l1,
                    },
                _ = async{
                    let PlayState::Playing(_) = &self.app_state_container.play_state else{
                        return future::pending::<()>().await;
                    };
                    if let Some(Ves { interval, .. }) = &mut self.app_state_container.ve_channel {
                    interval.tick().await;
                    } else {
                        future::pending::<()>().await;
                    }
                } =>'b1: {
                    {
                        let AppStateContainer{ve_channel: Some(ref mut ves), playing_track_info: Some(ref playing_track_info),..} = self.app_state_container else {break 'b1;};
                        let proc_res = ve_proc(ves, playing_track_info).await;
                        let VeProcResult::SyncRange = proc_res else { break 'b1;};
                    }
                    _ = self.render();
                    {
                        let AppStateContainer{ve_channel: Some(ref mut ves),..} = self.app_state_container else {break 'b1;};
                        Self::ve_sync(&mut ves.ve_read_exclusive).await;

                        _ = ves.ui_to_ve_signal_sender.send(UiVEThreadSyncSignal());
                        ves.ve_frame_count = ves.ve_frame_count + 1;
                    }
                },
                signal = async {
                    match ve_switcher_requst_signal{
                        Some(_) =>  future::pending().await,
                        None =>  {
                            self.app_state_container.ve_switcher_request_signal_recv.recv_many(&mut ve_switcher_request_recv_buf, 1000).await
                        },
                    };
                    let last = ve_switcher_request_recv_buf.last().as_mut().and_then(|x| Some(**x));
                    ve_switcher_request_recv_buf.clear();
                    last
                } => {
                    match signal{
                        Some(signal) => if prev_ve_switcher_requst != signal.request_tab  {
                            ve_switcher_requst_signal = Some(signal);
                            prev_ve_switcher_requst = signal.request_tab;
                        },
                        _ => {},
                    }
                },
                signal = async {
                    match ve_switcher_sync_signal{
                        Some(_) =>  future::pending().await,
                        None =>  self.ve_switcher_sync_signal_recv.recv().await,
                    }
                }=> {
                    match signal{
                        Some(signal) => {
                            ve_switcher_sync_signal = Some(signal);
                        },
                        None => {},
                    }
                },
                player_result = async{
                    match &mut self.app_state_container.player_thread{
                        Some(PlayerThread { handle, .. }) => handle.await,
                        None => future::pending().await,
                    }
                } => {
                    match player_result {
                        Result::Ok(Result::Ok(_)) => {}
                        Result::Ok(Result::Err(err)) => {
                            self.app_state_container.popup_object = Some(PopupObject::new(err.error_message))
                        }
                        Err(_) => panic!(),
                    }

                    self.handle_player_thread_completed();

                    _ = self.render();

                    before_start_player_clean_up_statement!();

                    if let Some(idx) = self.app_state_container.wait_next_tack_idx.take() {
                        self.start_player(idx);
                    }
                },
            };

            if let (Some(ref signal), Some(_)) =
                (ve_switcher_requst_signal, ve_switcher_sync_signal)
                && self.app_state_container.player_request_state.is_none()
            {
                self.handle_ve_switching(signal.request_tab)?;
                ve_switcher_requst_signal = None;
                ve_switcher_sync_signal = None;
                self.app_state_container.player_request_state =
                    Some(PlayerRequestState::VeSwitching);
            }
        }

        Ok(())
    }

    fn handle_player_thread_completed(&mut self) {
        self.app_state_container.player_thread = None;
        self.app_state_container.play_state = PlayState::Stopped;
        self.app_state_container.playing_track_info = None;
        self.app_state_container.ve_channel = None;
        self.app_state_container.ve_shared_buffer = None;
    }

    fn handle_ve_switching(&mut self, target_tab: VeSelectedTab) -> color_eyre::Result<()> {
        let app_state_container = &mut self.app_state_container;
        let Some(PlayerThread {
            player_control_singnal_sender: sender,
            ..
        }) = &app_state_container.player_thread
        else {
            return Ok(());
        };
        match target_tab {
            VeSelectedTab::Off => {
                _ = sender.send(PlayerControlSignal::VeDisabled);
            }
            _ => {
                let Some(playing_track_info) = &app_state_container.playing_track_info else {
                    return Ok(());
                };
                app_state_container.ve_shared_buffer = {
                    let move_window = playing_track_info.audio_device_sample_rate as usize
                        / crate::visual_effects::oscilloscope::FRAME_RATE;
                    let crate_move_window_size_vec =
                        || (0..move_window).map(|i| (i as f64, 0f64)).collect();
                    let arr = Box::pin(array_init(|| {
                        array_init(|| OscilloscopeData(crate_move_window_size_vec()))
                    }));
                    Some(arr)
                };
                let ve_shared_buffer = app_state_container
                    .ve_shared_buffer
                    .as_mut()
                    .expect("must be Some because init above line");

                let ptr = AtomicPtr::new(ve_shared_buffer);

                _ = sender.send(PlayerControlSignal::VeEnabled(ptr));
            }
        }
        Ok(())
    }

    async fn handle_player_to_ui_signal(
        &mut self,
        signal: PlayerToUISingnal,
    ) -> color_eyre::Result<()> {
        let playing_tarck_info = &mut self.app_state_container.playing_track_info;
        match signal {
            PlayerToUISingnal::NoticeTrackInfo(track_info) => {
                *playing_tarck_info = Some(PlayingTrackInfo::new(
                    track_info.file_sample_rate,
                    track_info.audio_device_sample_rate,
                    track_info.track_duration,
                ));

                let target_tab = self.app_state_container.ve_selected;
                if target_tab != VeSelectedTab::Off {
                    _ = self
                        .app_state_container
                        .ve_switcher_request_signal_sender
                        .send(VeSwitcherRequestSignal {
                            request_tab: target_tab,
                        });
                }

                self.render()?;
            }
            PlayerToUISingnal::PlayedFrames(PlayedFrames {
                frames,
                buffered_frames,
                seek_no,
            }) => 'b1: {
                let Some(ref mut track_info) = *playing_tarck_info else {
                    break 'b1;
                };

                let frames = match (track_info.seek_completed_recieved_seek_no, seek_no) {
                    (x, y) if x + 1 == y => {
                        let inner = self.app_state_container.played_frame_buffer.as_mut();
                        match inner {
                            Some(x) => *x = *x + frames,
                            None => self.app_state_container.played_frame_buffer = Some(frames),
                        };
                        0
                    }
                    (x, y) if x == y => {
                        if let Some(played_buffer_frames) =
                            self.app_state_container.played_frame_buffer
                        {
                            let ret = frames + played_buffer_frames;
                            self.app_state_container.played_frame_buffer = None;
                            ret
                        } else {
                            frames
                        }
                    }
                    _ => 0,
                };

                //info!("PlayedFrames {seek_no}");

                let before_duration = track_info.get_carib_duration();
                track_info.set_played_duration(frames, buffered_frames);
                let after_duration = track_info.get_carib_duration();

                if before_duration.as_secs() != after_duration.as_secs() {
                    self.render()?;
                }
            }
            PlayerToUISingnal::VeEnabled(PlayerToUISingnalVeEnabled {
                ui_to_ve_signal_sender,
                ve_to_ui_signal_recv,
                ve_buffer_duration_offset_sec,
            }) => {
                self.app_state_container.ve_channel = Some(Ves {
                    ui_to_ve_signal_sender,
                    ve_to_ui_signal_recv,
                    ve_buffer_duration_offset_sec,
                    interval: tokio::time::interval(Duration::from_secs_f64(1f64 / 60f64)),
                    ve_frame_count: 0,
                    ve_read_exclusive: None,
                });
            }
            PlayerToUISingnal::VeDisabled => {
                self.app_state_container.ve_shared_buffer = None;
                self.app_state_container.ve_channel = None;

                _ = self
                    .ve_switcher_sync_signal_sender
                    .send(VeSwitcherSyncSignal());
                self.app_state_container.player_request_state = None;
            }
            PlayerToUISingnal::VeEnabledSync => {
                _ = self
                    .ve_switcher_sync_signal_sender
                    .send(VeSwitcherSyncSignal());

                self.app_state_container.player_request_state = None;
            }
            PlayerToUISingnal::EndOfStream => 'b1: {
                let next_idx = match self.app_state_container.play_state {
                    PlayState::Playing(idx) | PlayState::Paused(idx) => idx + 1,
                    _ => break 'b1,
                };

                self.app_state_container.wait_next_tack_idx = Some(next_idx);
            }
            PlayerToUISingnal::RendererPlayCompleted => 'b1: {
                let Some(PlayerThread {
                    ref mut player_control_singnal_sender,
                    ..
                }) = self.app_state_container.player_thread
                else {
                    break 'b1;
                };
                _ = player_control_singnal_sender.send(PlayerControlSignal::Stop);
            }
            PlayerToUISingnal::SeekComplete(SeekCompleteSignal {
                seek_ve_completed_signal,
                actual_seek_duration_sec,
                seek_no,
            }) => 'b1: {
                let Some(ref mut playing_track_info) = self.app_state_container.playing_track_info
                else {
                    break 'b1;
                };
                playing_track_info.set_played_duration_direct(
                    Duration::from_secs_f64(actual_seek_duration_sec),
                    Duration::ZERO,
                );
                //info!("UI seek complete");

                playing_track_info.seek_completed_recieved_seek_no = seek_no;
                playing_track_info.seeking_duration = None;

                if let Some(SeekCompleteFromVeSignal {
                    ui_to_ve_signal_sender,
                    ve_to_ui_signal_recv,
                }) = seek_ve_completed_signal
                {
                    self.app_state_container.ve_channel = Some(Ves {
                        ui_to_ve_signal_sender,
                        ve_to_ui_signal_recv,
                        ve_buffer_duration_offset_sec: actual_seek_duration_sec,
                        interval: tokio::time::interval(Duration::from_secs_f64(1f64 / 60f64)),
                        ve_frame_count: 0,
                        ve_read_exclusive: None,
                    });
                }

                if let Some(PlayerRequestState::Seek(seek_no_reqeust_state)) =
                    self.app_state_container.player_request_state
                    && seek_no_reqeust_state == seek_no
                {
                    self.app_state_container.player_request_state = None;
                }

                self.render()?;
            }
        }

        Ok(())
    }

    async fn handle_key_event(&mut self, key: KeyEvent) -> color_eyre::Result<()> {
        use app_state::app_state::*;

        let move_key_pressed_handler =
            |key_code: KeyCode, app_state_container: &mut AppStateContainer| {
                match app_state_container.focus_state {
                    TabState::Focused(_) | TabState::None => {
                        let next = (app_state_container.focus_state).get_focus_tab(key_code);
                        app_state_container.focus_state = match next {
                            Some(next) => TabState::Focused(next),
                            None => TabState::None,
                        }
                    }
                    TabState::Selected(tabs) => {
                        tabs.handle_key(app_state_container, key_code);
                    }
                }
            };

        match key {
            KeyEvent {
                code: KeyCode::Char('d'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.event_loop_canceled = true;
                if let Some(PlayerThread {
                    player_control_singnal_sender,
                    ..
                }) = &self.app_state_container.player_thread
                {
                    _ = player_control_singnal_sender.send(PlayerControlSignal::Stop);
                }
            }
            KeyEvent {
                code: KeyCode::Char(' '),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                // TODO: play/pause
            }
            KeyEvent {
                code: KeyCode::Char('o'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                ..
            } => Self::add_new_files(&mut self.app_state_container),
            KeyEvent {
                code,
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::NONE,
                ..
            } => match code {
                code if self.app_state_container.popup_object.is_some() => {
                    Popup::handle_key(&mut self.app_state_container, code);
                }
                KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => {
                    move_key_pressed_handler(code, &mut self.app_state_container)
                }
                KeyCode::Enter => {
                    //  let app_state_container = &mut self.app_state_container;
                    match self.app_state_container.focus_state {
                        TabState::Focused(tab) => {
                            let app_state_container = &mut self.app_state_container;
                            app_state_container.focus_state.select();
                            tab.get_tab_selected_handler(app_state_container);
                        }
                        TabState::Selected(tab) => {
                            tab.handle_key(&mut self.app_state_container, KeyCode::Enter);
                        }
                        _ => {}
                    }
                }
                KeyCode::Esc => {
                    let app_state_container = &mut self.app_state_container;
                    match app_state_container.focus_state {
                        TabState::Focused(_) => app_state_container.focus_state = TabState::None,
                        TabState::Selected(tabs) => {
                            app_state_container.focus_state = TabState::Focused(tabs);
                            tabs.lost_tab_selection_handler(app_state_container);
                        }
                        TabState::None => {}
                    }
                }
                key_code => {
                    let app_state_container = &mut self.app_state_container;
                    match app_state_container.focus_state {
                        TabState::Selected(tabs) => {
                            tabs.handle_key(&mut self.app_state_container, key_code);
                        }
                        _ => {}
                    }
                }
            },
            _ => {}
        };

        self.render()?;
        Ok(())
    }

    pub fn play_previous(app_state_container: &mut AppStateContainer) {
        let (PlayState::Playing(idx) | PlayState::Paused(idx)) = app_state_container.play_state
        else {
            return;
        };
        if idx == 0 {
            return;
        }
        Self::play_track(app_state_container, idx - 1);
    }
    pub fn play_next(app_state_container: &mut AppStateContainer) {
        let (PlayState::Playing(idx) | PlayState::Paused(idx)) = app_state_container.play_state
        else {
            return;
        };
        Self::play_track(app_state_container, idx + 1);
    }

    pub fn play_track(app_state_container: &mut AppStateContainer, idx: usize) {
        let Some(_) = app_state_container.play_list.get(idx) else {
            return;
        };
        if let Some(PlayerThread {
            player_control_singnal_sender,
            ..
        }) = &mut app_state_container.player_thread
        {
            app_state_container.ve_channel = None;
            app_state_container.wait_next_tack_idx = Some(idx);
            _ = player_control_singnal_sender.send(PlayerControlSignal::Stop);
        } else {
            _ = app_state_container
                .app_control_signal_sender
                .send(AppContorlSignal::StartPlayer(idx));
        };
    }

    pub fn seek_prev(app_state_container: &mut AppStateContainer, move_amout: Duration) {
        let Some(ref mut playing_track_info) = app_state_container.playing_track_info else {
            return;
        };

        let carib_duration = playing_track_info.get_carib_duration();
        let reqest_pos = carib_duration.saturating_sub(move_amout);
        Self::seek(app_state_container, reqest_pos);
    }

    pub fn seek_forward(app_state_container: &mut AppStateContainer, move_amout: Duration) {
        let Some(ref mut playing_track_info) = app_state_container.playing_track_info else {
            return;
        };

        let carib_duration = if let Some(seeking_duration) = playing_track_info.seeking_duration {
            seeking_duration
        } else {
            playing_track_info.get_carib_duration()
        };

        let reqest_pos = carib_duration + move_amout;
        if reqest_pos > playing_track_info.track_duraion {
            Self::play_next(app_state_container);
            return;
        }

        Self::seek(app_state_container, reqest_pos);
    }

    pub fn seek(app_state_container: &mut AppStateContainer, reqest_pos: Duration) {
        let Some(ref mut playing_track_info) = app_state_container.playing_track_info else {
            return;
        };
        if let Some(PlayerRequestState::VeSwitching) = app_state_container.player_request_state {
            return;
        }

        let Some(PlayerThread {
            ref mut player_control_singnal_sender,
            ..
        }) = app_state_container.player_thread
        else {
            return;
        };
        let new_seek_no = app_state_container.seek_no.add(1);
        app_state_container.seek_no = new_seek_no;
        // app_state_container.play_state = PlayState::Seeking(playing_idx);
        app_state_container.player_request_state = Some(PlayerRequestState::Seek(new_seek_no));
        playing_track_info.seeking_duration = Some(reqest_pos);
        app_state_container.ve_channel = None;

        _ = player_control_singnal_sender.send(PlayerControlSignal::Seek(UiToPlayerSeekSignal {
            target_duration: reqest_pos,
            seek_no: new_seek_no,
        }));
    }

    pub fn add_new_files(app_state_container: &mut AppStateContainer) {
        let app_control_signal_sender = app_state_container.app_control_signal_sender.clone();
        tokio::task::spawn_blocking(move || {
            let added_files = Self::select_multiple_files();
            app_control_signal_sender.send(AppContorlSignal::AddFilesDialogCallBack(added_files))
        });
    }

    fn add_newfiles_callback(
        app_state_container: &mut AppStateContainer,
        added_files: windows::core::Result<Option<Vec<String>>>,
    ) {
        use core::result::Result::*;
        match added_files {
            Ok(Some(files)) => {
                let popup_queue_sender = app_state_container.popup_queue_signal_sender.clone();
                let mut audio_file_info_list =
                    AppStateContainer::validate_audio_file_and_create_audio_file_info_list(
                        files,
                        popup_queue_sender,
                    );
                let playlist = &mut app_state_container.play_list;
                let append_before_playlist_len = playlist.len();

                playlist.append(&mut audio_file_info_list);

                if append_before_playlist_len == 0 {
                    _ = app_state_container
                        .app_control_signal_sender
                        .send(AppContorlSignal::StartPlayer(0));
                }
            }
            Ok(None) => {}
            Err(_) => {
                app_state_container.popup_object = Some(PopupObject::new(
                    "an error occurred while opening file dialog".to_string(),
                ))
            }
        }
    }

    fn select_multiple_files() -> windows::core::Result<Option<Vec<String>>> {
        use core::result::Result::*;
        use windows::{Win32::System::Com::*, core::*};
        let proc = || unsafe {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            dialog.SetOptions(FOS_ALLOWMULTISELECT)?;
            let filter_name: Vec<u16> = "Supported audio files\0".encode_utf16().collect();
            let filter_pattern: Vec<u16> = SUPPORTED_EXTENSIONS
                .iter()
                .map(|extension| format!("*.{extension}"))
                .collect::<Vec<_>>()
                .join(";")
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            use windows::core::PCWSTR;
            let filter_specs = [COMDLG_FILTERSPEC {
                pszName: PCWSTR((&filter_name).as_ptr()),
                pszSpec: PCWSTR(filter_pattern.as_ptr()),
            }];
            dialog.SetFileTypes(&filter_specs)?;

            match dialog.Show(HWND::default()) {
                Ok(_) => {}
                Err(e) if e.code() == HRESULT::from_win32(ERROR_CANCELLED.0) => {
                    return Ok(None);
                }
                Err(e) => return Err(e),
            }

            let items = dialog.GetResults()?;
            let count = items.GetCount()?;
            let mut paths = Vec::with_capacity(count as usize);

            for index in 0..count {
                let item = items.GetItemAt(index)?;
                let path = item.GetDisplayName(SIGDN_FILESYSPATH)?;
                let path_str = path.to_string()?;
                if filter_valid_extension(&path_str) {
                    paths.push(path.to_string()?);
                }
                CoTaskMemFree(Some(path.0 as _));
            }
            Ok(Some(paths))
        };

        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            let ret = proc();
            CoUninitialize();
            ret
        }
    }

    async fn handle_crossterm_event(&mut self, event: &CrosstermEvent) -> color_eyre::Result<()> {
        match event {
            CrosstermEvent::Key(key) => self.handle_key_event(*key).await?,
            CrosstermEvent::Resize(w, h) => self.handle_resize(*w, *h)?,
            _ => {}
        }
        Ok(())
    }

    fn handle_resize(&mut self, w: u16, h: u16) -> color_eyre::Result<()> {
        self.tui.resize(Rect::new(0, 0, w, h))?;
        self.render()?;
        Ok(())
    }

    fn render(&mut self) -> color_eyre::Result<()> {
        self.tui.draw(|frame| {
            StatefulWidget::render(
                self.root_wiget,
                frame.area(),
                frame.buffer_mut(),
                &mut self.app_state_container,
            );
        })?;
        Ok(())
    }
}
