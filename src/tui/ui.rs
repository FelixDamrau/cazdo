use chrono::{TimeZone, Utc};
use chrono_humanize::HumanTime;
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

use super::app::{App, AppMode, WorkItemStatus};
use super::html_render::render_html;
use super::theme;
use crate::git::RemoteStatus;

pub fn render(frame: &mut Frame, app: &mut App) {
    // Split into main area and footer
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(frame.area());

    // Split main area into left (branches) and right panels
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(main_chunks[0]);

    // Split right panel into work item details (top, scrollable) and branch info (bottom, fixed)
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(5)])
        .split(chunks[1]);

    render_branches(frame, app, chunks[0]);
    render_details(frame, app, right_chunks[0]);
    render_branch_info(frame, app, right_chunks[1]);
    render_footer(frame, app, main_chunks[1]);

    // Render popup if needed
    if let AppMode::ConfirmDelete(ref branch_name) = app.mode {
        let area = centered_rect(frame.area(), 60, 20);
        render_delete_popup(frame, branch_name, area);
    }
}

fn centered_rect(r: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn render_delete_popup(frame: &mut Frame, branch_name: &str, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::ui::BORDER_ERROR)
        .title(Line::from(vec![Span::styled(
            " Delete Branch ",
            theme::ui::TITLE_ERROR,
        )]));

    let content = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("Are you sure you want to delete branch "),
            Span::styled(branch_name, theme::branch::CURRENT),
            Span::raw("?"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("Press "),
            Span::styled("y", theme::styles::ERROR),
            Span::raw(" to confirm"),
        ]),
        Line::from(vec![
            Span::raw("Press "),
            Span::styled("n", theme::ui::TITLE),
            Span::raw(" or "),
            Span::styled("Esc", theme::ui::TITLE),
            Span::raw(" to cancel"),
        ]),
    ];

    let paragraph = Paragraph::new(content)
        .block(block)
        .alignment(ratatui::layout::Alignment::Center);

    frame.render_widget(Clear, area); // Clear background
    frame.render_widget(paragraph, area);
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    // Check for active status message
    if let Some(msg) = app.get_status_message() {
        let style = if msg.is_error {
            theme::styles::ERROR
        } else {
            theme::styles::SUCCESS.add_modifier(Modifier::BOLD)
        };
        let paragraph = Paragraph::new(Line::from(vec![Span::styled(&msg.text, style)]));
        frame.render_widget(paragraph, area);
        return;
    }

    let refresh_available = app.current_branch_has_work_item();
    let refresh_style = if refresh_available {
        theme::styles::ACCENT
    } else {
        theme::styles::MUTED
    };
    let refresh_text_style = if refresh_available {
        theme::styles::MUTED
    } else {
        theme::styles::MUTED.add_modifier(Modifier::DIM)
    };

    let protected_prefix = if app.show_protected { "hide " } else { "show " };
    let spans = vec![
        Span::styled(" j/k ", theme::styles::ACCENT),
        Span::styled("navigate  ", theme::styles::MUTED),
        Span::styled("o", theme::styles::ACCENT),
        Span::styled("pen  ", theme::styles::MUTED),
        Span::styled("pg\u{2191}\u{2193} ", theme::styles::ACCENT),
        Span::styled("scroll  ", theme::styles::MUTED),
        Span::styled("d", theme::styles::ACCENT),
        Span::styled("elete  ", theme::styles::MUTED),
        Span::styled("r", refresh_style),
        Span::styled("efresh  ", refresh_text_style),
        Span::styled(protected_prefix, theme::styles::MUTED),
        Span::styled("p", theme::styles::ACCENT),
        Span::styled("rotected  ", theme::styles::MUTED),
        Span::styled("q", theme::styles::ACCENT),
        Span::styled("uit", theme::styles::MUTED),
    ];

    let help_text = Line::from(spans);
    let paragraph = Paragraph::new(help_text).style(theme::styles::MUTED);
    frame.render_widget(paragraph, area);
}

