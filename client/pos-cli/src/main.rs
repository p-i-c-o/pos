//! POS executable: interactive TUI and scriptable Tasks and Projects CLI.

mod onboarding;
mod remote;

use clap::{Parser, Subcommand};
use pos_core::{ProjectService, TaskEdit, TaskSearch, TaskService, export_local_data};
use pos_sqlite::{SqliteMaintenance, SqliteTaskRepository};
use pos_tasks::{TaskPriority, TaskSort, TaskStatus};
use pos_tui::{UiRequest, UiResponse};
use std::{
    env,
    error::Error,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::mpsc,
    thread,
};
use tempfile::Builder;

#[derive(Parser)]
#[command(
    name = "POS",
    version,
    about = "Personal Organisation System - interactive terminal UI, tasks and projects"
)]
struct Cli {
    /// Path to a local SQLite database. Also supports POS_DB_PATH.
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
    /// Organize tasks in local projects.
    Projects {
        #[command(subcommand)]
        action: ProjectAction,
    },
    /// Read-only remote POS server inspection (independent of local SQLite).
    Remote {
        /// Loopback server base URL, default POS_SERVER_URL or 127.0.0.1:8787.
        #[arg(long, value_name = "URL")]
        url: Option<String>,
        #[command(subcommand)]
        action: remote::RemoteAction,
    },
    /// Database integrity checks, lossless backup/restore and JSON export.
    Data {
        #[command(subcommand)]
        action: DataAction,
    },
}

