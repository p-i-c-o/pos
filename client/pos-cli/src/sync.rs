//! Explicit, authenticated workspace binding. No automatic synchronization.
//! A private anchor outside SQLite detects common restored/duplicated DBs.
use std::{
    env,
    error::Error,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use clap::Subcommand;
use pos_sqlite::{BindOutcome, SqliteTaskRepository, SyncBinding};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::remote;

#[derive(Subcommand)]
pub(crate) enum SyncAction {
    /// Read database identity and binding; never migrate or contact the server.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Bind local database to one *verified* readable server workspace.
    Bind {
        /// Workspace UUID from 'pos remote workspaces'.
        workspace: Uuid,
        /// The loopback server address (can be an SSH-forwarded port).
        #[arg(long, value_name = "URL")]
        url: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Anchor {
    format_version: u8,
    database_id: Uuid,
    canonical_path: String,
    server_origin: String,
    user_id: Uuid,
    device_id: Uuid,
    workspace_id: Uuid,
}

impl From<&SyncBinding> for Anchor {
    fn from(binding: &SyncBinding) -> Self {
        Self {
            format_version: 1,
            database_id: binding.database_id,
            canonical_path: binding.canonical_path.clone(),
            server_origin: binding.server_origin.clone(),
            user_id: binding.user_id,
            device_id: binding.device_id,
            workspace_id: binding.workspace_id,
        }
    }
}

fn failure(message: &'static str) -> Box<dyn Error> {
    io::Error::other(message).into()
}

fn private_anchor_path(database_id: Uuid) -> Result<PathBuf, Box<dyn Error>> {
    let config_root = if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(path)
    } else {
        let home =
            env::var_os("HOME").ok_or_else(|| failure("HOME required for POS sync binding"))?;
        PathBuf::from(home).join(".config")
    };
    if !config_root.is_absolute() {
        return Err(failure("POS config path must be absolute"));
    }
    Ok(config_root
        .join("pos")
        .join("sync-anchors")
        .join(format!("{database_id}.json")))
}

fn canonical_path(path: &Path) -> Result<String, Box<dyn Error>> {
    let full = fs::canonicalize(path).map_err(|_| failure("cannot resolve local database path"))?;
    Ok(full
        .to_str()
        .ok_or_else(|| failure("database path must be UTF-8"))?
        .to_owned())
}

fn check_private_dir(path: &Path) -> Result<(), Box<dyn Error>> {
    let meta =
        fs::symlink_metadata(path).map_err(|_| failure("POS sync anchor directory is missing"))?;
    if !meta.file_type().is_dir() {
        return Err(failure("sync anchor directory must not be a symlink"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err(failure("POS sync anchor directory must be private (0700)"));
        }
    }
    Ok(())
}

fn read_anchor(path: &Path) -> Result<Anchor, Box<dyn Error>> {
    check_private_dir(
        path.parent()
            .ok_or_else(|| failure("sync anchor missing parent directory"))?,
    )?;
    let meta = fs::symlink_metadata(path)
        .map_err(|_| failure("sync anchor missing: copied or restored database is fenced"))?;
    if !meta.file_type().is_file() || meta.len() > 4096 {
        return Err(failure("invalid sync anchor file or symlink"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err(failure(
                "sync anchor must have owner-only permissions (0600)",
            ));
        }
    }
    let data = fs::read(path).map_err(|_| failure("cannot read sync anchor"))?;
    serde_json::from_slice(&data).map_err(|_| failure("invalid sync anchor data"))
}

fn ensure_anchor(binding: &SyncBinding, allow_create: bool) -> Result<(), Box<dyn Error>> {
    let target = private_anchor_path(binding.database_id)?;
    let expected = Anchor::from(binding);
    if fs::symlink_metadata(&target).is_ok() {
        let actual = read_anchor(&target)?;
        if actual != expected {
            return Err(failure(
                "sync anchor conflicts with database/device/path; refusing possible clone",
            ));
        }
        return Ok(());
    }
    if !allow_create {
        return Err(failure(
            "sync anchor is missing; restored/copied database cannot be rebound automatically",
        ));
    }

    let parent = target
        .parent()
        .ok_or_else(|| failure("sync anchor needs a parent"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
    }
    #[cfg(not(unix))]
    return Err(failure(
        "POS sync anchors require Unix private file permissions",
    ));
    check_private_dir(parent)?;
    check_private_dir(
        parent
            .parent()
            .ok_or_else(|| failure("invalid anchor parent"))?,
    )?;

    let mut staging = tempfile::NamedTempFile::new_in(parent)?;
    let serialized = serde_json::to_vec(&expected)?;
    staging.write_all(&serialized)?;
    staging.as_file_mut().sync_all()?;
    match staging.persist_noclobber(&target) {
        Ok(file) => file.sync_all()?,
        Err(_) => {
            // A concurrent writer may have inserted a matching anchor.
            // Never overwrite any existing marker.
            if read_anchor(&target)? != expected {
                return Err(failure("conflicting concurrent sync anchor"));
            }
        }
    }
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

pub(crate) async fn execute(path: PathBuf, action: SyncAction) -> Result<(), Box<dyn Error>> {
    match action {
        SyncAction::Status { json } => {
            // Read-only status must not apply migrations or create DB files.
            let repo = SqliteTaskRepository::open_read_only(&path).await?;
            let state = repo.sync_state().await?;
            let canonical = canonical_path(&path)?;
            if let Some(ref bound) = state.binding {
                if bound.canonical_path != canonical {
                    return Err(failure(
                        "database moved or cloned: path does not match binding",
                    ));
                }
                ensure_anchor(bound, false)?;
            }
            if json {
                let data = serde_json::json!({
                    "database_id": state.database_id,
                    "bound": state.binding.is_some(),
                    "workspace_id": state.binding.as_ref().map(|b| b.workspace_id),
                    "server_origin": state.binding.as_ref().map(|b| &b.server_origin),
                    "user_id": state.binding.as_ref().map(|b| b.user_id),
                    "device_id": state.binding.as_ref().map(|b| b.device_id),
                    "journal_entries": state.journal_entries,
                    "mapped_records": state.mapped_records,
                    "sync_enabled": false,
                });
                println!("{}", serde_json::to_string_pretty(&data)?);
            } else {
                println!("Database: {}", state.database_id);
                if let Some(bound) = state.binding {
                    println!(
                        "Workspace: {} (bound; {} / user {})",
                        bound.workspace_id, bound.server_origin, bound.user_id
                    );
                    println!(
                        "Device: {} (private local anchor verified)",
                        bound.device_id
                    );
                } else {
                    println!("Workspace: unbound");
                }
                println!("Journal entries: {}", state.journal_entries);
                println!("Record mappings: {}", state.mapped_records);
                println!("Synchronization: disabled (foundation only)");
            }
        }
        SyncAction::Bind { workspace, url } => {
            // Authentication and workspace membership are verified via GET
            // before opening/migrating the local SQLite database.
            let verified = remote::verify_workspace(url.as_deref(), workspace).await?;
            let repo = SqliteTaskRepository::open(&path).await?;
            let state = repo.sync_state().await?;
            let binding = SyncBinding {
                database_id: state.database_id,
                server_origin: verified.origin,
                user_id: verified.user_id,
                device_id: verified.device_id,
                workspace_id: verified.workspace_id,
                canonical_path: canonical_path(&path)?,
            };
            if let Some(ref existing) = state.binding {
                if existing != &binding {
                    return Err(failure(
                        "database is already bound; refusing account/device/workspace switch",
                    ));
                }
                ensure_anchor(existing, false)?;
            } else {
                // Reserve external ownership before committing immutable DB
                // binding. A crash between these steps is recoverable by
                // repeating the exact same bind with the same credentials.
                ensure_anchor(&binding, true)?;
            }
            let outcome = repo.bind_workspace_once(&binding).await?;
            println!(
                "POS workspace {}: {}",
                binding.workspace_id,
                match outcome {
                    BindOutcome::Created => "bound",
                    BindOutcome::AlreadyBound => "already bound (unchanged)",
                }
            );
            println!(
                "Database: {} | Device: {}",
                binding.database_id, binding.device_id
            );
            println!("Synchronization remains disabled. No records were uploaded or downloaded.");
        }
    }
    Ok(())
}
