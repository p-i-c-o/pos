//! Noninteractive CLI behaviour must remain scriptable.

use std::process::Command;

fn pos(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_pos"))
        .args(args)
        .output()
        .expect("execute POS")
}

#[test]
fn help_succeeds_without_terminal() {
    let output = pos(&["--help"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("interactive terminal UI"));
}

#[test]
fn version_reports_package_version() {
    let output = pos(&["--version"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn missing_terminal_is_an_error() {
    let output = pos(&[]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("interactive terminal"));
}

#[test]
fn unsupported_command_is_an_error() {
    let output = pos(&["unknown-command"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unrecognized subcommand"));
}

#[test]
fn persists_full_task_lifecycle_across_processes() {
    use std::path::Path;

    fn run(db: &Path, args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_pos"))
            .arg("--db")
            .arg(db)
            .args(args)
            .output()
            .expect("run POS")
    }

    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("tasks.sqlite3");

    let created = run(
        &db,
        &["tasks", "add", "  Finish report  ", "--due", "2026-12-17"],
    );
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    assert!(String::from_utf8_lossy(&created.stdout).contains("Created #1: Finish report"));

    let invalid = run(&db, &["tasks", "add", "Bad", "--due", "2026-02-30"]);
    assert!(!invalid.status.success());

    let listed = run(&db, &["tasks", "list", "--json"]);
    assert!(listed.status.success());
    let tasks: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(tasks[0]["title"], "Finish report");
    assert_eq!(tasks[0]["due_date"], "2026-12-17");

    assert!(
        run(
            &db,
            &["tasks", "edit", "1", "--title", "Revised", "--clear-due"]
        )
        .status
        .success()
    );
    assert!(run(&db, &["tasks", "done", "1"]).status.success());
    assert_eq!(
        String::from_utf8_lossy(&run(&db, &["tasks", "list"]).stdout).trim(),
        ""
    );
    assert!(
        String::from_utf8_lossy(&run(&db, &["tasks", "list", "--all"]).stdout)
            .contains("[x] Revised")
    );

    assert!(run(&db, &["tasks", "reopen", "1"]).status.success());
    assert!(String::from_utf8_lossy(&run(&db, &["tasks", "list"]).stdout).contains("[ ] Revised"));

    let denied = run(&db, &["tasks", "delete", "1"]);
    assert!(!denied.status.success());
    assert!(String::from_utf8_lossy(&run(&db, &["tasks", "list"]).stdout).contains("Revised"));

    assert!(
        run(&db, &["tasks", "delete", "1", "--yes"])
            .status
            .success()
    );
    let missing = run(&db, &["tasks", "show", "1"]);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("not found"));
}
