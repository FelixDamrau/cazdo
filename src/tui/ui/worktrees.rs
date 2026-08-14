use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use crate::git::{WorktreeCleanliness, WorktreeInfo, WorktreeState, WorktreeSubmodules};
use crate::tui::app::App;
use crate::tui::theme;

/// Render the inventory list. Selection is deliberately independent from the
/// branch list so returning to the branch view preserves its selection.
pub fn render_worktrees(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app.worktrees().iter().map(worktree_list_item).collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::ui::BORDER)
        .title(Line::from(vec![Span::styled(
            format!(" Worktrees ({}) ", app.worktrees().len()),
            theme::ui::TITLE,
        )]));
    let list = List::new(items)
        .block(block)
        .highlight_style(theme::ui::SELECTED.add_modifier(Modifier::BOLD))
        .highlight_symbol("\u{25BA} ");
    let mut state = ListState::default();
    if !app.worktrees().is_empty() {
        state.select(Some(app.worktree_selected_index()));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn worktree_list_item(entry: &WorktreeInfo) -> ListItem<'static> {
    let mut markers = Vec::new();
    if entry.is_current {
        markers.push("current");
    }
    if entry.is_main {
        markers.push("main");
    }
    let marker = if markers.is_empty() {
        String::new()
    } else {
        format!("[{}] ", markers.join("/"))
    };
    let lock = if entry.is_locked() { "/locked" } else { "" };
    let prunable = if entry.prunable { "/prunable" } else { "" };
    let mut lines = vec![
        Line::from(vec![
            Span::styled(marker, theme::styles::ACCENT),
            Span::styled(
                format!(
                    "{}/{}{}{}",
                    entry.state.label(),
                    cleanliness_label(&entry.cleanliness),
                    lock,
                    prunable
                ),
                status_style(entry),
            ),
            Span::styled(format!("  {}", entry.name()), theme::styles::TEXT),
        ]),
        Line::from(vec![
            Span::styled("  HEAD: ", theme::styles::MUTED),
            Span::styled(entry.ref_display(), theme::styles::TEXT),
        ]),
        Line::from(vec![
            Span::styled("  Path: ", theme::styles::MUTED),
            Span::styled(
                entry.path.to_string_lossy().into_owned(),
                theme::styles::TEXT,
            ),
        ]),
    ];
    if let Some(reason) = &entry.lock_reason {
        lines.push(Line::from(vec![
            Span::styled("  Lock: ", theme::styles::MUTED),
            Span::styled(reason.clone(), theme::styles::TEXT),
        ]));
    }
    ListItem::new(lines)
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

    frame.render_widget(Paragraph::new(worktree_diagnostic_lines(entry)), inner);
}

pub(super) fn worktree_diagnostic_lines(entry: &WorktreeInfo) -> Vec<Line<'static>> {
    vec![
        field_line("Name", entry.name()),
        field_line("Path", entry.path.to_string_lossy()),
        field_line("Identity", if entry.is_main { "main" } else { "linked" }),
        field_line("Branch/HEAD", entry.ref_display()),
        field_line("Current", if entry.is_current { "yes" } else { "no" }),
        field_line("State", state_detail(&entry.state)),
        field_line("Cleanliness", cleanliness_detail(&entry.cleanliness)),
        field_line("Lock", entry.lock_reason.as_deref().unwrap_or("unlocked")),
        field_line("Prunable", if entry.prunable { "yes" } else { "no" }),
        field_line("Submodules", submodule_detail(&entry.submodules)),
    ]
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

fn submodule_detail(submodules: &WorktreeSubmodules) -> String {
    match submodules {
        WorktreeSubmodules::None => "none".to_string(),
        WorktreeSubmodules::Present => "present".to_string(),
        WorktreeSubmodules::Unknown(error) => format!("unknown: {error}"),
    }
}

fn field_line(label: &str, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {label}: "), theme::styles::MUTED),
        Span::styled(value.into(), theme::styles::TEXT),
    ])
}

fn cleanliness_label(cleanliness: &WorktreeCleanliness) -> String {
    match cleanliness {
        WorktreeCleanliness::Clean => "clean".to_string(),
        WorktreeCleanliness::Dirty(_) => "dirty".to_string(),
        WorktreeCleanliness::Unknown(_) => "unknown".to_string(),
    }
}

fn status_style(entry: &WorktreeInfo) -> Style {
    if entry.state.is_valid() && entry.cleanliness.is_clean() && !entry.is_locked() {
        theme::styles::SUCCESS
    } else {
        theme::styles::WARNING
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
                let chunks = ratatui::layout::Layout::default()
                    .direction(ratatui::layout::Direction::Horizontal)
                    .constraints([
                        ratatui::layout::Constraint::Percentage(40),
                        ratatui::layout::Constraint::Percentage(60),
                    ])
                    .split(frame.area());
                render_worktrees(frame, app, chunks[0]);
                render_worktree_details(frame, app, chunks[1]);
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
        assert!(text.contains("feature with spaces"));
        assert!(text.contains("/tmp/feature with spaces"));
        assert!(text.contains("dirty"));
        assert!(text.contains("in use"));
        assert!(text.contains("Submodules: present"));
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
        let text = worktree_diagnostic_lines(&entry)
            .iter()
            .map(Line::to_string)
            .collect::<Vec<_>>()
            .join("\n");

        assert!(text.contains("Submodules: unknown: repository unavailable"));
    }

    #[test]
    fn inventory_rows_show_markers_path_head_and_stale_state() {
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

        assert!(text.contains("[current/main] valid/clean"));
        assert!(text.contains("HEAD: main"));
        assert!(text.contains("Path: /repo"));
        assert!(text.contains("missing/unknown/locked/prunable"));
        assert!(text.contains("HEAD: detached 1234567"));
        assert!(text.contains("Path: /tmp/missing tree/雪"));
        assert!(text.contains("Lock: owned by editor"));
    }
    #[test]
    fn keeps_status_visible_before_long_worktree_identity() {
        let mut app = App::new(vec![], vec![]);
        app.update(Msg::SetWorktrees(vec![WorktreeInfo {
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
        }]));

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

        assert!(text.contains("unknown/clean"));
    }
}
