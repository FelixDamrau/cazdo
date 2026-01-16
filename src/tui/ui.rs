use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState,
    },
};

use super::app::{App, WorkItemStatus};
use super::html_render::render_html;

pub fn render(frame: &mut Frame, app: &mut App) {
    // Split into main area and footer
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(frame.area());

    // Split main area into left (branches) and right (details) panels
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(main_chunks[0]);

    render_branches(frame, app, chunks[0]);
    render_details(frame, app, chunks[1]);
    render_footer(frame, app, main_chunks[1]);
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let scroll_info = if app.content_height > area.height {
        format!(
            " [{}/{}]",
            app.scroll_offset + 1,
            app.content_height.saturating_sub(area.height) + 1
        )
    } else {
        String::new()
    };

    let help_text = Line::from(vec![
        Span::styled(" ↑/↓ ", Style::default().fg(Color::Cyan)),
        Span::raw("Navigate  "),
        Span::styled(" o/Enter ", Style::default().fg(Color::Cyan)),
        Span::raw("Open  "),
        Span::styled(" PgUp/Dn ", Style::default().fg(Color::Cyan)),
        Span::raw("Scroll"),
        Span::styled(&scroll_info, Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled(" q ", Style::default().fg(Color::Cyan)),
        Span::raw("Quit"),
    ]);

    let paragraph = Paragraph::new(help_text).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(paragraph, area);
}

fn render_branches(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .branches
        .iter()
        .map(|branch| {
            let prefix = if branch.is_current { "* " } else { "  " };
            let wi_suffix = match branch.work_item_id {
                Some(id) => format!(" [#{}]", id),
                None => String::new(),
            };

            let style = if branch.is_current {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(format!("{}{}{}", prefix, branch.name, wi_suffix)).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(Line::from(vec![Span::styled(
                    " Branches ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )])),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("► ");

    let mut state = ListState::default();
    state.select(Some(app.selected_index));

    frame.render_stateful_widget(list, area, &mut state);
}

fn render_details(frame: &mut Frame, app: &mut App, area: Rect) {
    let selected = app.selected_branch().cloned();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Line::from(vec![Span::styled(
            " Work Item Details ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Clear the inner area before rendering new content
    frame.render_widget(Clear, inner);

    if let Some(branch) = selected {
        match branch.work_item_id {
            Some(wi_id) => {
                render_work_item_details(frame, app, inner, wi_id);
            }
            None => {
                let text = Paragraph::new(vec![
                    Line::from(""),
                    Line::from(Span::styled(
                        "  No work item linked to this branch",
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::ITALIC),
                    )),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("  Branch: ", Style::default().fg(Color::DarkGray)),
                        Span::styled(&branch.name, Style::default().fg(Color::Green)),
                    ]),
                ]);
                app.set_content_height(4);
                frame.render_widget(text, inner);
            }
        }
    }
}

fn render_work_item_details(frame: &mut Frame, app: &mut App, area: Rect, wi_id: u32) {
    let status = app.get_work_item_status(wi_id).clone();

    let content: Vec<Line> = match status {
        WorkItemStatus::NotFetched | WorkItemStatus::Loading => {
            vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  Loading work item...",
                    Style::default().fg(Color::Yellow),
                )),
            ]
        }
        WorkItemStatus::Error(err) => {
            vec![
                Line::from(""),
                Line::from(Span::styled(
                    format!("  Error: {}", err),
                    Style::default().fg(Color::Red),
                )),
            ]
        }
        WorkItemStatus::NoWorkItem => {
            vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  Work item not found",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                )),
            ]
        }
        WorkItemStatus::Loaded(wi) => {
            let type_icon = wi.work_item_type.icon();
            let type_name = wi.work_item_type.display_name();
            let state_icon = wi.state.icon();
            let state_name = wi.state.display_name();
            let max_width = area.width.saturating_sub(4) as usize;

            let mut lines = vec![
                Line::from(""),
                // ID and Type
                Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(
                        format!("#{} ", wi.id),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(format!("{} {}", type_icon, type_name)),
                ]),
            ];

            lines.push(Line::from(""));

            // Title (bold + underlined)
            // Note: OSC 8 hyperlinks disabled - they break ratatui's width calculation
            for line in wrap_text(&wi.title, max_width).iter() {
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(
                        line.clone(),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD)
                            .add_modifier(Modifier::UNDERLINED),
                    ),
                ]));
            }

            // State
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  State: ", Style::default().fg(Color::DarkGray)),
                Span::raw(format!("{} {}", state_icon, state_name)),
            ]));

            // Assigned To
            if let Some(ref assigned) = wi.assigned_to {
                lines.push(Line::from(vec![
                    Span::styled("  Assigned To: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(assigned.clone(), Style::default().fg(Color::White)),
                ]));
            }

            // Tags
            if !wi.tags.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("  Tags: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(wi.tags.join(", "), Style::default().fg(Color::Magenta)),
                ]));
            }

            // All rich text fields (Description, Acceptance Criteria, etc.)
            for field in &wi.rich_text_fields {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    format!("  {}:", field.name),
                    Style::default().fg(Color::DarkGray),
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

/// Wrap text to fit within width
fn wrap_text(s: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![s.to_string()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();

    for word in s.split_whitespace() {
        if current.is_empty() {
            current = word.to_string();
        } else if current.len() + 1 + word.len() <= max_width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current);
            current = word.to_string();
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}
