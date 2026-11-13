//! Drawing only. No storage I/O or network access is performed by the TUI.

use chrono::{Duration as CalendarDuration, Local, NaiveDate};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Padding, Paragraph, Wrap},
};

use super::{
    App, Editor, Focus, OVERVIEW_SECTION, PROJECT_SECTION, SECTIONS, SETTINGS_ITEMS, TASK_SECTION,
};

fn login_name() -> String {
    let raw = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "local user".into());
    // Environment values are untrusted terminal text. Do not print control
    // characters, ANSI escapes or an unbounded string.
    let clean: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        .take(32)
        .collect();
    if clean.is_empty() {
        "local user".into()
    } else {
        clean
    }
}

fn clock_text() -> String {
    Local::now()
        .format("%Y/%m/%d | %I:%M %p")
        .to_string()
        .to_ascii_lowercase()
}

fn draw_header(frame: &mut Frame<'_>, area: ratatui::layout::Rect) {
    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let halves = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);
    frame.render_widget(
        Paragraph::new(format!(" {}", login_name())).style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        halves[0],
    );
    frame.render_widget(
        Paragraph::new(format!("{} ", clock_text()))
            .alignment(Alignment::Right)
            .style(Style::default().fg(Color::Cyan)),
        halves[1],
    );
}

