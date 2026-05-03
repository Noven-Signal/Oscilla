use std::time::Duration;

use crate::{
    AppState::{
        self,
        AppState::{AppStateContainer, PlayState, PlayingTrackInfo},
    },
    manipulation::{self, PlayerControlSignal},
    tui::Tui,
    widgets::AppRoot::AppRoot,
};
use color_eyre::eyre::Ok;
use crossterm::event::Event as CrosstermEvent;
use crossterm::event::{EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::{FutureExt, StreamExt};
use ratatui::{prelude::Rect, widgets::StatefulWidget};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

pub struct App {
    root_wiget: AppRoot,
    tui: Tui,
    app_state_container: AppStateContainer,
    player_to_ui_singnal_signal: UnboundedSender<PlayerToUISingnal>,
    player_to_ui_singnal_receiver: UnboundedReceiver<PlayerToUISingnal>,
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
pub enum PlayerToUISingnal {
    NoticeTrackInfo(TrackInfo),
    PlayedFrames(u32),
}

impl App {
    pub fn new(
        root_wiget: AppRoot,
        app_state_container: AppStateContainer,
    ) -> color_eyre::Result<Self> {
        let (sender, receiver) = unbounded_channel();
        Ok(Self {
            tui: Tui::new()?.mouse(true).paste(true),
            root_wiget: root_wiget,
            app_state_container: app_state_container,
            player_to_ui_singnal_signal: sender,
            player_to_ui_singnal_receiver: receiver,
            event_loop_canceled: false,
        })
    }

    async fn init_auto_play(&mut self) -> JoinHandle<()> {
        let play_list_arc = self.app_state_container.play_list.clone();
        self.app_state_container.play_state = PlayState::Playing(0);
        let (player_control_signal_sender, mut player_control_signal_recv) =
            unbounded_channel::<PlayerControlSignal>();
        self.app_state_container.player_control_singnal_sender = Some(player_control_signal_sender);
        let player_to_ui_singnal_sender = self.player_to_ui_singnal_signal.clone();
        tokio::spawn(async move {
            let Some(first_track) = play_list_arc.first() else {
                return;
            };
            manipulation::play_executor(
                first_track,
                &mut player_control_signal_recv,
                player_to_ui_singnal_sender,
            )
            .await;
        })
    }

    pub async fn run(&mut self) -> color_eyre::Result<()> {
        let init_auto_play_handle = self.init_auto_play().await;
        self.tui.enter()?;

        self.event_loop().await;
        init_auto_play_handle.await;

        self.tui.exit();

        Ok(())
    }

    async fn event_loop(&mut self) -> color_eyre::Result<()> {
        let mut event_stream = EventStream::new();

        'l1: loop {
            if self.event_loop_canceled {
                break 'l1;
            }
            tokio::select! {
                signal = self.player_to_ui_singnal_receiver.recv() => {
                    match signal{
                        Some(signal) => self.handle_player_to_ui_singnal(signal)?,
                        None => break 'l1
                    }
                }
                crossterm_event = event_stream.next().fuse() => match crossterm_event {
                    Some(Result::Ok(event)) => self.handle_crossterm_event(&event).await?,
                    _ => break 'l1,
                },
            };
        }
       
        if let Some(ref mut sender) = self.app_state_container.player_control_singnal_sender {
            sender.send(PlayerControlSignal::Stop);
        }

        Ok(())
    }

    fn handle_player_to_ui_singnal(&mut self, signal: PlayerToUISingnal) -> color_eyre::Result<()> {
        let playing_tarck_info = &mut self.app_state_container.playing_track_info;
        match signal {
            PlayerToUISingnal::NoticeTrackInfo(track_info) => {
                *playing_tarck_info = Some(PlayingTrackInfo {
                    track_duraion: track_info.track_duration,
                    current_played_duration: Duration::ZERO,
                    sample_rate: track_info.sample_rate,
                });
                self.render()?;
            }
            PlayerToUISingnal::PlayedFrames(frames) => 'b1: {
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
                if before_duration.as_secs() != after_duration.as_secs() {
                    self.render()?;
                }
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
