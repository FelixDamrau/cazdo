use ratatui::style::{Color, Modifier, Style};
use std::time::Duration;

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

/// Layout constants
pub mod layout {
    /// Percentage width for branches panel
    pub const BRANCHES_WIDTH_PERCENT: u16 = 35;
    /// Height of branch info panel
    pub const BRANCH_INFO_HEIGHT: u16 = 5;
    /// Popup size (width%, height%)
    pub const POPUP_SIZE: (u16, u16) = (60, 12);
}

/// Timing constants
pub mod timing {
    use super::Duration;

    /// Polling interval for event loop
    pub const POLL_INTERVAL: Duration = Duration::from_millis(50);
    /// Status message duration (seconds)
    pub const STATUS_DURATION_SECS: u64 = 4;
}

/// Scroll constants
pub mod scroll {
    /// Lines to scroll with j/k + shift or mouse wheel
    pub const LINE_SCROLL_AMOUNT: u16 = 3;
    /// Divisor for page scroll (e.g., 2 = half page)
    pub const PAGE_SCROLL_DIVISOR: u16 = 2;
    /// Height reserved for borders (top + bottom + footer)
    pub const BORDER_HEIGHT_OFFSET: u16 = 4;
}