fn render_branches(frame: &mut Frame, app: &App, area: Rect) {
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

            let style = if branch.is_current {
                theme::branch::CURRENT
            } else if branch.is_protected {
                theme::styles::MUTED
            } else {
                Style::default()
            };

            ListItem::new(format!(
                "{}{}{}{}",
                prefix, branch.name, protected_indicator, wi_suffix
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
                    " Branches ",
                    theme::ui::TITLE,
                )])),
        )
        .highlight_style(theme::ui::SELECTED.add_modifier(Modifier::BOLD))
        .highlight_symbol("\u{25BA} ");

    let mut state = ListState::default();
    state.select(Some(app.selected_index));

    frame.render_stateful_widget(list, area, &mut state);
}

fn render_details(frame: &mut Frame, app: &mut App, area: Rect) {
    let work_item_id = app.selected_branch().and_then(|b| b.work_item_id);

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

fn render_work_item_details(frame: &mut Frame, app: &mut App, area: Rect, wi_id: u32) {
    let status = app.get_work_item_status(wi_id);

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
            vec![
                Line::from(""),
                Line::from(Span::styled(
                    format!("  Error: {}", err),
                    Style::default().fg(Color::Red),
                )),
            ]
        }
        WorkItemStatus::Loaded(wi) => {
            let type_icon = wi.work_item_type.icon();
            let type_name = wi.work_item_type.display_name();
            let state_icon = wi.state.icon();
            let state_name = wi.state.display_name();
            let state_color = wi.state.color();
            let max_width = area.width.saturating_sub(4) as usize;

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

            // Title (bold + underlined)
            // Note: OSC 8 hyperlinks disabled - they break ratatui's width calculation
            for line in wrap_text(&wi.title, max_width).iter() {
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(
                        line.clone(),
                        theme::styles::TEXT
                            .add_modifier(Modifier::BOLD)
                            .add_modifier(Modifier::UNDERLINED),
                    ),
                ]));
            }

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

/// Format relative time from Unix timestamp
fn format_relative_time(timestamp: i64) -> String {
    match Utc.timestamp_opt(timestamp, 0) {
        chrono::LocalResult::Single(dt) => HumanTime::from(dt).to_string(),
        _ => "unknown".to_string(),
    }
}

/// Format remote status for display
fn format_remote_status(status: &RemoteStatus) -> (String, Color) {
    match status {
        RemoteStatus::LocalOnly => ("local only".to_string(), Color::DarkGray),
        RemoteStatus::UpToDate => ("up to date".to_string(), Color::Green),
        RemoteStatus::Ahead(n) => (format!("↑{}", n), Color::Yellow),
        RemoteStatus::Behind(n) => (format!("↓{}", n), Color::Yellow),
        RemoteStatus::Diverged { ahead, behind } => {
            (format!("↑{} ↓{}", ahead, behind), Color::Yellow)
        }
        RemoteStatus::Gone => ("remote gone".to_string(), Color::Red),
    }
}

/// Render the branch info panel
fn render_branch_info(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::ui::BORDER)
        .title(Line::from(vec![Span::styled(
            " Branch Info ",
            theme::ui::TITLE,
        )]));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    if let Some(branch) = app.selected_branch() {
        // Branch name
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(&branch.name, theme::branch::CURRENT),
        ]));

        // Remote status and last commit
        if let Some(status) = app.get_branch_status(&branch.name) {
            let (remote_text, remote_color) = format_remote_status(&status.remote_status);

            // Last commit info on same line as remote if available
            if let (Some(author), Some(time)) =
                (&status.last_commit_author, status.last_commit_time)
            {
                let relative_time = format_relative_time(time);
                lines.push(Line::from(vec![
                    Span::styled("  Remote: ", theme::styles::MUTED),
                    Span::styled(remote_text, Style::default().fg(remote_color)),
                    Span::styled("  │  ", theme::styles::MUTED),
                    Span::styled(author.clone(), theme::styles::TEXT),
                    Span::styled(", ", theme::styles::MUTED),
                    Span::styled(relative_time, theme::styles::MUTED),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::styled("  Remote: ", theme::styles::MUTED),
                    Span::styled(remote_text, Style::default().fg(remote_color)),
                ]));
            }
        } else {
            lines.push(Line::from(vec![Span::styled(
                "  Loading...",
                theme::styles::MUTED,
            )]));
        }
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}
