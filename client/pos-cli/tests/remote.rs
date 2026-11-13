//! POS CLI read-only client regression tests using a disposable loopback API.
//! No PostgreSQL, persistent SQLite or real credentials involved.

use std::{
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Output},
    thread,
    time::{Duration, Instant},
};

const WS: &str = "00000000-0000-0000-0000-000000000001";
const USER: &str = "00000000-0000-0000-0000-000000000002";
const DEVICE: &str = "00000000-0000-0000-0000-000000000003";
const RECORD: &str = "00000000-0000-0000-0000-000000000004";

fn mock(expected_path: String, status: u16, body: String) -> (u16, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let handle = thread::spawn(move || {
        let start = Instant::now();
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        start.elapsed() < Duration::from_secs(8),
                        "no HTTP request received"
                    );
                    thread::sleep(Duration::from_millis(10));
                }
                Err(e) => panic!("cannot accept: {e}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        loop {
            let mut bytes = [0u8; 1024];
            let read = stream.read(&mut bytes).unwrap();
            if read == 0 {
                break;
            }
            request.extend_from_slice(&bytes[..read]);
            if request.windows(4).any(|part| part == b"\r\n\r\n") {
                break;
            }
            assert!(request.len() <= 8192, "unbounded HTTP request headers");
        }
        let request = String::from_utf8(request).unwrap();
        assert!(request.starts_with(&format!("GET {expected_path} HTTP/1.1\r\n")));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("\r\nauthorization: bearer test-session-token\r\n"),
            "authenticated GET is mandatory"
        );
        let reason = match status {
            200 => "OK",
            302 => "Found",
            401 => "Unauthorized",
            404 => "Not Found",
            503 => "Service Unavailable",
            _ => "Error",
        };
        let response = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).unwrap();
        stream.flush().unwrap();
    });
    (port, handle)
}

fn pos(port: u16, args: &[&str], home: &std::path::Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pos"))
        .args(["remote", "--url", &format!("http://127.0.0.1:{port}")])
        .args(args)
        .env("POS_SERVER_TOKEN", "test-session-token")
        .env_remove("POS_SERVER_TOKEN_FILE")
        .env_remove("POS_DB_PATH")
        .env_remove("POS_SERVER_URL")
        .env("HOME", home)
        .env("XDG_DATA_HOME", home.join("data"))
        .output()
        .unwrap()
}

#[test]
fn remote_inspection_reads_four_routes_without_creating_sqlite() {
    let home = tempfile::tempdir().unwrap();
    let me = format!(r#"{{"user_id":"{USER}","device_id":"{DEVICE}"}}"#);
    let workspaces = format!(
        r#"[{{"id":"{WS}","name":"Private","kind":"personal","role":"owner","can_read":true,"can_write":true,"can_manage_members":true}}]"#
    );
    let projects = format!(
        r#"[{{"id":"{RECORD}","workspace_id":"{WS}","name":"University","slug":"university","archived":false,"version":1,"created_at":"2026-01-01","updated_at":"2026-01-01"}}]"#
    );
    let tasks = format!(
        r#"[{{"id":"{RECORD}","workspace_id":"{WS}","project_id":null,"title":"Write report","description":null,"priority":"high","status":"todo","due_date":"2026-12-17","completed":false,"version":1,"created_at":"2026-01-01","updated_at":"2026-01-01"}}]"#
    );
    let scenarios: Vec<(String, Vec<&str>, String, &str)> = vec![
        ("/api/v1/auth/me".into(), vec!["me", "--json"], me, USER),
        (
            "/api/v1/workspaces".into(),
            vec!["workspaces"],
            workspaces,
            "Private",
        ),
        (
            format!("/api/v1/workspaces/{WS}/projects"),
            vec!["projects", WS],
            projects,
            "University",
        ),
        (
            format!("/api/v1/workspaces/{WS}/tasks"),
            vec!["tasks", WS, "--json"],
            tasks,
            "Write report",
        ),
    ];
    for (path, args, response, expected) in scenarios {
        let (port, handle) = mock(path, 200, response);
        let output = pos(port, &args, home.path());
        handle.join().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains(expected));
    }
    assert!(
        !home.path().join("data/pos").exists(),
        "remote commands must not touch local SQLite"
    );
}

