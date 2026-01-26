use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::tui::theme;

/// Calculate a centered rectangle within the given area
pub fn centered_rect(r: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Render the delete confirmation popup
pub fn render_delete_popup(frame: &mut Frame, branch_name: &str, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::ui::BORDER_ERROR)
        .title(Line::from(vec![Span::styled(
            " Delete Branch ",
            theme::ui::TITLE_ERROR,
        )]));

    let content = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("Are you sure you want to delete branch "),
            Span::styled(branch_name, theme::branch::CURRENT),
            Span::raw("?"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("Press "),
            Span::styled("y", theme::styles::ERROR),
            Span::raw(" to confirm"),
        ]),
        Line::from(vec![
            Span::raw("Press "),
            Span::styled("n", theme::ui::TITLE),
            Span::raw(" or "),
            Span::styled("Esc", theme::ui::TITLE),
            Span::raw(" to cancel"),
        ]),
    ];

    let paragraph = Paragraph::new(content)
        .block(block)
        .alignment(ratatui::layout::Alignment::Center);

    frame.render_widget(Clear, area); // Clear background
    frame.render_widget(paragraph, area);
}
