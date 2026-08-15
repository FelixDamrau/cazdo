use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::git::{WorktreeCleanliness, WorktreeInfo, WorktreeState, WorktreeSubmodules};
use crate::tui::app::App;
use crate::tui::theme;

/// Render the inventory list. Selection is deliberately independent from the
/// branch list so returning to the branch view preserves its selection.
pub fn render_worktrees(frame: &mut Frame, app: &App, area: Rect) {
    let content_width = area.width.saturating_sub(2) as usize;
    let items: Vec<ListItem> = app
        .worktrees()
        .iter()
        .map(|entry| worktree_list_item(entry, content_width))
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::ui::BORDER)
        .title(Line::from(vec![Span::styled(
            format!(" Worktrees ({}) ", app.worktrees().len()),
            theme::ui::TITLE,
        )]));
    let list = List::new(items)
        .block(block)
        .highlight_style(theme::ui::SELECTED_BACKGROUND.add_modifier(Modifier::BOLD))
        .highlight_symbol("\u{25BA} ");
    let mut state = ListState::default();
    if !app.worktrees().is_empty() {
        state.select(Some(app.worktree_selected_index()));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn worktree_list_item(entry: &WorktreeInfo, width: usize) -> ListItem<'static> {
    ListItem::new(worktree_list_lines(entry, width))
}

fn worktree_list_lines(entry: &WorktreeInfo, width: usize) -> Vec<Line<'static>> {
    let (marker, marker_style) = if entry.is_current {
        ("* ", theme::branch::CURRENT)
    } else if entry.is_main {
        ("m ", theme::styles::ACCENT)
    } else {
        ("  ", theme::styles::TEXT)
    };
    let mut lines = vec![bounded_prefix_line(
        marker,
        entry.name(),
        marker_style,
        list_name_style(entry),
        width,
    )];
    lines.push(bounded_field_line(
        "HEAD",
        entry.ref_display(),
        theme::styles::TEXT,
        width,
    ));
    lines.push(bounded_field_line(
        "Path",
        entry.path.to_string_lossy(),
        theme::styles::TEXT,
        width,
    ));
    lines.push(bounded_field_line(
        "Status",
        list_status(entry),
        status_style(entry),
        width,
    ));
    if let Some(reason) = &entry.lock_reason {
        lines.push(bounded_field_line(
            "Lock",
            reason.clone(),
            theme::styles::WARNING,
            width,
        ));
    }
    if entry.prunable {
        lines.push(bounded_field_line(
            "Prunable",
            "yes",
            theme::styles::WARNING,
            width,
        ));
    }
    lines
}

/// Render details for the selected inventory entry.
pub fn render_worktree_details(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::ui::BORDER)
        .title(Line::from(vec![Span::styled(
            " Worktree Details ",
            theme::ui::TITLE,
        )]));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(entry) = app.selected_worktree() else {
        frame.render_widget(
            Paragraph::new("  No worktree inventory loaded").style(theme::styles::MUTED),
            inner,
        );
        return;
    };

    frame.render_widget(
        Paragraph::new(worktree_diagnostic_lines(entry, inner.width as usize)),
        inner,
    );
}

