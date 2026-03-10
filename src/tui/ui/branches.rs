use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
};

use crate::tui::app::App;
use crate::tui::theme;

/// Render the branch list panel
pub fn render_branches(frame: &mut Frame, app: &App, area: Rect) {
    let visible = app.visible_branches();
    let items: Vec<ListItem> = visible
        .iter()
        .map(|branch| {
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

            let style = if branch.is_current {
                theme::branch::CURRENT
            } else if branch.is_protected || branch.is_stale {
                theme::styles::MUTED
            } else {
                Style::default()
            };

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
                .title(Line::from(vec![Span::styled(
                    format!(" Branches ({}) ", app.active_view.label()),
                    theme::ui::TITLE,
                )])),
        )
        .highlight_style(theme::ui::SELECTED.add_modifier(Modifier::BOLD))
        .highlight_symbol("\u{25BA} ");

    let mut state = ListState::default();
    state.select(Some(app.selected_index()));

    frame.render_stateful_widget(list, area, &mut state);

    // Render scrollbar inside the borders to match details view
    let inner_area = Block::default().borders(Borders::ALL).inner(area);
    super::helpers::render_scrollbar(frame, inner_area, visible.len(), state.offset());
}
