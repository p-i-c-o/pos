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
    assert!(String::from_utf8_lossy(&output.stdout).contains("Launch the interactive terminal UI"));
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
    let output = pos(&["tasks"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Unsupported POS command"));
}
