//! Ratatui presentation and keyboard handling. A worker in pos-cli owns I/O.

use std::{
    io,
    sync::mpsc::{Receiver, Sender},
    time::Duration,
};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use pos_projects::Project;
use pos_tasks::Task;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

const SECTIONS: &[&str] = &[
    "Overview", "Tasks", "Projects", "Calendar", "Mail", "Notes", "Files", "Contacts", "Time",
    "Chat", "Search", "Git", "GitHub", "GitLab",
];
const TASK_SECTION: usize = 1;
const PROJECT_SECTION: usize = 2;

#[derive(Debug)]
pub enum UiRequest {
    Refresh,
    Filter(Option<String>),
    Add(String),
    EditTitle { id: i64, title: String },
    SetDue { id: i64, due_date: Option<String> },
    Toggle { id: i64, completed: bool },
    Delete(i64),
    AssignProject { id: i64, slug: Option<String> },
    AddProject(String),
    RenameProject { id: i64, name: String },
    ArchiveProject { id: i64, archived: bool },
    DeleteProject(i64),
}

pub struct UiResponse {
    pub tasks: Option<Vec<Task>>,
    pub projects: Option<Vec<Project>>,
    pub filter: Option<String>,
    pub status: String,
    pub is_error: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
enum Focus {
    #[default]
    Navigation,
    Tasks,
    Projects,
}

#[derive(Debug)]
enum Editor {
    AddTask,
    EditTask(i64),
    TaskDue(i64),
    TaskProject(i64),
    TaskFilter,
    DeleteTask(i64),
    AddProject,
    RenameProject(i64),
    DeleteProject(i64),
}

#[derive(Debug, Default)]
struct App {
    selected: usize,
    task_selected: usize,
    project_selected: usize,
    tasks: Vec<Task>,
    projects: Vec<Project>,
    filter: Option<String>,
    focus: Focus,
    editor: Option<Editor>,
    input: String,
    status: String,
    is_error: bool,
    show_help: bool,
}

enum KeyOutcome {
    Nothing,
    Quit,
    Request(UiRequest),
}

impl App {
    fn accept_response(&mut self, result: UiResponse) {
        if let Some(tasks) = result.tasks {
            self.tasks = tasks;
            self.task_selected = self.task_selected.min(self.tasks.len().saturating_sub(1));
        }
        if let Some(projects) = result.projects {
            self.projects = projects;
            self.project_selected = self
                .project_selected
                .min(self.projects.len().saturating_sub(1));
        }
        if !result.is_error {
            self.filter = result.filter;
        }
        self.status = result.status;
        self.is_error = result.is_error;
    }

    fn selected_task(&self) -> Option<&Task> {
        self.tasks.get(self.task_selected)
    }

    fn selected_project(&self) -> Option<&Project> {
        self.projects.get(self.project_selected)
    }

    fn begin_input(&mut self, editor: Editor, value: String) {
        self.input = value;
        self.editor = Some(editor);
    }

