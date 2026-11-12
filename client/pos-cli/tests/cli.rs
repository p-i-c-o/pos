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

#[test]
fn backup_restore_export_and_integrity_work_across_processes() {
    use std::{fs, path::Path};

    fn run(db: Option<&Path>, args: &[&str]) -> std::process::Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pos"));
        if let Some(db) = db {
            cmd.arg("--db").arg(db);
        }
        cmd.args(args).output().expect("POS command")
    }
    fn success(db: Option<&Path>, args: &[&str]) -> String {
        let result = run(db, args);
        assert!(
            result.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8_lossy(&result.stdout).into_owned()
    }
    fn fails(db: Option<&Path>, args: &[&str]) -> String {
        let result = run(db, args);
        assert!(!result.status.success(), "unexpected success: {args:?}");
        String::from_utf8_lossy(&result.stderr).into_owned()
    }

    let dir = tempfile::tempdir().unwrap();
    let live = dir.path().join("live.sqlite3");
    let backup = dir.path().join("backup.sqlite3");
    let restored = dir.path().join("restored.sqlite3");
    let json = dir.path().join("snapshot.json");

    let backup_str = backup.to_str().unwrap();
    let restored_str = restored.to_str().unwrap();
    let json_str = json.to_str().unwrap();

    assert!(success(Some(&live), &["projects", "add", "Research"]).contains("research"));
    assert!(
        success(
            Some(&live),
            &["tasks", "add", "First task", "--project", "research"]
        )
        .contains("Created")
    );
    assert!(success(Some(&live), &["tasks", "done", "1"]).contains("Completed"));
    assert!(success(Some(&live), &["tasks", "add", "Second task"]).contains("Created"));
    assert!(success(Some(&live), &["data", "check"]).contains("integrity and foreign keys OK"));

    let stdout_json = success(Some(&live), &["data", "export"]);
    let parsed: serde_json::Value = serde_json::from_str(&stdout_json).unwrap();
    assert_eq!(parsed["format_version"], 2);
    assert_eq!(parsed["projects"][0]["slug"], "research");
    assert_eq!(parsed["tasks"].as_array().unwrap().len(), 2);
    assert_eq!(parsed["tasks"][0]["project_id"], 1);
    assert_eq!(parsed["tasks"][0]["completed"], true);
    assert_eq!(parsed["tasks"][1]["project_id"], serde_json::Value::Null);

    // Export files must never be silently replaced.
    success(Some(&live), &["data", "export", "--output", json_str]);
    assert!(String::from_utf8_lossy(&fs::read(&json).unwrap()).contains("\"format_version\": 2"));
    let sentinel = fs::read(&json).unwrap();
    assert!(
        fails(Some(&live), &["data", "export", "--output", json_str]).contains("already exists")
    );
    assert_eq!(fs::read(&json).unwrap(), sentinel);

    success(Some(&live), &["data", "backup", backup_str]);
    assert!(success(Some(&live), &["data", "check"]).contains("OK"));
    assert!(success(Some(&backup), &["data", "check"]).contains("OK"));

    assert!(fails(Some(&live), &["data", "backup", backup_str]).contains("already exists"));
    assert!(
        fails(Some(&live), &["data", "backup", live.to_str().unwrap()]).contains("already exists")
    );
    assert!(
        fails(
            Some(&live),
            &["data", "restore", backup_str, "--output", restored_str]
        )
        .contains("--db does not apply to restore")
    );
    assert!(!restored.exists());

    // State diverges after snapshot; restoring must not touch the live DB.
    success(Some(&live), &["tasks", "add", "After backup"]);
    assert!(
        success(
            None,
            &["data", "restore", backup_str, "--output", restored_str]
        )
        .contains("Verified and restored")
    );
    let recovered: serde_json::Value =
        serde_json::from_str(&success(Some(&restored), &["data", "export"])).unwrap();
    let live_data: serde_json::Value =
        serde_json::from_str(&success(Some(&live), &["data", "export"])).unwrap();
    assert_eq!(recovered["tasks"].as_array().unwrap().len(), 2);
    assert_eq!(live_data["tasks"].as_array().unwrap().len(), 3);
    assert_eq!(recovered["projects"][0]["slug"], "research");
    assert_eq!(recovered["tasks"][0]["completed"], true);

    // No overwrite on restore (including an existing recovered database).
    assert!(
        fails(
            None,
            &["data", "restore", backup_str, "--output", restored_str]
        )
        .contains("already exists")
    );
    assert_eq!(fs::read(&json).unwrap(), sentinel);

    // Invalid SQLite and missing source cannot produce partial restored files.
    let corrupt = dir.path().join("corrupt.sqlite3");
    fs::write(&corrupt, b"bad db").unwrap();
    let invalid_dest = dir.path().join("invalid.sqlite3");
    assert!(
        !run(
            None,
            &[
                "data",
                "restore",
                corrupt.to_str().unwrap(),
                "--output",
                invalid_dest.to_str().unwrap()
            ]
        )
        .status
        .success()
    );
    assert!(!invalid_dest.exists());
    let nonexistent = dir.path().join("missing.sqlite3");
    assert!(
        !run(Some(&nonexistent), &["data", "backup", "unused.sqlite3"])
            .status
            .success()
    );
    assert!(!nonexistent.exists());
}

