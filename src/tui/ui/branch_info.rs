use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::tui::app::App;
use crate::tui::theme;

use super::helpers::{format_relative_time, format_remote_status};

/// Render the branch info panel (bottom-right)
pub fn render_branch_info(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::ui::BORDER)
        .title(Line::from(vec![Span::styled(
            " Branch Info ",
            theme::ui::TITLE,
        )]));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    if let Some(branch) = app.selected_branch() {
        // Branch name
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(&branch.name, theme::branch::CURRENT),
        ]));

        // Remote status and last commit
        if let Some(status) = app.get_branch_status(&branch.name) {
            let (remote_text, remote_color) = format_remote_status(&status.remote_status);

            // Last commit info on same line as remote if available
            if let (Some(author), Some(time)) =
                (&status.last_commit_author, status.last_commit_time)
            {
                let relative_time = format_relative_time(time);
                lines.push(Line::from(vec![
                    Span::styled("  Remote: ", theme::styles::MUTED),
                    Span::styled(remote_text, Style::default().fg(remote_color)),
                    Span::styled("  │  ", theme::styles::MUTED),
                    Span::styled(author.clone(), theme::styles::TEXT),
                    Span::styled(", ", theme::styles::MUTED),
                    Span::styled(relative_time, theme::styles::MUTED),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::styled("  Remote: ", theme::styles::MUTED),
                    Span::styled(remote_text, Style::default().fg(remote_color)),
                ]));
            }
        } else {
            lines.push(Line::from(vec![Span::styled(
                "  Loading...",
                theme::styles::MUTED,
            )]));
        }
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}
