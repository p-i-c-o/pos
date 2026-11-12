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

#[test]
fn project_lifecycle_and_task_links_survive_separate_processes() {
    use std::path::Path;

    fn run(db: &Path, args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_pos"))
            .arg("--db")
            .arg(db)
            .args(args)
            .output()
            .expect("execute POS")
    }

    fn ok(db: &Path, args: &[&str]) -> String {
        let output = run(db, args);
        assert!(
            output.status.success(),
            "{}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn err(db: &Path, args: &[&str]) -> String {
        let output = run(db, args);
        assert!(
            !output.status.success(),
            "expected failure for: {}",
            args.join(" ")
        );
        String::from_utf8_lossy(&output.stderr).into_owned()
    }

    let directory = tempfile::tempdir().unwrap();
    let db = directory.path().join("pos.sqlite3");

    assert!(ok(&db, &["projects", "add", "University Work"]).contains("[university-work]"));
    assert!(ok(&db, &["projects", "add", "Personal", "--slug", "personal"]).contains("personal"));
    assert!(
        err(
            &db,
            &["projects", "add", "Duplicate", "--slug", "university-work"]
        )
        .contains("already exists")
    );
    assert!(err(&db, &["projects", "add", "Broken", "--slug", "../break"]).contains("slug"));

    assert!(
        ok(
            &db,
            &[
                "tasks",
                "add",
                "Write report",
                "--project",
                "university-work"
            ]
        )
        .contains("Created #1")
    );
    assert!(ok(&db, &["tasks", "add", "Buy groceries"]).contains("Created #2"));
    assert!(
        err(
            &db,
            &["tasks", "add", "Not allowed", "--project", "missing"]
        )
        .contains("not found")
    );
    assert!(ok(&db, &["tasks", "list", "--project", "university-work"]).contains("Write report"));
    assert!(!ok(&db, &["tasks", "list", "--project", "university-work"]).contains("groceries"));
    assert!(err(&db, &["tasks", "list", "--project", "missing"]).contains("not found"));

    let renamed = ok(
        &db,
        &["projects", "edit", "1", "--name", "Warwick University"],
    );
    assert!(renamed.contains("[university-work]"));
    let projects_json: serde_json::Value =
        serde_json::from_str(&ok(&db, &["projects", "list", "--json"])).unwrap();
    assert_eq!(projects_json[1]["slug"], "university-work");
    assert!(ok(&db, &["tasks", "list", "--project", "university-work"]).contains("Write report"));

    assert!(ok(&db, &["tasks", "edit", "2", "--project", "personal"]).contains("Updated #2"));
    assert!(ok(&db, &["tasks", "list", "--project", "personal"]).contains("Buy groceries"));
    assert!(ok(&db, &["tasks", "edit", "2", "--clear-project"]).contains("Updated #2"));
    assert!(ok(&db, &["tasks", "list", "--project", "personal"]).is_empty());

    assert!(ok(&db, &["projects", "archive", "1"]).contains("Archived"));
    assert!(ok(&db, &["projects", "list"]).contains("Personal"));
    assert!(!ok(&db, &["projects", "list"]).contains("Warwick"));
    assert!(ok(&db, &["projects", "list", "--all"]).contains("Warwick"));
    assert!(err(&db, &["tasks", "add", "No", "--project", "university-work"]).contains("archived"));
    assert!(err(&db, &["tasks", "edit", "2", "--project", "university-work"]).contains("archived"));
    assert!(ok(&db, &["tasks", "list", "--project", "university-work"]).contains("Write report"));
    assert!(ok(&db, &["projects", "unarchive", "1"]).contains("Unarchived"));

    assert!(err(&db, &["projects", "delete", "1"]).contains("--yes"));
    assert!(ok(&db, &["tasks", "list", "--project", "university-work"]).contains("Write report"));
    assert!(ok(&db, &["projects", "delete", "1", "--yes"]).contains("tasks remain unassigned"));
    let tasks_json: serde_json::Value =
        serde_json::from_str(&ok(&db, &["tasks", "list", "--all", "--json"])).unwrap();
    assert_eq!(tasks_json.as_array().unwrap().len(), 2);
    assert_eq!(tasks_json[0]["project_id"], serde_json::Value::Null);
    assert_eq!(tasks_json[1]["project_id"], serde_json::Value::Null);
    assert!(err(&db, &["projects", "show", "1"]).contains("not found"));
}
