use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::Direction,
    prelude::Rect,
    widgets::{StatefulWidget, Tabs, Widget},
};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tokio::sync::{
    mpsc::{self, unbounded_channel},
    watch::error,
};
use tracing::{debug, info};

use crate::{
    AppState::{self, AppState::AppStateContainer},
    action::Action,
    app,
    components::{Component, fps::FpsCounter},
    config::Config,
    extensions::OnceLock::OnceLock_ext,
    tui::{Event, Tui},
    widgets::{
        AppRoot::AppRoot,
        Button::Button,
        ButtonArea::{self, ButtonsArea},
    },
};

pub struct App {
    root_wiget: AppRoot,
    should_quit: bool,
    should_suspend: bool,
    tui: Tui,
    app_state_container: AppStateContainer,
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Home,
}

impl App {
    pub fn new(
        root_wiget: AppRoot,
        app_state_container: AppStateContainer,
    ) -> color_eyre::Result<Self> {
        Ok(Self {
            tui: Tui::new()?.mouse(true).paste(true),
            root_wiget: root_wiget,
            should_quit: false,
            should_suspend: false,
            app_state_container: app_state_container,
        })
    }

    pub async fn run(&mut self) -> color_eyre::Result<()> {
        self.tui.enter()?;
        loop {
            // self.handle_events(&mut tui).await?;
            self.handle_actions().await?;
            if self.should_suspend {
                self.tui.suspend()?;
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

    async fn handle_actions(&mut self) -> color_eyre::Result<()> {
        let Some(event) = self.tui.next_event().await else {
            return Ok(());
        };
        match &event {
            // Event::Tick => {
            //     self.last_tick_key_events.drain(..);
            // }
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