#[derive(Subcommand)]
enum DataAction {
    /// Create a consistent SQLite snapshot at a new path; never overwrite.
    Backup {
        /// New SQLite snapshot path.
        output: PathBuf,
    },
    /// Validate and restore a backup to a NEW database path. Never overwrite.
    Restore {
        /// Existing POS SQLite backup file.
        source: PathBuf,
        /// New database path; the active database is never replaced.
        #[arg(long)]
        output: PathBuf,
    },
    /// Export tasks/projects as versioned JSON to stdout or a new file.
    Export {
        /// Save JSON to a new file rather than printing it.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Check SQLite integrity and foreign key consistency read-only.
    Check,
}

#[derive(Subcommand)]
enum TaskAction {
    /// Create a task.
    Add {
        title: String,
        /// Due date in YYYY-MM-DD format.
        #[arg(long)]
        due: Option<String>,
        /// Project slug (e.g. university-work).
        #[arg(long)]
        project: Option<String>,
        /// Optional multiline description.
        #[arg(long)]
        description: Option<String>,
        #[arg(long, default_value = "normal")]
        priority: TaskPriority,
        #[arg(long, default_value = "todo")]
        status: TaskStatus,
    },
    /// List incomplete tasks, or all tasks with --all; filter and sort results.
    List {
        #[arg(long)]
        all: bool,
        #[arg(long)]
        json: bool,
        /// Project slug (not a project name).
        #[arg(long)]
        project: Option<String>,
        /// Case-insensitive substring search over title and description.
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        priority: Option<TaskPriority>,
        /// Explicit completed status includes completed records automatically.
        #[arg(long)]
        status: Option<TaskStatus>,
        /// Inclusive due-date deadline in YYYY-MM-DD.
        #[arg(long)]
        due_before: Option<String>,
        /// Sort by due date (default), priority or newest creation.
        #[arg(long, default_value = "due")]
        sort: TaskSort,
    },
    /// Inspect a task.
    Show { id: i64 },
    /// Edit a task's title, due date or project.
    Edit {
        id: i64,
        #[arg(long)]
        title: Option<String>,
        #[arg(long, conflicts_with = "clear_due")]
        due: Option<String>,
        #[arg(long)]
        clear_due: bool,
        #[arg(long, conflicts_with = "clear_project")]
        project: Option<String>,
        #[arg(long)]
        clear_project: bool,
        #[arg(long, conflicts_with = "clear_description")]
        description: Option<String>,
        #[arg(long)]
        clear_description: bool,
        #[arg(long)]
        priority: Option<TaskPriority>,
        #[arg(long)]
        status: Option<TaskStatus>,
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

#[derive(Subcommand)]
enum ProjectAction {
    /// Create a project; slug generated from name unless --slug given.
    Add {
        name: String,
        #[arg(long)]
        slug: Option<String>,
    },
    /// List active projects, or include archived with --all.
    List {
        #[arg(long)]
        all: bool,
        #[arg(long)]
        json: bool,
    },
    /// Inspect a project by numeric ID.
    Show { id: i64 },
    /// Rename a project without changing its stable slug.
    Edit {
        id: i64,
        #[arg(long)]
        name: String,
    },
    /// Archive a project (its tasks remain intact).
    Archive { id: i64 },
    /// Restore an archived project.
    Unarchive { id: i64 },
    /// Permanently delete a project and unassign its tasks (requires --yes).
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
    let Cli { db, command } = cli;
    if command.is_none() && (!io::stdin().is_terminal() || !io::stdout().is_terminal()) {
        return Err("POS needs an interactive terminal. Run 'pos --help' for usage.".into());
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    // Maintenance runs before opening the normal repository. In particular,
    // restore must never create or migrate the active POS database.
    let command = match command {
        Some(Commands::Data { action }) => return runtime.block_on(run_data(db, action)),
        Some(Commands::Remote { url, action }) => {
            return runtime.block_on(remote::execute(url.as_deref(), action));
        }
        other => other,
    };

    let path = database_path(db)?;
    let repository = runtime.block_on(SqliteTaskRepository::open(&path))?;
    let tasks = TaskService::new(repository.clone());
    let projects = ProjectService::new(repository);

    match command {
        Some(Commands::Tasks { action }) => {
            runtime.block_on(run_tasks(&tasks, action))?;
        }
        Some(Commands::Projects { action }) => {
            runtime.block_on(run_projects(&projects, action))?;
        }
        Some(Commands::Data { .. } | Commands::Remote { .. }) => {
            unreachable!("non-local operations handled before repository open")
        }
        None => {
            let (request_tx, request_rx) = mpsc::channel();
            let (response_tx, response_rx) = mpsc::channel();
            let handle = runtime.handle().clone();

            let worker = thread::spawn(move || {
                let mut filter = None;
                for command in request_rx {
                    let response =
                        handle.block_on(handle_request(&tasks, &projects, &mut filter, command));
                    if response_tx.send(response).is_err() {
                        break;
                    }
                }
            });
            let result = pos_tui::run(request_tx, response_rx);
            worker
                .join()
                .map_err(|_| "POS task/project worker panicked")?;
            result?;
        }
    }
    Ok(())
}

async fn run_data(db: Option<PathBuf>, action: DataAction) -> Result<(), Box<dyn Error>> {
    match action {
        DataAction::Restore { source, output } => {
            if db.is_some() {
                return Err(
                    "--db does not apply to restore; use --output for the new database".into(),
                );
            }
            SqliteMaintenance::restore(&source, &output).await?;
            println!("Verified and restored database to {}", output.display());
        }
        DataAction::Backup { output } => {
            let source = database_path(db)?;
            SqliteMaintenance::backup(&source, &output).await?;
            println!("Verified SQLite backup: {}", output.display());
        }
        DataAction::Check => {
            let path = database_path(db)?;
            SqliteMaintenance::check(&path).await?;
            println!("SQLite integrity and foreign keys OK: {}", path.display());
        }
        DataAction::Export { output } => {
            let path = database_path(db)?;
            if !path.is_file() {
                return Err(format!("POS database not found: {}", path.display()).into());
            }
            // Export is strictly read-only and does not run any migrations.
            // Both SELECTs share a consistent database read transaction.
            let repository = SqliteTaskRepository::open_read_only(&path).await?;
            let snapshot = export_local_data(&repository).await?;
            let mut content = serde_json::to_vec_pretty(&snapshot)?;
            content.push(b'\n');
            if let Some(output) = output {
                write_export_noclobber(&output, &content)?;
                eprintln!("Exported POS JSON to {}", output.display());
            } else {
                io::stdout().lock().write_all(&content)?;
            }
        }
    }
    Ok(())
}

/// Files are only published after serialization and fsync, with no-overwrite
/// semantics even if another process creates the destination concurrently.
fn write_export_noclobber(path: &Path, content: &[u8]) -> Result<(), Box<dyn Error>> {
    if std::fs::symlink_metadata(path).is_ok() {
        return Err(format!("destination already exists: {}", path.display()).into());
    }
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !directory.is_dir() {
        return Err(format!(
            "destination directory does not exist: {}",
            directory.display()
        )
        .into());
    }

    let mut staging = Builder::new()
        .prefix(".pos-export-")
        .tempfile_in(directory)?;
    staging.write_all(content)?;
    staging.as_file().sync_all()?;
    let published = staging.persist_noclobber(path)?;
    published.sync_all()?;
    std::fs::File::open(directory)?.sync_all()?;
    Ok(())
}

async fn run_tasks(
    service: &TaskService<SqliteTaskRepository>,
    action: TaskAction,
) -> Result<(), Box<dyn Error>> {
    match action {
        TaskAction::Add {
            title,
            due,
            project,
            description,
            priority,
            status,
        } => {
            let task = service
                .add_detailed(
                    &title,
                    due.as_deref(),
                    project.as_deref(),
                    description.as_deref(),
                    priority,
                    status,
                )
                .await?;
            println!("Created #{}: {}", task.id, task.title);
        }
        TaskAction::List {
            all,
            json,
            project,
            search,
            priority,
            status,
            due_before,
            sort,
        } => {
            let tasks = service
                .search(TaskSearch {
                    include_completed: all || status == Some(TaskStatus::Completed),
                    project_slug: project.as_deref(),
                    search: search.as_deref(),
                    priority,
                    status,
                    due_before: due_before.as_deref(),
                    sort,
                })
                .await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&tasks)?);
            } else {
                for task in tasks {
                    print_task(&task);
                }
            }
        }
        TaskAction::Show { id } => {
            let task = service.get(id).await?;
            print_task(&task);
            if let Some(description) = task.description {
                println!("{description}");
            }
        }
        TaskAction::Edit {
            id,
            title,
            due,
            clear_due,
            project,
            clear_project,
            description,
            clear_description,
            priority,
            status,
        } => {
            let due_update = if clear_due {
                Some(None)
            } else {
                due.as_deref().map(Some)
            };
            let description_update = if clear_description {
                Some(None)
            } else {
                description.as_deref().map(Some)
            };
            let project_update = if clear_project {
                Some(None)
            } else {
                project.as_deref().map(Some)
            };
            // One validated atomic update, including project assignment.
            let task = service
                .edit_full(
                    id,
                    TaskEdit {
                        title: title.as_deref(),
                        due_date: due_update,
                        description: description_update,
                        priority,
                        status,
                        project_slug: project_update,
                    },
                )
                .await?;
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

async fn run_projects(
    service: &ProjectService<SqliteTaskRepository>,
    action: ProjectAction,
) -> Result<(), Box<dyn Error>> {
    match action {
        ProjectAction::Add { name, slug } => {
            let project = service.add(&name, slug.as_deref()).await?;
            println!(
                "Created project #{}: {} [{}]",
                project.id, project.name, project.slug
            );
        }
        ProjectAction::List { all, json } => {
            let projects = service.list(all).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&projects)?);
            } else {
                for project in projects {
                    print_project(&project);
                }
            }
        }
        ProjectAction::Show { id } => print_project(&service.get(id).await?),
        ProjectAction::Edit { id, name } => {
            let project = service.rename(id, &name).await?;
            println!(
                "Renamed project #{}: {} [{}]",
                project.id, project.name, project.slug
            );
        }
        ProjectAction::Archive { id } => {
            service.archive(id, true).await?;
            println!("Archived project #{id}");
        }
        ProjectAction::Unarchive { id } => {
            service.archive(id, false).await?;
            println!("Unarchived project #{id}");
        }
        ProjectAction::Delete { id, yes } => {
            if !yes {
                return Err(
                    "project deletion requires --yes; associated tasks become unassigned".into(),
                );
            }
            service.delete(id).await?;
            println!("Deleted project #{id}; its tasks remain unassigned");
        }
    }
    Ok(())
}

fn print_task(task: &pos_tasks::Task) {
    let mark = if task.completed { 'x' } else { ' ' };
    let due = task
        .due_date
        .as_deref()
        .map(|date| format!(" due {date}"))
        .unwrap_or_default();
    let project = task
        .project_id
        .map(|id| format!(" project #{id}"))
        .unwrap_or_default();
    println!(
        "#{} [{}] {}{}{} priority={} status={}",
        task.id, mark, task.title, due, project, task.priority, task.status
    );
}

fn print_project(project: &pos_projects::Project) {
    let status = if project.archived {
        "archived"
    } else {
        "active"
    };
    println!(
        "#{} [{}] {} ({status})",
        project.id, project.slug, project.name
    );
}

async fn handle_request(
    tasks: &TaskService<SqliteTaskRepository>,
    projects: &ProjectService<SqliteTaskRepository>,
    filter: &mut Option<String>,
    command: UiRequest,
) -> UiResponse {
    let result: Result<String, String> = match command {
        UiRequest::Refresh => Ok("Projects and tasks loaded".into()),
        UiRequest::Filter(slug) => match tasks.list_for_project(true, slug.as_deref()).await {
            Ok(_) => {
                *filter = slug;
                Ok("Task filter updated".into())
            }
            Err(error) => Err(error.to_string()),
        },
        UiRequest::Add(title) => tasks
            .add_with_project(&title, None, filter.as_deref())
            .await
            .map(|t| format!("Created task #{}", t.id))
            .map_err(|e| e.to_string()),
        UiRequest::EditTitle { id, title } => tasks
            .edit(id, Some(&title), None)
            .await
            .map(|_| format!("Updated task #{id}"))
            .map_err(|e| e.to_string()),
        UiRequest::SetDue { id, due_date } => tasks
            .edit(id, None, Some(due_date.as_deref()))
            .await
            .map(|_| format!("Updated due date for #{id}"))
            .map_err(|e| e.to_string()),
        UiRequest::Toggle { id, completed } => tasks
            .complete(id, completed)
            .await
            .map(|_| format!("Updated task #{id}"))
            .map_err(|e| e.to_string()),
        UiRequest::Delete(id) => tasks
            .delete(id)
            .await
            .map(|_| format!("Deleted task #{id}"))
            .map_err(|e| e.to_string()),
        UiRequest::AssignProject { id, slug } => tasks
            .assign_project(id, slug.as_deref())
            .await
            .map(|_| format!("Updated project for task #{id}"))
            .map_err(|e| e.to_string()),
        UiRequest::AddProject(name) => projects
            .add(&name, None)
            .await
            .map(|p| format!("Created project {}", p.slug))
            .map_err(|e| e.to_string()),
        UiRequest::RenameProject { id, name } => projects
            .rename(id, &name)
            .await
            .map(|_| format!("Renamed project #{id}"))
            .map_err(|e| e.to_string()),
        UiRequest::ArchiveProject { id, archived } => projects
            .archive(id, archived)
            .await
            .map(|_| format!("Updated project #{id}"))
            .map_err(|e| e.to_string()),
        UiRequest::DeleteProject(id) => {
            match projects.get(id).await {
                Ok(project) => match projects.delete(id).await {
                    Ok(()) => {
                        // Clear the task filter only if it referenced this project.
                        if filter.as_deref() == Some(project.slug.as_str()) {
                            *filter = None;
                        }
                        Ok(format!("Deleted project #{id}; tasks retained"))
                    }
                    Err(error) => Err(error.to_string()),
                },
                Err(error) => Err(error.to_string()),
            }
        }
    };
    match result {
        Ok(message) => {
            let task_list = tasks.list_for_project(true, filter.as_deref()).await;
            let project_list = projects.list(true).await;
            match (task_list, project_list) {
                (Ok(tasks), Ok(projects)) => UiResponse {
                    tasks: Some(tasks),
                    projects: Some(projects),
                    filter: filter.clone(),
                    status: message,
                    is_error: false,
                },
                (Err(error), _) => UiResponse {
                    tasks: None,
                    projects: None,
                    filter: filter.clone(),
                    status: error.to_string(),
                    is_error: true,
                },
                (_, Err(error)) => UiResponse {
                    tasks: None,
                    projects: None,
                    filter: filter.clone(),
                    status: error.to_string(),
                    is_error: true,
                },
            }
        }
        Err(error) => UiResponse {
            tasks: None,
            projects: None,
            filter: filter.clone(),
            status: error,
            is_error: true,
        },
    }
}
