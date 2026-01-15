use anyhow::Result;
use crossterm::terminal;
use ratatui::{
    backend::CrosstermBackend,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Padding, Widget},
    Terminal,
};
use std::io::stdout;

use crate::azure_devops::WorkItem;

/// Render work item information in a styled box
pub fn render_work_item(work_item: &WorkItem, branch: &str) -> Result<()> {
    let mut stdout = stdout();
    
    terminal::enable_raw_mode()?;
    
    let backend = CrosstermBackend::new(&mut stdout);
    let mut terminal = Terminal::new(backend)?;
    
    let content_height = 7;
    
    terminal.insert_before(content_height, |buf| {
        let area = Rect::new(0, 0, buf.area.width.min(60), content_height);
        render_work_item_widget(work_item, branch, area, buf);
    })?;
    
    terminal::disable_raw_mode()?;
    
    println!();
    
    Ok(())
}

fn render_work_item_widget(work_item: &WorkItem, branch: &str, area: Rect, buf: &mut ratatui::buffer::Buffer) {
    let type_icon = work_item.work_item_type.icon();
    let type_name = work_item.work_item_type.display_name();
    let state_icon = work_item.state.icon();
    let state_name = work_item.state.display_name();
    
    let title_line = format!("Work Item #{}", work_item.id);
    
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Line::from(vec![
            Span::styled(" ", Style::default()),
            Span::styled(&title_line, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(" ", Style::default()),
        ]))
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    Widget::render(block, area, buf);
    
    let lines = vec![
        Line::from(vec![
            Span::styled("Title:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(&work_item.title, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("Type:   ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("{} {}", type_icon, type_name)),
        ]),
        Line::from(vec![
            Span::styled("State:  ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("{} {}", state_icon, state_name)),
        ]),
        Line::from(vec![
            Span::styled("Branch: ", Style::default().fg(Color::DarkGray)),
            Span::styled(branch, Style::default().fg(Color::Green)),
        ]),
    ];

    let paragraph = Paragraph::new(lines);
    Widget::render(paragraph, inner, buf);
}

/// Render only branch info when no work item number found
pub fn render_branch_only(branch: &str) -> Result<()> {
    let mut stdout = stdout();
    
    terminal::enable_raw_mode()?;
    
    let backend = CrosstermBackend::new(&mut stdout);
    let mut terminal = Terminal::new(backend)?;
    
    let content_height = 4;
    
    terminal.insert_before(content_height, |buf| {
        let area = Rect::new(0, 0, buf.area.width.min(60), content_height);
        
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .title(Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled("Branch Info", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(" ", Style::default()),
            ]))
            .padding(Padding::horizontal(1));

        let inner = block.inner(area);
        Widget::render(block, area, buf);
        
        let lines = vec![
            Line::from(vec![
                Span::styled("Branch: ", Style::default().fg(Color::DarkGray)),
                Span::styled(branch, Style::default().fg(Color::Green)),
            ]),
            Line::from(vec![
                Span::styled("No work item number found in branch name", Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)),
            ]),
        ];

        let paragraph = Paragraph::new(lines);
        Widget::render(paragraph, inner, buf);
    })?;
    
    terminal::disable_raw_mode()?;
    println!();
    
    Ok(())
}

/// Render an error message
pub fn render_error(message: &str) -> Result<()> {
    let mut stdout = stdout();
    
    terminal::enable_raw_mode()?;
    
    let backend = CrosstermBackend::new(&mut stdout);
    let mut terminal = Terminal::new(backend)?;
    
    let content_height = 4;
    
    terminal.insert_before(content_height, |buf| {
        let area = Rect::new(0, 0, buf.area.width.min(70), content_height);
        
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red))
            .title(Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled("Error", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                Span::styled(" ", Style::default()),
            ]))
            .padding(Padding::horizontal(1));

        let inner = block.inner(area);
        Widget::render(block, area, buf);
        
        let lines = vec![
            Line::from(vec![
                Span::styled(message, Style::default().fg(Color::Red)),
            ]),
        ];

        let paragraph = Paragraph::new(lines);
        Widget::render(paragraph, inner, buf);
    })?;
    
    terminal::disable_raw_mode()?;
    println!();
    
    Ok(())
}