fn draw_navigation(frame: &mut Frame<'_>, app: &App, area: ratatui::layout::Rect) {
    let (items, selected, title) = if app.settings_open {
        (
            SETTINGS_ITEMS
                .iter()
                .map(|item| ListItem::new(*item))
                .collect::<Vec<_>>(),
            app.settings_selected,
            " Settings ",
        )
    } else {
        (
            app.order()
                .iter()
                .flat_map(|&id| {
                    if id == super::SETTINGS_SECTION {
                        vec![ListItem::new(""), ListItem::new(SECTIONS[id])]
                    } else {
                        vec![ListItem::new(SECTIONS[id])]
                    }
                })
                .collect::<Vec<_>>(),
            app.order()
                .iter()
                .position(|&id| id == app.selected)
                .unwrap_or(0)
                + usize::from(app.selected == super::SETTINGS_SECTION),
            " Navigation ",
        )
    };
    let highlighted = if app.focus == Focus::Navigation {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        // Retain module selection after the keyboard focus moves right.
        // This color is NOT a second focus indicator.
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(highlighted)
        .highlight_symbol(if app.focus == Focus::Navigation {
            "> "
        } else {
            "  "
        });
    let mut state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn overview_lines(app: &App) -> Text<'static> {
    let today = Local::now().date_naive();
    let due_soon = today + CalendarDuration::days(7);
    let remaining: Vec<_> = app.tasks.iter().filter(|task| !task.completed).collect();
    let overdue = remaining
        .iter()
        .filter(|task| {
            task.due_date
                .as_deref()
                .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
                .is_some_and(|date| date < today)
        })
        .count();

    let task_label = if app.filter.is_some() {
        "Tasks (filtered view)"
    } else {
        "Tasks"
    };
    let mut lines: Vec<Line<'static>> = vec![
        Line::from("Welcome to POS."),
        Line::from(""),
        Line::from(format!(
            "{task_label}: {} open  |  {} overdue  |  {} projects",
            remaining.len(),
            overdue,
            app.projects.len()
        )),
        Line::from(""),
        Line::from("Upcoming deadlines"),
    ];

    let mut due: Vec<_> = remaining
        .iter()
        .filter_map(|task| {
            let date = task.due_date.as_deref()?;
            let parsed = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
            if parsed <= due_soon {
                Some((parsed, task))
            } else {
                None
            }
        })
        .collect();
    due.sort_by_key(|(date, task)| (*date, task.id));

    if due.is_empty() {
        lines.push(Line::from(
            "  No overdue or upcoming tasks in the next 7 days",
        ));
    } else {
        for (date, task) in due.into_iter().take(4) {
            let note = if date < today { "overdue" } else { "due" };
            lines.push(Line::from(format!(
                "  {}  {}  ({note})",
                date.format("%Y-%m-%d"),
                task.title
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from("Projects"));
    if app.projects.is_empty() {
        lines.push(Line::from("  No projects yet"));
    } else {
        for project in app
            .projects
            .iter()
            .filter(|project| !project.archived)
            .take(4)
        {
            lines.push(Line::from(format!("  {}", project.name)));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(
        "Select Tasks or Projects to manage your local work.",
    ));
    Text::from(lines)
}

fn workspace_title(app: &App) -> String {
    if app.settings_open {
        let selected = SETTINGS_ITEMS
            .get(app.settings_selected)
            .copied()
            .unwrap_or("");
        if selected == "Back" {
            "Settings".into()
        } else {
            format!("Settings - {selected}")
        }
    } else if app.selected == TASK_SECTION {
        match app.filter.as_deref() {
            Some(slug) => {
                let name = app
                    .projects
                    .iter()
                    .find(|project| project.slug == slug)
                    .map(|project| project.name.as_str())
                    .unwrap_or(slug);
                format!("Workspace - Tasks / {name}")
            }
            None => "Workspace - Tasks / All".into(),
        }
    } else {
        format!("Workspace - {}", SECTIONS[app.selected])
    }
}

fn settings_lines(app: &App) -> Text<'static> {
    match app.settings_selected {
        0 => Text::from(vec![
            Line::from("Settings"),
            Line::from(""),
            Line::from("Select a setting on the left, or choose Back."),
        ]),
        2 => Text::from(vec![
            Line::from("Git"),
            Line::from(""),
            Line::from("Git repository integration is planned, not implemented."),
        ]),
        3 => Text::from(vec![
            Line::from("GitHub"),
            Line::from(""),
            Line::from("GitHub integration is planned, not implemented."),
        ]),
        4 => Text::from(vec![
            Line::from("GitLab"),
            Line::from(""),
            Line::from("GitLab integration is planned, not implemented."),
        ]),
        5 => Text::from(vec![
            Line::from("User Account"),
            Line::from(""),
            Line::from(format!("Local OS user: {}", login_name())),
            Line::from(""),
            Line::from("The local TUI does not have server sign-in yet."),
        ]),
        6 => Text::from(vec![
            Line::from("General"),
            Line::from(""),
            Line::from("Overview and Settings stay fixed in the sidebar."),
            Line::from("Other modules rank by visits in the past 7 days."),
            Line::from("Module order refreshes when POS is launched."),
        ]),
        7 => Text::from(vec![
            Line::from("About POS"),
            Line::from(""),
            Line::from(format!("Version {}", env!("CARGO_PKG_VERSION"))),
            Line::from("Personal Organisation System"),
            Line::from("Local-first prototype"),
        ]),
        _ => Text::from("Settings"),
    }
}

fn draw_workspace(frame: &mut Frame<'_>, app: &App, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", workspace_title(app)))
        .padding(Padding::horizontal(2));

    if app.show_help {
        let leading = if app.focus == Focus::Navigation {
            " "
        } else {
            "> "
        };
        let help = Text::from(vec![
            Line::from(format!("{leading}Keyboard shortcuts")),
            Line::from(""),
            Line::from("j / k          Move in current list"),
            Line::from("Enter / Tab    Open / focus section"),
            Line::from("Esc            Back / close help / quit"),
            Line::from("?              Toggle help"),
            Line::from("r              Refresh local data"),
            Line::from(""),
            Line::from("Tasks: a add  e edit  d due  p assign project"),
            Line::from("Tasks: u unassign  f filter  Space toggle  x delete"),
            Line::from("Projects: a add  e rename  Space archive  x delete"),
            Line::from("Projects: Enter open filtered Tasks"),
            Line::from(""),
            Line::from("Settings: Enter select  Esc return"),
            Line::from("q / Ctrl+C     Quit POS"),
        ]);
        frame.render_widget(
            Paragraph::new(help).block(block).wrap(Wrap { trim: true }),
            area,
        );
    } else if app.settings_open {
        frame.render_widget(
            Paragraph::new(settings_lines(app))
                .block(block)
                .wrap(Wrap { trim: true }),
            area,
        );
    } else if app.selected == TASK_SECTION {
        let items: Vec<_> = if app.tasks.is_empty() {
            vec![ListItem::new("No tasks yet - press a to add")]
        } else {
            app.tasks
                .iter()
                .map(|task| {
                    let mark = if task.completed { "x" } else { " " };
                    let due = task
                        .due_date
                        .as_deref()
                        .map(|date| format!(" due {date}"))
                        .unwrap_or_default();
                    let project = task.project_id.and_then(|id| {
                        app.projects
                            .iter()
                            .find(|project| project.id == id)
                            .map(|project| project.slug.as_str())
                    });
                    let project = project.map(|name| format!(" [{name}]")).unwrap_or_default();
                    ListItem::new(format!(
                        "[{mark}] #{} {}{due}{project}",
                        task.id, task.title
                    ))
                })
                .collect()
        };
        let list = List::new(items)
            .block(block)
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(if app.focus == Focus::Tasks {
                "> "
            } else {
                "  "
            });
        let mut state = ListState::default().with_selected(
            if app.focus == Focus::Tasks || !app.tasks.is_empty() {
                Some(app.task_selected)
            } else {
                None
            },
        );
        frame.render_stateful_widget(list, area, &mut state);
    } else if app.selected == PROJECT_SECTION {
        let items: Vec<_> = if app.projects.is_empty() {
            vec![ListItem::new("No projects yet - press a to add")]
        } else {
            app.projects
                .iter()
                .map(|project| {
                    let mark = if project.archived {
                        "archived"
                    } else {
                        "active"
                    };
                    ListItem::new(format!("{} [{}] ({mark})", project.name, project.slug))
                })
                .collect()
        };
        let list = List::new(items)
            .block(block)
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(if app.focus == Focus::Projects {
                "> "
            } else {
                "  "
            });
        let mut state = ListState::default().with_selected(
            if app.focus == Focus::Projects || !app.projects.is_empty() {
                Some(app.project_selected)
            } else {
                None
            },
        );
        frame.render_stateful_widget(list, area, &mut state);
    } else if app.selected == OVERVIEW_SECTION {
        frame.render_widget(
            Paragraph::new(overview_lines(app))
                .block(block)
                .wrap(Wrap { trim: true }),
            area,
        );
    } else {
        let text = Text::from(vec![
            Line::from(SECTIONS[app.selected]),
            Line::from(""),
            Line::from("This module is planned, but has not been implemented."),
        ]);
        frame.render_widget(
            Paragraph::new(text).block(block).wrap(Wrap { trim: true }),
            area,
        );
    }
}

fn status_line(app: &App) -> String {
    match &app.editor {
        Some(Editor::AddTask) => format!(" New task title: {}_", app.input),
        Some(Editor::EditTask(id)) => format!(" Edit task #{id}: {}_", app.input),
        Some(Editor::TaskDue(id)) => format!(" Due for #{id} (YYYY-MM-DD): {}_", app.input),
        Some(Editor::TaskProject(id)) => {
            format!(" Project slug for task #{id}: {}_", app.input)
        }
        Some(Editor::TaskFilter) => format!(" Filter by project slug (blank=all): {}_", app.input),
        Some(Editor::DeleteTask(id)) => format!(" Delete task #{id} permanently? y/n"),
        Some(Editor::AddProject) => format!(" New project name: {}_", app.input),
        Some(Editor::RenameProject(id)) => format!(" Rename project #{id}: {}_", app.input),
        Some(Editor::DeleteProject(id)) => {
            format!(" Delete project #{id}? Its tasks remain unassigned. y/n")
        }
        None => format!(" {}", app.status),
    }
}

fn keys_line(app: &App) -> &'static str {
    if app.editor.is_some() {
        " Enter Save/confirm   │   Esc Cancel   │   Backspace Erase "
    } else if app.show_help {
        " ? Close help   │   Esc Back   │   q Quit "
    } else if app.settings_open {
        " j/k Navigate   │   Enter Select   │   Esc Back   │   ? Help   │   q Quit "
    } else if app.focus == Focus::Tasks {
        " a Add   │   e Edit   │   d Due   │   p Project   │   Space Toggle   │   x Delete   │   Esc Back "
    } else if app.focus == Focus::Projects {
        " a Add   │   e Rename   │   Space Archive   │   x Delete   │   Enter Open   │   Esc Back "
    } else {
        " j/k Navigate   │   Enter Select   │   Tab Focus   │   ? Help   │   q Quit "
    }
}

pub(super) fn render(frame: &mut Frame<'_>, app: &App) {
    let screen = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(frame.area());
    draw_header(frame, screen[0]);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(22), Constraint::Min(16)])
        .split(screen[1]);
    draw_navigation(frame, app, columns[0]);
    draw_workspace(frame, app, columns[1]);

    frame.render_widget(
        Paragraph::new(status_line(app))
            .style(if app.is_error {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Green)
            })
            .block(Block::default().borders(Borders::ALL).title(" Status ")),
        screen[2],
    );
    frame.render_widget(
        Paragraph::new(keys_line(app))
            .block(Block::default().borders(Borders::ALL).title(" Keys "))
            .style(Style::default().fg(Color::Yellow)),
        screen[3],
    );
}
