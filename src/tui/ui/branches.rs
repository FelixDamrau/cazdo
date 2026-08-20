use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use crate::tui::app::{App, BranchInfo};
use crate::tui::theme;

fn branch_row_style(is_current: bool, is_protected: bool, is_stale: bool, selected: bool) -> Style {
    match (is_current, is_protected || is_stale, selected) {
        (true, _, _) => theme::branch::CURRENT,
        (_, true, true) => theme::ui::SELECTED_LABEL,
        (_, true, false) => theme::styles::MUTED,
        _ => Style::default(),
    }
}

/// Render the branch list panel
pub fn render_branches(frame: &mut Frame, app: &App, area: Rect) {
    let visible = app.visible_branches();
    let title = panel_title(app);

    if visible.is_empty() {
        render_empty_state(frame, app, area, title);
        return;
    }

    let selected_index = app.selected_index();
    let items = visible
        .iter()
        .enumerate()
        .map(|(index, branch)| branch_item(branch, index == selected_index));

    let list = List::new(items)
        .block(panel_block(title))
        .highlight_style(theme::ui::SELECTED_BACKGROUND.add_modifier(Modifier::BOLD))
        .highlight_symbol("\u{25BA} ");

    let mut state = ListState::default();
    state.select(Some(selected_index));

    frame.render_stateful_widget(list, area, &mut state);

    // Inside the borders, so the scrollbar lines up with the details view.
    let inner_area = Block::default().borders(Borders::ALL).inner(area);

    super::helpers::render_scrollbar(frame, inner_area, visible.len(), state.offset());
}

/// `" Branches (Local) / query "`, with the filter shown only when set.
fn panel_title(app: &App) -> String {
    let view = app.active_view().label();
    let filter = app.effective_branch_filter().trim();
    if filter.is_empty() {
        format!(" Branches ({view}) ")
    } else {
        format!(" Branches ({view}) / {filter} ")
    }
}

fn panel_block(title: String) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(theme::ui::BORDER)
        .title(Line::from(vec![Span::styled(title, theme::ui::TITLE)]))
}

/// One row: a current-branch marker, the name, then the protected, work item,
/// and stale markers, each shown only when it applies.
fn branch_item(branch: &BranchInfo, selected: bool) -> ListItem<'static> {
    let prefix = if branch.is_current { "* " } else { "  " };
    let protected = if branch.is_protected {
        " \u{1F512}"
    } else {
        ""
    };
    let work_item = match branch.work_item_id {
        Some(id) => format!(" [#{id}]"),
        None => String::new(),
    };
    let stale = if branch.is_stale { " \u{26A0}" } else { "" };

    ListItem::new(format!(
        "{prefix}{}{protected}{work_item}{stale}",
        branch.display_name
    ))
    .style(branch_row_style(
        branch.is_current,
        branch.is_protected,
        branch.is_stale,
        selected,
    ))
}

/// Why the list is empty matters to the user: a filter that matches nothing, a
/// hidden-protected view, or a genuinely empty repository each need a different
/// next step.
fn render_empty_state(frame: &mut Frame, app: &App, area: Rect, title: String) {
    let block = panel_block(title);
    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let view = app.active_view().label().to_lowercase();
    let filter = app.effective_branch_filter().trim();
    let message = if !filter.is_empty() {
        format!("No {view} branches match \"{filter}\".")
    } else if app.has_hidden_branches_in_active_view() {
        format!("No {view} branches shown. Press p to show protected branches.")
    } else {
        format!("No {view} branches found.")
    };

    let empty_msg = Paragraph::new(message)
        .style(theme::styles::MUTED)
        .alignment(Alignment::Center);

    frame.render_widget(empty_msg, super::helpers::centered_line_area(inner_area));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_current_branch_keeps_current_style() {
        assert_eq!(
            branch_row_style(true, false, false, true),
            theme::branch::CURRENT
        );
    }

    #[test]
    fn selected_protected_and_stale_branches_use_readable_style() {
        assert_eq!(
            branch_row_style(false, true, false, true),
            theme::ui::SELECTED_LABEL
        );
        assert_eq!(
            branch_row_style(false, false, true, true),
            theme::ui::SELECTED_LABEL
        );
    }

    #[test]
    fn unselected_protected_and_stale_branches_remain_muted() {
        assert_eq!(
            branch_row_style(false, true, false, false),
            theme::styles::MUTED
        );
        assert_eq!(
            branch_row_style(false, false, true, false),
            theme::styles::MUTED
        );
    }

    #[test]
    fn selected_current_branch_preserves_foreground_in_rendered_list() {
        let list = List::new(vec![ListItem::new("* main").style(theme::branch::CURRENT)])
            .highlight_style(theme::ui::SELECTED_BACKGROUND.add_modifier(Modifier::BOLD))
            .highlight_symbol("\u{25BA} ");
        let mut state = ListState::default();
        state.select(Some(0));
        let area = Rect::new(0, 0, 20, 1);
        let mut buffer = ratatui::buffer::Buffer::empty(area);

        ratatui::widgets::StatefulWidget::render(list, area, &mut buffer, &mut state);

        assert_eq!(
            buffer.cell((2, 0)).expect("current branch cell").fg,
            ratatui::style::Color::Green
        );
    }
}