    fn current_focus(&self) -> Focus {
        match self.selected {
            TASK_SECTION => Focus::Tasks,
            PROJECT_SECTION => Focus::Projects,
            _ => Focus::Navigation,
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> KeyOutcome {
        if key.kind != KeyEventKind::Press {
            return KeyOutcome::Nothing;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return KeyOutcome::Quit;
        }

        if let Some(editor) = self.editor.take() {
            return self.handle_editor_key(editor, key);
        }

        match key.code {
            KeyCode::Char('q') => KeyOutcome::Quit,
            KeyCode::Esc => {
                if self.focus != Focus::Navigation {
                    self.focus = Focus::Navigation;
                    KeyOutcome::Nothing
                } else {
                    KeyOutcome::Quit
                }
            }
            KeyCode::Char('?') => {
                self.show_help = !self.show_help;
                KeyOutcome::Nothing
            }
            KeyCode::Char('r') => KeyOutcome::Request(UiRequest::Refresh),
            KeyCode::Tab | KeyCode::Right => {
                self.focus = if self.focus == Focus::Navigation {
                    self.current_focus()
                } else {
                    Focus::Navigation
                };
                self.show_help = false;
                KeyOutcome::Nothing
            }
            KeyCode::Left => {
                self.focus = Focus::Navigation;
                KeyOutcome::Nothing
            }
            KeyCode::Enter if self.focus == Focus::Navigation => {
                self.focus = self.current_focus();
                self.show_help = false;
                KeyOutcome::Nothing
            }
            KeyCode::Enter if self.focus == Focus::Projects => {
                if let Some(project) = self.selected_project() {
                    let slug = project.slug.clone();
                    self.selected = TASK_SECTION;
                    self.focus = Focus::Tasks;
                    self.task_selected = 0;
                    KeyOutcome::Request(UiRequest::Filter(Some(slug)))
                } else {
                    KeyOutcome::Nothing
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                match self.focus {
                    Focus::Navigation => {
                        self.selected = (self.selected + 1) % SECTIONS.len();
                        self.show_help = false;
                    }
                    Focus::Tasks if !self.tasks.is_empty() => {
                        self.task_selected = (self.task_selected + 1) % self.tasks.len();
                    }
                    Focus::Projects if !self.projects.is_empty() => {
                        self.project_selected = (self.project_selected + 1) % self.projects.len();
                    }
                    _ => {}
                }
                KeyOutcome::Nothing
            }
            KeyCode::Up | KeyCode::Char('k') => {
                match self.focus {
                    Focus::Navigation => {
                        self.selected = (self.selected + SECTIONS.len() - 1) % SECTIONS.len();
                        self.show_help = false;
                    }
                    Focus::Tasks if !self.tasks.is_empty() => {
                        self.task_selected =
                            (self.task_selected + self.tasks.len() - 1) % self.tasks.len();
                    }
                    Focus::Projects if !self.projects.is_empty() => {
                        self.project_selected =
                            (self.project_selected + self.projects.len() - 1) % self.projects.len();
                    }
                    _ => {}
                }
                KeyOutcome::Nothing
            }
            _ if !self.show_help && self.focus == Focus::Tasks => self.handle_task_key(key),
            _ if !self.show_help && self.focus == Focus::Projects => self.handle_project_key(key),
            _ => KeyOutcome::Nothing,
        }
    }

    fn handle_task_key(&mut self, key: KeyEvent) -> KeyOutcome {
        match key.code {
            KeyCode::Char('a') => self.begin_input(Editor::AddTask, String::new()),
            KeyCode::Char('e') => {
                if let Some(task) = self.selected_task() {
                    self.begin_input(Editor::EditTask(task.id), task.title.clone());
                }
            }
            KeyCode::Char('d') => {
                if let Some(task) = self.selected_task() {
                    self.begin_input(
                        Editor::TaskDue(task.id),
                        task.due_date.clone().unwrap_or_default(),
                    );
                }
            }
            KeyCode::Char('p') => {
                if let Some(task) = self.selected_task() {
                    let name = self
                        .projects
                        .iter()
                        .find(|p| Some(p.id) == task.project_id)
                        .map(|p| p.slug.clone())
                        .unwrap_or_default();
                    self.begin_input(Editor::TaskProject(task.id), name);
                }
            }
            KeyCode::Char('u') => {
                if let Some(task) = self.selected_task() {
                    return KeyOutcome::Request(UiRequest::AssignProject {
                        id: task.id,
                        slug: None,
                    });
                }
            }
            KeyCode::Char('f') => {
                self.begin_input(Editor::TaskFilter, self.filter.clone().unwrap_or_default())
            }
            KeyCode::Char('x') => {
                if let Some(task) = self.selected_task() {
                    self.begin_input(Editor::DeleteTask(task.id), String::new());
                }
            }
            KeyCode::Char(' ') => {
                if let Some(task) = self.selected_task() {
                    return KeyOutcome::Request(UiRequest::Toggle {
                        id: task.id,
                        completed: !task.completed,
                    });
                }
            }
            _ => {}
        }
        KeyOutcome::Nothing
    }

    fn handle_project_key(&mut self, key: KeyEvent) -> KeyOutcome {
        match key.code {
            KeyCode::Char('a') => self.begin_input(Editor::AddProject, String::new()),
            KeyCode::Char('e') => {
                if let Some(project) = self.selected_project() {
                    self.begin_input(Editor::RenameProject(project.id), project.name.clone());
                }
            }
            KeyCode::Char(' ') => {
                if let Some(project) = self.selected_project() {
                    return KeyOutcome::Request(UiRequest::ArchiveProject {
                        id: project.id,
                        archived: !project.archived,
                    });
                }
            }
            KeyCode::Char('x') => {
                if let Some(project) = self.selected_project() {
                    self.begin_input(Editor::DeleteProject(project.id), String::new());
                }
            }
            _ => {}
        }
        KeyOutcome::Nothing
    }

    fn handle_editor_key(&mut self, editor: Editor, key: KeyEvent) -> KeyOutcome {
        match editor {
            Editor::DeleteTask(id) | Editor::DeleteProject(id) => {
                return match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        self.status = format!("Deleting item #{id}...");
                        if matches!(editor, Editor::DeleteTask(_)) {
                            KeyOutcome::Request(UiRequest::Delete(id))
                        } else {
                            KeyOutcome::Request(UiRequest::DeleteProject(id))
                        }
                    }
                    KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => KeyOutcome::Nothing,
                    _ => {
                        self.editor = Some(editor);
                        KeyOutcome::Nothing
                    }
                };
            }
            _ => {}
        }

        match key.code {
            KeyCode::Esc => {
                self.input.clear();
                KeyOutcome::Nothing
            }
            KeyCode::Backspace => {
                self.input.pop();
                self.editor = Some(editor);
                KeyOutcome::Nothing
            }
            KeyCode::Enter => {
                let value = std::mem::take(&mut self.input);
                let request = match editor {
                    Editor::AddTask => UiRequest::Add(value),
                    Editor::EditTask(id) => UiRequest::EditTitle { id, title: value },
                    Editor::TaskDue(id) => UiRequest::SetDue {
                        id,
                        due_date: if value.is_empty() { None } else { Some(value) },
                    },
                    Editor::TaskProject(id) => UiRequest::AssignProject {
                        id,
                        slug: if value.is_empty() { None } else { Some(value) },
                    },
                    Editor::TaskFilter => {
                        UiRequest::Filter(if value.is_empty() { None } else { Some(value) })
                    }
                    Editor::AddProject => UiRequest::AddProject(value),
                    Editor::RenameProject(id) => UiRequest::RenameProject { id, name: value },
                    Editor::DeleteTask(_) | Editor::DeleteProject(_) => unreachable!(),
                };
                self.status = "Saving locally...".into();
                KeyOutcome::Request(request)
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.push(ch);
                self.editor = Some(editor);
                KeyOutcome::Nothing
            }
            _ => {
                self.editor = Some(editor);
                KeyOutcome::Nothing
            }
        }
    }
}

/// Run interactive POS. The caller owns a background worker that performs
/// storage operations and sends updated Tasks and Projects lists.
pub fn run(requests: Sender<UiRequest>, responses: Receiver<UiResponse>) -> io::Result<()> {
    requests
        .send(UiRequest::Refresh)
        .map_err(|error| io::Error::new(io::ErrorKind::BrokenPipe, error.to_string()))?;

    ratatui::run(|terminal| {
        let mut app = App {
            status: "Loading local tasks and projects...".into(),
            ..App::default()
        };
        let mut needs_draw = true;
        loop {
            while let Ok(result) = responses.try_recv() {
                app.accept_response(result);
                needs_draw = true;
            }
            if needs_draw {
                terminal.draw(|frame| render(frame, &app))?;
                needs_draw = false;
            }

            if event::poll(Duration::from_millis(50))? {
                match event::read()? {
                    Event::Key(key) => {
                        let outcome = app.handle_key(key);
                        match outcome {
                            KeyOutcome::Quit => return Ok(()),
                            KeyOutcome::Request(request) => {
                                requests.send(request).map_err(|error| {
                                    io::Error::new(io::ErrorKind::BrokenPipe, error.to_string())
                                })?
                            }
                            KeyOutcome::Nothing => {}
                        }
                        needs_draw = true;
                    }
                    Event::Resize(_, _) => needs_draw = true,
                    _ => {}
                }
            }
        }
    })
}

fn render(frame: &mut Frame<'_>, app: &App) {
    let screen = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(frame.area());

    let header = Paragraph::new(
        Line::from(" Personal Organisation System   |   v0.0.0   |   local-first ").style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .block(Block::default().borders(Borders::ALL).title(" POS "));
    frame.render_widget(header, screen[0]);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(22), Constraint::Min(16)])
        .split(screen[1]);