#[test]
fn data_restore_does_not_initialize_default_or_explicit_source() {
    let dir = tempfile::tempdir().unwrap();
    let absent = dir.path().join("absent.sqlite3");
    let restored = dir.path().join("new.sqlite3");
    let result = Command::new(env!("CARGO_BIN_EXE_pos"))
        .args([
            "data",
            "restore",
            absent.to_str().unwrap(),
            "--output",
            restored.to_str().unwrap(),
        ])
        .env("XDG_DATA_HOME", dir.path().join("config"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!absent.exists());
    assert!(!restored.exists());
    assert!(!dir.path().join("config").exists());
}

#[test]
fn task_engine_v2_cli_validates_filters_and_atomic_edits() {
    use std::path::Path;

    fn run(db: &Path, args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_pos"))
            .arg("--db")
            .arg(db)
            .args(args)
            .output()
            .expect("run POS")
    }
    fn success(db: &Path, args: &[&str]) -> String {
        let output = run(db, args);
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }
    fn json(db: &Path, args: &[&str]) -> serde_json::Value {
        serde_json::from_str(&success(db, args)).unwrap()
    }
    fn failure(db: &Path, args: &[&str]) -> String {
        let output = run(db, args);
        assert!(!output.status.success(), "unexpected success: {args:?}");
        String::from_utf8_lossy(&output.stderr).into_owned()
    }

    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("pos.sqlite3");

    success(&db, &["projects", "add", "University"]);
    success(
        &db,
        &[
            "tasks",
            "add",
            "Urgent fix",
            "--project",
            "university",
            "--description",
            "Needle in a haystack\n100% important",
            "--priority",
            "urgent",
            "--status",
            "in-progress",
            "--due",
            "2026-12-01",
        ],
    );
    success(
        &db,
        &["tasks", "add", "Normal report", "--due", "2026-11-28"],
    );
    success(
        &db,
        &[
            "tasks",
            "add",
            "High _ priority",
            "--priority",
            "high",
            "--status",
            "blocked",
        ],
    );
    success(
        &db,
        &[
            "tasks",
            "add",
            "Finished",
            "--status",
            "completed",
            "--priority",
            "low",
        ],
    );

    let tasks = json(&db, &["tasks", "list", "--json", "--sort", "priority"]);
    let titles: Vec<_> = tasks
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["title"].as_str().unwrap())
        .collect();
    assert_eq!(
        titles,
        vec!["Urgent fix", "High _ priority", "Normal report"]
    );
    assert_eq!(tasks[0]["priority"], "urgent");
    assert_eq!(tasks[0]["status"], "in-progress");
    assert_eq!(
        tasks[0]["description"],
        "Needle in a haystack\n100% important"
    );
    assert_eq!(tasks[0]["completed"], false);

    let done = json(&db, &["tasks", "list", "--json", "--status", "completed"]);
    assert_eq!(done.as_array().unwrap().len(), 1);
    assert_eq!(done[0]["title"], "Finished");
    assert_eq!(done[0]["completed"], true);
    assert_eq!(
        json(&db, &["tasks", "list", "--json", "--all"])
            .as_array()
            .unwrap()
            .len(),
        4
    );

    for (filter, expected) in [
        (vec!["--search", "NEEDLE"], "Urgent fix"),
        (vec!["--search", "100%"], "Urgent fix"),
        (vec!["--search", "_"], "High _ priority"),
        (vec!["--due-before", "2026-11-28"], "Normal report"),
        (vec!["--project", "university"], "Urgent fix"),
        (vec!["--priority", "high"], "High _ priority"),
        (vec!["--status", "blocked"], "High _ priority"),
    ] {
        let mut args = vec!["tasks", "list", "--json"];
        args.extend(filter);
        let result = json(&db, &args);
        assert_eq!(result.as_array().unwrap().len(), 1, "{args:?}");
        assert_eq!(result[0]["title"], expected);
    }

    assert!(
        json(&db, &["tasks", "list", "--json", "--search", "' OR 1=1 --"])
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(failure(&db, &["tasks", "list", "--due-before", "2026-02-30"]).contains("due date"));
    assert!(failure(&db, &["tasks", "list", "--sort", "random"]).contains("sort must be"));
    assert!(failure(&db, &["tasks", "add", "Bad", "--priority", "extreme"]).contains("priority"));
    assert!(failure(&db, &["tasks", "add", "Bad", "--status", "nonexistent"]).contains("status"));

    let original = json(&db, &["tasks", "list", "--json", "--all"]);
    assert!(
        failure(
            &db,
            &[
                "tasks",
                "edit",
                "1",
                "--priority",
                "low",
                "--title",
                "Bad change",
                "--due",
                "2026-02-30",
                "--project",
                "university"
            ]
        )
        .contains("due date")
    );
    assert_eq!(json(&db, &["tasks", "list", "--json", "--all"]), original);

    // Failed project reassignment must not update other columns.
    assert!(
        failure(
            &db,
            &[
                "tasks",
                "edit",
                "1",
                "--title",
                "Changed",
                "--priority",
                "low",
                "--project",
                "nonexistent"
            ]
        )
        .contains("not found")
    );
    assert_eq!(json(&db, &["tasks", "list", "--json", "--all"]), original);

    success(
        &db,
        &[
            "tasks",
            "edit",
            "1",
            "--title",
            "Updated urgent",
            "--priority",
            "high",
            "--status",
            "blocked",
            "--description",
            "Brief note",
            "--clear-project",
        ],
    );
    let edited = json(&db, &["tasks", "list", "--json", "--all"]);
    let edited_task = edited
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == 1)
        .unwrap();
    assert_eq!(edited_task["title"], "Updated urgent");
    assert_eq!(edited_task["priority"], "high");
    assert_eq!(edited_task["status"], "blocked");
    assert_eq!(edited_task["project_id"], serde_json::Value::Null);
    assert_eq!(edited_task["description"], "Brief note");
    assert_eq!(edited_task["completed"], false);

    success(
        &db,
        &[
            "tasks",
            "edit",
            "1",
            "--clear-description",
            "--status",
            "completed",
        ],
    );
    let task = json(&db, &["tasks", "list", "--json", "--status", "completed"]);
    assert!(
        task.as_array()
            .unwrap()
            .iter()
            .any(|t| t["id"] == 1 && t["description"].is_null() && t["completed"] == true)
    );
    success(&db, &["tasks", "reopen", "1"]);
    let task = json(
        &db,
        &["tasks", "list", "--json", "--search", "Updated urgent"],
    );
    assert_eq!(task[0]["status"], "todo");
    success(&db, &["tasks", "done", "1"]);
    let task = json(&db, &["tasks", "list", "--json", "--status", "completed"]);
    assert!(
        task.as_array()
            .unwrap()
            .iter()
            .any(|t| t["id"] == 1 && t["status"] == "completed")
    );

    let export = json(&db, &["data", "export"]);
    assert_eq!(export["format_version"], 2);
    assert_eq!(export["tasks"].as_array().unwrap().len(), 4);
    let backup = dir.path().join("backup.sqlite3");
    success(&db, &["data", "backup", backup.to_str().unwrap()]);
    let backed_up = json(&backup, &["data", "export"]);
    assert_eq!(backed_up["tasks"].as_array().unwrap().len(), 4);
    assert_eq!(backed_up["tasks"][0]["status"], "completed");
    success(&db, &["data", "check"]);
}
