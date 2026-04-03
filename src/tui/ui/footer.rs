use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::tui::app::{App, BranchView};
use crate::tui::theme;

/// Render the footer bar with status messages or key hints
pub fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    if app.is_filter_input_mode() {
        let help_text = Line::from(vec![
            Span::styled(" type ", theme::styles::ACCENT),
            Span::styled("filter  ", theme::styles::MUTED),
            Span::styled("backspace", theme::styles::ACCENT),
            Span::styled(" delete  ", theme::styles::MUTED),
            Span::styled("ctrl+u", theme::styles::ACCENT),
            Span::styled(" clear  ", theme::styles::MUTED),
            Span::styled("enter", theme::styles::ACCENT),
            Span::styled(" apply  ", theme::styles::MUTED),
            Span::styled("esc", theme::styles::ACCENT),
            Span::styled(" cancel", theme::styles::MUTED),
        ]);
        let paragraph = Paragraph::new(help_text).style(theme::styles::MUTED);
        frame.render_widget(paragraph, area);
        return;
    }

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
    let toggle_label = match app.active_view {
        BranchView::Local => "remote",
        BranchView::Remote => "local",
    };
    let mut spans = vec![
        Span::styled(" j/k ", theme::styles::ACCENT),
        Span::styled("navigate  ", theme::styles::MUTED),
        Span::styled("/", theme::styles::ACCENT),
        Span::styled(" filter  ", theme::styles::MUTED),
        Span::styled("t", theme::styles::ACCENT),
        Span::styled(format!("oggle {}  ", toggle_label), theme::styles::MUTED),
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
    ];

    if app.has_active_filter() {
        spans.push(Span::styled("esc", theme::styles::ACCENT));
        spans.push(Span::styled(" clear filter  ", theme::styles::MUTED));
        spans.push(Span::styled("q", theme::styles::ACCENT));
        spans.push(Span::styled(" quit", theme::styles::MUTED));
    } else {
        spans.push(Span::styled("q/esc", theme::styles::ACCENT));
        spans.push(Span::styled(" quit", theme::styles::MUTED));
    }

    let help_text = Line::from(spans);
    let paragraph = Paragraph::new(help_text).style(theme::styles::MUTED);
    frame.render_widget(paragraph, area);
}
