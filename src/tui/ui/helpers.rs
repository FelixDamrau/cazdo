use chrono::{TimeZone, Utc};
use chrono_humanize::HumanTime;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::git::RemoteStatus;

/// Return a one-line area vertically centered within `area`.
pub fn centered_line_area(area: Rect) -> Rect {
    let height = 1;
    let y_offset = area.height.saturating_sub(height) / 2;
    Rect {
        x: area.x,
        y: area.y + y_offset,
        width: area.width,
        height,
    }
}

/// Helper to render a consistent scrollbar
pub fn render_scrollbar(
    frame: &mut Frame,
    area: Rect,
    content_height: usize,
    scroll_offset: usize,
) {
    if content_height > area.height as usize {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));

        let max_scroll = content_height.saturating_sub(area.height as usize);
        let mut scrollbar_state = ScrollbarState::new(max_scroll).position(scroll_offset);

        frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }
}

/// Helper to wrap text and append to lines with standard indentation
pub fn append_wrapped_text(lines: &mut Vec<Line>, text: &str, max_width: usize, style: Style) {
    for line in wrap_text(text, max_width) {
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(line, style),
        ]));
    }
}

/// Wrap prose to fit within width, preserving word boundaries and normalizing whitespace.
pub fn wrap_text(s: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![s.to_string()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();

    for word in s.split_whitespace() {
        if current.is_empty() {
            current = word.to_string();
        } else if display_width(&current) + 1 + display_width(word) <= max_width {
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

/// Return the number of terminal columns occupied by `value`.
pub(super) fn display_width(value: &str) -> usize {
    UnicodeWidthStr::width(value)
}

/// Split `value` at the largest prefix that fits within `width` columns.
pub(super) fn split_at_width(value: &str, width: usize) -> (String, &str) {
    let mut used = 0;
    let mut split_at = 0;
    for (index, character) in value.char_indices() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if used + character_width > width {
            break;
        }
        used += character_width;
        split_at = index + character.len_utf8();
        if used >= width {
            break;
        }
    }
    if split_at == 0 {
        return (String::new(), value);
    }
    (value[..split_at].to_string(), &value[split_at..])
}

/// Wrap a value at character boundaries while preserving every character.
pub(super) fn wrap_value(value: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    if value.is_empty() {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut remainder = value;
    while !remainder.is_empty() {
        let (line, rest) = split_at_width(remainder, width);
        if rest.len() == remainder.len() {
            let character = remainder.chars().next().expect("value is not empty");
            lines.push(character.to_string());
            remainder = &remainder[character.len_utf8()..];
        } else {
            lines.push(line);
            remainder = rest;
        }
    }
    lines
}

/// Truncate a value to `width` columns, using an ellipsis when needed.
pub(super) fn truncate_to_width(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if display_width(value) <= width {
        return value.to_string();
    }
    if width == 1 {
        return "…".to_string();
    }
    let (prefix, _) = split_at_width(value, width - 1);
    format!("{prefix}…")
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
        RemoteStatus::RemoteTracking => {
            unreachable!("remote-tracking branches are rendered separately")
        }
        RemoteStatus::UpToDate => ("up to date".to_string(), Color::Green),
        RemoteStatus::Ahead(n) => (format!("↑{}", n), Color::Yellow),
        RemoteStatus::Behind(n) => (format!("↓{}", n), Color::Yellow),
        RemoteStatus::Diverged { ahead, behind } => {
            (format!("↑{} ↓{}", ahead, behind), Color::Yellow)
        }
        RemoteStatus::Gone => ("remote gone".to_string(), Color::Red),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_text_uses_display_width_for_unicode_words() {
        for (text, width) in [("Prüfstand: Größe ändern", 23), ("日本語 はい", 11)] {
            let lines = wrap_text(text, width);
            assert!(
                lines.iter().all(|line| display_width(line) <= width),
                "wrapped lines exceed {width} columns: {lines:?}"
            );
            assert_eq!(lines, vec![text.to_string()]);
        }
    }

    #[test]
    fn centered_line_area_preserves_width_and_centers_line() {
        let centered = centered_line_area(Rect::new(4, 3, 20, 8));

        assert_eq!(centered, Rect::new(4, 6, 20, 1));
    }

    #[test]
    #[should_panic(expected = "remote-tracking branches are rendered separately")]
    fn test_format_remote_status_rejects_remote_tracking() {
        let _ = format_remote_status(&RemoteStatus::RemoteTracking);
    }
}
