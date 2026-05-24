use std::{
    collections::VecDeque,
    pin::Pin,
    sync::atomic::{AtomicPtr, Ordering},
    time::{Duration, SystemTime},
};

use crate::{
    AppState::{
        self,
        AppState::{AppStateContainer, PlayState, PlayingTrackInfo},
    },
    app,
    // event_handler::{EventHandler, EventHndlerToAppSignal},
    manipulation::{
        self, BLOCK_SIZE, DecoderControlSignal::VeDisabled, NUM_OF_BLOCK_VE, OscilloscopeData,
        PlayerControlSignal, UiVEThreadSyncSignal, VESharedBuffer,
    },
    tui::Tui,
    utils::array_init,
    widgets::AppRoot::AppRoot,
};
use color_eyre::eyre::Ok;
use crossterm::event::Event as CrosstermEvent;
use crossterm::event::{EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::{FutureExt, StreamExt, future};
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
use windows::Win32::System::Com::IInternalUnknown;

struct Ves {
    pub ui_to_ve_signal_sender: UnboundedSender<UiVEThreadSyncSignal>,
    pub ve_to_ui_signal_recv: UnboundedReceiver<UiVEThreadSyncSignal>,
    pub ve_buffer_duration_offset_sec: f64,
}

pub struct App {
    root_wiget: AppRoot,
    tui: Tui,
    app_state_container: AppStateContainer,
    ve_channel: Option<Ves>,
    event_loop_canceled: bool,
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Home,
}

pub struct TrackInfo {
    pub sample_rate: u32,
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
    ) -> color_eyre::Result<Self> {
        Ok(Self {
            tui: Tui::new()?.mouse(true).paste(true),
            root_wiget: root_wiget,
            app_state_container: app_state_container,
            event_loop_canceled: false,
            ve_channel: None,
        })
    }

    async fn init_auto_play(
        &mut self,
        player_to_ui_signal_sender: UnboundedSender<PlayerToUISingnal>,
    ) -> JoinHandle<()> {
        let app_state_container = &mut self.app_state_container;

        let play_list_arc = app_state_container.play_list.clone();
        app_state_container.play_state = PlayState::Playing(0);
        let (player_control_signal_sender, mut player_control_signal_recv) = unbounded_channel();

        app_state_container.player_control_singnal_sender = Some(player_control_signal_sender);
        let player_to_ui_singnal_sender = player_to_ui_signal_sender.clone();

        let ve_shaerd_buffer_ptr = {
            let buffer_ref = &mut self.app_state_container.ve_shared_buffer;
            AtomicPtr::new(&raw mut *buffer_ref)
        };

        tokio::spawn(async move {
            let Some(first_track) = play_list_arc.first() else {
                return;
            };

            manipulation::play_executor(
                first_track,
                &mut player_control_signal_recv,
                player_to_ui_singnal_sender,
                ve_shaerd_buffer_ptr,
            )
            .await;
        })
    }

    async fn ve_sync(app_state_container: &mut AppStateContainer) {
        const NUM_OF_BLOCK_VE_LAST_INDEX: usize = NUM_OF_BLOCK_VE - 1;
        let raed_exclusive = &mut app_state_container.ve_read_exclusive;
        *raed_exclusive = match *raed_exclusive {
            NUM_OF_BLOCK_VE_LAST_INDEX => 0,
            x => x + 1,
        };
    }

    pub async fn run(&mut self) -> color_eyre::Result<()> {
        let (player_to_ui_signal_sender, player_to_ui_singnal_receiver) = unbounded_channel();


        let init_auto_play_handle = self
            .init_auto_play(
                player_to_ui_signal_sender,
            )
            .await;

        self.tui.enter()?;

        self.event_loop(
            player_to_ui_singnal_receiver,
        )
        .await?;

        if let Some(ref mut sender) = self.app_state_container.player_control_singnal_sender {
            sender.send(PlayerControlSignal::Stop)?;
        }

        init_auto_play_handle.await?;

        self.tui.exit()?;

        Ok(())
    }

    pub async fn event_loop(
        &mut self,
        mut player_to_ui_singnal_receiver: UnboundedReceiver<PlayerToUISingnal>,
    ) -> color_eyre::Result<()> {
        let mut event_stream = EventStream::new();

        let mut interval = tokio::time::interval(Duration::from_secs_f64(1f64 / 60f64));
        let mut ve_frame_count: u32 = 0;

        'l1: loop {
            if self.event_loop_canceled {
                break 'l1;
            }

            enum VeProcResult {
                VeDisabled,
                SyncRange(UnboundedSender<UiVEThreadSyncSignal>),
                VeIsTooForward,
            }

            let mut ve_timing_awaiter = async |s: &Self| {
                if let Some(_) = s.ve_channel {
                    interval.tick().await;
                } else {
                    future::pending::<()>().await;
                }
            };
            let mut ve_proc = async |s: &mut Self, ve_frame_count: &mut u32| -> VeProcResult {
                let Some(Ves {
                    ref ui_to_ve_signal_sender,
                    ref mut ve_to_ui_signal_recv,
                    ve_buffer_duration_offset_sec: init_offset,
                }) = s.ve_channel
                else {
                    return VeProcResult::VeDisabled;
                };
                let Some(track_info) = &s.app_state_container.playing_track_info else {
                    return VeProcResult::VeDisabled;
                };
                let carib_played_duration = track_info
                    .current_played_duration
                    .saturating_sub(track_info.audio_device_buffered_duration)
                    .as_secs_f64();

                let ve_position = (*ve_frame_count as f64 / 60f64) + init_offset;

                const n1_60_dobule: f64 = 2f64 / 60f64;
                const m_n1_60_dobule: f64 = -n1_60_dobule;
                match carib_played_duration - ve_position {
                    ..=m_n1_60_dobule => VeProcResult::VeIsTooForward,
                    m_n1_60_dobule..=n1_60_dobule => {
                        ve_to_ui_signal_recv.recv().await;
                        VeProcResult::SyncRange(ui_to_ve_signal_sender.clone())
                    }
                    diff => {
                        let num_of_frame_forward = (diff / n1_60_dobule).abs().floor() as u32;
                        for _ in 0..num_of_frame_forward - 1 {
                            //dbg!(i);
                            ve_to_ui_signal_recv.recv().await;

                            Self::ve_sync(&mut s.app_state_container).await;
                            ui_to_ve_signal_sender.send(UiVEThreadSyncSignal());
                            *ve_frame_count = *ve_frame_count + 1;
                        }
                        ve_to_ui_signal_recv.recv().await;
                        VeProcResult::SyncRange(ui_to_ve_signal_sender.clone())
                    }
                }
            };
            tokio::select! {
                signal = player_to_ui_singnal_receiver.recv() => {
                    match signal{
                        Some(signal) => self.handle_player_to_ui_signal(signal)?,
                        None => break 'l1
                    }
                }
                crossterm_event = event_stream.next().fuse() => match crossterm_event {
                    Some(Result::Ok(event)) => self.handle_crossterm_event(&event).await?,
                    _ => break 'l1,
                },
                _ = ve_timing_awaiter(self) =>'b1: {
                    let proc_res = ve_proc(self,&mut ve_frame_count).await;
                    let VeProcResult::SyncRange(ui_to_ve_signal_sender) = proc_res else { break 'b1;};

                    _ = self.render();
                    Self::ve_sync(&mut self.app_state_container).await;

                    ui_to_ve_signal_sender.send(UiVEThreadSyncSignal());
                    ve_frame_count = ve_frame_count+1;

                },
            };
        }

        Ok(())
    }

    fn handle_player_to_ui_signal(&mut self, signal: PlayerToUISingnal) -> color_eyre::Result<()> {
        let playing_tarck_info = &mut self.app_state_container.playing_track_info;
        match signal {
            PlayerToUISingnal::NoticeTrackInfo(track_info) => {
                *playing_tarck_info = Some(PlayingTrackInfo {
                    track_duraion: track_info.track_duration,
                    current_played_duration: Duration::ZERO,
                    sample_rate: track_info.sample_rate,
                    audio_device_buffered_duration: Duration::ZERO,
                });

                if let Some(sender) = &self.app_state_container.player_control_singnal_sender {
                    let sender = sender.clone();
                    tokio::spawn(async move {
                        //tokio::time::sleep(Duration::from_secs(1)).await;
                        // sender.send(PlayerControlSignal::VeEnabled);
                    });

                    sender.send(PlayerControlSignal::VeEnabled);
                    //self.ve_event_tick_enabled = true;
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

                let add_duration =
                    Duration::from_secs_f64(frames as f64 / track_info.sample_rate as f64);
                let before_duration = track_info.current_played_duration;
                let after_duration = track_info
                    .current_played_duration
                    .saturating_add(add_duration);
                track_info.current_played_duration = after_duration;
                track_info.audio_device_buffered_duration = Duration::from_secs_f64(
                    (buffered_frames + frames) as f64 / track_info.sample_rate as f64,
                );
                if before_duration.as_secs() != after_duration.as_secs() {
                    self.render()?;
                }
            }
            PlayerToUISingnal::VeEnabled(PlayerToUISingnalVeEnabled {
                ui_to_ve_signal_sender,
                ve_to_ui_signal_recv,
                ve_buffer_duration_offset_sec,
            }) => {
                self.ve_channel = Some(Ves {
                    ui_to_ve_signal_sender,
                    ve_to_ui_signal_recv,
                    ve_buffer_duration_offset_sec,
                })
            }
        }
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> color_eyre::Result<()> {
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
            } => {
                self.event_loop_canceled = true;
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
            CrosstermEvent::Key(key) => self.handle_key_event(*key)?,
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
