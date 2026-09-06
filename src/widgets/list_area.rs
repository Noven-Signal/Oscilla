use std::cmp;

use crate::app::App;
use crate::app_state::app_state::{AppStateContainer, AreaHandler, PlayState};
use crate::extensions::rect::RectExtension;
use crate::key_guide::{KeyGuide, LineExt};
use crate::utils::VecExt;
use crate::{app_state, get_decorated_border};
use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, HighlightSpacing, List, ListDirection, ListItem, Widget};

#[derive(Default)]
pub struct ListArea {}

impl StatefulWidget for ListArea {
    type State = AppStateContainer;
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut AppStateContainer) {
        use app_state::app_state::*;

        let block = get_decorated_border!(state.focus_state, Tabs::ListArea);

        let items = state.play_list.iter().enumerate().map(|(i, x)| {
            let name = x.get_disp_name();
            let is_selected_item = state.play_list_selected.selected().is_some_and(|x| x == i);
            use PlayState::*;
            match state.play_state {
                Playing(idx) | Paused(idx) if idx == i => ListItem::new(
                    Span::raw(format!("♬  {name}")).style(match is_selected_item {
                        true => Color::Yellow,
                        false => Color::Magenta,
                    }),
                ),
                _ if is_selected_item => ListItem::new(
                    Span::raw("> ").style(Color::Red) + Span::raw(name).style(Color::Yellow),
                ),
                _ => ListItem::new(name),
            }
        });

        let add_list_base = Span::raw("add files<Ctrl + O>");

        let add_list_item = match state.play_list_selected.selected() {
            Some(play_list_selected_idx)
                if match state.play_list.last_index() {
                    Some(play_list_last_idx) => play_list_last_idx + 1 == play_list_selected_idx,
                    None => true,
                } =>
            {
                add_list_base
                    .style(Color::Red)
                    .bg(Color::White)
                    .into_centered_line()
            }
            _ => add_list_base.underlined().into_centered_line(),
        };

        let items = items.chain([ListItem::new(add_list_item)]);

        let list = List::new(items)
            .style(Color::White)
            .highlight_spacing(HighlightSpacing::Never)
            .scroll_padding(1)
            .direction(ListDirection::TopToBottom)
            .repeat_highlight_symbol(true);

        let block = match block {
            Some(block) => block,
            None => Block::bordered().border_style(Style::new().fg(Color::White)),
        };
        block.title("playlist").render(area, buf);
        StatefulWidget::render(list, area.margin(None), buf, &mut state.play_list_selected);
    }
}

impl AreaHandler for ListArea {
    fn handle_key(app_state_container: &mut AppStateContainer, key_code: KeyCode) {
        match key_code {
            KeyCode::Up => app_state_container.play_list_selected.select_previous(),
            KeyCode::Down => app_state_container.play_list_selected.select_next(),
            KeyCode::Enter => 'b1: {
                let Some(selected_idx) = app_state_container.play_list_selected.selected() else {
                    break 'b1;
                };
                match selected_idx {
                    _x if app_state_container
                        .play_list
                        .last_index()
                        .is_some_and(|playlist_last_idx| selected_idx <= playlist_last_idx) =>
                    {
                        App::play_track(app_state_container, selected_idx)
                    }
                    _ => App::add_new_files(app_state_container),
                };
            }
            KeyCode::Delete => 'b1: {
                let Some(idx) = app_state_container.play_list_selected.selected() else {
                    break 'b1;
                };
                if let PlayState::Playing(playing_idx) | PlayState::Paused(playing_idx) =
                    app_state_container.play_state
                    && idx == playing_idx
                {
                    break 'b1;
                };
                let Some(_) = app_state_container.play_list.get(idx) else {
                    break 'b1;
                };
                app_state_container.play_list.remove(idx);
                match app_state_container.play_state {
                    PlayState::Playing(ref mut playing_idx)
                    | PlayState::Paused(ref mut playing_idx)
                        if idx < *playing_idx =>
                    {
                        *playing_idx = playing_idx.saturating_sub(1);
                    }
                    _ => {}
                };
            }
            _ => {}
        }
    }

    fn lost_tab_selection_handler(app_state_container: &mut AppStateContainer) {
        app_state_container.play_list_selected.select(None);
    }

    fn render_bottom_line_text_area_selected(
        buf: &mut Buffer,
        area: Rect,
        app_state_container: &mut AppStateContainer,
    ) {
        let audio_file_info = app_state_container
            .play_list_selected
            .selected()
            .and_then(|idx| app_state_container.play_list.get(idx));

        let file_desc = match audio_file_info {
            Some(x) => Span::raw("👉 ") + Span::from(x.get_disp_name().to_string()),
            None => Line::default(),
        };

        let min_selected_width =
            cmp::min(file_desc.width(), (area.width as f32 * 0.6) as usize) as u16;
        let [keygides_area, selected_item_disp_area] = area.layout(
            &Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Fill(1), Constraint::Max(min_selected_width)])
                .spacing(1),
        );

        let now_selected_item_is_now_playing = {
            if let Some(now_playing_idx) = app_state_container.play_state.get_now_playing_idx()
                && let Some(selected_idx) = app_state_container.play_list_selected.selected()
            {
                now_playing_idx == selected_idx
            } else {
                false
            }
        };

        let key_guides = Line::from_key_guide(
            [
                KeyGuide::ESC_DEFAULT,
                KeyGuide::new_mazenta(
                    "Enter",
                    match audio_file_info {
                        Some(_) => "Play Selected Item",
                        None => "Open files",
                    },
                ),
            ]
            .into_iter()
            .chain(match audio_file_info {
                Some(_) if !now_selected_item_is_now_playing => {
                    vec![KeyGuide::new_mazenta("Del", "Remove Selected Item")].into_iter()
                }
                _ => vec![].into_iter(),
            })
            .chain(KeyGuide::get_global_gudies(app_state_container)),
            keygides_area.width.into(),
        );

        key_guides.render(keygides_area, buf);
        file_desc.render(selected_item_disp_area, buf);
    }
}
