use ratatui::{
    style::{Color, Stylize},
    text::{Line, Span},
};

use crate::app_state::app_state::{AppStateContainer, PlayState};

#[derive(Clone, Copy)]
pub struct KeyGuide<'a> {
    pub key: &'a str,
    pub desc: &'a str,
    pub color: Color,
}

impl<'a> KeyGuide<'a> {
    pub const fn new_mazenta(key: &'a str, desc: &'a str) -> Self {
        Self {
            key,
            desc,
            color: Color::Magenta,
        }
    }

    pub const fn new_gray(key: &'a str, desc: &'a str) -> Self {
        Self {
            key,
            desc,
            color: Color::Rgb(88, 61, 92),
        }
    }

    pub const ESC_DEFAULT: Self = KeyGuide {
        key: "Esc",
        desc: "Cancel",
        color: Color::Magenta,
    };

    pub fn get_global_gudies(app_state_container: &AppStateContainer) -> Vec<KeyGuide<'static>> {
        let space_guide = match app_state_container.play_state {
            PlayState::Playing(_) => vec![
                KeyGuide::new_gray("Space", "Pause"),
                KeyGuide::new_gray("Ctrl+Space", "Stop"),
            ],
            PlayState::Paused(_) => vec![
                KeyGuide::new_gray("Space", "Resume"),
                KeyGuide::new_gray("Ctrl+Space", "Stop"),
            ],
            PlayState::Stopped => vec![],
        };

        space_guide
            .into_iter()
            .chain(KeyGuide::GLOBAL_GUIDES)
            .collect()
    }

    const GLOBAL_GUIDES: [KeyGuide<'static>; 4] = [
        KeyGuide::new_gray("Ctrl+↑", "Volume +10"),
        KeyGuide::new_gray("Ctrl+↓", "Volume -10"),
        KeyGuide::new_gray("Ctrl+O", "Open files"),
        KeyGuide::new_gray("Ctrl+D", "Quit Oscilla"),
    ];
}

pub trait LineExt {
    fn from_key_guide<'a>(
        key_guides: impl Iterator<Item = KeyGuide<'a>>,
        available_width: usize,
    ) -> Line<'a>;

    fn from_key_guide_optioanl<'a>(
        key_guides: impl Iterator<Item = Option<KeyGuide<'a>>>,
        available_width: usize,
    ) -> Line<'a>;
}
impl LineExt for Line<'_> {
    fn from_key_guide_optioanl<'a>(
        key_guides: impl Iterator<Item = Option<KeyGuide<'a>>>,
        available_width: usize,
    ) -> Line<'a> {
        Self::from_key_guide(key_guides.flat_map(|x| x), available_width)
    }

    fn from_key_guide<'a>(
        key_guides: impl Iterator<Item = KeyGuide<'a>>,
        available_width: usize,
    ) -> Line<'a> {
        let lines = key_guides.map(|x| {
            Span::from(format!(" {} ", x.key)).bg(x.color) + Span::raw(" ") + Span::from(x.desc)
        });

        let mut final_line = Line::default();

        'l1: for (idx, line) in lines.enumerate() {
            if idx != 0 {
                let separator = Span::raw("   ");
                match final_line.width() + separator.width() <= available_width {
                    true => final_line.spans.push(separator),
                    false => break 'l1,
                }
            }

            match final_line.width() + line.width() <= available_width {
                true => final_line.spans.extend(line.spans),
                false => break 'l1,
            }
        }

        final_line
    }
}
