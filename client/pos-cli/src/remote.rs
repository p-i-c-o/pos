//! Read-only, loopback-only POS server client.
//!
//! This transport is deliberately independent of local SQLite and the TUI.
//! Server UUIDs are never converted to local integer record IDs.

use std::{env, error::Error, fmt, fs, path::PathBuf, time::Duration};

use clap::Subcommand;
use reqwest::{
    Client, StatusCode, Url,
    header::{AUTHORIZATION, HeaderValue},
    redirect::Policy,
};
use serde::Deserialize;
use uuid::Uuid;

const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const DEFAULT_SERVER: &str = "http://127.0.0.1:8787";

#[derive(Subcommand, Debug)]
pub(crate) enum RemoteAction {
    /// Confirm which user and device the bearer session represents.
    Me {
        #[arg(long)]
        json: bool,
    },
    /// List the authenticated user's accessible workspaces.
    Workspaces {
        #[arg(long)]
        json: bool,
    },
    /// List remote projects in one accessible workspace.
    Projects {
        /// Workspace UUID obtained with 'pos remote workspaces'.
        workspace: Uuid,
        #[arg(long)]
        json: bool,
    },
    /// List remote tasks in one accessible workspace.
    Tasks {
        /// Workspace UUID obtained with 'pos remote workspaces'.
        workspace: Uuid,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug)]
struct ClientError(&'static str);

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl Error for ClientError {}

fn error(message: &'static str) -> Box<dyn Error> {
    Box::new(ClientError(message))
}

fn server_url(override_url: Option<&str>) -> Result<Url, Box<dyn Error>> {
    let configured = override_url
        .map(str::to_owned)
        .or_else(|| env::var("POS_SERVER_URL").ok())
        .unwrap_or_else(|| DEFAULT_SERVER.into());
    let url = Url::parse(&configured)
        .map_err(|_| error("invalid POS server URL; use http://127.0.0.1:PORT"))?;

    // Until TLS and a trusted onboarding flow exist, bearer tokens must
    // NEVER be sent to arbitrary hosts, even over HTTPS. Loopback permits
    // accessing a remote POS through an SSH local port forward.
    // Explicit loopback IPs avoid hostname resolution / DNS rebinding.
    let loopback_host = matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "::1"));
    if url.scheme() != "http"
        || !loopback_host
        || url.port().is_none()
        || url.username() != ""
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(error(
            "POS server URL must be http://127.0.0.1:PORT (loopback only, no path, userinfo, query or fragment)",
        ));
    }
    Ok(url)
}

fn token() -> Result<HeaderValue, Box<dyn Error>> {
    let raw = match env::var_os("POS_SERVER_TOKEN_FILE") {
        Some(path) => {
            let path = PathBuf::from(path);
            let metadata = fs::symlink_metadata(&path)
                .map_err(|_| error("cannot read POS_SERVER_TOKEN_FILE"))?;
            if !metadata.file_type().is_file() || metadata.len() > 4096 {
                return Err(error("POS_SERVER_TOKEN_FILE must be a small regular file"));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode() & 0o077 != 0 {
                    return Err(error(
                        "POS_SERVER_TOKEN_FILE permissions must be owner-only (0600)",
                    ));
                }
            }
            fs::read_to_string(path).map_err(|_| error("cannot read POS_SERVER_TOKEN_FILE"))?
        }
        None => env::var("POS_SERVER_TOKEN").map_err(|_| {
            error("set POS_SERVER_TOKEN_FILE or POS_SERVER_TOKEN for remote access")
        })?,
    };
    let raw = raw.trim_end_matches(['\r', '\n']);
    if raw.is_empty()
        || raw.len() > 4096
        || !raw.is_ascii()
        || raw
            .bytes()
            .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
    {
        return Err(error("POS bearer token is missing or malformed"));
    }
    HeaderValue::from_str(&format!("Bearer {raw}"))
        .map_err(|_| error("POS bearer token is invalid"))
}

struct RemoteClient {
    http: Client,
    base: Url,
    authorization: HeaderValue,
}

impl RemoteClient {
    fn new(url: Option<&str>) -> Result<Self, Box<dyn Error>> {
        // Validate destination before acquiring a credential.
        let base = server_url(url)?;
        let authorization = token()?;
        let http = Client::builder()
            .timeout(Duration::from_secs(8))
            .connect_timeout(Duration::from_secs(3))
            .no_proxy()
            .redirect(Policy::none())
            .build()?;
        Ok(Self {
            http,
            base,
            authorization,
        })
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, Box<dyn Error>> {
        let url = self
            .base
            .join(path)
            .map_err(|_| error("invalid API route"))?;
        // The path comes only from static strings and parsed UUIDs.
        let response = self
            .http
            .get(url)
            .header(AUTHORIZATION, self.authorization.clone())
            .send()
            .await
            .map_err(|_| error("POS server unreachable or request timed out"))?;

        if !response.status().is_success() {
            return Err(error(match response.status() {
                StatusCode::UNAUTHORIZED => "session missing, expired or revoked (HTTP 401)",
                StatusCode::FORBIDDEN => "remote workspace access denied (HTTP 403)",
                StatusCode::NOT_FOUND => {
                    "remote API unavailable, or workspace not accessible (HTTP 404)"
                }
                StatusCode::SERVICE_UNAVAILABLE => "remote database unavailable (HTTP 503)",
                _ => "POS server rejected the request",
            }));
        }
        // A development server is trusted to authenticate the session, but
        // never trust unbounded response bodies from HTTP.
        let mut response = response;
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| error("incomplete POS server response"))?
        {
            if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                return Err(error("POS server response exceeds 1 MiB"));
            }
            body.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&body)
            .map_err(|_| error("POS server returned an incompatible response"))
    }
}

