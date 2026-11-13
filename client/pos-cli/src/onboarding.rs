//! Interactive, development-only device session onboarding.
//! Credentials stay outside SQLite and POS workspace data.

use std::{
    env,
    error::Error,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use reqwest::{
    Client, StatusCode, Url,
    header::{AUTHORIZATION, HeaderValue},
    redirect::Policy,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::remote;

fn err(message: &'static str) -> Box<dyn Error> {
    io::Error::other(message).into()
}

/// The default file is compatible with the raw bearer-token file from PR #16.
/// A configured token file takes precedence and is never silently ignored.
pub(crate) fn session_path() -> Result<PathBuf, Box<dyn Error>> {
    if let Some(path) = env::var_os("POS_SERVER_TOKEN_FILE") {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err(err("POS_SERVER_TOKEN_FILE must be an absolute path"));
        }
        return Ok(path);
    }
    let root = if let Some(xdg) = env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(xdg)
    } else {
        let home = env::var_os("HOME").ok_or_else(|| err("HOME is required for POS sign-in"))?;
        PathBuf::from(home).join(".config")
    };
    if !root.is_absolute() {
        return Err(err("POS configuration directory must be an absolute path"));
    }
    Ok(root.join("pos/session.token"))
}

fn device_path(token_path: &Path) -> PathBuf {
    token_path.with_extension("device")
}

fn inspect_private_file(path: &Path) -> Result<bool, Box<dyn Error>> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            if !meta.file_type().is_file() {
                return Err(err("credential path must be a regular file, not a symlink"));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if meta.permissions().mode() & 0o077 != 0 {
                    return Err(err("credential file must be owner-only (0600)"));
                }
            }
            if meta.len() > 4096 {
                return Err(err("credential file exceeds 4 KiB"));
            }
            Ok(true)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(err("cannot inspect credential path")),
    }
}

