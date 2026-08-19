use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use crate::tui::app::App;
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
    let filter = app.effective_branch_filter().trim();
    let title = if filter.is_empty() {
        format!(" Branches ({}) ", app.active_view().label())
    } else {
        format!(" Branches ({}) / {} ", app.active_view().label(), filter)
    };

    if visible.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme::ui::BORDER)
            .title(Line::from(vec![Span::styled(
                title.clone(),
                theme::ui::TITLE,
            )]));

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        let empty_text = if !filter.is_empty() {
            format!(
                "No {} branches match \"{}\".",
                app.active_view().label().to_lowercase(),
                filter
            )
        } else if app.has_hidden_branches_in_active_view() {
            format!(
                "No {} branches shown. Press p to show protected branches.",
                app.active_view().label().to_lowercase()
            )
        } else {
            format!(
                "No {} branches found.",
                app.active_view().label().to_lowercase()
            )
        };

        let empty_msg = Paragraph::new(empty_text)
            .style(theme::styles::MUTED)
            .alignment(Alignment::Center);

        let centered_area = super::helpers::centered_line_area(inner_area);

        frame.render_widget(empty_msg, centered_area);
        return;
    }

    let selected_index = app.selected_index();
    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .map(|(index, branch)| {
            let selected = index == selected_index;
            let prefix = if branch.is_current { "* " } else { "  " };

            // Show lock for protected branches (when visible)
            let protected_indicator = if branch.is_protected {
                " \u{1F512}"
            } else {
                ""
            };

            let wi_suffix = match branch.work_item_id {
                Some(id) => format!(" [#{}]", id),
                None => String::new(),
            };

            let stale_indicator = if branch.is_stale { " ⚠" } else { "" };

            let style = branch_row_style(
                branch.is_current,
                branch.is_protected,
                branch.is_stale,
                selected,
            );

            ListItem::new(format!(
                "{}{}{}{}{}",
                prefix, branch.display_name, protected_indicator, wi_suffix, stale_indicator
            ))
            .style(style)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme::ui::BORDER)
                .title(Line::from(vec![Span::styled(title, theme::ui::TITLE)])),
        )
        .highlight_style(theme::ui::SELECTED_BACKGROUND.add_modifier(Modifier::BOLD))
        .highlight_symbol("\u{25BA} ");

    let mut state = ListState::default();
    state.select(Some(app.selected_index()));

    frame.render_stateful_widget(list, area, &mut state);

    // Render scrollbar inside the borders to match details view
    let inner_area = Block::default().borders(Borders::ALL).inner(area);

    super::helpers::render_scrollbar(frame, inner_area, visible.len(), state.offset());
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
}
