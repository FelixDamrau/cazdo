use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::tui::app::{App, BranchView, StatusMessage};
use crate::tui::theme;

enum FooterVariant<'a> {
    FilterInput,
    Status(&'a StatusMessage),
    Normal,
}

/// Render the footer bar with status messages or key hints
pub fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    match footer_variant(app) {
        FooterVariant::FilterInput => render_filter_footer(frame, area),
        FooterVariant::Status(msg) => render_status_footer(frame, area, msg),
        FooterVariant::Normal => render_normal_footer(frame, app, area),
    }
}

fn footer_variant(app: &App) -> FooterVariant<'_> {
    if app.is_filter_input_mode() {
        FooterVariant::FilterInput
    } else if let Some(msg) = app.get_status_message() {
        FooterVariant::Status(msg)
    } else {
        FooterVariant::Normal
    }
}

fn render_filter_footer(frame: &mut Frame, area: Rect) {
    let help_text = Line::from(vec![
        key_span(" type "),
        label_span("filter  "),
        key_span("backspace"),
        label_span(" delete  "),
        key_span("ctrl+u"),
        label_span(" clear  "),
        key_span("enter"),
        label_span(" apply  "),
        key_span("esc"),
        label_span(" cancel"),
    ]);

    render_footer_line(frame, area, help_text, theme::styles::MUTED);
}

fn render_status_footer(frame: &mut Frame, area: Rect, msg: &StatusMessage) {
    let style = if msg.is_error {
        theme::styles::ERROR
    } else {
        theme::styles::SUCCESS.add_modifier(Modifier::BOLD)
    };

    let paragraph = Paragraph::new(Line::from(vec![Span::styled(&msg.text, style)]));
    frame.render_widget(paragraph, area);
}

fn render_normal_footer(frame: &mut Frame, app: &App, area: Rect) {
    let spans = normal_footer_spans(app);

    render_footer_line(frame, area, Line::from(spans), theme::styles::MUTED);
}

fn normal_footer_spans(app: &App) -> Vec<Span<'static>> {
    let toggle_label = match app.active_view() {
        BranchView::Local => "remote",
        BranchView::Remote => "local",
    };

    let mut spans = Vec::new();
    spans.push(label_span(" "));
    push_hint(&mut spans, "j/k", "navigate");
    push_hint(&mut spans, "/", "filter");
    push_hint(&mut spans, "t", format!("toggle {}", toggle_label));
    push_hint(&mut spans, "o", "open");
    push_hint(&mut spans, "pg↑↓", "scroll");
    push_hint(&mut spans, "d", "delete");
    if app.current_branch_has_work_item() {
        push_hint(&mut spans, "r", "refresh");
    }
    push_hint(&mut spans, "p", "protected");
    spans.extend(normal_footer_tail(app.has_active_filter()));

    spans
}

fn normal_footer_tail(has_active_filter: bool) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    if has_active_filter {
        push_hint(&mut spans, "esc", "clear filter");
        push_hint(&mut spans, "q", "quit");
    } else {
        push_hint(&mut spans, "q/esc", "quit");
    }
    spans
}

fn push_hint(spans: &mut Vec<Span<'static>>, key: &'static str, label: impl Into<String>) {
    spans.push(key_span(key));
    spans.push(label_span(" "));
    spans.push(label_span(label));
    spans.push(label_span("  "));
}

fn key_span(key: &'static str) -> Span<'static> {
    Span::styled(key, theme::styles::ACCENT)
}

fn label_span(label: impl Into<String>) -> Span<'static> {
    Span::styled(label.into(), theme::styles::MUTED)
}

fn render_footer_line(frame: &mut Frame, area: Rect, line: Line<'static>, style: Style) {
    let paragraph = Paragraph::new(line).style(style);
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::git::BranchScope;
    use crate::tui::app::{BranchInfo, Msg};

    #[test]
    fn test_footer_variant_prioritizes_filter_input_over_status() {
        let mut app = test_app();
        app.update(Msg::StartFilter);
        app.set_status_message("Saved".to_string(), false, 5);

        assert!(matches!(footer_variant(&app), FooterVariant::FilterInput));
    }

    #[test]
    fn test_footer_variant_prioritizes_status_over_normal_hints() {
        let mut app = test_app();
        app.set_status_message("Deleted branch".to_string(), false, 5);

        match footer_variant(&app) {
            FooterVariant::Status(status) => assert_eq!(status.text, "Deleted branch"),
            _ => panic!("expected status footer"),
        }
    }

    #[test]
    fn test_normal_footer_tail_with_active_filter() {
        assert_eq!(
            spans_text(&normal_footer_tail(true)),
            "esc clear filter  q quit  "
        );
    }

    #[test]
    fn test_normal_footer_tail_without_active_filter() {
        assert_eq!(spans_text(&normal_footer_tail(false)), "q/esc quit  ");
    }

    #[test]
    fn test_normal_footer_omits_refresh_when_unavailable() {
        let app = test_app();

        assert!(!spans_text(&normal_footer_spans(&app)).contains("refresh"));
    }

    #[test]
    fn test_normal_footer_includes_refresh_when_available() {
        let mut app = test_app();
        app.branches[0].work_item_id = Some(42);

        assert!(spans_text(&normal_footer_spans(&app)).contains("refresh"));
    }

    #[test]
    fn test_normal_footer_uses_explicit_action_labels() {
        let mut app = test_app();
        app.branches[0].work_item_id = Some(42);

        assert_eq!(
            spans_text(&normal_footer_spans(&app)),
            " j/k navigate  / filter  t toggle remote  o open  pg↑↓ scroll  d delete  r refresh  p protected  q/esc quit  "
        );
    }

    fn spans_text(spans: &[Span<'static>]) -> String {
        spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    fn test_app() -> App {
        App::new(
            vec![BranchInfo {
                key: "refs/heads/feature/1".to_string(),
                display_name: "feature/1".to_string(),
                branch_name: "feature/1".to_string(),
                remote_name: None,
                scope: BranchScope::Local,
                work_item_id: None,
                is_current: false,
                is_protected: false,
                is_stale: false,
            }],
            vec![],
        )
    }
}
