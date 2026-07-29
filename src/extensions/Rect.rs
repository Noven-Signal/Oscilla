use ratatui::layout::{Constraint, Direction, Layout, Rect};



#[derive(Default)]
pub struct Margin {
    pub left: u16,
    pub right: u16,
    pub top: u16,
    pub bottom: u16,
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

pub trait RectExtension {
    fn margin(&self, margin: Option<Margin>) -> Self;
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
