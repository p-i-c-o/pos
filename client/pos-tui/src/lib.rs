//! Ratatui presentation and keyboard handling. A worker in pos-cli owns I/O.
//! Navigation usage is a separate UI-only state file, never the Tasks database.

mod navigation;
mod render;

use render::render;

use std::{
    io,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use chrono::Local;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use pos_projects::Project;
use pos_tasks::Task;

// Stable IDs, independent from the displayed usage-based menu order.
const SECTIONS: &[&str] = &[
    "Overview", "Projects", "Tasks", "Calendar", "Mail", "Notes", "Files", "Contacts", "Time",
    "Chat", "Settings",
];
const DEFAULT_ORDER: &[usize] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
const SETTINGS_ITEMS: &[&str] = &[
    "Back",
    "",
    "Git",
    "GitHub",
    "GitLab",
    "User Account",
    "General",
    "About",
];
const OVERVIEW_SECTION: usize = 0;
const PROJECT_SECTION: usize = 1;
const TASK_SECTION: usize = 2;
const SETTINGS_SECTION: usize = 10;

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
    navigation_order: Vec<usize>,
    usage_events: Option<Sender<usize>>,
    settings_open: bool,
    settings_selected: usize,
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

    fn order(&self) -> &[usize] {
        if self.navigation_order.is_empty() {
            DEFAULT_ORDER
        } else {
            &self.navigation_order
        }
    }

    fn record_visit(&self, section: usize) {
        if let Some(tx) = &self.usage_events {
            let _ = tx.send(section);
        }
    }

    fn move_navigation(&mut self, forward: bool) {
        if self.settings_open {
            let mut index = self.settings_selected;
            loop {
                index = if forward {
                    (index + 1) % SETTINGS_ITEMS.len()
                } else {
                    (index + SETTINGS_ITEMS.len() - 1) % SETTINGS_ITEMS.len()
                };
                if !SETTINGS_ITEMS[index].is_empty() {
                    break;
                }
            }
            self.settings_selected = index;
        } else {
            let order = self.order();
            let current = order
                .iter()
                .position(|id| *id == self.selected)
                .unwrap_or(0);
            let next = if forward {
                (current + 1) % order.len()
            } else {
                (current + order.len() - 1) % order.len()
            };
            self.selected = order[next];
        }
        self.show_help = false;
    }

    fn select_navigation(&mut self) {
        if self.settings_open {
            if self.settings_selected == 0 {
                self.settings_open = false;
                self.focus = Focus::Navigation;
            }
        } else if self.selected == SETTINGS_SECTION {
            self.settings_open = true;
            self.settings_selected = 0;
            self.focus = Focus::Navigation;
            self.record_visit(SETTINGS_SECTION);
        } else {
            self.focus = self.current_focus();
            self.record_visit(self.selected);
        }
        self.show_help = false;
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
                if self.show_help {
                    self.show_help = false;
                } else if self.settings_open {
                    self.settings_open = false;
                    self.focus = Focus::Navigation;
                } else if self.focus != Focus::Navigation {
                    self.focus = Focus::Navigation;
                } else {
                    return KeyOutcome::Quit;
                }
                KeyOutcome::Nothing
            }
            KeyCode::Char('?') => {
                self.show_help = !self.show_help;
                KeyOutcome::Nothing
            }
            KeyCode::Char('r') => KeyOutcome::Request(UiRequest::Refresh),
            KeyCode::Tab | KeyCode::Right => {
                if !self.settings_open {
                    if self.focus == Focus::Navigation {
                        let focus = self.current_focus();
                        if focus != Focus::Navigation {
                            self.focus = focus;
                            self.record_visit(self.selected);
                        }
                    } else {
                        self.focus = Focus::Navigation;
                    }
                }
                self.show_help = false;
                KeyOutcome::Nothing
            }
            KeyCode::Left => {
                self.focus = Focus::Navigation;
                KeyOutcome::Nothing
            }
            KeyCode::Enter if self.focus == Focus::Navigation => {
                self.select_navigation();
                KeyOutcome::Nothing
            }
            KeyCode::Enter if self.focus == Focus::Projects => {
                if let Some(project) = self.selected_project() {
                    let slug = project.slug.clone();
                    self.selected = TASK_SECTION;
                    self.focus = Focus::Tasks;
                    self.task_selected = 0;
                    self.record_visit(TASK_SECTION);
                    KeyOutcome::Request(UiRequest::Filter(Some(slug)))
                } else {
                    KeyOutcome::Nothing
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                match self.focus {
                    Focus::Navigation => self.move_navigation(true),
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
                    Focus::Navigation => self.move_navigation(false),
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

    // Load navigation counts once so the menu cannot rearrange while
    // keyboard navigation is in progress. Persist visits off the UI thread.
    let history_path = navigation::history_path();
    let ordered = history_path
        .as_deref()
        .map(|path| navigation::load_ranked(path, DEFAULT_ORDER, navigation::now_secs()))
        .unwrap_or_else(|| DEFAULT_ORDER.to_vec());
    let (visit_tx, visit_rx) = mpsc::channel::<usize>();
    let usage_writer = thread::spawn(move || {
        for section in visit_rx {
            if let Some(path) = &history_path {
                let _ = navigation::record(path, section, navigation::now_secs());
            }
        }
    });

    let result = ratatui::run(|terminal| {
        let mut app = App {
            navigation_order: ordered,
            usage_events: Some(visit_tx),
            status: "Loading local tasks and projects...".into(),
            ..App::default()
        };
        let mut clock_minute = Local::now().timestamp() / 60;
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
            // Refresh the visible local date/time when the minute changes.
            // No filesystem reads or external commands in the input loop.
            let minute = Local::now().timestamp() / 60;
            if minute != clock_minute {
                clock_minute = minute;
                needs_draw = true;
            }
        }
    });
    usage_writer
        .join()
        .map_err(|_| io::Error::other("navigation history writer panicked"))?;
    result
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
            description: None,
            priority: pos_tasks::TaskPriority::Normal,
            status: pos_tasks::TaskStatus::Todo,
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

    // Test the exact 100x32 terminal dimensions used by the approved mockup.
    fn snapshot(app: &App) -> (String, Vec<(usize, usize)>) {
        let mut terminal = Terminal::new(TestBackend::new(100, 32)).unwrap();
        terminal.draw(|frame| render(frame, app)).unwrap();
        let buffer = terminal.backend().buffer();
        let mut output = String::new();
        let mut focus_markers = Vec::new();
        for (index, cell) in buffer.content.iter().enumerate() {
            if cell.symbol() == ">" {
                focus_markers.push((index % 100, index / 100));
            }
            output.push_str(cell.symbol());
            if index % 100 == 99 {
                output.push('\n');
            }
        }
        (output, focus_markers)
    }

    #[test]
    fn focus_marker_is_unique_and_moves_with_keyboard_focus() {
        let mut app = App {
            selected: TASK_SECTION,
            tasks: vec![task()],
            ..App::default()
        };
        let (navigation, markers) = snapshot(&app);
        assert!(navigation.contains("Workspace - Tasks / All"));
        assert_eq!(markers.len(), 1, "only sidebar may show focus marker");
        assert_eq!(
            markers[0].0, 2,
            "navigation marker needs one space after border"
        );
        assert_eq!(
            markers[0].1, 7,
            "navigation items start one blank row below border"
        );

        app.handle_key(key(KeyCode::Enter));
        let (_, markers) = snapshot(&app);
        assert_eq!(markers.len(), 1, "must not show a second sidebar marker");
        assert!(markers[0].0 >= 22, "focus moved to task row");

        app.handle_key(key(KeyCode::Char('?')));
        let (_, markers) = snapshot(&app);
        assert_eq!(markers.len(), 1, "help overlay must not duplicate focus");
        assert!(markers[0].0 >= 22);

        app.handle_key(key(KeyCode::Esc));
        app.handle_key(key(KeyCode::Esc));
        let (_, markers) = snapshot(&app);
        assert_eq!(markers.len(), 1);
        assert!(markers[0].0 < 22);
    }

    #[test]
    fn empty_focused_lists_still_show_one_focus_marker() {
        for selected in [TASK_SECTION, PROJECT_SECTION] {
            let mut app = App {
                selected,
                ..App::default()
            };
            app.handle_key(key(KeyCode::Enter));
            let (_, markers) = snapshot(&app);
            assert_eq!(markers.len(), 1);
            assert!(markers[0].0 >= 22);
        }
    }

    #[test]
    fn settings_back_menu_and_divider_behave_as_requested() {
        let mut app = App {
            selected: SETTINGS_SECTION,
            ..App::default()
        };
        let (main, markers) = snapshot(&app);
        assert!(main.contains("Settings"));
        assert_eq!(markers.len(), 1);

        app.handle_key(key(KeyCode::Enter));
        assert!(app.settings_open);
        assert_eq!(app.settings_selected, 0);
        let (settings, markers) = snapshot(&app);
        assert!(settings.contains("Back"));
        assert!(settings.contains("Git"));
        assert!(settings.contains("GitHub"));
        assert!(settings.contains("User Account"));
        assert_eq!(markers.len(), 1);

        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.settings_selected, 2, "blank separator must be skipped");
        app.handle_key(key(KeyCode::Enter));
        let (git, _) = snapshot(&app);
        assert!(git.contains("Settings - Git"));
        assert!(git.contains("not implemented"));

        app.handle_key(key(KeyCode::Esc));
        assert!(!app.settings_open);
        assert_eq!(app.selected, SETTINGS_SECTION);

        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Enter)); // Back
        assert!(!app.settings_open);
        assert_eq!(app.focus, Focus::Navigation);
    }

    #[test]
    fn ranking_only_changes_main_navigation_order_at_startup() {
        let ordered = navigation::rank(
            DEFAULT_ORDER,
            "2000000\t8\n1999999\t8\n1999998\t2\n",
            2_000_000,
        );
        assert_eq!(ordered[0], OVERVIEW_SECTION);
        assert_eq!(ordered[ordered.len() - 1], SETTINGS_SECTION);
        assert_eq!(ordered[1], 8);

        let mut app = App {
            navigation_order: ordered,
            ..App::default()
        };
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected, 8, "ranked module takes second menu slot");
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected, TASK_SECTION);
        // No menu reorder while the app is running.
        assert_eq!(app.order()[1], 8);
    }

    #[test]
    fn overview_shows_real_local_data_and_contextual_titles() {
        let mut imminent = task();
        imminent.due_date = Some(
            (Local::now().date_naive() + chrono::Duration::days(1))
                .format("%Y-%m-%d")
                .to_string(),
        );
        let mut app = App {
            tasks: vec![imminent],
            projects: vec![project()],
            ..App::default()
        };
        let (overview, markers) = snapshot(&app);
        assert_eq!(markers.len(), 1);
        assert!(overview.contains("Workspace - Overview"));
        assert_eq!(
            overview
                .lines()
                .position(|line| line.contains("Welcome to POS.")),
            Some(5),
            "workspace content must begin one blank row below its top border"
        );
        assert!(overview.contains("Tasks: 1 open"));
        assert!(overview.contains("Upcoming deadlines"));
        assert!(overview.contains("Finish report"));
        assert!(overview.contains("University"));
        assert!(overview.contains("│")); // real terminal panel borders

        app.selected = TASK_SECTION;
        app.filter = Some("university".into());
        let (tasks, markers) = snapshot(&app);
        assert!(tasks.contains("Workspace - Tasks / University"));
        assert_eq!(markers.len(), 1);
        let footer = tasks.lines().find(|l| l.contains("j/k Navigate")).unwrap();
        assert!(footer.contains("│"), "keyboard shortcuts need separators");
    }
}
