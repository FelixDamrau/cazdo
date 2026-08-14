//! UI rendering module - orchestrates all UI components

mod branch_info;
mod branches;
mod details;
mod footer;
mod helpers;
mod popup;
mod worktrees;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};

use super::app::{App, AppMode, DetailsMetrics};
use super::theme;

/// Main render function - orchestrates all UI components
pub fn render(frame: &mut Frame, app: &App) -> DetailsMetrics {
    // Split into main area and footer
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(frame.area());

    if app.is_worktree_view() {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(theme::layout::BRANCHES_WIDTH_PERCENT),
                Constraint::Percentage(100 - theme::layout::BRANCHES_WIDTH_PERCENT),
            ])
            .split(main_chunks[0]);
        worktrees::render_worktrees(frame, app, chunks[0]);
        worktrees::render_worktree_details(frame, app, chunks[1]);
        footer::render_footer(frame, app, main_chunks[1]);

        if let AppMode::WorktreeDiagnostics { .. } = app.mode() {
            if let Some(entry) = app.worktree_diagnostics() {
                popup::render_worktree_diagnostics(frame, entry);
            }
        } else if let AppMode::ErrorPopup(message) = app.mode() {
            popup::render_error_popup(frame, message);
        }
        return DetailsMetrics::default();
    }

    // Split main area into left (branches) and right panels
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(theme::layout::BRANCHES_WIDTH_PERCENT),
            Constraint::Percentage(100 - theme::layout::BRANCHES_WIDTH_PERCENT),
        ])
        .split(main_chunks[0]);

    // Split right panel into work item details (top, scrollable) and branch info (bottom, fixed)
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(theme::layout::BRANCH_INFO_HEIGHT),
        ])
        .split(chunks[1]);

    branches::render_branches(frame, app, chunks[0]);
    let metrics = details::render_details(frame, app, right_chunks[0]);
    branch_info::render_branch_info(frame, app, right_chunks[1]);
    footer::render_footer(frame, app, main_chunks[1]);

    // Render popup if needed
    if let Some(branch) = app.confirm_delete_branch() {
        popup::render_confirm_popup(
            frame,
            &branch.display_name,
            branch.scope.is_remote(),
            app.confirm_delete_is_prune(),
        );
    } else if let AppMode::ErrorPopup(message) = app.mode() {
        popup::render_error_popup(frame, message);
    }

    metrics
}
