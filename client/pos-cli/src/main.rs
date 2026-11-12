//! POS executable: interactive TUI and shared-operation Tasks CLI.

use clap::{Parser, Subcommand};
use pos_core::TaskService;
use pos_sqlite::SqliteTaskRepository;
use pos_tui::{UiRequest, UiResponse};
use std::{
    env,
    error::Error,
    io::{self, IsTerminal},
    path::PathBuf,
    process::ExitCode,
    sync::mpsc,
    thread,
};

#[derive(Parser)]
#[command(
    name = "POS",
    version,
    about = "Personal Organisation System - launch the interactive terminal UI or manage tasks"
)]
struct Cli {
    /// Path to a local SQLite database. Also supports the POS_DB_PATH environment variable.
    #[arg(long, global = true, value_name = "PATH")]
    db: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Create, inspect and update local tasks.
    Tasks {
        #[command(subcommand)]
        action: TaskAction,
    },
}

#[derive(Subcommand)]
enum TaskAction {
    /// Create a task.
    Add {
        title: String,
        /// Due date in YYYY-MM-DD format.
        #[arg(long)]
        due: Option<String>,
    },
    /// List incomplete tasks, or all tasks with --all.
    List {
        #[arg(long)]
        all: bool,
        #[arg(long)]
        json: bool,
    },
    /// Inspect a task.
    Show { id: i64 },
    /// Edit title and/or due date.
    Edit {
        id: i64,
        #[arg(long)]
        title: Option<String>,
        #[arg(long, conflicts_with = "clear_due")]
        due: Option<String>,
        #[arg(long)]
        clear_due: bool,
    },
    /// Mark a task as completed.
    Done { id: i64 },
    /// Reopen a completed task.
    Reopen { id: i64 },
    /// Permanently delete one task (requires --yes).
    Delete {
        id: i64,
        #[arg(long)]
        yes: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("POS: {error}");
            ExitCode::FAILURE
        }
    }
}

fn database_path(override_path: Option<PathBuf>) -> Result<PathBuf, Box<dyn Error>> {
    if let Some(path) = override_path {
        return Ok(path);
    }
    if let Some(path) = env::var_os("POS_DB_PATH").filter(|s| !s.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    let home = env::var_os("HOME").ok_or("HOME is not set; specify --db PATH or POS_DB_PATH")?;
    let root = env::var_os("XDG_DATA_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(home).join(".local/share"));
    let pos_dir = root.join("pos");
    std::fs::create_dir_all(&pos_dir)?;
    Ok(pos_dir.join("pos.sqlite3"))
}

fn run(cli: Cli) -> Result<(), Box<dyn Error>> {
    if cli.command.is_none() && (!io::stdin().is_terminal() || !io::stdout().is_terminal()) {
        return Err("POS needs an interactive terminal. Run 'pos --help' for usage.".into());
    }

    let path = database_path(cli.db)?;
    // Keep Tokio alive for both the interactive UI worker and the CLI calls.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let store = runtime.block_on(SqliteTaskRepository::open(&path))?;
    let service = TaskService::new(store);

    match cli.command {
        Some(Commands::Tasks { action }) => {
            runtime.block_on(run_tasks(&service, action))?;
        }
        None => {
            let (request_tx, request_rx) = mpsc::channel();
            let (response_tx, response_rx) = mpsc::channel();
            let handle = runtime.handle().clone();

            let worker = thread::spawn(move || {
                for command in request_rx {
                    let response = handle.block_on(handle_request(&service, command));
                    if response_tx.send(response).is_err() {
                        break;
                    }
                }
            });
            let result = pos_tui::run(request_tx, response_rx);
            worker.join().map_err(|_| "POS task worker panicked")?;
            result?;
        }
    }
    Ok(())
}

async fn run_tasks(
    service: &TaskService<SqliteTaskRepository>,
    action: TaskAction,
) -> Result<(), Box<dyn Error>> {
    match action {
        TaskAction::Add { title, due } => {
            let task = service.add(&title, due.as_deref()).await?;
            println!("Created #{}: {}", task.id, task.title);
        }
        TaskAction::List { all, json } => {
            let tasks = service.list(all).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&tasks)?);
            } else {
                for task in tasks {
                    print_task(&task);
                }
            }
        }
        TaskAction::Show { id } => {
            print_task(&service.get(id).await?);
        }
        TaskAction::Edit {
            id,
            title,
            due,
            clear_due,
        } => {
            let due_update = if clear_due {
                Some(None)
            } else {
                due.as_deref().map(Some)
            };
            let task = service.edit(id, title.as_deref(), due_update).await?;
            println!("Updated #{}: {}", task.id, task.title);
        }
        TaskAction::Done { id } => {
            service.complete(id, true).await?;
            println!("Completed #{id}");
        }
        TaskAction::Reopen { id } => {
            service.complete(id, false).await?;
            println!("Reopened #{id}");
        }
        TaskAction::Delete { id, yes } => {
            if !yes {
                return Err("permanent deletion requires --yes".into());
            }
            service.delete(id).await?;
            println!("Deleted #{id}");
        }
    }
    Ok(())
}

fn print_task(task: &pos_tasks::Task) {
    let mark = if task.completed { 'x' } else { ' ' };
    match &task.due_date {
        Some(due) => println!("#{} [{}] {} (due {due})", task.id, mark, task.title),
        None => println!("#{} [{}] {}", task.id, mark, task.title),
    }
}

async fn handle_request(
    service: &TaskService<SqliteTaskRepository>,
    command: UiRequest,
) -> UiResponse {
    let result = match command {
        UiRequest::Refresh => Ok("Tasks loaded".to_string()),
        UiRequest::Add(title) => service
            .add(&title, None)
            .await
            .map(|task| format!("Created task #{}", task.id)),
        UiRequest::EditTitle { id, title } => service
            .edit(id, Some(&title), None)
            .await
            .map(|_| format!("Updated task #{id}")),
        UiRequest::SetDue { id, due_date } => service
            .edit(id, None, Some(due_date.as_deref()))
            .await
            .map(|_| format!("Updated due date for #{id}")),
        UiRequest::Toggle { id, completed } => service
            .complete(id, completed)
            .await
            .map(|_| format!("Updated task #{id}")),
        UiRequest::Delete(id) => service
            .delete(id)
            .await
            .map(|_| format!("Deleted task #{id}")),
    };
    match result {
        Ok(message) => match service.list(true).await {
            Ok(tasks) => UiResponse {
                tasks: Some(tasks),
                status: message,
                is_error: false,
            },
            Err(error) => UiResponse {
                tasks: None,
                status: error.to_string(),
                is_error: true,
            },
        },
        Err(error) => UiResponse {
            tasks: None,
            status: error.to_string(),
            is_error: true,
        },
    }
}
