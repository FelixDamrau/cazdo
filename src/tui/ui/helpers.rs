use chrono::{TimeZone, Utc};
use chrono_humanize::HumanTime;
use ratatui::{
    style::Style,
    text::{Line, Span},
};

use crate::git::RemoteStatus;

/// Helper to wrap text and append to lines with standard indentation
pub fn append_wrapped_text(lines: &mut Vec<Line>, text: &str, max_width: usize, style: Style) {
    for line in wrap_text(text, max_width) {
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(line, style),
        ]));
    }
}

/// Wrap text to fit within width
pub fn wrap_text(s: &str, max_width: usize) -> Vec<String> {
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
pub fn format_relative_time(timestamp: i64) -> String {
    match Utc.timestamp_opt(timestamp, 0) {
        chrono::LocalResult::Single(dt) => HumanTime::from(dt).to_string(),
        _ => "unknown".to_string(),
    }
}

/// Format remote status for display
pub fn format_remote_status(status: &RemoteStatus) -> (String, ratatui::style::Color) {
    use ratatui::style::Color;

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
