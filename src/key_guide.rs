use ratatui::{
    style::{Color, Stylize},
    text::{Line, Span},
};

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
            color: Color::Rgb(170, 142, 176),
        }
    }

    pub const ESC_DEFAULT: Self = KeyGuide {
        key: "Esc",
        desc: "Cancel selection",
        color: Color::Magenta,
    };

    pub const GLOBAL_GUIDES: [KeyGuide<'static>; 2] = [
        KeyGuide::new_gray("Ctrl+O", "Open file selection dialog"),
        KeyGuide::new_gray("Ctrl+D", "Quit Oscilla"),
    ];
}

pub trait LineExt {
    fn from_key_guide<'a>(
        key_guides: impl Iterator<Item = KeyGuide<'a>>,
        available_width: usize,
    ) -> Line<'a>;
}
impl LineExt for Line<'_> {
    fn from_key_guide<'a>(
        key_guides: impl Iterator<Item = KeyGuide<'a>>,
        available_width: usize,
    ) -> Line<'a> {
        let lines = key_guides.map(|x| {
            Span::from(format!(" {} ", x.key)).bg(x.color)
                + Span::raw(" ")
                + Span::from(x.desc)
        });

        let mut final_line = Line::default();

        'l1: for (idx, line) in lines.enumerate() {
            let separator = Span::raw("   ");
            if idx != 0 {
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

        // let spans_separated_by_space = lines.enumerate().flat_map(|(index, line)| {
        //     let separator = (index > 0).then(|| Span::raw("   "));
        //     separator.into_iter().chain(line.spans)
        // });

        // Line::from_iter(spans_separated_by_space)
        final_line
    }
}
