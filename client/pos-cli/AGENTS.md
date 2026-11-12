# pos-cli: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a binary crate.

## Responsibility

The single pos client executable, its argument parsing and TUI launch wiring.

## Boundaries

Do not implement module business rules here. Route operations through shared core and validated interfaces.

## Verification

Future tests should cover argument parsing, exit codes and non-interactive output.
The pos executable supports Tasks (including v2 search/sort/metadata), Projects and Data commands and launches Ratatui. Route operations to shared services and SQLite maintenance; do not add database logic to rendering or block the TUI thread.