#[derive(Deserialize, serde::Serialize)]
struct Identity {
    user_id: Uuid,
    device_id: Uuid,
}

#[derive(Deserialize, serde::Serialize)]
struct Workspace {
    id: Uuid,
    name: String,
    kind: String,
    role: String,
    can_read: bool,
    can_write: bool,
    can_manage_members: bool,
}

#[derive(Deserialize, serde::Serialize)]
struct Project {
    id: Uuid,
    workspace_id: Uuid,
    name: String,
    slug: String,
    archived: bool,
    version: i64,
    created_at: String,
    updated_at: String,
}

#[derive(Deserialize, serde::Serialize)]
struct Task {
    id: Uuid,
    workspace_id: Uuid,
    project_id: Option<Uuid>,
    title: String,
    description: Option<String>,
    priority: String,
    status: String,
    due_date: Option<String>,
    completed: bool,
    version: i64,
    created_at: String,
    updated_at: String,
}

/// Server strings are untrusted terminal text. Do not print control codes.
fn display(value: &str) -> String {
    value
        .chars()
        .take(500)
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect()
}

fn output_json<T: serde::Serialize>(value: &T) -> Result<(), Box<dyn Error>> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

pub(crate) async fn execute(url: Option<&str>, action: RemoteAction) -> Result<(), Box<dyn Error>> {
    let client = RemoteClient::new(url)?;
    match action {
        RemoteAction::Me { json } => {
            let identity: Identity = client.get("/api/v1/auth/me").await?;
            if json {
                output_json(&identity)?;
            } else {
                println!("user:   {}", identity.user_id);
                println!("device: {}", identity.device_id);
            }
        }
        RemoteAction::Workspaces { json } => {
            let workspaces: Vec<Workspace> = client.get("/api/v1/workspaces").await?;
            if json {
                output_json(&workspaces)?;
            } else if workspaces.is_empty() {
                println!("No accessible workspaces");
            } else {
                for ws in &workspaces {
                    println!(
                        "{}  {}  [{} / {}]",
                        ws.id,
                        display(&ws.name),
                        display(&ws.kind),
                        display(&ws.role)
                    );
                }
            }
        }
        RemoteAction::Projects { workspace, json } => {
            let projects: Vec<Project> = client
                .get(&format!("/api/v1/workspaces/{workspace}/projects"))
                .await?;
            if json {
                output_json(&projects)?;
            } else if projects.is_empty() {
                println!("No remote projects in this workspace");
            } else {
                for project in &projects {
                    println!(
                        "{}  {} [{}]{}",
                        project.id,
                        display(&project.name),
                        display(&project.slug),
                        if project.archived { " (archived)" } else { "" }
                    );
                }
            }
        }
        RemoteAction::Tasks { workspace, json } => {
            let tasks: Vec<Task> = client
                .get(&format!("/api/v1/workspaces/{workspace}/tasks"))
                .await?;
            if json {
                output_json(&tasks)?;
            } else if tasks.is_empty() {
                println!("No remote tasks in this workspace");
            } else {
                for task in &tasks {
                    let mark = if task.completed { "x" } else { " " };
                    let due = task
                        .due_date
                        .as_deref()
                        .map(|value| format!(" due {}", display(value)))
                        .unwrap_or_default();
                    println!("{}  [{mark}] {}{due}", task.id, display(&task.title));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untrusted_terminal_text_cannot_inject_control_sequences() {
        assert_eq!(
            display("University\u{1b}[31m\nunsafe"),
            "University [31m unsafe"
        );
    }

    #[test]
    fn reject_unsafe_servers_before_network_use() {
        for url in [
            "https://example.com",
            "http://example.com:8787",
            "http://localhost:8787",
            "http://192.168.1.5:8787",
            "http://127.0.0.2:8787",
            "http://127.0.0.1",
            "http://127.0.0.1:8787/foo",
            "http://127.0.0.1:8787/?x=y",
            "http://user:secret@127.0.0.1:8787",
            "http://localhost:8787/#fragment",
            "file:///etc/passwd",
            "invalid",
        ] {
            assert!(server_url(Some(url)).is_err(), "accepted {url}");
        }
        assert!(server_url(Some("http://localhost:8787")).is_err());
        assert!(server_url(Some("http://127.0.0.1:8787")).is_ok());
        assert!(server_url(Some("http://[::1]:8787")).is_ok());
    }
}
