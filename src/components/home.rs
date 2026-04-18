use std::{default, ops::Deref, sync::Mutex, vec};

use color_eyre::{eyre::Error, owo_colors::OwoColorize};
use ratatui::{
    macros,
    prelude::*,
    style::Styled,
    symbols::border::{self, DOUBLE, ROUNDED},
    widgets::*,
};
use tokio::sync::mpsc::UnboundedSender;

use super::Component;
use crate::{
    AppState, action::Action, app::App, config::Config, extensions::OnceLock::OnceLock_ext,
    widgets::BottomPart::BottomPart,
};

#[derive(Default)]
pub struct Home {
    command_tx: Option<UnboundedSender<Action>>,
    config: Config,
    dropped_file_path: Option<String>,
}

impl Home {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Component for Home {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> color_eyre::Result<()> {
        self.command_tx = Some(tx);
        Ok(())
    }

    fn register_config_handler(&mut self, config: Config) -> color_eyre::Result<()> {
        self.config = config;
        Ok(())
    }

    fn update(&mut self, action: Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::Tick => {
                // add any logic here that should run on every tick
            }
            Action::Render => {
                // add any logic here that should run on every render
            }
            Action::FileDropped(path) => {
                self.dropped_file_path = Some(path);
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, f: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let [top, bottom] = area.layout(
            &Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Fill(1), Constraint::Length(6)])
        );

        let [list_area, effect_area] = top.layout(
            &Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(25), Constraint::Percentage(75)]),
        );
        use AppState::AppState::*;
        let mut state = ListState::default();
        state.select_first();

        let focus_state_mutex = focus_state.get_mutex_guard();

        let block = match *focus_state_mutex {
            TabState::Focused(Tabs::ListArea) => Block::focused_block(),
            TabState::Selected(Tabs::ListArea) => Block::selected_block(),
            _ => Block::bordered(),
        };

        let items = [
            "[Gusteau]: With enough passion, yes.",
            "[Remy]: But can anyone build a TUI in Rust?",
            "[Gusteau]: Anyone can cook!",
            &format!("focus_state_mutex_ref: {:?}", *focus_state_mutex),
        ];
        drop(focus_state_mutex);

        let list = List::new(items)
            .style(Color::White)
            .highlight_style(Style::new().yellow().italic())
            .highlight_symbol("> ".red())
            .scroll_padding(1)
            .direction(ListDirection::TopToBottom)
            .repeat_highlight_symbol(true);

        f.render_widget(block.title("song list"), list_area);

        f.render_stateful_widget(list, list_area.margin(None), &mut state);

        f.render_widget(Block::default(), effect_area);

        f.render_widget(BottomPart::default(), bottom);

        Ok(())
    }
}

pub trait SelectedBlock {
    fn focused_block() -> Self;
    fn selected_block() -> Self;
}
impl SelectedBlock for Block<'_> {
    fn focused_block() -> Self {
        Block::bordered()
            .border_style(Style::new().fg(Color::Rgb(180, 120, 120)))
            .border_set(border::LIGHT_DOUBLE_DASHED)
    }

    fn selected_block() -> Self {
        Block::bordered()
            .border_style(Style::new().fg(Color::Red))
            .border_set(border::ROUNDED)
    }
}

pub trait RectExtension {
    fn margin(&self, margin: Option<Margin>) -> Self;
}
#[derive(Default)]
pub struct Margin {
    left: u16,
    right: u16,
    top: u16,
    bottom: u16,
}
impl Margin {
    pub const fn default() -> Self {
        Self {
            left: 1,
            right: 1,
            top: 1,
            bottom: 1,
        }
    }
}

impl RectExtension for Rect {
    fn margin(&self, margin: Option<Margin>) -> Rect {
        let margin = if let Some(margin) = margin {
            margin
        } else {
            Margin::default()
        };
        let [_vheader, v_list_inner, _vfodter] = self.layout(
            &Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(margin.left),
                    Constraint::Fill(1),
                    Constraint::Length(margin.right),
                ]),
        );
        let [_h_header, h_list_inner, _h_fodter] = v_list_inner.layout(
            &Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(margin.top),
                    Constraint::Fill(1),
                    Constraint::Length(margin.bottom),
                ]),
        );
        h_list_inner
    }
}
