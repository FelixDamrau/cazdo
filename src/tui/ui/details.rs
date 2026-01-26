use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

use crate::tui::app::{App, WorkItemStatus};
use crate::tui::html_render::render_html;
use crate::tui::theme;

use super::helpers::append_wrapped_text;

/// Render the work item details panel
pub fn render_details(frame: &mut Frame, app: &mut App, area: Rect) {
    let work_item_id = app.selected_work_item_id();

    // Calculate inner area first to determine visible height
    let inner = Block::default().borders(Borders::ALL).inner(area);
    let visible_height = inner.height;

    // Build scroll info for bottom border (only if scrollable)
    let scroll_title = if app.content_height > visible_height {
        Line::from(vec![
            Span::styled(
                format!(
                    " {}/{} ",
                    app.scroll_offset + 1,
                    app.content_height.saturating_sub(visible_height) + 1
                ),
                theme::styles::MUTED,
            ),
            Span::styled("─", theme::styles::ACCENT),
        ])
    } else {
        Line::default()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::ui::BORDER)
        .title(Line::from(vec![Span::styled(
            " Work Item Details ",
            theme::ui::TITLE,
        )]))
        .title_bottom(scroll_title.right_aligned());

    frame.render_widget(block, area);

    // Clear the inner area before rendering new content
    frame.render_widget(Clear, inner);

    match work_item_id {
        Some(wi_id) => {
            render_work_item_details(frame, app, inner, wi_id);
        }
        None => {
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  No work item linked to this branch",
                    theme::styles::MUTED.add_modifier(Modifier::ITALIC),
                )),
            ];

            app.set_content_height(lines.len() as u16);
            let text = Paragraph::new(lines);
            frame.render_widget(text, inner);
        }
    }
}

/// Render the work item details content
fn render_work_item_details(frame: &mut Frame, app: &mut App, area: Rect, wi_id: u32) {
    let status = app.get_work_item_status(wi_id);
    let max_width = area.width.saturating_sub(4) as usize;

    let content: Vec<Line> = match status {
        WorkItemStatus::NotFetched | WorkItemStatus::Loading => {
            vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  Loading work item...",
                    theme::styles::WARNING,
                )),
            ]
        }
        WorkItemStatus::Error(err) => {
            let mut lines = vec![Line::from("")];
            append_wrapped_text(
                &mut lines,
                &format!("Error: {}", err),
                max_width,
                Style::default().fg(Color::Red),
            );
            lines
        }
        WorkItemStatus::Loaded(wi) => {
            let type_icon = wi.work_item_type.icon();
            let type_name = wi.work_item_type.display_name();
            let state_icon = wi.state.icon();
            let state_name = wi.state.display_name();
            let state_color = wi.state.color();

            // ID and Type
            let mut lines = vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(
                        format!("#{} ", wi.id),
                        theme::styles::ACCENT.add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(format!("{} {}", type_icon, type_name)),
                ]),
            ];

            // Metadata line: State • Assigned To • Tags (directly under ID/Type)
            let mut meta_spans = vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    format!("{} {}", state_icon, state_name),
                    Style::default().fg(state_color),
                ),
            ];

            // Add assigned to if present
            if let Some(ref assigned) = wi.assigned_to {
                meta_spans.push(Span::styled("  •  ", theme::styles::MUTED));
                meta_spans.push(Span::styled(assigned.clone(), theme::styles::TEXT));
            }

            // Add tags if present
            if !wi.tags.is_empty() {
                meta_spans.push(Span::styled("  •  ", theme::styles::MUTED));
                meta_spans.push(Span::styled(
                    wi.tags.join(", "),
                    Style::default().fg(Color::Magenta),
                ));
            }

            lines.push(Line::from(meta_spans));

            // Blank line before title
            lines.push(Line::from(""));

            append_wrapped_text(
                &mut lines,
                &wi.title,
                max_width,
                theme::styles::TEXT
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::UNDERLINED),
            );

            // All rich text fields (Description, Acceptance Criteria, etc.)
            for field in &wi.rich_text_fields {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    format!("  {}:", field.name),
                    theme::styles::MUTED,
                )]));

                // Render HTML with formatting preserved
                let rendered = render_html(&field.value, max_width.saturating_sub(4));
                for rendered_line in rendered {
                    // Add indent to each line
                    let mut indented_spans = vec![Span::raw("    ")];
                    indented_spans.extend(rendered_line.spans);
                    lines.push(Line::from(indented_spans));
                }
            }

            lines
        }
    };

    // Set content height for scroll bounds
    let content_height = content.len() as u16;
    app.set_content_height(content_height);

    // Apply scroll offset
    let paragraph = Paragraph::new(content).scroll((app.scroll_offset, 0));

    frame.render_widget(paragraph, area);

    // Render scrollbar if content exceeds visible area
    if content_height > area.height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));

        let mut scrollbar_state =
            ScrollbarState::new(content_height.saturating_sub(area.height) as usize)
                .position(app.scroll_offset as usize);

        frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }
}
