use ratatui::style::{Color, Modifier, Style};

pub mod styles {
    use super::*;

    pub const ACCENT: Style = Style::new().fg(Color::Cyan);
    pub const MUTED: Style = Style::new().fg(Color::DarkGray);
    pub const TEXT: Style = Style::new().fg(Color::White);
    pub const ERROR: Style = Style::new().fg(Color::Red).add_modifier(Modifier::BOLD);
    pub const SUCCESS: Style = Style::new().fg(Color::Green);
    pub const WARNING: Style = Style::new().fg(Color::Yellow);
}

pub mod ui {
    use super::*;

    pub const BORDER: Style = Style::new().fg(Color::Cyan);
    pub const BORDER_ERROR: Style = Style::new().fg(Color::Red);
    pub const TITLE: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    pub const TITLE_ERROR: Style = Style::new().fg(Color::Red).add_modifier(Modifier::BOLD);
    pub const SELECTED: Style = Style::new().bg(Color::DarkGray);
}

pub mod branch {
    use super::*;

    pub const CURRENT: Style = Style::new().fg(Color::Green).add_modifier(Modifier::BOLD);
}