pub(super) fn worktree_diagnostic_lines(entry: &WorktreeInfo, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.extend(wrapped_field_lines(
        "Name",
        entry.name(),
        list_name_style(entry),
        width,
    ));
    lines.extend(wrapped_field_lines(
        "Path",
        entry.path.to_string_lossy(),
        theme::styles::TEXT,
        width,
    ));
    lines.extend(wrapped_field_lines(
        "Identity",
        if entry.is_main { "main" } else { "linked" },
        if entry.is_main {
            theme::styles::ACCENT
        } else {
            theme::styles::TEXT
        },
        width,
    ));
    lines.extend(wrapped_field_lines(
        "Branch/HEAD",
        entry.ref_display(),
        if entry.is_current {
            theme::branch::CURRENT
        } else {
            theme::styles::TEXT
        },
        width,
    ));
    lines.extend(wrapped_field_lines(
        "Current",
        if entry.is_current { "yes" } else { "no" },
        if entry.is_current {
            theme::branch::CURRENT
        } else {
            theme::styles::TEXT
        },
        width,
    ));
    lines.extend(wrapped_field_lines(
        "State",
        state_detail(&entry.state),
        state_style(&entry.state),
        width,
    ));
    lines.extend(wrapped_field_lines(
        "Cleanliness",
        cleanliness_detail(&entry.cleanliness),
        cleanliness_style(&entry.cleanliness),
        width,
    ));
    lines.extend(wrapped_field_lines(
        "Lock",
        entry.lock_reason.as_deref().unwrap_or("unlocked"),
        if entry.is_locked() {
            theme::styles::WARNING
        } else {
            theme::styles::TEXT
        },
        width,
    ));
    lines.extend(wrapped_field_lines(
        "Prunable",
        if entry.prunable { "yes" } else { "no" },
        if entry.prunable {
            theme::styles::WARNING
        } else {
            theme::styles::TEXT
        },
        width,
    ));
    lines.extend(wrapped_field_lines(
        "Submodules",
        submodule_detail(&entry.submodules),
        submodule_style(&entry.submodules),
        width,
    ));
    lines
}

fn state_detail(state: &WorktreeState) -> String {
    match state {
        WorktreeState::Invalid(error) => format!("invalid: {error}"),
        WorktreeState::Unknown(error) => format!("unknown: {error}"),
        _ => state.label().to_string(),
    }
}

fn cleanliness_detail(cleanliness: &WorktreeCleanliness) -> String {
    match cleanliness {
        WorktreeCleanliness::Clean => "clean".to_string(),
        WorktreeCleanliness::Dirty(reasons) if reasons.is_empty() => "dirty".to_string(),
        WorktreeCleanliness::Dirty(reasons) => format!(
            "dirty: {}",
            reasons
                .iter()
                .map(|reason| reason.label())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        WorktreeCleanliness::Unknown(error) => format!("unknown: {error}"),
    }
}

fn list_status(entry: &WorktreeInfo) -> String {
    let cleanliness = match &entry.cleanliness {
        WorktreeCleanliness::Clean => "clean",
        WorktreeCleanliness::Dirty(_) => "dirty",
        WorktreeCleanliness::Unknown(_) => "unknown",
    };
    format!("{} / {cleanliness}", entry.state.label())
}
fn submodule_detail(submodules: &WorktreeSubmodules) -> String {
    match submodules {
        WorktreeSubmodules::None => "none".to_string(),
        WorktreeSubmodules::Present => "present".to_string(),
        WorktreeSubmodules::Unknown(error) => format!("unknown: {error}"),
    }
}

fn bounded_field_line(
    label: &str,
    value: impl Into<String>,
    value_style: Style,
    width: usize,
) -> Line<'static> {
    bounded_prefix_line(
        &format!("  {label}: "),
        value,
        theme::styles::MUTED,
        value_style,
        width,
    )
}

fn bounded_prefix_line(
    prefix: &str,
    value: impl Into<String>,
    prefix_style: Style,
    value_style: Style,
    width: usize,
) -> Line<'static> {
    let prefix = truncate_to_width(prefix, width);
    let prefix_width = display_width(&prefix);
    let value_width = width.saturating_sub(prefix_width);
    let value = truncate_to_width(&value.into(), value_width);
    Line::from(vec![
        Span::styled(prefix, prefix_style),
        Span::styled(value, value_style),
    ])
}

fn wrapped_field_lines(
    label: &str,
    value: impl Into<String>,
    value_style: Style,
    width: usize,
) -> Vec<Line<'static>> {
    wrapped_prefix_lines(
        &format!("  {label}: "),
        value,
        theme::styles::MUTED,
        value_style,
        width,
    )
}

