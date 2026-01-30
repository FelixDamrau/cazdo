use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap},
};

use crate::tui::theme;

/// Render the delete confirmation popup
pub fn render_delete_popup(frame: &mut Frame, branch_name: &str) {
    let content = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("Are you sure you want to delete branch "),
            Span::styled(branch_name, theme::branch::CURRENT),
            Span::raw("?"),
        ]),
        Line::from(""),
        make_key_hint(&["y"], "conform"),
        make_key_hint(&["n", "Esc"], "cancel"),
    ];

    let area = centered_rect(frame.area());
    render_popup_impl(frame, " Delete Branch ", content, area);
}

/// Render an error popup with the given message
pub fn render_error_popup(frame: &mut Frame, message: &str) {
    let content = vec![
        Line::from(""),
        Line::from(Span::styled(message, theme::styles::ERROR)),
        Line::from(""),
        make_key_hint(&["Enter", "Esc"], "Dismiss"),
    ];

    let area = centered_rect(frame.area());
    render_popup_impl(frame, " Error ", content, area);
}

fn make_key_hint<'a>(keys: &[&'a str], action: &str) -> Line<'a> {
    let mut spans = vec![Span::raw("Press ")];
    for (i, &key) in keys.iter().enumerate() {
        spans.push(Span::styled(key, theme::ui::TITLE));
        if i < keys.len() - 1 {
            spans.push(Span::raw(" or "));
        }
    }
    spans.push(Span::raw(format!(" to {}.", action)));
    Line::from(spans)
}

fn render_popup_impl(frame: &mut Frame, title: &str, content: Vec<Line>, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::ui::BORDER_ERROR)
        .padding(Padding::horizontal(1))
        .title(Line::from(vec![Span::styled(
            title,
            theme::ui::TITLE_ERROR,
        )]));

    let paragraph = Paragraph::new(content)
        .block(block)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    frame.render_widget(Clear, area);
    frame.render_widget(paragraph, area);
}

/// Get the popup rect
fn centered_rect(r: Rect) -> Rect {
    let (popup_width, popup_height) = theme::layout::POPUP_SIZE;
    let width = popup_width.min(r.width - 2);
    let height = popup_height.min(r.height - 3);

    let x = r.width.saturating_sub(width) / 2;
    let y = r.height.saturating_sub(height) / 2;

    Rect::new(r.x + x, r.y + y, width, height)
}