#[test]
fn errors_are_safe_and_never_print_credentials() {
    let home = tempfile::tempdir().unwrap();
    for (status, needle) in [(401, "HTTP 401"), (404, "HTTP 404"), (503, "HTTP 503")] {
        let (port, handle) = mock("/api/v1/auth/me".into(), status, "{}".to_string());
        let output = pos(port, &["me"], home.path());
        handle.join().unwrap();
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(needle));
        assert!(!stderr.contains("test-session-token"));
    }
    // No automatic retry across a server-side redirect.
    let (port, handle) = mock("/api/v1/auth/me".into(), 302, "{}".to_string());
    let output = pos(port, &["me"], home.path());
    handle.join().unwrap();
    assert!(!output.status.success());
}

#[test]
fn rejects_non_loopback_and_missing_token_without_local_io() {
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_pos"))
        .args(["remote", "--url", "http://example.com:8787", "me"])
        .env("POS_SERVER_TOKEN", "test-session-token")
        .env_remove("POS_SERVER_TOKEN_FILE")
        .env("HOME", home.path())
        .env("XDG_DATA_HOME", home.path().join("data"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("loopback only"));

    let output = Command::new(env!("CARGO_BIN_EXE_pos"))
        .args(["remote", "me"])
        .env_remove("POS_SERVER_TOKEN")
        .env_remove("POS_SERVER_TOKEN_FILE")
        .env("HOME", home.path())
        .env("XDG_DATA_HOME", home.path().join("data"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("set POS_SERVER_TOKEN"));
    assert!(!home.path().join("data/pos").exists());
}

#[cfg(unix)]
#[test]
fn token_file_must_be_private_and_not_a_symlink() {
    use std::{
        fs,
        os::unix::fs::{PermissionsExt, symlink},
    };
    let home = tempfile::tempdir().unwrap();
    let token_path = home.path().join("session.token");
    fs::write(&token_path, "test-session-token\n").unwrap();
    fs::set_permissions(&token_path, fs::Permissions::from_mode(0o644)).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_pos"))
        .args(["remote", "me"])
        .env("POS_SERVER_TOKEN_FILE", &token_path)
        .env_remove("POS_SERVER_TOKEN")
        .env("HOME", home.path())
        .env("XDG_DATA_HOME", home.path().join("data"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("0600"));

    let symlink_path = home.path().join("link.token");
    fs::set_permissions(&token_path, fs::Permissions::from_mode(0o600)).unwrap();
    symlink(&token_path, &symlink_path).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_pos"))
        .args(["remote", "me"])
        .env("POS_SERVER_TOKEN_FILE", &symlink_path)
        .env_remove("POS_SERVER_TOKEN")
        .env("HOME", home.path())
        .env("XDG_DATA_HOME", home.path().join("data"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("regular file"));

    let (port, handle) = mock(
        "/api/v1/auth/me".into(),
        200,
        format!(r#"{{"user_id":"{USER}","device_id":"{DEVICE}"}}"#),
    );
    let output = Command::new(env!("CARGO_BIN_EXE_pos"))
        .args(["remote", "--url", &format!("http://127.0.0.1:{port}"), "me"])
        .env("POS_SERVER_TOKEN_FILE", &token_path)
        .env_remove("POS_SERVER_TOKEN")
        .env("HOME", home.path())
        .env("XDG_DATA_HOME", home.path().join("data"))
        .output()
        .unwrap();
    handle.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!home.path().join("data/pos").exists());
}

#[test]
fn malformed_remote_json_is_rejected_without_printing_body() {
    let home = tempfile::tempdir().unwrap();
    let (port, handle) = mock("/api/v1/auth/me".into(), 200, "secret output body".into());
    let output = pos(port, &["me"], home.path());
    handle.join().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("incompatible response"));
    assert!(!stderr.contains("secret output body"));
}
