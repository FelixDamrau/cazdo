use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::azure_devops::{FieldFormat, RichTextField, WorkItem};
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
    let max_width = area.width.saturating_sub(4) as usize;

    let content: Vec<Line> = match app.get_work_item_status(wi_id) {
        WorkItemStatus::NotFetched | WorkItemStatus::Loading => vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Loading work item...",
                theme::styles::WARNING,
            )),
        ],
        WorkItemStatus::Error(err) => {
            let mut lines = vec![Line::from("")];
            append_wrapped_text(
                &mut lines,
                &format!("Error: {err}"),
                max_width,
                Style::default().fg(Color::Red),
            );
            lines
        }
        WorkItemStatus::Loaded(wi) => work_item_lines(wi, max_width),
    };

    // Returned to the update loop, which owns scroll bounds.
    let content_height = content.len() as u16;

    frame.render_widget(
        Paragraph::new(content).scroll((app.scroll_offset(), 0)),
        area,
    );
    super::helpers::render_scrollbar(
        frame,
        area,
        content_height as usize,
        app.scroll_offset() as usize,
    );

    content_height
}

/// The full body of a loaded work item: identity, metadata, title, then each
/// rich text field in the order Azure DevOps returned it.
fn work_item_lines(wi: &WorkItem, max_width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(""),
        identity_line(wi),
        metadata_line(wi),
        Line::from(""),
    ];

    append_wrapped_text(
        &mut lines,
        &wi.title,
        max_width,
        theme::styles::TEXT
            .add_modifier(Modifier::BOLD)
            .add_modifier(Modifier::UNDERLINED),
    );

    for field in &wi.rich_text_fields {
        lines.extend(rich_text_field_lines(field, max_width));
    }

    lines
}

/// `#1234 <icon> Bug`
fn identity_line(wi: &WorkItem) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("#{} ", wi.id),
            theme::styles::ACCENT.add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "{} {}",
            wi.work_item_type.icon(),
            wi.work_item_type.display_name()
        )),
    ])
}

/// `<icon> Active  •  Alice  •  tag-a, tag-b`, skipping the parts that are absent.
fn metadata_line(wi: &WorkItem) -> Line<'static> {
    let mut spans = vec![
        Span::raw("  "),
        Span::styled(
            format!("{} {}", wi.state.icon(), wi.state.display_name()),
            Style::default().fg(wi.state.color()),
        ),
    ];

    if let Some(assigned) = &wi.assigned_to {
        spans.push(Span::styled("  •  ", theme::styles::MUTED));
        spans.push(Span::styled(assigned.clone(), theme::styles::TEXT));
    }

    if !wi.tags.is_empty() {
        spans.push(Span::styled("  •  ", theme::styles::MUTED));
        spans.push(Span::styled(
            wi.tags.join(", "),
            Style::default().fg(Color::Magenta),
        ));
    }

    Line::from(spans)
}

/// A named field, blank-line separated and indented one level under its name.
fn rich_text_field_lines(field: &RichTextField, max_width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {}:", field.name),
            theme::styles::MUTED,
        )),
    ];

    let field_width = max_width.saturating_sub(4);
    let rendered = match field.format {
        FieldFormat::Html => render_html(&field.value, field_width),
        FieldFormat::Markdown => render_markdown(&field.value, field_width),
    };
    for rendered_line in rendered {
        let mut spans = vec![Span::raw("    ")];
        spans.extend(rendered_line.spans);
        lines.push(Line::from(spans));
    }

    lines
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

        assert!(
            text.contains("mark"),
            "rendered text missing; got: {text:?}"
        );
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