fn wrapped_prefix_lines(
    prefix: &str,
    value: impl Into<String>,
    prefix_style: Style,
    value_style: Style,
    width: usize,
) -> Vec<Line<'static>> {
    let prefix = truncate_to_width(prefix, width);
    let prefix_width = display_width(&prefix);
    let value_width = width.saturating_sub(prefix_width).max(1);
    let continuation = " ".repeat(prefix_width);
    wrap_value(&value.into(), value_width)
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            let line_prefix = if index == 0 {
                prefix.clone()
            } else {
                continuation.clone()
            };
            Line::from(vec![
                Span::styled(line_prefix, prefix_style),
                Span::styled(value, value_style),
            ])
        })
        .collect()
}

fn wrap_value(value: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in value.split_whitespace() {
        if !current.is_empty() && display_width(&current) + 1 + display_width(word) > width {
            lines.push(std::mem::take(&mut current));
        }

        let mut remainder = word;
        while display_width(remainder) > width {
            let (chunk, rest) = split_at_width(remainder, width);
            lines.push(chunk);
            remainder = rest;
        }

        if current.is_empty() {
            current.push_str(remainder);
        } else {
            current.push(' ');
            current.push_str(remainder);
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

fn split_at_width(value: &str, width: usize) -> (String, &str) {
    let mut used = 0;
    let mut split_at = 0;
    for (index, character) in value.char_indices() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if split_at > 0 && used + character_width > width {
            break;
        }
        used += character_width;
        split_at = index + character.len_utf8();
        if used >= width {
            break;
        }
    }
    if split_at == 0 {
        let character = value.chars().next().expect("value is not empty");
        split_at = character.len_utf8();
    }
    (value[..split_at].to_string(), &value[split_at..])
}

fn truncate_to_width(value: &str, width: usize) -> String {
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

fn display_width(value: &str) -> usize {
    UnicodeWidthStr::width(value)
}

fn list_name_style(entry: &WorktreeInfo) -> Style {
    if entry.is_current {
        theme::branch::CURRENT
    } else {
        status_style(entry)
    }
}

fn status_style(entry: &WorktreeInfo) -> Style {
    match &entry.state {
        WorktreeState::Invalid(_) | WorktreeState::Unknown(_) => theme::styles::ERROR,
        WorktreeState::Missing | WorktreeState::Prunable => theme::styles::WARNING,
        WorktreeState::Valid if entry.is_locked() || entry.prunable => theme::styles::WARNING,
        WorktreeState::Valid => {
            if entry.cleanliness.is_clean() {
                theme::styles::SUCCESS
            } else {
                match &entry.cleanliness {
                    WorktreeCleanliness::Dirty(_) => theme::styles::WARNING,
                    WorktreeCleanliness::Unknown(_) => theme::styles::ERROR,
                    WorktreeCleanliness::Clean => theme::styles::SUCCESS,
                }
            }
        }
    }
}

fn state_style(state: &WorktreeState) -> Style {
    match state {
        WorktreeState::Valid => theme::styles::SUCCESS,
        WorktreeState::Missing | WorktreeState::Prunable => theme::styles::WARNING,
        WorktreeState::Invalid(_) | WorktreeState::Unknown(_) => theme::styles::ERROR,
    }
}

fn cleanliness_style(cleanliness: &WorktreeCleanliness) -> Style {
    match cleanliness {
        WorktreeCleanliness::Clean => theme::styles::SUCCESS,
        WorktreeCleanliness::Dirty(_) => theme::styles::WARNING,
        WorktreeCleanliness::Unknown(_) => theme::styles::ERROR,
    }
}

fn submodule_style(submodules: &WorktreeSubmodules) -> Style {
    match submodules {
        WorktreeSubmodules::None => theme::styles::TEXT,
        WorktreeSubmodules::Present => theme::styles::WARNING,
        WorktreeSubmodules::Unknown(_) => theme::styles::ERROR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{WorktreeDirtyReason, WorktreeIdentity, WorktreeState};
    use crate::tui::app::Msg;
    use ratatui::{Terminal, backend::TestBackend};

    fn rendered_text(app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
        terminal
            .draw(|frame| {
                crate::tui::ui::render(frame, app);
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
    fn diagnostic_value_style(lines: &[Line<'_>], label: &str) -> Style {
        lines
            .iter()
            .find(|line| line.to_string().starts_with(&format!("  {label}: ")))
            .expect("diagnostic field should exist")
            .spans[1]
            .style
    }

    #[test]
    fn renders_identity_path_and_diagnostics_fields() {
        let mut app = App::new(vec![], vec![]);
        app.update(Msg::SetWorktrees(vec![WorktreeInfo {
            identity: WorktreeIdentity::Linked {
                name: "feature with spaces".to_string(),
            },
            path: "/tmp/feature with spaces".into(),
            branch: Some("feature/test".to_string()),
            detached_short_sha: None,
            is_main: false,
            is_current: true,
            cleanliness: WorktreeCleanliness::Dirty(vec![WorktreeDirtyReason::Untracked]),
            lock_reason: Some("in use".to_string()),
            state: WorktreeState::Valid,
            prunable: false,
            submodules: WorktreeSubmodules::Present,
        }]));
        app.update(Msg::ToggleWorktreeView);

        let text = rendered_text(&app);
        assert!(text.contains("* feature with spaces"));
        assert!(text.contains("/tmp/feature with spaces"));
        assert!(text.contains("dirty"));
        assert!(text.contains("in use"));
        assert!(text.contains("Submodules: present"));
        assert!(text.contains("Status: valid / dirty"));

        let entry = app.selected_worktree().expect("selected worktree");
        let lines = worktree_diagnostic_lines(entry, 80);
        assert_eq!(
            diagnostic_value_style(&lines, "Current"),
            theme::branch::CURRENT
        );
        assert_eq!(
            diagnostic_value_style(&lines, "State"),
            theme::styles::SUCCESS
        );
        assert_eq!(
            diagnostic_value_style(&lines, "Cleanliness"),
            theme::styles::WARNING
        );
        assert!(text.contains("Worktree Details"));
        assert!(!text.contains("Work Item Details"));
        assert_eq!(
            diagnostic_value_style(&lines, "Lock"),
            theme::styles::WARNING
        );
        assert_eq!(
            diagnostic_value_style(&lines, "Submodules"),
            theme::styles::WARNING
        );
    }

    #[test]
    fn diagnostics_preserve_unknown_submodule_reason() {
        let entry = WorktreeInfo {
            identity: WorktreeIdentity::Linked {
                name: "missing".to_string(),
            },
            path: "/tmp/missing".into(),
            branch: Some("feature/missing".to_string()),
            detached_short_sha: None,
            is_main: false,
            is_current: false,
            cleanliness: WorktreeCleanliness::Unknown("path missing".to_string()),
            lock_reason: None,
            state: WorktreeState::Missing,
            prunable: true,
            submodules: WorktreeSubmodules::Unknown("repository unavailable".to_string()),
        };
        let text = worktree_diagnostic_lines(&entry, 80)
            .iter()
            .map(Line::to_string)
            .collect::<Vec<_>>()
            .join("\n");

        assert!(text.contains("Submodules: unknown: repository unavailable"));
    }

    #[test]
    fn inventory_rows_show_markers_path_head_and_status() {
        let mut app = App::new(vec![], vec![]);
        app.update(Msg::SetWorktrees(vec![
            WorktreeInfo {
                identity: WorktreeIdentity::Main,
                path: "/repo".into(),
                branch: Some("main".to_string()),
                detached_short_sha: None,
                is_main: true,
                is_current: true,
                cleanliness: WorktreeCleanliness::Clean,
                lock_reason: None,
                state: WorktreeState::Valid,
                prunable: false,
                submodules: WorktreeSubmodules::None,
            },
            WorktreeInfo {
                identity: WorktreeIdentity::Linked {
                    name: "missing-tree".to_string(),
                },
                path: "/tmp/missing tree/雪".into(),
                branch: None,
                detached_short_sha: Some("1234567".to_string()),
                is_main: false,
                is_current: false,
                cleanliness: WorktreeCleanliness::Unknown("path missing".to_string()),
                lock_reason: Some("owned by editor".to_string()),
                state: WorktreeState::Missing,
                prunable: true,
                submodules: WorktreeSubmodules::Unknown("path missing".to_string()),
            },
        ]));
        let mut terminal = Terminal::new(TestBackend::new(80, 14)).expect("terminal");

        terminal
            .draw(|frame| render_worktrees(frame, &app, frame.area()))
            .expect("draw");
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();

        assert!(text.contains("* main"));
        assert!(text.contains("HEAD: main"));
        assert!(text.contains("Path: /repo"));
        assert!(text.contains("missing-tree"));
        assert!(text.contains("HEAD: detached 1234567"));
        assert!(text.contains("Path: /tmp/missing tree/雪"));
        assert!(text.contains("Lock: owned by editor"));
        assert!(text.contains("Prunable: yes"));
        assert!(text.contains("Status: valid / clean"));
        assert!(text.contains("Status: missing / unknown"));
    }
    #[test]
    fn inventory_rows_show_compact_status_labels_and_colors() {
        let entry = WorktreeInfo {
            identity: WorktreeIdentity::Linked {
                name: "a-very-long-worktree-name-that-would-hide-state".to_string(),
            },
            path: "/tmp/a-very-long-worktree-name-that-would-hide-state".into(),
            branch: Some("feature/long".to_string()),
            detached_short_sha: None,
            is_main: false,
            is_current: false,
            cleanliness: WorktreeCleanliness::Clean,
            lock_reason: None,
            state: WorktreeState::Unknown("metadata unavailable".to_string()),
            prunable: false,
            submodules: WorktreeSubmodules::None,
        };

        assert_eq!(list_name_style(&entry), theme::styles::ERROR);

        let mut app = App::new(vec![], vec![]);
        app.update(Msg::SetWorktrees(vec![entry]));
        let mut terminal = Terminal::new(TestBackend::new(42, 6)).expect("terminal");
        terminal
            .draw(|frame| render_worktrees(frame, &app, frame.area()))
            .expect("draw");
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();

        assert!(text.contains("a-very-long-worktree-name"));
        assert!(text.contains("Status: unknown / clean"));
    }

    #[test]
    fn worktree_lines_fit_narrow_panels() {
        let entry = WorktreeInfo {
            identity: WorktreeIdentity::Linked {
                name: "invalid-linked".to_string(),
            },
            path: "/tmp/cazdo-111-test/invalid linked".into(),
            branch: Some("feature/invalid".to_string()),
            detached_short_sha: None,
            is_main: false,
            is_current: false,
            cleanliness: WorktreeCleanliness::Unknown(
                "could not inspect repository at /tmp/cazdo-111-test/invalid linked".to_string(),
            ),
            lock_reason: None,
            state: WorktreeState::Invalid(
                "could not find repository at /tmp/cazdo-111-test/invalid linked".to_string(),
            ),
            prunable: false,
            submodules: WorktreeSubmodules::Unknown(
                "could not inspect repository at /tmp/cazdo-111-test/invalid linked".to_string(),
            ),
        };
        let width = 32;

        for line in worktree_list_lines(&entry, width)
            .into_iter()
            .chain(worktree_diagnostic_lines(&entry, width))
        {
            assert!(
                line.width() <= width,
                "line width {} exceeds panel width {width}: {line:?}",
                line.width()
            );
        }
    }
}
