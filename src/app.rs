use std::{
    collections::VecDeque,
    fmt::Debug,
    pin::Pin,
    sync::atomic::{AtomicPtr, Ordering},
    time::{Duration, SystemTime},
    usize,
};

use crate::{
    AppState::{
        self,
        AppState::{
            AppStateContainer, PlayState, PlayerThread, PlayingTrackInfo, VeSelectedTab,
            VeSwitcherRequestSignal,
        },
    },
    app,
    // event_handler::{EventHandler, EventHndlerToAppSignal},
    manipulation::{
        self, AudioDeviceInfo, DecoderControlSignal::VeDisabled, NUM_OF_BLOCK_VE, OscilloscopeData,
        PlayerControlSignal, UiVEThreadSyncSignal, VESharedBuffer,
    },
    tui::Tui,
    utils::array_init,
    widgets::AppRoot::AppRoot,
};
use color_eyre::eyre::Ok;
use crossterm::event::Event as CrosstermEvent;
use crossterm::event::{EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::{FutureExt, StreamExt, channel::mpsc::unbounded, future};
use ratatui::{prelude::Rect, widgets::StatefulWidget};
use serde::{Deserialize, Serialize};
use symphonia::core::units::TimeStamp;
use tokio::{
    io::join,
    join, pin,
    sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    task::JoinHandle,
    time::Interval,
};
use tokio_util::sync::CancellationToken;
use tracing::info;
use windows::Win32::System::Com::IInternalUnknown;

pub struct Ves {
    pub ui_to_ve_signal_sender: UnboundedSender<UiVEThreadSyncSignal>,
    pub ve_to_ui_signal_recv: UnboundedReceiver<UiVEThreadSyncSignal>,
    pub ve_buffer_duration_offset_sec: f64,
    pub interval: Interval,
    pub ve_frame_count: u32,
    pub ve_read_exclusive: usize,
}

#[derive(Clone, Copy)]
pub struct VeSwitcherSyncSignal();

pub enum AppContorlSignal {
    StartPlayer(usize),
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
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Home,
}

pub struct TrackInfo {
    pub file_sample_rate: usize,
    pub audio_device_sample_rate: usize,
    pub track_duration: Duration,
}
pub struct PlayedFrames {
    pub frames: u32,
    pub buffered_frames: u32,
}
pub enum PlayerToUISingnal {
    NoticeTrackInfo(TrackInfo),
    PlayedFrames(PlayedFrames),
    VeEnabled(PlayerToUISingnalVeEnabled),
    VeEnabledSync,
    VeDisabled,
    RendererPlayCompleted,
    EndOfStream,
}

pub struct PlayerToUISingnalVeEnabled {
    pub ui_to_ve_signal_sender: UnboundedSender<UiVEThreadSyncSignal>,
    pub ve_to_ui_signal_recv: UnboundedReceiver<UiVEThreadSyncSignal>,
    pub ve_buffer_duration_offset_sec: f64,
}

impl App {
    pub fn new(
        root_wiget: AppRoot,
        app_state_container: AppStateContainer,
        app_control_signal_recv: UnboundedReceiver<AppContorlSignal>,
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
        })
    }

    fn start_player(&mut self, idx: usize) {
        if let Some(_) = self.app_state_container.player_thread {
            return;
        }

        let (player_to_ui_signal_sender, player_to_ui_signal_recv) = unbounded_channel();
        self.player_to_ui_signal_recv = Some(player_to_ui_signal_recv);

        let app_state_container = &mut self.app_state_container;

        let play_list_arc = app_state_container.play_list.clone();
        app_state_container.play_state = PlayState::Playing(idx);
        let (player_control_signal_sender, mut player_control_signal_recv) = unbounded_channel();

        //app_state_container.player_control_singnal_sender = Some(player_control_signal_sender);
        let init_vol = app_state_container.vol_state;
        let player_handle = tokio::spawn(async move {
            let Some(first_track) = play_list_arc.get(idx) else {
                return;
            };

            manipulation::play_executor(
                first_track,
                init_vol,
                &mut player_control_signal_recv,
                player_to_ui_signal_sender,
            )
            .await;
        });

        app_state_container.player_thread = Some(PlayerThread {
            handle: player_handle,
            player_control_singnal_sender: player_control_signal_sender,
        });
    }

    async fn ve_sync(ve_read_exclusive: &mut usize) {
        const NUM_OF_BLOCK_VE_LAST_INDEX: usize = NUM_OF_BLOCK_VE - 1;
        *ve_read_exclusive = match *ve_read_exclusive {
            NUM_OF_BLOCK_VE_LAST_INDEX => 0,
            x => x + 1,
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
            player_control_singnal_sender.send(PlayerControlSignal::Stop);
            handle.await?;
        }

        self.tui.exit()?;

        Ok(())
    }

    pub async fn event_loop(&mut self) -> color_eyre::Result<()> {
        let mut event_stream = EventStream::new();

        let mut ve_switcher_requst_signal: Option<VeSwitcherRequestSignal> = None;
        let mut ve_switcher_sync_signal: Option<VeSwitcherSyncSignal> = None;

        'l1: loop {
            if self.event_loop_canceled {
                break 'l1;
            }

            enum VeProcResult {
                VeDisabled,
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
                            ui_to_ve_signal_sender.send(UiVEThreadSyncSignal());
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
                    _ = self
                        .ve_switcher_sync_signal_sender
                        .send(VeSwitcherSyncSignal());
                };
            }

            tokio::select! {
                Some(signal) = self.app_control_signal_recv.recv() => {
                    match signal {
                        AppContorlSignal::StartPlayer(idx) => {
                           before_start_player_clean_up_statement!();

                            self.start_player(idx);
                        },
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

                        ves.ui_to_ve_signal_sender.send(UiVEThreadSyncSignal());
                        ves.ve_frame_count = ves.ve_frame_count + 1;
                    }
                },
                signal = async {
                    match ve_switcher_requst_signal{
                        Some(_) =>  future::pending().await,
                        None =>  self.app_state_container.ve_switcher_request_signal_recv.recv().await,
                    }
                } => {
                    match signal{
                        Some(signal) => {
                        ve_switcher_requst_signal = Some(signal);
                        },
                        None => {},
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
                _ = async{
                    match &mut self.app_state_container.player_thread{
                        Some(PlayerThread { handle, .. }) => handle.await,
                        None => future::pending().await,
                    }
                } => {
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
            {
                self.handle_ve_switching(signal.request_tab)?;
                ve_switcher_requst_signal = None;
                ve_switcher_sync_signal = None;
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
            VeSelectedTab::Off => 'b1: {
                sender.send(PlayerControlSignal::VeDisabled);
            }
            _ => {
                let Some(playing_track_info) = &app_state_container.playing_track_info else {
                    return Ok(());
                };
                app_state_container.ve_shared_buffer = {
                    let move_window = playing_track_info.audio_device_sample_rate as usize
                        / crate::visual_effects::Oscilloscope::FRAME_RATE;
                    let crate_move_window_size_vec =
                        || (0..move_window).map(|i| (i as f64, 0f64)).collect();
                    let arr = array_init(|| {
                        array_init(|| OscilloscopeData(crate_move_window_size_vec()))
                    });
                    Some(arr)
                };
                let ve_shared_buffer = app_state_container
                    .ve_shared_buffer
                    .as_mut()
                    .expect("must be Some because init above line");

                let ptr = AtomicPtr::new(ve_shared_buffer);

                sender.send(PlayerControlSignal::VeEnabled(ptr));
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
                    self.app_state_container
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
            }) => 'b1: {
                let Some(ref mut track_info) = *playing_tarck_info else {
                    break 'b1;
                };

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
                    ve_read_exclusive: 0,
                });
            }
            PlayerToUISingnal::VeDisabled => {
                self.app_state_container.ve_shared_buffer = None;
                self.app_state_container.ve_channel = None;

                _ = self
                    .ve_switcher_sync_signal_sender
                    .send(VeSwitcherSyncSignal());
            }
            PlayerToUISingnal::VeEnabledSync => {
                _ = self
                    .ve_switcher_sync_signal_sender
                    .send(VeSwitcherSyncSignal());
            }
            // PlayerToUISingnal::PlayCompleted => {
            //     if let Some(PlayerThread {
            //         player_control_singnal_sender,
            //         ..
            //     }) = &mut self.app_state_container.player_thread
            //     {
            //         _ = player_control_singnal_sender.send(PlayerControlSignal::Stop);
            //     }
            //     self.app_state_container.play_state = PlayState::Stopped;
            //     self.app_state_container.playing_track_info = None;
            //     self.app_state_container.ve_channel = None;

            //     _ = self.render();
            // }
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
                player_control_singnal_sender.send(PlayerControlSignal::Stop);
            }
        }
        Ok(())
    }

    async fn handle_key_event(&mut self, key: KeyEvent) -> color_eyre::Result<()> {
        use AppState::AppState::*;

        let mut move_key_pressed_handler = |key_code: KeyCode| {
            let app_state_container = &mut self.app_state_container;
            match app_state_container.focus_state {
                TabState::Focused(_) | TabState::None => {
                    let next = (app_state_container.focus_state).get_focus_tab(key_code);
                    app_state_container.focus_state = match next {
                        Some(next) => TabState::Focused(next),
                        None => TabState::None,
                    }
                }
                TabState::Selected(tabs) => {
                    tabs.handle_key(&mut self.app_state_container, key_code);
                }
            }
        };

        match key {
            KeyEvent {
                code: KeyCode::Char('d'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                ..
            } => 'b1: {
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
                code,
                kind: KeyEventKind::Press,
                ..
            } => match code {
                KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => {
                    move_key_pressed_handler(code)
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
                            app_state_container.focus_state = TabState::Focused(tabs)
                        }
                        TabState::None => {}
                    }
                }
                _ => {}
            },
            _ => {}
        };

        self.render()?;
        Ok(())
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
        self.partial_render(self.root_wiget)
    }

    fn partial_render(&mut self, app_root: AppRoot) -> color_eyre::Result<()> {
        self.tui.draw(|frame| {
            StatefulWidget::render(
                app_root,
                frame.area(),
                frame.buffer_mut(),
                &mut self.app_state_container,
            );
        })?;
        Ok(())
    }
}
