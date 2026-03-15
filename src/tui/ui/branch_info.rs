use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::git::BranchScope;
use crate::tui::app::App;
use crate::tui::theme;

use super::helpers::{format_relative_time, format_remote_status};

fn remote_freshness_line(app: &App) -> Option<Line<'static>> {
    if app.remote_freshness_is_checking() {
        return Some(Line::from(vec![
            Span::styled("  Origin: ", theme::styles::MUTED),
            Span::styled("Checking origin...", theme::styles::MUTED),
        ]));
    }

    app.remote_freshness_error().map(|error| {
        Line::from(vec![
            Span::styled("  Origin: ", theme::styles::MUTED),
            Span::styled(error.to_string(), theme::styles::ERROR),
        ])
    })
}

fn local_branch_lines(status: &crate::git::BranchStatus) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let (remote_text, remote_color) = format_remote_status(&status.remote_status);

    if let (Some(author), Some(time)) = (&status.last_commit_author, status.last_commit_time) {
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

    lines
}

fn remote_branch_lines(
    app: &App,
    branch: &crate::tui::app::BranchInfo,
    status: &crate::git::BranchStatus,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let remote_name = branch.remote_name.as_deref().unwrap_or("origin");
    lines.push(Line::from(vec![
        Span::styled("  Source: ", theme::styles::MUTED),
        Span::styled(remote_name.to_string(), theme::styles::TEXT),
    ]));

    if branch.is_stale {
        lines.push(Line::from(vec![
            Span::styled("  Stale: ", theme::styles::MUTED),
            Span::styled(
                "Missing on origin; cached remote-tracking ref may be stale",
                theme::styles::ERROR,
            ),
        ]));
    } else if let Some(line) = remote_freshness_line(app) {
        lines.push(line);
    }

    if let (Some(author), Some(time)) = (&status.last_commit_author, status.last_commit_time) {
        let relative_time = format_relative_time(time);
        lines.push(Line::from(vec![
            Span::styled("  Last commit: ", theme::styles::MUTED),
            Span::styled(author.clone(), theme::styles::TEXT),
            Span::styled(", ", theme::styles::MUTED),
            Span::styled(relative_time, theme::styles::MUTED),
        ]));
    }

    lines
}

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
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(&branch.display_name, theme::branch::CURRENT),
        ]));

        if let Some(status) = app.get_branch_status(&branch.key) {
            match branch.scope {
                BranchScope::Local => {
                    lines.extend(local_branch_lines(status));
                }
                BranchScope::Remote => {
                    lines.extend(remote_branch_lines(app, branch, status));
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{BranchStatus, RemoteStatus};
    use crate::tui::app::{BranchInfo, BranchView};

    fn remote_branch(stale: bool) -> BranchInfo {
        BranchInfo {
            key: "refs/remotes/origin/feature/1".to_string(),
            display_name: "origin/feature/1".to_string(),
            branch_name: "feature/1".to_string(),
            remote_name: Some("origin".to_string()),
            scope: BranchScope::Remote,
            work_item_id: None,
            is_current: false,
            is_protected: false,
            is_stale: stale,
        }
    }

    fn remote_status() -> BranchStatus {
        BranchStatus {
            remote_status: RemoteStatus::RemoteTracking,
            last_commit_author: Some("Alice".to_string()),
            last_commit_time: Some(123),
        }
    }

    fn local_status() -> BranchStatus {
        BranchStatus {
            remote_status: RemoteStatus::UpToDate,
            last_commit_author: Some("Bob".to_string()),
            last_commit_time: Some(456),
        }
    }

    #[test]
    fn test_remote_freshness_line_for_checking_uses_muted_style() {
        let mut app = App::new(vec![], vec![]);
        app.set_remote_freshness_checking();

        let line = remote_freshness_line(&app).expect("checking line");

        assert_eq!(line.spans[0].content.as_ref(), "  Origin: ");
        assert_eq!(line.spans[1].content.as_ref(), "Checking origin...");
        assert_eq!(line.spans[1].style, theme::styles::MUTED);
    }

    #[test]
    fn test_remote_freshness_line_for_error_uses_error_style() {
        let mut app = App::new(vec![], vec![]);
        app.set_remote_freshness_error("Network timeout".to_string());

        let line = remote_freshness_line(&app).expect("error line");

        assert_eq!(line.spans[0].content.as_ref(), "  Origin: ");
        assert_eq!(line.spans[1].content.as_ref(), "Network timeout");
        assert_eq!(line.spans[1].style, theme::styles::ERROR);
    }

    #[test]
    fn test_remote_freshness_line_returns_none_when_checked() {
        let mut app = App::new(vec![], vec![]);
        app.set_remote_freshness(std::collections::HashSet::new());

        assert!(remote_freshness_line(&app).is_none());
    }

    #[test]
    fn test_remote_branch_lines_stale_warning_takes_precedence() {
        let branch = remote_branch(true);
        let status = remote_status();
        let mut app = App::new(vec![branch.clone()], vec![]);
        app.active_view = BranchView::Remote;
        app.set_branch_status(branch.key.clone(), status.clone());
        app.set_remote_freshness_error("Network timeout".to_string());

        let lines = remote_branch_lines(&app, &branch, &status);
        let combined = lines
            .iter()
            .flat_map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref().to_string())
            })
            .collect::<Vec<_>>()
            .join("");

        assert!(combined.contains("Missing on origin; cached remote-tracking ref may be stale"));
        assert!(!combined.contains("Network timeout"));
    }

    #[test]
    fn test_local_branch_lines_include_remote_and_commit_metadata() {
        let status = local_status();

        let lines = local_branch_lines(&status);

        assert_eq!(lines.len(), 1);
        let combined = lines[0]
            .spans
            .iter()
            .map(|span| span.content.as_ref().to_string())
            .collect::<Vec<_>>()
            .join("");

        assert!(combined.contains("Remote: up to date"));
        assert!(combined.contains("Bob"));
    }

    #[test]
    fn test_remote_status_fixture_matches_remote_tracking_behavior() {
        let status = remote_status();

        assert!(matches!(status.remote_status, RemoteStatus::RemoteTracking));
    }
}