fn private_parent(path: &Path) -> Result<(), Box<dyn Error>> {
    let parent = path
        .parent()
        .ok_or_else(|| err("credential path needs a parent directory"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
        let meta = fs::symlink_metadata(parent)?;
        if !meta.is_dir() || meta.permissions().mode() & 0o077 != 0 {
            return Err(err("POS credential directory must be private (0700)"));
        }
    }
    #[cfg(not(unix))]
    {
        return Err(err(
            "session-file onboarding currently requires Unix permissions",
        ));
    }
    Ok(())
}

/// Explicit replacement is atomic and never truncates the old credential.
fn save_private(path: &Path, contents: &str, replace: bool) -> Result<(), Box<dyn Error>> {
    private_parent(path)?;
    let existed = inspect_private_file(path)?;
    if existed && !replace {
        return Err(err("credential file exists; use --replace to rotate it"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| err("credential path needs a parent directory"))?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(contents.as_bytes())?;
    file.write_all(b"\n")?;
    file.as_file_mut().sync_all()?;
    // The temporary file is created as 0600; persist_noclobber is used
    // unless a replacement was specifically authorized.
    if replace {
        file.persist(path)
            .map_err(|_| err("cannot replace private credential file"))?;
    } else {
        file.persist_noclobber(path)
            .map_err(|_| err("credential file already exists or cannot be created"))?;
    }
    Ok(())
}

fn device_id(path: &Path) -> Result<Option<Uuid>, Box<dyn Error>> {
    if !inspect_private_file(path)? {
        return Ok(None);
    }
    let value = fs::read_to_string(path).map_err(|_| err("cannot read POS device identity"))?;
    let id =
        Uuid::parse_str(value.trim()).map_err(|_| err("invalid stored POS device identity"))?;
    Ok(Some(id))
}

fn password(stdin: bool) -> Result<Zeroizing<String>, Box<dyn Error>> {
    if stdin {
        let mut bytes = Vec::new();
        io::stdin().take(1025).read_to_end(&mut bytes)?;
        let mut value = Zeroizing::new(String::from_utf8(bytes)?);
        while value.ends_with(['\r', '\n']) {
            value.pop();
        }
        return Ok(value);
    }
    Ok(Zeroizing::new(rpassword::prompt_password(
        "POS password: ",
    )?))
}

fn http() -> Result<Client, Box<dyn Error>> {
    Ok(Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(3))
        .no_proxy()
        .redirect(Policy::none())
        .build()?)
}

#[derive(Serialize)]
struct LoginRequest<'a> {
    email: &'a str,
    password: &'a str,
    device_label: &'a str,
    existing_device: Option<Uuid>,
}

#[derive(Deserialize)]
struct LoginReply {
    user_id: Uuid,
    device_id: Uuid,
    token: String,
    expires_in_seconds: u64,
}

pub(crate) async fn login(
    url: Option<&str>,
    email: &str,
    label: &str,
    read_stdin: bool,
    replace: bool,
    new_device: bool,
) -> Result<(), Box<dyn Error>> {
    let base: Url = remote::server_url(url)?;
    if env::var_os("POS_SERVER_TOKEN").is_some() && env::var_os("POS_SERVER_TOKEN_FILE").is_none() {
        return Err(err(
            "unset POS_SERVER_TOKEN before login; POS stores sessions in a private file",
        ));
    }
    let path = session_path()?;
    let existing = inspect_private_file(&path)?;
    if existing && !replace {
        return Err(err(
            "POS session file already exists; use --replace to reauthenticate",
        ));
    }
    let previous = if existing {
        Some(remote::token_from_file(&path)?)
    } else {
        None
    };
    let known_device = if new_device {
        None
    } else {
        device_id(&device_path(&path))?
    };
    // Validate local storage before sending a password or enrolling a device.
    private_parent(&path)?;
    let pass = password(read_stdin)?;
    if !(12..=1024).contains(&pass.len()) {
        return Err(err("POS password must be between 12 and 1024 bytes"));
    }
    let request = LoginRequest {
        email,
        password: &pass,
        device_label: label,
        existing_device: known_device,
    };
    let response = http()?
        .post(base.join("/api/v1/auth/login")?)
        .json(&request)
        .send()
        .await
        .map_err(|_| err("POS login server unreachable or request timed out"))?;
    match response.status() {
        StatusCode::OK => {}
        StatusCode::UNAUTHORIZED => {
            return Err(err(
                "invalid account credentials or revoked device (HTTP 401)",
            ));
        }
        StatusCode::TOO_MANY_REQUESTS => {
            return Err(err(
                "too many login attempts; retry after one minute (HTTP 429)",
            ));
        }
        StatusCode::BAD_REQUEST => return Err(err("invalid login request (HTTP 400)")),
        StatusCode::NOT_FOUND => return Err(err("POS server login API is unavailable (HTTP 404)")),
        StatusCode::SERVICE_UNAVAILABLE => {
            return Err(err("POS identity database unavailable (HTTP 503)"));
        }
        _ => return Err(err("POS server rejected login")),
    }
    let mut response = response;
    let mut body = Zeroizing::new(Vec::new());
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| err("invalid login response"))?
    {
        if body.len().saturating_add(chunk.len()) > 4096 {
            return Err(err("login response exceeded 4 KiB"));
        }
        body.extend_from_slice(&chunk);
    }
    let reply: LoginReply =
        serde_json::from_slice(&body).map_err(|_| err("invalid login response"))?;
    if reply.token.len() != 64
        || !reply
            .token
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || reply.expires_in_seconds != 24 * 3600
    {
        return Err(err("invalid server-issued POS session"));
    }

    // Keep the device UUID in a separate owner-only metadata file, so a
    // later login can reuse it rather than creating a new device each day.
    let metadata = device_path(&path);
    let meta_replace = inspect_private_file(&metadata)?;
    save_private(&metadata, &reply.device_id.to_string(), meta_replace)?;
    // Persist the new session atomically; never print it to stdout/stderr.
    let secret = Zeroizing::new(reply.token);
    save_private(&path, &secret, replace)?;
    if let Some(old) = previous {
        // Old session is revoked after the replacement has been stored. A
        // server outage must not leave the user without the new credential.
        match http()?
            .post(base.join("/api/v1/auth/logout")?)
            .header(AUTHORIZATION, old)
            .send()
            .await
        {
            Ok(reply)
                if matches!(
                    reply.status(),
                    StatusCode::NO_CONTENT | StatusCode::UNAUTHORIZED
                ) => {}
            _ => eprintln!(
                "Warning: previous POS session may remain active until its 24-hour expiry"
            ),
        }
    }
    println!(
        "POS signed in as {} on device {}",
        reply.user_id, reply.device_id
    );
    println!("Session saved to {} (24-hour expiry)", path.display());
    Ok(())
}

pub(crate) async fn logout(url: Option<&str>) -> Result<(), Box<dyn Error>> {
    let path = session_path()?;
    if !inspect_private_file(&path)? {
        return Err(err("no session file found to log out"));
    }
    let base = remote::server_url(url)?;
    let authorization: HeaderValue = remote::token_from_file(&path)?;
    let response = http()?
        .post(base.join("/api/v1/auth/logout")?)
        .header(AUTHORIZATION, authorization)
        .send()
        .await
        .map_err(|_| err("POS logout server unreachable; local session retained"))?;
    if response.status() == StatusCode::UNAUTHORIZED {
        return Err(err(
            "POS session already expired or revoked; use login --replace to renew",
        ));
    }
    if response.status() != StatusCode::NO_CONTENT {
        return Err(err(
            "POS server did not confirm session revocation; local session retained",
        ));
    }
    // Keep device metadata, enabling re-authentication on the same device.
    fs::remove_file(&path)
        .map_err(|_| err("session revoked, but local credential removal failed"))?;
    println!("POS session revoked and local token file removed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secure_files_never_overwrite_without_explicit_replace() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("private/session.token");
        save_private(&path, "first", false).unwrap();
        assert!(save_private(&path, "second", false).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "first\n");
        save_private(&path, "second", true).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn device_metadata_roundtrips_without_password_or_session() {
        let dir = tempfile::tempdir().unwrap();
        let token = dir.path().join("private/session.token");
        let id = Uuid::parse_str("5e41e814-94c8-4b31-9d0a-b857799f705d").unwrap();
        save_private(&device_path(&token), &id.to_string(), false).unwrap();
        assert_eq!(device_id(&device_path(&token)).unwrap(), Some(id));
        assert!(!token.exists());
    }
}
