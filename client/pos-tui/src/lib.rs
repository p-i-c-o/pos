//! Ratatui terminal shell for POS.
//!
//! This crate owns rendering and keyboard interaction only. POS domain modules,
//! data access and networking are not yet connected.

use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
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

#[derive(Debug, Default)]
struct App {
    selected: usize,
    show_help: bool,
}

impl App {
    /// Returns true when the user requests an exit.
    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.kind != KeyEventKind::Press {
            return false;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return true,
            KeyCode::Char('?') => self.show_help = !self.show_help,
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1) % SECTIONS.len();
                self.show_help = false;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = (self.selected + SECTIONS.len() - 1) % SECTIONS.len();
                self.show_help = false;
            }
            _ => {}
        }

        false
    }
}

/// Launch the interactive POS terminal UI and restore terminal state on exit.
pub fn run() -> io::Result<()> {
    ratatui::run(|terminal| {
        let mut app = App::default();
        terminal.draw(|frame| render(frame, &app))?;

        loop {
            match event::read()? {
                Event::Key(key) => {
                    if app.handle_key(key) {
                        return Ok(());
                    }
                    terminal.draw(|frame| render(frame, &app))?;
                }
                Event::Resize(_, _) => {
                    terminal.draw(|frame| render(frame, &app))?;
                }
                _ => {}
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
        ])
        .split(frame.area());

    let header = Paragraph::new(
        Line::from(" Personal Organisation System   |   v0.0.0   |   early development ").style(
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
        .highlight_symbol("> ");
    let mut state = ListState::default().with_selected(Some(app.selected));
    frame.render_stateful_widget(navigation, columns[0], &mut state);

    let section = SECTIONS[app.selected];
    let body = if app.show_help {
        Text::from(vec![
            Line::from("Keyboard shortcuts"),
            Line::from(""),
            Line::from("  j / Down     Next section"),
            Line::from("  k / Up       Previous section"),
            Line::from("  ?            Toggle help"),
            Line::from("  q / Esc      Quit POS"),
            Line::from("  Ctrl+C       Quit POS"),
        ])
    } else if app.selected == 0 {
        Text::from(vec![
            Line::from("Welcome to POS."),
            Line::from(""),
            Line::from("Ratatui is running with the Crossterm terminal backend."),
            Line::from(""),
            Line::from("This is the initial interactive shell. Domain features,"),
            Line::from("accounts, storage and synchronisation are not yet wired up."),
            Line::from(""),
            Line::from("Choose a module to inspect its placeholder."),
        ])
    } else {
        Text::from(vec![
            Line::from(section),
            Line::from(""),
            Line::from("This module is planned, but has not been implemented."),
            Line::from("No data, actions or service connections are available yet."),
        ])
    };

    frame.render_widget(
        Paragraph::new(body)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(if app.show_help {
                        " Help "
                    } else {
                        " Workspace "
                    }),
            )
            .wrap(Wrap { trim: true }),
        columns[1],
    );

    let footer = Paragraph::new(" j/k or arrows: navigate   ?: help   q/Esc: quit ")
        .block(Block::default().borders(Borders::ALL).title(" Keys "))
        .style(Style::default().fg(Color::Yellow));
    frame.render_widget(footer, screen[2]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
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
    fn q_and_escape_quit() {
        let mut app = App::default();
        assert!(app.handle_key(key(KeyCode::Char('q'))));
        assert!(app.handle_key(key(KeyCode::Esc)));
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
    fn renders_in_test_terminal_without_io() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render(frame, &App::default()))
            .expect("render");
        let content: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(content.contains("Personal Organisation System"));
        assert!(content.contains("Ratatui is running"));
        assert!(content.contains("Navigation"));
    }
}
