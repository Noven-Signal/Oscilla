use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::Direction,
    prelude::Rect,
    widgets::{Tabs, Widget},
};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tokio::sync::{mpsc, watch::error};
use tracing::{debug, info};

use crate::{
    AppState,
    action::Action,
    components::{Component, fps::FpsCounter},
    config::Config,
    extensions::OnceLock::OnceLock_ext,
    tui::{Event, Tui},
    widgets::{
        Button::Button,
        ButtonArea::{self, ButtonsArea},
    },
};

pub struct App<T: Widget + Copy> {
    config: Config,
    root_wiget: T,
    should_quit: bool,
    should_suspend: bool,
    mode: Mode,
    last_tick_key_events: Vec<KeyEvent>,
    action_tx: mpsc::UnboundedSender<Action>,
    action_rx: mpsc::UnboundedReceiver<Action>,
    tui: Tui,
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Home,
}

impl<T: Widget + Copy> App<T> {
    pub fn new(root_wiget: T) -> color_eyre::Result<Self> {
        let (action_tx, action_rx) = mpsc::unbounded_channel();
        Ok(Self {
            tui: Tui::new()?.mouse(true).paste(true),
            root_wiget: root_wiget,
            should_quit: false,
            should_suspend: false,
            config: Config::new()?,
            mode: Mode::Home,
            last_tick_key_events: Vec::new(),
            action_tx,
            action_rx,
        })
    }

    pub async fn run(&mut self) -> color_eyre::Result<()> {
        self.tui.enter()?;

        let action_tx = self.action_tx.clone();
        loop {
            // self.handle_events(&mut tui).await?;
            self.handle_actions().await?;
            if self.should_suspend {
                self.tui.suspend()?;
                action_tx.send(Action::Resume)?;
                action_tx.send(Action::ClearScreen)?;
                // tui.mouse(true);
                self.tui.enter()?;
            } else if self.should_quit {
                self.tui.stop()?;
                break;
            }
        }
        self.tui.exit()?;
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> color_eyre::Result<()> {
        let action_tx = self.action_tx.clone();

        let Some(keymap) = self.config.keybindings.0.get(&self.mode) else {
            return Ok(());
        };
        use AppState::AppState::*;

        let move_key_pressed_handler = |key_code: KeyCode| {
            let mut focus_state_mutex = focus_state.get_mutex_guard();

            match *focus_state_mutex {
                TabState::Focused(_) | TabState::None => {
                    let next = (*focus_state_mutex).get_focus_tab(key_code);
                    *focus_state_mutex = match next {
                        Some(next) => TabState::Focused(next),
                        None => TabState::None,
                    }
                }
                TabState::Selected(tabs) => {
                    tabs.handle_key(key_code);
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
                let _ = self.tui.event_tx.send(Event::Quit);
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
                    let mut focus_state_mutex = focus_state.get_mutex_guard();
                    match *focus_state_mutex {
                        TabState::Focused(tab) => {
                            focus_state_mutex.select();
                            tab.get_tab_selected_handler();
                        }
                        _ => {}
                    }
                }
                KeyCode::Esc => {
                    let mut focus_state_mutex = focus_state.get_mutex_guard();
                    match *focus_state_mutex {
                        TabState::Focused(_) => *focus_state_mutex = TabState::None,
                        TabState::Selected(tabs) => *focus_state_mutex = TabState::Focused(tabs),
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

    async fn handle_actions(&mut self) -> color_eyre::Result<()> {
        let Some(event) = self.tui.next_event().await else {
            return Ok(());
        };
        let action_tx = self.action_tx.clone();
        match &event {
            Event::Tick => {
                self.last_tick_key_events.drain(..);
            }
            Event::Quit => self.should_quit = true,
            Event::Key(key) => self.handle_key_event(*key)?,
            Event::Suspend => self.should_suspend = true,
            Event::Resume => self.should_suspend = false,
            Event::ClearScreen => self.tui.terminal.clear()?,
            Event::Resize(w, h) => self.handle_resize(*w, *h)?,
            Event::Render => self.render()?,
            // Event::FileDropped(_) => {
            //     // Pass file drop events to components
            // }
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

    fn partial_render(&mut self, reder_wiget:impl Widget) -> color_eyre::Result<()> {
        self.tui.draw(|frame| {
            //self.root_wiget.render(frame.area(), frame.buffer_mut());
            frame.render_widget(reder_wiget, frame.area());
        })?;
        Ok(())
    }
}
