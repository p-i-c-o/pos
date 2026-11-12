//! POS client executable: terminal UI or a minimal noninteractive CLI.

use std::{
    env,
    io::{self, IsTerminal},
    process::ExitCode,
};

const USAGE: &str = "POS - Personal Organisation System

Usage:
  pos             Launch the interactive terminal UI
  pos --help      Display this help
  pos --version   Display the version

The application modules are not implemented yet.
";

fn main() -> ExitCode {
    let mut args = env::args_os().skip(1);
    match (args.next(), args.next()) {
        (Some(flag), None) if flag == "--help" || flag == "-h" => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        (Some(flag), None) if flag == "--version" || flag == "-V" => {
            println!("{} {}", pos_core::PRODUCT_NAME, env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        (None, None) => {
            if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
                eprintln!("POS needs an interactive terminal. Run 'pos --help' for usage.");
                return ExitCode::FAILURE;
            }
            match pos_tui::run() {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("POS terminal error: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            eprintln!("Unsupported POS command. Run 'pos --help' for usage.");
            ExitCode::FAILURE
        }
    }
}
