use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::tui::app::App;
use crate::tui::theme;

/// Render the footer bar with status messages or key hints
pub fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    // Check for active status message
    if let Some(msg) = app.get_status_message() {
        let style = if msg.is_error {
            theme::styles::ERROR
        } else {
            theme::styles::SUCCESS.add_modifier(Modifier::BOLD)
        };
        let paragraph = Paragraph::new(Line::from(vec![Span::styled(&msg.text, style)]));
        frame.render_widget(paragraph, area);
        return;
    }

    let refresh_available = app.current_branch_has_work_item();
    let refresh_style = if refresh_available {
        theme::styles::ACCENT
    } else {
        theme::styles::MUTED
    };
    let refresh_text_style = if refresh_available {
        theme::styles::MUTED
    } else {
        theme::styles::MUTED.add_modifier(Modifier::DIM)
    };

    let protected_prefix = if app.show_protected { "hide " } else { "show " };
    let spans = vec![
        Span::styled(" j/k ", theme::styles::ACCENT),
        Span::styled("navigate  ", theme::styles::MUTED),
        Span::styled("o", theme::styles::ACCENT),
        Span::styled("pen  ", theme::styles::MUTED),
        Span::styled("pg\u{2191}\u{2193} ", theme::styles::ACCENT),
        Span::styled("scroll  ", theme::styles::MUTED),
        Span::styled("d", theme::styles::ACCENT),
        Span::styled("elete  ", theme::styles::MUTED),
        Span::styled("r", refresh_style),
        Span::styled("efresh  ", refresh_text_style),
        Span::styled(protected_prefix, theme::styles::MUTED),
        Span::styled("p", theme::styles::ACCENT),
        Span::styled("rotected  ", theme::styles::MUTED),
        Span::styled("q", theme::styles::ACCENT),
        Span::styled("uit", theme::styles::MUTED),
    ];

    let help_text = Line::from(spans);
    let paragraph = Paragraph::new(help_text).style(theme::styles::MUTED);
    frame.render_widget(paragraph, area);
}