    let items = SECTIONS.iter().map(|section| ListItem::new(*section));
    let navigation = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Navigation "))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(if app.focus == Focus::Navigation {
            "> "
        } else {
            "  "
        });
    let mut state = ListState::default().with_selected(Some(app.selected));
    frame.render_stateful_widget(navigation, columns[0], &mut state);

    if app.show_help {
        let help = Text::from(vec![
            Line::from("Keyboard shortcuts"),
            Line::from(""),
            Line::from(" j / k         Move in current list"),
            Line::from(" Enter / Tab   Focus Tasks or Projects"),
            Line::from(" Esc           Back to sidebar / quit"),
            Line::from(" ?             Toggle help"),
            Line::from(" r             Refresh local data"),
            Line::from(""),
            Line::from(" Tasks: a add, e edit, d due, p assign project"),
            Line::from(" Tasks: u unassign, f filter, Space toggle, x delete"),
            Line::from(""),
            Line::from(" Projects: a add, e rename, Space archive"),
            Line::from(" Projects: x delete, Enter filter Tasks"),
            Line::from(""),
            Line::from(" Clear task filter with f + blank Enter"),
            Line::from(" q / Ctrl+C    Quit POS"),
        ]);
        frame.render_widget(
            Paragraph::new(help)
                .block(Block::default().borders(Borders::ALL).title(" Help "))
                .wrap(Wrap { trim: true }),
            columns[1],
        );
    } else if app.selected == TASK_SECTION {
        let items = app.tasks.iter().map(|task| {
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
                    .map(|p| p.slug.as_str())
            });
            let project = project.map(|p| format!(" [{p}]")).unwrap_or_default();
            ListItem::new(format!(
                "[{mark}] #{} {}{due}{project}",
                task.id, task.title
            ))
        });
        let title = app
            .filter
            .as_deref()
            .map(|slug| format!(" Tasks / {slug} "))
            .unwrap_or_else(|| " Local Tasks / All ".into());
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(title))
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");
        let mut state = ListState::default().with_selected(if app.tasks.is_empty() {
            None
        } else {
            Some(app.task_selected)
        });
        frame.render_stateful_widget(list, columns[1], &mut state);
    } else if app.selected == PROJECT_SECTION {
        let items = app.projects.iter().map(|project| {
            let mark = if project.archived {
                "archived"
            } else {
                "active"
            };
            ListItem::new(format!("{} [{}] ({mark})", project.name, project.slug))
        });
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Local Projects "),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");
        let mut state = ListState::default().with_selected(if app.projects.is_empty() {
            None
        } else {
            Some(app.project_selected)
        });
        frame.render_stateful_widget(list, columns[1], &mut state);
    } else {
        let body = if app.selected == 0 {
            Text::from(vec![
                Line::from("Welcome to POS."),
                Line::from(""),
                Line::from("Tasks and Projects persist locally in SQLite."),
                Line::from(""),
                Line::from(format!("{} tasks in current view", app.tasks.len())),
                Line::from(format!("{} projects", app.projects.len())),
                Line::from(""),
                Line::from("Navigate to Tasks or Projects and press Enter."),
                Line::from("All other modules remain placeholders."),
            ])
        } else {
            Text::from(vec![
                Line::from(SECTIONS[app.selected]),
                Line::from(""),
                Line::from("This module is planned, but has not been implemented."),
            ])
        };
        frame.render_widget(
            Paragraph::new(body)
                .block(Block::default().borders(Borders::ALL).title(" Workspace "))
                .wrap(Wrap { trim: true }),
            columns[1],
        );
    }

    let status = match &app.editor {
        Some(Editor::AddTask) => format!(" New task title: {}_", app.input),
        Some(Editor::EditTask(id)) => format!(" Edit task #{id}: {}_", app.input),
        Some(Editor::TaskDue(id)) => format!(" Due for #{id} (YYYY-MM-DD): {}_", app.input),
        Some(Editor::TaskProject(id)) => format!(" Project slug for task #{id}: {}_", app.input),
        Some(Editor::TaskFilter) => format!(" Filter by project slug (blank=all): {}_", app.input),
        Some(Editor::DeleteTask(id)) => format!(" Delete task #{id} permanently? y/n"),
        Some(Editor::AddProject) => format!(" New project name: {}_", app.input),
        Some(Editor::RenameProject(id)) => format!(" Rename project #{id}: {}_", app.input),
        Some(Editor::DeleteProject(id)) => {
            format!(" Delete project #{id}? Its tasks remain unassigned. y/n")
        }
        None => format!(" {}", app.status),
    };
    frame.render_widget(
        Paragraph::new(status)
            .style(if app.is_error {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Green)
            })
            .block(Block::default().borders(Borders::ALL).title(" Status ")),
        screen[2],
    );

    let footer = if app.editor.is_some() {
        " Enter: save/confirm   Esc: cancel   Backspace: erase "
    } else if app.focus == Focus::Tasks {
        " a add e edit d due p project u clear f filter Space done x delete Esc back "
    } else if app.focus == Focus::Projects {
        " a add e rename Space archive x delete Enter open Tasks Esc back "
    } else {
        " j/k navigate Enter/Tab focus Tasks/Projects ?: help q/Esc quit "
    };
    frame.render_widget(
        Paragraph::new(footer)
            .block(Block::default().borders(Borders::ALL).title(" Keys "))
            .style(Style::default().fg(Color::Yellow)),
        screen[3],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn task() -> Task {
        Task {
            id: 7,
            title: "Finish report".into(),
            due_date: Some("2026-12-17".into()),
            project_id: Some(5),
            completed: false,
            created_at: "2026-11-12T00:00:00Z".into(),
            updated_at: "2026-11-12T00:00:00Z".into(),
        }
    }

    fn project() -> Project {
        Project {
            id: 5,
            name: "University".into(),
            slug: "university".into(),
            archived: false,
            created_at: "2026-11-12T00:00:00Z".into(),
            updated_at: "2026-11-12T00:00:00Z".into(),
        }
    }

    #[test]
    fn navigation_wraps_both_directions() {
        let mut app = App::default();
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.selected, SECTIONS.len() - 1);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn help_toggles_without_changing_selection() {
        let mut app = App::default();
        app.handle_key(key(KeyCode::Char('?')));
        assert!(app.show_help);
        app.handle_key(key(KeyCode::Char('?')));
        assert!(!app.show_help);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn q_and_escape_quit_from_navigation() {
        let mut app = App::default();
        assert!(matches!(
            app.handle_key(key(KeyCode::Char('q'))),
            KeyOutcome::Quit
        ));
        assert!(matches!(
            app.handle_key(key(KeyCode::Esc)),
            KeyOutcome::Quit
        ));
    }

    #[test]
    fn key_release_does_not_trigger_navigation() {
        let mut app = App::default();
        let mut release = key(KeyCode::Down);
        release.kind = KeyEventKind::Release;
        app.handle_key(release);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn tasks_can_be_focused_and_toggled() {
        let mut app = App {
            selected: TASK_SECTION,
            tasks: vec![task()],
            ..App::default()
        };
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.focus, Focus::Tasks);
        assert!(matches!(
            app.handle_key(key(KeyCode::Char(' '))),
            KeyOutcome::Request(UiRequest::Toggle {
                id: 7,
                completed: true
            })
        ));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.focus, Focus::Navigation);
    }

    #[test]
    fn editing_text_accepts_q() {
        let mut app = App {
            selected: TASK_SECTION,
            focus: Focus::Tasks,
            ..App::default()
        };
        app.handle_key(key(KeyCode::Char('a')));
        app.handle_key(key(KeyCode::Char('q')));
        assert!(matches!(
            app.handle_key(key(KeyCode::Enter)),
            KeyOutcome::Request(UiRequest::Add(ref title)) if title=="q"
        ));
    }

    #[test]
    fn projects_open_filtered_tasks_without_losing_context() {
        let mut app = App {
            selected: PROJECT_SECTION,
            projects: vec![project()],
            ..App::default()
        };
        app.handle_key(key(KeyCode::Enter)); // focus projects
        assert_eq!(app.focus, Focus::Projects);
        assert!(matches!(
            app.handle_key(key(KeyCode::Enter)),
            KeyOutcome::Request(UiRequest::Filter(Some(ref slug))) if slug=="university"
        ));
        assert_eq!(app.selected, TASK_SECTION);
        assert_eq!(app.focus, Focus::Tasks);
    }

    #[test]
    fn project_archiving_does_not_delete() {
        let mut app = App {
            selected: PROJECT_SECTION,
            focus: Focus::Projects,
            projects: vec![project()],
            ..App::default()
        };
        assert!(matches!(
            app.handle_key(key(KeyCode::Char(' '))),
            KeyOutcome::Request(UiRequest::ArchiveProject {
                id: 5,
                archived: true
            })
        ));
    }

    #[test]
    fn project_delete_needs_y_confirmation() {
        let mut app = App {
            selected: PROJECT_SECTION,
            focus: Focus::Projects,
            projects: vec![project()],
            ..App::default()
        };
        app.handle_key(key(KeyCode::Char('x')));
        assert!(matches!(
            app.handle_key(key(KeyCode::Char('n'))),
            KeyOutcome::Nothing
        ));
        app.handle_key(key(KeyCode::Char('x')));
        assert!(matches!(
            app.handle_key(key(KeyCode::Char('y'))),
            KeyOutcome::Request(UiRequest::DeleteProject(5))
        ));
    }

    #[test]
    fn filter_can_be_cleared() {
        let mut app = App {
            selected: TASK_SECTION,
            focus: Focus::Tasks,
            filter: Some("university".into()),
            ..App::default()
        };
        app.handle_key(key(KeyCode::Char('f')));
        while !app.input.is_empty() {
            app.handle_key(key(KeyCode::Backspace));
        }
        assert!(matches!(
            app.handle_key(key(KeyCode::Enter)),
            KeyOutcome::Request(UiRequest::Filter(None))
        ));
    }

    #[test]
    fn renders_tasks_and_projects() {
        let backend = TestBackend::new(100, 26);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App {
            selected: TASK_SECTION,
            tasks: vec![task()],
            projects: vec![project()],
            ..App::default()
        };
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("Finish report"));
        assert!(text.contains("university"));

        let app = App {
            selected: PROJECT_SECTION,
            projects: vec![project()],
            ..App::default()
        };
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("University"));
    }

    #[test]
    fn result_clamps_indices() {
        let mut app = App {
            task_selected: 10,
            project_selected: 10,
            ..App::default()
        };
        app.accept_response(UiResponse {
            tasks: Some(vec![task()]),
            projects: Some(vec![project()]),
            filter: Some("university".into()),
            status: "Loaded".into(),
            is_error: false,
        });
        assert_eq!(app.task_selected, 0);
        assert_eq!(app.project_selected, 0);
        assert_eq!(app.filter.as_deref(), Some("university"));
    }
}
