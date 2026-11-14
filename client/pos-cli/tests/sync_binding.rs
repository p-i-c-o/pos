//! POS sync binding CLI integration: authenticated GET only; no server writes.
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    process::{Command, Output},
    thread,
    time::{Duration, Instant},
};

const USER: &str = "ac764621-c770-4bf4-89ee-25e1234dec99";
const DEVICE: &str = "6831fd40-f38c-4a98-9cde-658567b38b67";
const WS: &str = "ed91a5ab-f2f7-4c23-a69a-c512e9667cd5";
const TOKEN: &str = "test-only-bearer";

fn mock_workspace(readable: bool, repeats: usize) -> (u16, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let worker = thread::spawn(move || {
        for _ in 0..repeats {
            for (path, content) in [
                (
                    "/api/v1/auth/me",
                    format!(r#"{{"user_id":"{USER}","device_id":"{DEVICE}"}}"#),
                ),
                (
                    "/api/v1/workspaces",
                    format!(
                        r#"[{{"id":"{WS}","name":"Personal","kind":"personal","role":"owner","can_read":{readable},"can_write":true,"can_manage_members":true}}]"#
                    ),
                ),
            ] {
                let started = Instant::now();
                let mut conn = loop {
                    match listener.accept() {
                        Ok((conn, _)) => break conn,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(
                                started.elapsed() < Duration::from_secs(8),
                                "missing request {path}"
                            );
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(e) => panic!("socket accept: {e}"),
                    }
                };
                conn.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
                let mut received = Vec::new();
                while !received.windows(4).any(|v| v == b"\r\n\r\n") {
                    let mut chunk = [0u8; 2048];
                    let n = conn.read(&mut chunk).unwrap();
                    assert!(n > 0);
                    received.extend_from_slice(&chunk[..n]);
                    assert!(received.len() < 8192);
                }
                let head = String::from_utf8(received).unwrap();
                assert!(head.starts_with(&format!("GET {path} HTTP/1.1")), "{head}");
                assert!(
                    head.to_lowercase()
                        .contains(&format!("authorization: bearer {TOKEN}\r\n"))
                );
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{content}",
                    content.len()
                );
                conn.write_all(response.as_bytes()).unwrap();
            }
        }
    });
    (port, worker)
}

fn pos(root: &Path, db: &Path, args: &[&str], token: bool) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_pos"));
    c.arg("--db")
        .arg(db)
        .args(args)
        .env("HOME", root)
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env_remove("POS_SERVER_TOKEN_FILE")
        .env_remove("POS_SERVER_URL")
        .env_remove("POS_DB_PATH");
    if token {
        c.env("POS_SERVER_TOKEN", TOKEN);
    } else {
        c.env_remove("POS_SERVER_TOKEN");
    }
    c.output().unwrap()
}

fn success(out: &Output) {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn verified_workspace_bind_is_immutable_and_clone_fenced() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("original.sqlite3");
    success(&pos(
        dir.path(),
        &original,
        &["projects", "add", "Existing"],
        false,
    ));
    let before = pos(dir.path(), &original, &["sync", "status", "--json"], false);
    success(&before);
    let prior: serde_json::Value = serde_json::from_slice(&before.stdout).unwrap();
    assert_eq!(prior["bound"], false);
    assert_eq!(prior["journal_entries"], 1);

    let (port, server) = mock_workspace(true, 2);
    let url = format!("http://127.0.0.1:{port}");
    let first = pos(
        dir.path(),
        &original,
        &["sync", "bind", WS, "--url", &url],
        true,
    );
    success(&first);
    assert!(String::from_utf8_lossy(&first.stdout).contains("Synchronization remains disabled"));

    let repeat = pos(
        dir.path(),
        &original,
        &["sync", "bind", WS, "--url", &url],
        true,
    );
    success(&repeat);
    assert!(String::from_utf8_lossy(&repeat.stdout).contains("already bound"));
    server.join().unwrap();

    let after = pos(dir.path(), &original, &["sync", "status", "--json"], false);
    success(&after);
    let state: serde_json::Value = serde_json::from_slice(&after.stdout).unwrap();
    assert_eq!(state["bound"], true);
    assert_eq!(state["workspace_id"], WS);
    assert_eq!(state["user_id"], USER);
    assert_eq!(state["device_id"], DEVICE);
    assert_eq!(state["journal_entries"], 1);
    assert_eq!(state["mapped_records"], 0);
    assert_eq!(state["sync_enabled"], false);
    assert_eq!(state["database_id"], prior["database_id"]);
    let anchor = dir
        .path()
        .join("config/pos/sync-anchors")
        .join(format!("{}.json", state["database_id"].as_str().unwrap()));
    assert!(anchor.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&anchor).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(anchor.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }

    let backup = dir.path().join("backup.sqlite3");
    success(&pos(
        dir.path(),
        &original,
        &["data", "backup", backup.to_str().unwrap()],
        false,
    ));
    let cloned = pos(dir.path(), &backup, &["sync", "status"], false);
    assert!(!cloned.status.success());
    assert!(String::from_utf8_lossy(&cloned.stderr).contains("moved or cloned"));
    assert!(!String::from_utf8_lossy(&cloned.stderr).contains(TOKEN));
    success(&pos(dir.path(), &original, &["sync", "status"], false));

    // Simulate another device without its private, external anchor.
    let other_config = dir.path().join("other-config");
    let absent = Command::new(env!("CARGO_BIN_EXE_pos"))
        .arg("--db")
        .arg(&original)
        .args(["sync", "status"])
        .env("HOME", dir.path())
        .env("XDG_CONFIG_HOME", &other_config)
        .output()
        .unwrap();
    assert!(!absent.status.success());
    assert!(String::from_utf8_lossy(&absent.stderr).contains("anchor"));
    success(&pos(dir.path(), &original, &["sync", "status"], false));
}

#[test]
fn denied_remote_workspace_causes_no_database_creation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("not-created.sqlite3");
    let (port, server) = mock_workspace(false, 1);
    let url = format!("http://127.0.0.1:{port}");
    let rejected = pos(
        dir.path(),
        &path,
        &["sync", "bind", WS, "--url", &url],
        true,
    );
    server.join().unwrap();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("not readable"));
    assert!(
        !path.exists(),
        "failed auth must not initialize local SQLite"
    );
    assert!(!dir.path().join("config/pos/sync-anchors").exists());
}

#[test]
fn status_on_missing_database_does_not_create_it() {
    let dir = tempfile::tempdir().unwrap();
    let absent = dir.path().join("missing.sqlite3");
    let result = pos(dir.path(), &absent, &["sync", "status"], false);
    assert!(!result.status.success());
    assert!(!absent.exists());
}
