# pos-cli: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a binary crate.

## Responsibility

The single pos client executable, its argument parsing and TUI launch wiring.

## Boundaries

Do not implement module business rules here. Route operations through shared core and validated interfaces.

## Verification

Future tests should cover argument parsing, exit codes and non-interactive output.
The pos executable supports Tasks (including v2 search/sort/metadata), Projects and Data commands and launches Ratatui. Route operations to shared services and SQLite maintenance; do not add database logic to rendering or block the TUI thread.

## Read-only server access

The `remote` CLI command is separate from local SQLite operations and
runs before local database startup. It inspects only authenticated identity,
accessible workspaces, remote projects and remote tasks via the server's
existing GET API. It never writes server records, converts remote UUIDs into
local integer IDs, caches remote responses, or starts synchronization.

No credentials in CLI arguments, configuration files tracked by Git, logs
or error bodies. Read a device-bound bearer token from a permission-restricted
token file, or from environment only in development. Explicit HTTP loopback
address only, no redirects or proxies; no public or LAN exposure. Preserve
bounded response handling, timeouts and terminal text sanitization.
Test via disposable loopback mocks and preserve local data isolation.
See docs/client-server-readonly.md.
