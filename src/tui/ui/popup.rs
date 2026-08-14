use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap},
};

use super::worktrees::worktree_diagnostic_lines;
use crate::git::WorktreeInfo;
use crate::tui::theme;

/// Render the delete or prune confirmation popup
pub fn render_confirm_popup(frame: &mut Frame, branch_name: &str, is_remote: bool, is_prune: bool) {
    let content = if is_prune {
        vec![
            Line::from(""),
            Line::from(vec![
                Span::raw("Prune stale tracking ref for "),
                Span::styled(branch_name, theme::branch::CURRENT),
                Span::raw("?"),
            ]),
            Line::from(Span::raw("(branch no longer exists on origin)")),
            Line::from(""),
            make_key_hint(&["y"], "confirm"),
            make_key_hint(&["n", "Esc"], "cancel"),
        ]
    } else {
        let branch_kind = if is_remote { "remote branch" } else { "branch" };
        vec![
            Line::from(""),
            Line::from(vec![
                Span::raw(format!("Are you sure you want to delete {} ", branch_kind)),
                Span::styled(branch_name, theme::branch::CURRENT),
                Span::raw("?"),
            ]),
            Line::from(""),
            make_key_hint(&["y"], "confirm"),
            make_key_hint(&["n", "Esc"], "cancel"),
        ]
    };

    let title = if is_prune {
        " Prune Stale Branch "
    } else {
        " Delete Branch "
    };

    let area = centered_rect(frame.area());
    render_popup_impl(frame, title, content, area);
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

/// Render diagnostics for a selected worktree. Enter never checks out or
/// removes a worktree; this popup is intentionally read-only.
pub fn render_worktree_diagnostics(frame: &mut Frame, entry: &WorktreeInfo) {
    let mut content = vec![Line::from("")];
    content.extend(worktree_diagnostic_lines(entry));
    content.push(Line::from(""));
    content.push(make_key_hint(&["Enter", "Esc"], "dismiss"));
    let area = centered_rect_for_content(frame.area(), &content);
    render_popup_impl(frame, " Worktree Diagnostics ", content, area);
}

/// Get the popup rect
fn centered_rect(r: Rect) -> Rect {
    let (popup_width, popup_height) = theme::layout::POPUP_SIZE;
    let max_width = r.width.saturating_sub(2); // -2: keep main left/right border
    let max_height = r.height.saturating_sub(3); // -3: keep top/bottom border and help line
    let width = popup_width.min(max_width);
    let height = popup_height.min(max_height);

    let x = r.width.saturating_sub(width) / 2;
    let y = r.height.saturating_sub(height) / 2;

    Rect::new(r.x + x, r.y + y, width, height)
}

fn centered_rect_for_content(r: Rect, content: &[Line<'_>]) -> Rect {
    let max_width = r.width.saturating_sub(2);
    let max_height = r.height.saturating_sub(3);
    let desired_width = content
        .iter()
        .map(Line::width)
        .max()
        .unwrap_or_default()
        .saturating_add(4)
        .max(theme::layout::POPUP_SIZE.0 as usize);
    let width = desired_width.min(max_width as usize) as u16;
    let inner_width = width.saturating_sub(4).max(1) as usize;
    let content_height = content
        .iter()
        .map(|line| wrapped_line_count(line, inner_width))
        .sum::<usize>()
        .saturating_add(2)
        .max(5);
    let height = content_height.min(max_height as usize) as u16;

    centered_rect_with_dimensions(r, width, height)
}

fn centered_rect_with_dimensions(r: Rect, width: u16, height: u16) -> Rect {
    let x = r.width.saturating_sub(width) / 2;
    let y = r.height.saturating_sub(height) / 2;
    Rect::new(r.x + x, r.y + y, width, height)
}

fn wrapped_line_count(line: &Line<'_>, max_width: usize) -> usize {
    let mut rows = 1;
    let mut current_width = 0;

    for word in line
        .spans
        .iter()
        .flat_map(|span| span.content.split_whitespace())
    {
        let word_width = Span::raw(word).width();
        if word_width > max_width {
            if current_width > 0 {
                rows += 1;
            }
            rows += word_width.saturating_sub(1) / max_width;
            current_width = word_width % max_width;
            if current_width == 0 {
                current_width = max_width;
            }
        } else if current_width == 0 {
            current_width = word_width;
        } else if current_width + 1 + word_width <= max_width {
            current_width += 1 + word_width;
        } else {
            rows += 1;
            current_width = word_width;
        }
    }

    rows
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{
        WorktreeCleanliness, WorktreeDirtyReason, WorktreeIdentity, WorktreeState,
        WorktreeSubmodules,
    };
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn worktree_diagnostics_renders_complete_error_context() {
        let entry = WorktreeInfo {
            identity: WorktreeIdentity::Linked {
                name: "feature with spaces".to_string(),
            },
            path: "/tmp/worktree with spaces/雪".into(),
            branch: None,
            detached_short_sha: Some("1234567".to_string()),
            is_main: false,
            is_current: true,
            cleanliness: WorktreeCleanliness::Dirty(vec![
                WorktreeDirtyReason::Untracked,
                WorktreeDirtyReason::Conflict,
            ]),
            lock_reason: Some("owned by editor".to_string()),
            state: WorktreeState::Invalid("common dir unreadable".to_string()),
            prunable: true,
            submodules: WorktreeSubmodules::Present,
        };
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");

        terminal
            .draw(|frame| render_worktree_diagnostics(frame, &entry))
            .expect("draw");
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();

        for expected in [
            "feature with spaces",
            "/tmp/worktree with spaces/雪",
            "detached 1234567",
            "Current: yes",
            "invalid: common dir unreadable",
            "dirty: untracked, conflict",
            "Lock: owned by editor",
            "Prunable: yes",
            "Submodules: present",
            "dismiss",
        ] {
            assert!(text.contains(expected), "missing {expected:?}");
        }
    }

    #[test]
    fn test_centered_rect_stays_within_tiny_area() {
        let area = Rect::new(0, 0, 1, 1);
        let popup = centered_rect(area);

        assert!(popup.width <= area.width);
        assert!(popup.height <= area.height);
        assert!(popup.x >= area.x);
        assert!(popup.y >= area.y);
    }

    #[test]
    fn test_centered_rect_respects_padding_limits() {
        let area = Rect::new(0, 0, 10, 8);
        let popup = centered_rect(area);

        assert_eq!(popup.width, 8);
        assert_eq!(popup.height, 5);
    }
}
