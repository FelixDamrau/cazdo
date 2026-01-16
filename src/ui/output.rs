use anyhow::Result;
use crossterm::style::{self, Color, Stylize};
use std::io::{self, Write};
use unicode_width::UnicodeWidthStr;

use crate::azure_devops::WorkItem;

/// Get display width of a string (accounts for wide chars like emojis)
fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Render work item information in a styled box
pub fn render_work_item(work_item: &WorkItem, branch: &str) -> Result<()> {
    let type_icon = work_item.work_item_type.icon();
    let type_name = work_item.work_item_type.display_name();
    let state_icon = work_item.state.icon();
    let state_name = work_item.state.display_name();

    let title = format!(" Work Item #{} ", work_item.id);
    let width = 58;

    let mut stdout = io::stdout();

    // Top border
    print_colored(&format!("╭{}╮", "─".repeat(width)), Color::Cyan)?;
    println!();

    // Title line
    print_colored("│", Color::Cyan)?;
    print_colored(&title, Color::Cyan)?;
    print!("{}", " ".repeat(width - display_width(&title)));
    print_colored("│", Color::Cyan)?;
    println!();

    // Separator
    print_colored(&format!("├{}┤", "─".repeat(width)), Color::Cyan)?;
    println!();

    // Title field
    print_colored("│", Color::Cyan)?;
    print_colored("  Title:  ", Color::DarkGrey)?;
    let title_text = truncate(&work_item.title, width - 12);
    print!("{}", title_text.clone().white().bold());
    print!("{}", " ".repeat(width - 10 - display_width(&title_text)));
    print_colored("│", Color::Cyan)?;
    println!();

    // Type field
    print_colored("│", Color::Cyan)?;
    print_colored("  Type:   ", Color::DarkGrey)?;
    let type_text = format!("{} {}", type_icon, type_name);
    print!("{}", &type_text);
    print!("{}", " ".repeat(width - 10 - display_width(&type_text)));
    print_colored("│", Color::Cyan)?;
    println!();

    // State field
    print_colored("│", Color::Cyan)?;
    print_colored("  State:  ", Color::DarkGrey)?;
    let state_text = format!("{} {}", state_icon, state_name);
    print!("{}", &state_text);
    print!("{}", " ".repeat(width - 10 - display_width(&state_text)));
    print_colored("│", Color::Cyan)?;
    println!();

    // Branch field
    print_colored("│", Color::Cyan)?;
    print_colored("  Branch: ", Color::DarkGrey)?;
    let branch_text = truncate(branch, width - 12);
    print!("{}", branch_text.clone().green());
    print!("{}", " ".repeat(width - 10 - display_width(&branch_text)));
    print_colored("│", Color::Cyan)?;
    println!();

    // Bottom border
    print_colored(&format!("╰{}╯", "─".repeat(width)), Color::Cyan)?;
    println!();

    stdout.flush()?;
    Ok(())
}

/// Render only branch info when no work item number found
pub fn render_branch_only(branch: &str) -> Result<()> {
    let width = 58;
    let mut stdout = io::stdout();

    // Top border
    print_colored(&format!("╭{}╮", "─".repeat(width)), Color::Yellow)?;
    println!();

    // Title
    let title = " Branch Info ";
    print_colored("│", Color::Yellow)?;
    print_colored(title, Color::Yellow)?;
    print!("{}", " ".repeat(width - display_width(title)));
    print_colored("│", Color::Yellow)?;
    println!();

    // Separator
    print_colored(&format!("├{}┤", "─".repeat(width)), Color::Yellow)?;
    println!();

    // Branch field
    print_colored("│", Color::Yellow)?;
    print_colored("  Branch: ", Color::DarkGrey)?;
    let branch_text = truncate(branch, width - 12);
    print!("{}", branch_text.clone().green());
    print!("{}", " ".repeat(width - 10 - display_width(&branch_text)));
    print_colored("│", Color::Yellow)?;
    println!();

    // Info
    let info_text = "  No work item number found in branch name";
    print_colored("│", Color::Yellow)?;
    print_colored(info_text, Color::DarkGrey)?;
    print!("{}", " ".repeat(width - display_width(info_text)));
    print_colored("│", Color::Yellow)?;
    println!();

    // Bottom border
    print_colored(&format!("╰{}╯", "─".repeat(width)), Color::Yellow)?;
    println!();

    stdout.flush()?;
    Ok(())
}

/// Render an error message
pub fn render_error(message: &str) -> Result<()> {
    let width = 68;
    let mut stdout = io::stdout();

    // Top border
    print_colored(&format!("╭{}╮", "─".repeat(width)), Color::Red)?;
    println!();

    // Title
    let title = " Error ";
    print_colored("│", Color::Red)?;
    print_colored(title, Color::Red)?;
    print!("{}", " ".repeat(width - display_width(title)));
    print_colored("│", Color::Red)?;
    println!();

    // Separator
    print_colored(&format!("├{}┤", "─".repeat(width)), Color::Red)?;
    println!();

    // Message (may span multiple lines)
    for line in wrap_text(message, width - 4) {
        print_colored("│", Color::Red)?;
        print!("  ");
        print!("{}", line.clone().red());
        print!("{}", " ".repeat(width - 2 - display_width(&line)));
        print_colored("│", Color::Red)?;
        println!();
    }

    // Bottom border
    print_colored(&format!("╰{}╯", "─".repeat(width)), Color::Red)?;
    println!();

    stdout.flush()?;
    Ok(())
}

fn print_colored(text: &str, color: Color) -> Result<()> {
    print!("{}", style::style(text).with(color));
    Ok(())
}

fn truncate(s: &str, max_width: usize) -> String {
    if display_width(s) <= max_width {
        s.to_string()
    } else {
        let mut result = String::new();
        let mut current_width = 0;
        for c in s.chars() {
            let char_width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            if current_width + char_width + 3 > max_width {
                break;
            }
            result.push(c);
            current_width += char_width;
        }
        result.push_str("...");
        result
    }
}

fn wrap_text(s: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0;

    for word in s.split_whitespace() {
        let word_width = display_width(word);
        if current.is_empty() {
            current = word.to_string();
            current_width = word_width;
        } else if current_width + 1 + word_width <= max_width {
            current.push(' ');
            current.push_str(word);
            current_width += 1 + word_width;
        } else {
            lines.push(current);
            current = word.to_string();
            current_width = word_width;
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
