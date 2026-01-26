//! UI rendering module - orchestrates all UI components

mod branch_info;
mod branches;
mod details;
mod footer;
mod helpers;
mod popup;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};

use super::app::{App, AppMode};
use super::theme;

/// Main render function - orchestrates all UI components
pub fn render(frame: &mut Frame, app: &mut App) {
    // Split into main area and footer
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(frame.area());

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
    details::render_details(frame, app, right_chunks[0]);
    branch_info::render_branch_info(frame, app, right_chunks[1]);
    footer::render_footer(frame, app, main_chunks[1]);

    // Render popup if needed
    if let AppMode::ConfirmDelete(ref branch_name) = app.mode {
        let (popup_width, popup_height) = theme::layout::POPUP_SIZE;
        let area = popup::centered_rect(frame.area(), popup_width, popup_height);
        popup::render_delete_popup(frame, branch_name, area);
    }
}
