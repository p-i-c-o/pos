//! Client-side login, session storage and logout against a disposable mock
//! HTTP server. No PostgreSQL, real tokens or local SQLite are accessed.

use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

const USER: &str = "00000000-0000-0000-0000-000000000002";
const DEVICE: &str = "00000000-0000-0000-0000-000000000003";
const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const ROTATED: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

struct Exchange {
    method: &'static str,
    path: &'static str,
    expected_auth: Option<&'static str>,
    request_content: Option<&'static str>,
    status: u16,
    body: String,
}

fn spawn(exchanges: Vec<Exchange>) -> (u16, thread::JoinHandle<()>) {
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    server.set_nonblocking(true).unwrap();
    let port = server.local_addr().unwrap().port();
    let thread = thread::spawn(move || {
        for expected in exchanges {
            let started = Instant::now();
            let mut stream = loop {
                match server.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            started.elapsed() < Duration::from_secs(8),
                            "expected HTTP request not received"
                        );
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("accept: {error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut read = Vec::new();
            let (header, body) = loop {
                let mut chunk = [0u8; 4096];
                let count = stream.read(&mut chunk).unwrap();
                assert!(count > 0, "unexpected EOF in request");
                read.extend_from_slice(&chunk[..count]);
                assert!(read.len() <= 8192, "request too long");
                if let Some(pos) = read.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    let head = String::from_utf8(read[..pos].to_vec()).unwrap();
                    let length = head
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|value| value.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if read.len() >= pos + 4 + length {
                        break (
                            head,
                            String::from_utf8(read[pos + 4..pos + 4 + length].to_vec()).unwrap(),
                        );
                    }
                }
            };
            assert!(
                header.starts_with(&format!("{} {} HTTP/1.1", expected.method, expected.path)),
                "{header}"
            );
            let lower = header.to_ascii_lowercase();
            if let Some(token) = expected.expected_auth {
                assert!(
                    lower.contains(&format!("authorization: bearer {token}")),
                    "missing bearer header"
                );
            } else {
                assert!(
                    !lower.contains("authorization:"),
                    "login must not carry old credentials"
                );
            }
            if let Some(content) = expected.request_content {
                assert!(body.contains(content), "missing request JSON field");
            }
            let reason = match expected.status {
                200 => "OK",
                204 => "No Content",
                401 => "Unauthorized",
                _ => "Error",
            };
            let reply = format!(
                "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                expected.status,
                reason,
                expected.body.len(),
                expected.body
            );
            stream.write_all(reply.as_bytes()).unwrap();
            stream.flush().unwrap();
        }
    });
    (port, thread)
}

fn run(port: u16, root: &Path, args: &[&str], password: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pos"));
    command
        .args(["remote", "--url", &format!("http://127.0.0.1:{port}")])
        .args(args)
        .env_remove("POS_SERVER_TOKEN")
        .env_remove("POS_SERVER_TOKEN_FILE")
        .env_remove("POS_DB_PATH")
        .env("HOME", root)
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if password.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().unwrap();
    if let Some(password) = password {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(password.as_bytes())
            .unwrap();
    }
    child.wait_with_output().unwrap()
}

fn session_file(root: &Path) -> std::path::PathBuf {
    root.join("config/pos/session.token")
}

#[test]
fn first_login_is_private_then_me_uses_file_and_logout_revokes() {
    let root = tempfile::tempdir().unwrap();
    let response = format!(
        r#"{{"user_id":"{USER}","device_id":"{DEVICE}","token":"{TOKEN}","expires_in_seconds":86400}}"#
    );
    let (port, server) = spawn(vec![
        Exchange {
            method: "POST",
            path: "/api/v1/auth/login",
            expected_auth: None,
            request_content: Some(r#""device_label":"Test laptop""#),
            status: 200,
            body: response,
        },
        Exchange {
            method: "GET",
            path: "/api/v1/auth/me",
            expected_auth: Some(TOKEN),
            request_content: None,
            status: 200,
            body: format!(r#"{{"user_id":"{USER}","device_id":"{DEVICE}"}}"#),
        },
        Exchange {
            method: "POST",
            path: "/api/v1/auth/logout",
            expected_auth: Some(TOKEN),
            request_content: None,
            status: 204,
            body: String::new(),
        },
    ]);

    let output = run(
        port,
        root.path(),
        &[
            "login",
            "--email",
            "test@example.test",
            "--device-label",
            "Test laptop",
            "--password-stdin",
        ],
        Some("a sufficiently long test password\n"),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains(TOKEN));
    let file = session_file(root.path());
    assert_eq!(fs::read_to_string(&file).unwrap().trim(), TOKEN);
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let metadata = file.with_extension("device");
    assert_eq!(fs::read_to_string(&metadata).unwrap().trim(), DEVICE);
    assert_eq!(
        fs::metadata(&metadata).unwrap().permissions().mode() & 0o777,
        0o600
    );

    let output = run(port, root.path(), &["me"], None);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(USER));

    let output = run(port, root.path(), &["logout"], None);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!file.exists());
    assert!(metadata.exists(), "device identity retained for next login");
    assert!(!root.path().join("data/pos").exists());
    server.join().unwrap();
}

#[test]
fn explicit_replace_reuses_device_and_revokes_previous_session() {
    let root = tempfile::tempdir().unwrap();
    let session = session_file(root.path());
    fs::create_dir_all(session.parent().unwrap()).unwrap();
    fs::set_permissions(session.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(&session, format!("{TOKEN}\n")).unwrap();
    fs::set_permissions(&session, fs::Permissions::from_mode(0o600)).unwrap();
    let device = session.with_extension("device");
    fs::write(&device, format!("{DEVICE}\n")).unwrap();
    fs::set_permissions(&device, fs::Permissions::from_mode(0o600)).unwrap();

    let output = run(
        1,
        root.path(),
        &["login", "--email", "test@example.test", "--password-stdin"],
        Some("a sufficiently long test password\n"),
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--replace"));
    assert_eq!(fs::read_to_string(&session).unwrap().trim(), TOKEN);

    let response = format!(
        r#"{{"user_id":"{USER}","device_id":"{DEVICE}","token":"{ROTATED}","expires_in_seconds":86400}}"#
    );
    let (port, server) = spawn(vec![
        Exchange {
            method: "POST",
            path: "/api/v1/auth/login",
            expected_auth: None,
            request_content: Some(r#""existing_device":"00000000-0000-0000-0000-000000000003""#),
            status: 200,
            body: response,
        },
        Exchange {
            method: "POST",
            path: "/api/v1/auth/logout",
            expected_auth: Some(TOKEN),
            request_content: None,
            status: 204,
            body: String::new(),
        },
    ]);
    let output = run(
        port,
        root.path(),
        &[
            "login",
            "--email",
            "test@example.test",
            "--password-stdin",
            "--replace",
        ],
        Some("a sufficiently long test password\n"),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_to_string(&session).unwrap().trim(), ROTATED);
    assert_eq!(fs::read_to_string(&device).unwrap().trim(), DEVICE);
    assert!(!root.path().join("data/pos").exists());
    server.join().unwrap();
}
