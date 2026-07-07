use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::azure_devops::FieldFormat;
use crate::tui::app::{App, DetailsMetrics, WorkItemStatus};
use crate::tui::html_render::render_html;
use crate::tui::markdown_render::render_markdown;
use crate::tui::theme;

use super::helpers::append_wrapped_text;

/// Render the work item details panel
pub fn render_details(frame: &mut Frame, app: &App, area: Rect) -> DetailsMetrics {
    let work_item_id = app.selected_work_item_id();

    // Calculate inner area first to determine visible height
    let inner = Block::default().borders(Borders::ALL).inner(area);
    let visible_height = inner.height;

    // Build scroll info for bottom border (only if scrollable).
    // This intentionally reads the content height measured on the previous frame
    // (`app.content_height()`); the freshly measured height is returned below and
    // applied after the draw, preserving the prior one-frame indicator lag.
    let scroll_title = if app.content_height() > visible_height {
        Line::from(vec![
            Span::styled(
                format!(
                    " {}/{} ",
                    app.scroll_offset() + 1,
                    app.content_height().saturating_sub(visible_height) + 1
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

    let content_height = match work_item_id {
        Some(wi_id) => render_work_item_details(frame, app, inner, wi_id),
        None => {
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  No work item linked to this branch",
                    theme::styles::MUTED.add_modifier(Modifier::ITALIC),
                )),
            ];

            let content_height = lines.len() as u16;
            let text = Paragraph::new(lines);
            frame.render_widget(text, inner);
            content_height
        }
    };

    DetailsMetrics {
        content_height,
        visible_height,
    }
}

/// Render the work item details content
fn render_work_item_details(frame: &mut Frame, app: &App, area: Rect, wi_id: u32) -> u16 {
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

                let field_width = max_width.saturating_sub(4);
                let rendered = match field.format {
                    FieldFormat::Html => render_html(&field.value, field_width),
                    FieldFormat::Markdown => render_markdown(&field.value, field_width),
                };
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

    // Content height for scroll bounds (returned to the update loop).
    let content_height = content.len() as u16;

    // Apply scroll offset
    let paragraph = Paragraph::new(content).scroll((app.scroll_offset(), 0));

    frame.render_widget(paragraph, area);

    // Render scrollbar
    super::helpers::render_scrollbar(
        frame,
        area,
        content_height as usize,
        app.scroll_offset() as usize,
    );

    content_height
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::azure_devops::{RichTextField, WorkItem, WorkItemState, WorkItemType};
    use crate::git::BranchScope;
    use crate::tui::app::{BranchInfo, Msg};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn work_item_with(fields: Vec<RichTextField>) -> WorkItem {
        WorkItem {
            id: 204,
            title: "Sample item".to_string(),
            work_item_type: WorkItemType::ProductBacklogItem,
            state: WorkItemState::New,
            assigned_to: None,
            url: None,
            tags: vec![],
            rich_text_fields: fields,
        }
    }

    fn branch_linked_to(work_item_id: u32) -> BranchInfo {
        BranchInfo {
            key: "wi".to_string(),
            display_name: "wi".to_string(),
            branch_name: "feature/wi".to_string(),
            remote_name: None,
            scope: BranchScope::Local,
            work_item_id: Some(work_item_id),
            is_current: false,
            is_protected: false,
            is_stale: false,
        }
    }

    /// Render the details pane to an off-screen buffer and return its text.
    fn rendered_text(app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(80, 30)).expect("terminal");
        terminal
            .draw(|frame| {
                render_details(frame, app, frame.area());
            })
            .expect("draw");
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn markdown_field_is_rendered_not_shown_as_raw_source() {
        let mut app = App::new(vec![branch_linked_to(204)], vec![]);
        app.update(Msg::SetWorkItemLoaded {
            id: 204,
            work_item: work_item_with(vec![RichTextField {
                name: "Description".to_string(),
                // WI 204's real Description value.
                value: "THIS IS IN _**mark** down_".to_string(),
                format: FieldFormat::Markdown,
            }]),
        });

        let text = rendered_text(&app);

        assert!(text.contains("mark"), "rendered text missing; got: {text:?}");
        assert!(!text.contains("_**"), "raw markdown leaked: {text:?}");
    }

    #[test]
    fn html_and_markdown_fields_dispatch_to_their_own_renderers() {
        let mut app = App::new(vec![branch_linked_to(204)], vec![]);
        app.update(Msg::SetWorkItemLoaded {
            id: 204,
            work_item: work_item_with(vec![
                RichTextField {
                    name: "Description".to_string(),
                    value: "a _markdownish_ line".to_string(),
                    format: FieldFormat::Markdown,
                },
                RichTextField {
                    name: "Acceptance Criteria".to_string(),
                    value: "<b>htmlish</b> line".to_string(),
                    format: FieldFormat::Html,
                },
            ]),
        });

        let text = rendered_text(&app);

        assert!(text.contains("markdownish") && text.contains("htmlish"));
        assert!(!text.contains("_markdownish_"), "markdown leaked: {text:?}");
        assert!(!text.contains("<b>"), "html leaked: {text:?}");
    }
}
