//! Ratatui user interface. Presentation and input live here; the CLI worker
//! executes the shared TaskService operations asynchronously.

use std::{
    io,
    sync::mpsc::{Receiver, Sender},
    time::Duration,
};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
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

#[derive(Debug)]
pub enum UiRequest {
    Refresh,
    Add(String),
    EditTitle { id: i64, title: String },
    SetDue { id: i64, due_date: Option<String> },
    Toggle { id: i64, completed: bool },
    Delete(i64),
}

pub struct UiResponse {
    pub tasks: Option<Vec<Task>>,
    pub status: String,
    pub is_error: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
enum Focus {
    #[default]
    Navigation,
    Tasks,
}

#[derive(Debug)]
enum Editor {
    Add,
    EditTitle(i64),
    DueDate(i64),
    ConfirmDelete(i64),
}

#[derive(Debug, Default)]
struct App {
    selected: usize,
    task_selected: usize,
    tasks: Vec<Task>,
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
        self.status = result.status;
        self.is_error = result.is_error;
    }

    fn selected_task(&self) -> Option<&Task> {
        self.tasks.get(self.task_selected)
    }

    fn begin_input(&mut self, mode: Editor, value: String) {
        self.input = value;
        self.editor = Some(mode);
    }

    fn handle_key(&mut self, key: KeyEvent) -> KeyOutcome {
        if key.kind != KeyEventKind::Press {
            return KeyOutcome::Nothing;
        }
        if matches!(key.code, KeyCode::Char('c')) && key.modifiers.contains(KeyModifiers::CONTROL) {
            return KeyOutcome::Quit;
        }

        if let Some(editor) = self.editor.take() {
            return self.handle_editor_key(editor, key);
        }

        match key.code {
            KeyCode::Char('q') => KeyOutcome::Quit,
            KeyCode::Esc => {
                if self.focus == Focus::Tasks {
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
            KeyCode::Tab | KeyCode::Right if self.selected == TASK_SECTION => {
                self.focus = if self.focus == Focus::Tasks {
                    Focus::Navigation
                } else {
                    Focus::Tasks
                };
                self.show_help = false;
                KeyOutcome::Nothing
            }
            KeyCode::Left => {
                self.focus = Focus::Navigation;
                KeyOutcome::Nothing
            }
            KeyCode::Enter if self.focus == Focus::Navigation && self.selected == TASK_SECTION => {
                self.focus = Focus::Tasks;
                self.show_help = false;
                KeyOutcome::Nothing
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.focus == Focus::Tasks {
                    if !self.tasks.is_empty() {
                        self.task_selected = (self.task_selected + 1) % self.tasks.len();
                    }
                } else {
                    self.selected = (self.selected + 1) % SECTIONS.len();
                    self.show_help = false;
                }
                KeyOutcome::Nothing
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.focus == Focus::Tasks {
                    if !self.tasks.is_empty() {
                        self.task_selected =
                            (self.task_selected + self.tasks.len() - 1) % self.tasks.len();
                    }
                } else {
                    self.selected = (self.selected + SECTIONS.len() - 1) % SECTIONS.len();
                    self.show_help = false;
                }
                KeyOutcome::Nothing
            }
            _ if self.focus == Focus::Tasks && !self.show_help => self.handle_task_key(key),
            _ => KeyOutcome::Nothing,
        }
    }

    fn handle_task_key(&mut self, key: KeyEvent) -> KeyOutcome {
        match key.code {
            KeyCode::Char('a') => self.begin_input(Editor::Add, String::new()),
            KeyCode::Char('e') => {
                if let Some(task) = self.selected_task() {
                    self.begin_input(Editor::EditTitle(task.id), task.title.clone());
                }
            }
            KeyCode::Char('d') => {
                if let Some(task) = self.selected_task() {
                    self.begin_input(
                        Editor::DueDate(task.id),
                        task.due_date.clone().unwrap_or_default(),
                    );
                }
            }
            KeyCode::Char('x') => {
                if let Some(task) = self.selected_task() {
                    self.begin_input(Editor::ConfirmDelete(task.id), String::new());
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

    fn handle_editor_key(&mut self, editor: Editor, key: KeyEvent) -> KeyOutcome {
        if let Editor::ConfirmDelete(id) = editor {
            return match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.status = format!("Deleting task #{id}...");
                    KeyOutcome::Request(UiRequest::Delete(id))
                }
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => KeyOutcome::Nothing,
                _ => {
                    self.editor = Some(Editor::ConfirmDelete(id));
                    KeyOutcome::Nothing
                }
            };
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
                    Editor::Add => UiRequest::Add(value),
                    Editor::EditTitle(id) => UiRequest::EditTitle { id, title: value },
                    Editor::DueDate(id) => UiRequest::SetDue {
                        id,
                        due_date: if value.is_empty() { None } else { Some(value) },
                    },
                    Editor::ConfirmDelete(_) => unreachable!(),
                };
                self.status = "Saving task locally...".into();
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

/// Run the keyboard-driven POS interface. The caller owns a background worker
/// that receives requests and returns updated tasks without blocking redraws.
pub fn run(requests: Sender<UiRequest>, responses: Receiver<UiResponse>) -> io::Result<()> {
    requests
        .send(UiRequest::Refresh)
        .map_err(|error| io::Error::new(io::ErrorKind::BrokenPipe, error.to_string()))?;

    ratatui::run(|terminal| {
        let mut app = App {
            status: "Loading local tasks...".into(),
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
            Line::from("  j / Down     Next item / section"),
            Line::from("  k / Up       Previous item / section"),
            Line::from("  Enter / Tab  Focus Tasks list"),
            Line::from("  Esc          Back to navigation / exit"),
            Line::from("  ?            Toggle help"),
            Line::from("  r            Refresh tasks"),
            Line::from("  a            Add task (in Tasks list)"),
            Line::from("  e            Edit selected task title"),
            Line::from("  d            Change due date (blank clears)"),
            Line::from("  Space        Toggle task completion"),
            Line::from("  x            Delete task (requires confirmation)"),
            Line::from("  q / Ctrl+C   Quit POS"),
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
                .map(|date| format!("  due {date}"))
                .unwrap_or_default();
            ListItem::new(format!("[{mark}] #{} {}{due}", task.id, task.title))
        });
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Local Tasks "),
            )
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
    } else {
        let body = if app.selected == 0 {
            Text::from(vec![
                Line::from("Welcome to POS."),
                Line::from(""),
                Line::from("Tasks now persist locally in SQLite."),
                Line::from(""),
                Line::from(format!("{} tasks saved", app.tasks.len())),
                Line::from(""),
                Line::from("Navigate to Tasks and press Enter to manage them."),
                Line::from("All other modules are still placeholders."),
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
        Some(Editor::Add) => format!(" New task title: {}_", app.input),
        Some(Editor::EditTitle(id)) => format!(" Edit #{id} title: {}_", app.input),
        Some(Editor::DueDate(id)) => format!(
            " Due date for #{id} (YYYY-MM-DD, blank clears): {}_",
            app.input
        ),
        Some(Editor::ConfirmDelete(id)) => {
            format!(" Delete task #{id} permanently? y/n")
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
        " Enter: save/confirm    Esc: cancel    Backspace: delete character "
    } else if app.focus == Focus::Tasks {
        " j/k: navigate  a: add  e: edit  d: due  Space: done  x: delete  Esc: back "
    } else {
        " j/k: navigate  Enter/Tab: focus Tasks  ?: help  q/Esc: quit "
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

    fn sample_task() -> Task {
        Task {
            id: 7,
            title: "Finish report".into(),
            due_date: Some("2026-12-17".into()),
            completed: false,
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
            tasks: vec![sample_task()],
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
    fn tasks_edit_mode_accepts_characters_including_q() {
        let mut app = App {
            selected: TASK_SECTION,
            focus: Focus::Tasks,
            ..App::default()
        };
        app.handle_key(key(KeyCode::Char('a')));
        app.handle_key(key(KeyCode::Char('q')));
        assert!(matches!(
            app.handle_key(key(KeyCode::Enter)),
            KeyOutcome::Request(UiRequest::Add(ref title)) if title == "q"
        ));
    }

    #[test]
    fn deletion_requires_explicit_confirmation() {
        let mut app = App {
            selected: TASK_SECTION,
            focus: Focus::Tasks,
            ..App::default()
        };
        app.tasks.push(sample_task());
        app.handle_key(key(KeyCode::Char('x')));
        assert!(matches!(
            app.handle_key(key(KeyCode::Char('n'))),
            KeyOutcome::Nothing
        ));
        app.handle_key(key(KeyCode::Char('x')));
        assert!(matches!(
            app.handle_key(key(KeyCode::Char('y'))),
            KeyOutcome::Request(UiRequest::Delete(7))
        ));
    }

    #[test]
    fn renders_task_list_in_test_terminal() {
        let backend = TestBackend::new(100, 26);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let app = App {
            selected: TASK_SECTION,
            tasks: vec![sample_task()],
            ..App::default()
        };
        terminal.draw(|frame| render(frame, &app)).expect("render");
        let content: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(content.contains("Personal Organisation System"));
        assert!(content.contains("Finish report"));
        assert!(content.contains("Navigation"));
    }

    #[test]
    fn task_response_clamps_selection() {
        let mut app = App {
            task_selected: 10,
            ..App::default()
        };
        app.accept_response(UiResponse {
            tasks: Some(vec![sample_task()]),
            status: "Loaded".into(),
            is_error: false,
        });
        assert_eq!(app.task_selected, 0);
    }
}
