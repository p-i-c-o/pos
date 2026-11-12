# pos-cli: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a binary crate.

## Responsibility

The single pos client executable, its argument parsing and TUI launch wiring.

## Boundaries

Do not implement module business rules here. Route operations through shared core and validated interfaces.

## Verification

Future tests should cover argument parsing, exit codes and non-interactive output.
The pos executable supports Tasks and Projects subcommands and launches Ratatui. Use shared services and keep all database I/O off the TUI thread.
